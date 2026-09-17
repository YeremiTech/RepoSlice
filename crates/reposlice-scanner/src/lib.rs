use reposlice_adapter_laravel::LaravelAdapter;
use reposlice_adapter_spring::SpringAdapter;
use reposlice_adapter_web::WebFrameworkSuite;
use reposlice_core::{
    clear_source_text_cache, read_source_text, technology_classification, AnalysisContribution,
    AnalysisDiagnostic, AnalysisMetadata, CompatibilityLevel, Component, ComponentKind,
    DiagnosticLevel, EntrypointMetadata, Evidence, EvidenceKind, FrameworkAdapter,
    FrameworkDetectionPolicy, FrameworkSuite, LanguageAnalyzer, ProjectModel, ProjectRole,
    RuntimeRequirement, ScanPolicy, SymbolMetadata, Technology, DEFAULT_MAX_FALLBACK_FILES,
    DEFAULT_MAX_INDEXED_TEXT_BYTES, DEFAULT_MAX_SOURCE_SIZE,
};
use reposlice_parser_java::JavaLanguageAnalyzer;
use reposlice_parser_php::PhpLanguageAnalyzer;
use reposlice_parser_polyglot::PolyglotLanguageAnalyzer;
use reposlice_parser_typescript::{discover_http_calls, TypeScriptLanguageAnalyzer};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Mutex, OnceLock};
use std::time::UNIX_EPOCH;

static MODEL_CACHE: OnceLock<Mutex<BTreeMap<String, (u64, ProjectModel)>>> = OnceLock::new();
static CLEAN_GIT_MODEL_CACHE: OnceLock<Mutex<BTreeMap<String, (String, ProjectModel)>>> =
    OnceLock::new();

const ANALYZER_REGISTRY_SCHEMA: u32 = 2;

#[derive(Default)]
struct ScanTextIndex {
    lowercase: HashMap<PathBuf, String>,
    truncated: bool,
}

impl ScanTextIndex {
    fn build(files: &[PathBuf]) -> Self {
        let mut index = Self::default();
        let mut used = 0usize;
        for file in files {
            let size = fs::metadata(file)
                .map(|metadata| metadata.len())
                .unwrap_or(DEFAULT_MAX_SOURCE_SIZE + 1);
            if size > DEFAULT_MAX_SOURCE_SIZE
                || (language_from_extension(file) == "Unknown" && !likely_text_file(file))
            {
                continue;
            }
            let Ok(content) = read_source_text(file) else {
                continue;
            };
            if used.saturating_add(content.len()) > DEFAULT_MAX_INDEXED_TEXT_BYTES {
                index.truncated = true;
                continue;
            }
            used = used.saturating_add(content.len());
            index
                .lowercase
                .insert(file.clone(), content.to_ascii_lowercase());
        }
        index
    }

    fn get(&self, path: &Path) -> Option<&str> {
        self.lowercase.get(path).map(String::as_str)
    }
}

const UNIT_MANIFESTS: &[&str] = &[
    "pom.xml",
    "build.gradle",
    "build.gradle.kts",
    "settings.gradle",
    "settings.gradle.kts",
    "package.json",
    "angular.json",
    "nx.json",
    "pnpm-workspace.yaml",
    "Cargo.toml",
    "pyproject.toml",
    "requirements.txt",
    "Pipfile",
    "go.mod",
    "composer.json",
    "Gemfile",
    "pubspec.yaml",
    "CMakeLists.txt",
    "mix.exs",
];

pub struct AnalyzerRegistry {
    language_analyzers: Vec<Box<dyn LanguageAnalyzer>>,
    framework_adapters: Vec<Box<dyn FrameworkAdapter>>,
    framework_suites: Vec<Box<dyn FrameworkSuite>>,
    detection_policy: FrameworkDetectionPolicy,
}

impl Default for AnalyzerRegistry {
    fn default() -> Self {
        Self::with_defaults()
    }
}

impl AnalyzerRegistry {
    pub fn new() -> Self {
        Self::empty()
    }

    pub fn empty() -> Self {
        Self {
            language_analyzers: Vec::new(),
            framework_adapters: Vec::new(),
            framework_suites: Vec::new(),
            detection_policy: FrameworkDetectionPolicy::default(),
        }
    }

    pub fn with_defaults() -> Self {
        let mut registry = Self::empty();
        registry.register_language(JavaLanguageAnalyzer);
        registry.register_language(TypeScriptLanguageAnalyzer);
        registry.register_language(PhpLanguageAnalyzer);
        registry.register_language(PolyglotLanguageAnalyzer);
        registry.register_framework(SpringAdapter);
        registry.register_framework(LaravelAdapter);
        registry.register_framework_suite(WebFrameworkSuite);
        registry
    }

    pub fn register_language<A>(&mut self, analyzer: A)
    where
        A: LanguageAnalyzer + 'static,
    {
        let id = analyzer.id();
        self.language_analyzers
            .retain(|existing| existing.id() != id);
        self.language_analyzers.push(Box::new(analyzer));
    }

    pub fn register_framework<A>(&mut self, adapter: A)
    where
        A: FrameworkAdapter + 'static,
    {
        let id = adapter.id();
        self.framework_adapters
            .retain(|existing| existing.id() != id);
        self.framework_adapters.push(Box::new(adapter));
    }

    pub fn register_framework_suite<A>(&mut self, suite: A)
    where
        A: FrameworkSuite + 'static,
    {
        let id = suite.id();
        self.framework_suites.retain(|existing| existing.id() != id);
        self.framework_suites.push(Box::new(suite));
    }

    pub fn language_ids(&self) -> Vec<&'static str> {
        self.language_analyzers
            .iter()
            .map(|item| item.id())
            .collect()
    }

    pub fn framework_ids(&self) -> Vec<&'static str> {
        self.framework_adapters
            .iter()
            .map(|item| item.id())
            .chain(self.framework_suites.iter().map(|item| item.id()))
            .collect()
    }

    pub fn set_detection_policy(&mut self, policy: FrameworkDetectionPolicy) {
        self.detection_policy = policy;
    }

    pub fn with_detection_policy(mut self, policy: FrameworkDetectionPolicy) -> Self {
        self.detection_policy = policy;
        self
    }

    pub fn detection_policy(&self) -> &FrameworkDetectionPolicy {
        &self.detection_policy
    }

    fn is_actionable(&self, detection: &reposlice_core::FrameworkDetection) -> bool {
        detection.actionable_with(&self.detection_policy)
    }

    pub fn is_empty(&self) -> bool {
        self.language_analyzers.is_empty()
            && self.framework_adapters.is_empty()
            && self.framework_suites.is_empty()
    }

    pub fn signature(&self) -> String {
        let mut ids = self
            .language_analyzers
            .iter()
            .map(|item| format!("language:{}", item.id()))
            .chain(
                self.framework_adapters
                    .iter()
                    .map(|item| format!("framework:{}", item.id())),
            )
            .chain(
                self.framework_suites
                    .iter()
                    .map(|item| format!("suite:{}", item.id())),
            )
            .collect::<Vec<_>>();
        ids.sort();
        format!(
            "schema:{}|{}|policy:{}:{}:{}",
            ANALYZER_REGISTRY_SCHEMA,
            ids.join("|"),
            self.detection_policy.minimum_confidence,
            self.detection_policy.conditional_confidence,
            self.detection_policy.strong_source_confidence
        )
    }
}

pub fn scan_project_with_registry(
    root: &Path,
    registry: &AnalyzerRegistry,
) -> io::Result<ProjectModel> {
    scan_project_pipeline(root, registry)
}

pub fn discover_project_units(root: &Path) -> io::Result<Vec<PathBuf>> {
    let canonical = fs::canonicalize(root)?;
    let mut candidates = Vec::new();
    discover_manifest_directories(&canonical, &canonical, &mut candidates)?;
    candidates.sort();
    candidates.dedup();

    if candidates.is_empty() {
        return Ok(vec![canonical]);
    }

    let discovered = candidates.clone();
    candidates.retain(|candidate| {
        let contains_nested = discovered
            .iter()
            .any(|other| other != candidate && other.starts_with(candidate));
        !contains_nested || is_application_root(candidate)
    });
    Ok(candidates)
}

pub fn scan_project_excluding(root: &Path, excluded_roots: &[PathBuf]) -> io::Result<ProjectModel> {
    let registry = AnalyzerRegistry::with_defaults();
    scan_project_excluding_with_registry(root, excluded_roots, &registry)
}

pub fn scan_project_excluding_with_registry(
    root: &Path,
    excluded_roots: &[PathBuf],
    registry: &AnalyzerRegistry,
) -> io::Result<ProjectModel> {
    let mut model = scan_project_with_registry(root, registry)?;
    let excluded: Vec<PathBuf> = excluded_roots
        .iter()
        .filter_map(|path| fs::canonicalize(path).ok())
        .collect();
    let is_excluded = |file: &str| {
        let path = Path::new(file);
        excluded.iter().any(|root| path.starts_with(root))
    };
    let removed_ids: HashSet<String> = model
        .components
        .iter()
        .filter(|item| is_excluded(&item.file))
        .map(|item| item.id.clone())
        .collect();
    model
        .components
        .retain(|item| !removed_ids.contains(&item.id));
    model
        .entrypoints
        .retain(|item| !removed_ids.contains(&item.component_id) && !is_excluded(&item.file));
    model.dependencies.retain(|item| {
        !removed_ids.contains(&item.source_id) && !removed_ids.contains(&item.target_id)
    });
    model
        .analysis
        .symbols
        .retain(|item| !removed_ids.contains(&item.symbol_id));
    model.analysis.entrypoints.retain(|item| {
        model
            .entrypoints
            .iter()
            .any(|entrypoint| entrypoint.id == item.entrypoint_id)
    });
    model.analysis.dependencies.retain(|item| {
        !removed_ids.contains(&item.source_id) && !removed_ids.contains(&item.target_id)
    });
    for symbol in &mut model.analysis.symbols {
        symbol.evidence.retain(|item| !is_excluded(&item.source));
    }
    for entrypoint in &mut model.analysis.entrypoints {
        entrypoint
            .evidence
            .retain(|item| !is_excluded(&item.source));
    }
    for dependency in &mut model.analysis.dependencies {
        dependency
            .evidence
            .retain(|item| !is_excluded(&item.source));
    }
    model.analysis.framework_detections.retain_mut(|detection| {
        let had_evidence = !detection.evidence.is_empty();
        detection.evidence.retain(|item| !is_excluded(&item.source));
        !had_evidence || !detection.evidence.is_empty()
    });
    let remaining_languages: HashSet<String> = model
        .components
        .iter()
        .map(|component| component.language.clone())
        .collect();
    let remaining_frameworks: HashSet<String> = model
        .analysis
        .framework_detections
        .iter()
        .map(|detection| detection.name.clone())
        .collect();
    model
        .technologies
        .retain(|technology| match technology.category.as_str() {
            "language" => remaining_languages.contains(&technology.name),
            "framework" => remaining_frameworks.contains(&technology.name),
            _ => true,
        });
    model.analysis.runtime_requirements.retain(|item| {
        let required_by = Path::new(&item.required_by);
        !required_by.is_absolute() || !excluded.iter().any(|root| required_by.starts_with(root))
    });
    model
        .analysis
        .diagnostics
        .retain(|item| item.file.as_deref().is_none_or(|file| !is_excluded(file)));
    let excluded_count: usize = excluded
        .iter()
        .filter_map(|root| collect_all_files(root).ok())
        .map(|files| files.len())
        .sum();
    model.files = model.files.saturating_sub(excluded_count);
    model.compatibility = compatibility_for_model(&model, registry.detection_policy());
    Ok(model)
}

pub fn discover_http_calls_for_unit(
    root: &Path,
    components: &[reposlice_core::Component],
) -> anyhow::Result<Vec<reposlice_core::HttpCall>> {
    Ok(discover_http_calls(root, components)?)
}

pub fn classify_project_role(model: &ProjectModel) -> ProjectRole {
    let names: Vec<&str> = model
        .technologies
        .iter()
        .map(|technology| technology.name.as_str())
        .collect();
    let frontend = names.iter().any(|name| {
        matches!(
            *name,
            "Angular"
                | "React"
                | "Next.js"
                | "Vue"
                | "Nuxt"
                | "Svelte"
                | "SvelteKit"
                | "Astro"
                | "Blazor"
                | "Vite"
        )
    }) || Path::new(&model.root).join("index.html").is_file();
    let backend = names.iter().any(|name| {
        matches!(
            *name,
            "Spring Boot"
                | "Quarkus"
                | "Micronaut"
                | "Ktor"
                | "NestJS"
                | "Express"
                | "Fastify"
                | "Hono"
                | "FastAPI"
                | "Django"
                | "Flask"
                | "ASP.NET"
                | "Laravel"
                | "Symfony"
                | "Rails"
                | "Go Web"
                | "Rust Web"
                | "Phoenix"
        )
    }) || model.entrypoints.iter().any(|entrypoint| {
        matches!(
            entrypoint.kind.as_str(),
            "http" | "cli" | "event" | "scheduled" | "graphql"
        )
    });
    let frontend_application = model
        .entrypoints
        .iter()
        .any(|entrypoint| entrypoint.kind.as_str() == "route")
        || Path::new(&model.root).join("index.html").is_file();
    if frontend && backend {
        ProjectRole::Fullstack
    } else if frontend && package_is_library(Path::new(&model.root)) && !frontend_application {
        ProjectRole::Library
    } else if frontend {
        ProjectRole::Frontend
    } else if backend {
        ProjectRole::Backend
    } else if infrastructure_only(Path::new(&model.root)) {
        ProjectRole::Infrastructure
    } else if package_is_library(Path::new(&model.root)) {
        ProjectRole::Library
    } else {
        ProjectRole::Unknown
    }
}

pub fn scan_project(root: &Path) -> io::Result<ProjectModel> {
    let registry = AnalyzerRegistry::with_defaults();
    scan_project_pipeline(root, &registry)
}

fn scan_project_pipeline(root: &Path, registry: &AnalyzerRegistry) -> io::Result<ProjectModel> {
    let canonical = fs::canonicalize(root)?;
    let name = canonical
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("project")
        .to_string();

    let cache_key = format!("{}::{}", canonical.to_string_lossy(), registry.signature());
    let clean_head = clean_git_head(&canonical);
    if let Some(head) = clean_head.as_deref() {
        if let Some((cached_head, model)) = clean_git_model_cache()
            .lock()
            .ok()
            .and_then(|cache| cache.get(&cache_key).cloned())
        {
            if cached_head == head {
                return Ok(model);
            }
        }
    }

    let all_files = collect_all_files(&canonical)?;
    let fingerprint = project_fingerprint(&canonical, &all_files);
    if let Some((cached_fingerprint, model)) = model_cache()
        .lock()
        .ok()
        .and_then(|cache| cache.get(&cache_key).cloned())
    {
        if cached_fingerprint == fingerprint {
            return Ok(model);
        }
    }
    let text_index = ScanTextIndex::build(&all_files);
    let files = all_files.len();
    let mut technologies = Vec::new();
    let mut components = Vec::new();
    let mut dependencies = Vec::new();
    let mut entrypoints = Vec::new();
    let mut compatibility = CompatibilityLevel::Filesystem;
    let mut analysis = AnalysisMetadata::default();
    if text_index.truncated {
        analysis.diagnostics.push(AnalysisDiagnostic {
            level: DiagnosticLevel::Info,
            code: "text-index-memory-limit".into(),
            message: format!(
                "In-memory text index reached its {} MiB safety limit; remaining files use conservative detection",
                DEFAULT_MAX_INDEXED_TEXT_BYTES / (1024 * 1024)
            ),
            file: None,
        });
    }

    technologies.extend(detect_databases(&canonical, &all_files, &text_index));
    technologies.extend(detect_sql_and_orm(&all_files, &text_index));

    for language in [
        "Java",
        "Kotlin",
        "JavaScript",
        "TypeScript",
        "Python",
        "C#",
        "F#",
        "Go",
        "Rust",
        "PHP",
        "Ruby",
        "Dart",
        "C",
        "C++",
        "Swift",
        "Scala",
        "Shell",
        "SQL",
        "HTML",
        "XML",
        "CSS",
        "SCSS",
        "GraphQL",
        "Vue",
        "Svelte",
        "Astro",
        "Razor",
        "Elixir",
    ] {
        if all_files
            .iter()
            .any(|path| language_from_extension(path) == language)
        {
            technologies.push(technology("language", language, 100));
        }
    }

    let has_java = all_files
        .iter()
        .any(|path| path.extension().and_then(|value| value.to_str()) == Some("java"));
    let has_php = all_files
        .iter()
        .any(|path| path.extension().and_then(|value| value.to_str()) == Some("php"));

    for analyzer in &registry.language_analyzers {
        if !all_files.iter().any(|path| analyzer.supports(path)) {
            continue;
        }
        let contribution = analyzer.analyze(&canonical)?;
        merge_contribution_parts(
            &mut technologies,
            &mut components,
            &mut entrypoints,
            &mut dependencies,
            &mut analysis,
            &mut compatibility,
            contribution,
            CompatibilityLevel::Syntax,
        );
    }

    let pom = canonical.join("pom.xml");
    let gradle = [
        canonical.join("build.gradle"),
        canonical.join("build.gradle.kts"),
    ]
    .into_iter()
    .find(|path| path.is_file());
    let mut java_manifest = String::new();
    let mut java_manifest_path = pom.clone();
    if pom.exists() {
        technologies.push(technology("build", "Maven", 100));
        let content = read_source_text(&pom).unwrap_or_default();
        java_manifest = content.clone();
        if content.contains("org.postgresql") || content.contains("postgresql") {
            technologies.push(technology("data", "PostgreSQL", 80));
        }
    } else if let Some(path) = gradle {
        technologies.push(technology("build", "Gradle", 100));
        java_manifest = read_source_text(&path).unwrap_or_default();
        java_manifest_path = path;
    }
    if has_java {
        analysis.runtime_requirements.push(RuntimeRequirement {
            name: "Java".into(),
            executable: "java".into(),
            version_hint: maven_java_version(&java_manifest),
            required_by: java_manifest_path.to_string_lossy().into_owned(),
            confidence: if java_manifest.is_empty() { 80 } else { 95 },
        });
    }

    let package_json = canonical.join("package.json");
    if package_json.exists() {
        let content = read_source_text(&package_json).unwrap_or_default();

        if canonical.join("package-lock.json").exists() {
            technologies.push(technology("build", "npm", 100));
        } else if canonical.join("pnpm-lock.yaml").exists() {
            technologies.push(technology("build", "pnpm", 100));
        } else if canonical.join("yarn.lock").exists() {
            technologies.push(technology("build", "Yarn", 100));
        }

        if content.contains("\"vite\"")
            || all_files.iter().any(|path| {
                path.file_name()
                    .is_some_and(|name| name.to_string_lossy().starts_with("vite.config"))
            })
        {
            technologies.push(technology("build", "Vite", 85));
        }
        if content.contains("\"webpack\"")
            || all_files.iter().any(|path| {
                path.file_name()
                    .is_some_and(|name| name.to_string_lossy().starts_with("webpack.config"))
            })
        {
            technologies.push(technology("build", "Webpack", 85));
        }

        analysis.runtime_requirements.push(RuntimeRequirement {
            name: "Node.js".into(),
            executable: "node".into(),
            version_hint: json_string_after(&content, "node"),
            required_by: package_json.to_string_lossy().into_owned(),
            confidence: 90,
        });
    }

    if canonical.join("package-lock.json").exists()
        && !technologies.iter().any(|item| item.name == "npm")
    {
        technologies.push(technology("build", "npm", 100));
    }
    if canonical.join("pnpm-lock.yaml").exists()
        && !technologies.iter().any(|item| item.name == "pnpm")
    {
        technologies.push(technology("build", "pnpm", 100));
    }
    if canonical.join("yarn.lock").exists() && !technologies.iter().any(|item| item.name == "Yarn")
    {
        technologies.push(technology("build", "Yarn", 100));
    }

    if canonical.join("Cargo.toml").exists() {
        technologies.push(technology("build", "Cargo", 100));
        analysis.runtime_requirements.push(RuntimeRequirement {
            name: "Rust".into(),
            executable: "rustc".into(),
            version_hint: None,
            required_by: canonical.join("Cargo.toml").to_string_lossy().into_owned(),
            confidence: 100,
        });
    }

    if canonical.join("pyproject.toml").exists() || canonical.join("requirements.txt").exists() {
        analysis.runtime_requirements.push(RuntimeRequirement {
            name: "Python".into(),
            executable: "python".into(),
            version_hint: None,
            required_by: "Python manifest".into(),
            confidence: 90,
        });
    }

    if canonical.join("go.mod").exists() {
        technologies.push(technology("build", "Go Modules", 100));
        analysis.runtime_requirements.push(RuntimeRequirement {
            name: "Go".into(),
            executable: "go".into(),
            version_hint: None,
            required_by: canonical.join("go.mod").to_string_lossy().into_owned(),
            confidence: 100,
        });
    }

    if let Ok(entries) = fs::read_dir(&canonical) {
        for entry in entries.flatten() {
            let path = entry.path();
            let name = path
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("");
            if name.ends_with(".csproj") {
                analysis.runtime_requirements.push(RuntimeRequirement {
                    name: ".NET".into(),
                    executable: "dotnet".into(),
                    version_hint: None,
                    required_by: path.to_string_lossy().into_owned(),
                    confidence: 100,
                });
            }
        }
    }

    if has_php {
        let composer_path = canonical.join("composer.json");
        if composer_path.is_file() {
            technologies.push(technology("build", "Composer", 100));
            let composer = read_source_text(&composer_path).unwrap_or_default();
            analysis.runtime_requirements.extend([
                RuntimeRequirement {
                    name: "PHP".into(),
                    executable: "php".into(),
                    version_hint: json_string_after(&composer, "php"),
                    required_by: composer_path.to_string_lossy().into_owned(),
                    confidence: 100,
                },
                RuntimeRequirement {
                    name: "Composer".into(),
                    executable: "composer".into(),
                    version_hint: None,
                    required_by: composer_path.to_string_lossy().into_owned(),
                    confidence: 100,
                },
            ]);
        } else {
            analysis.runtime_requirements.push(RuntimeRequirement {
                name: "PHP".into(),
                executable: "php".into(),
                version_hint: None,
                required_by: "PHP source files".into(),
                confidence: 90,
            });
        }
    }

    for adapter in &registry.framework_adapters {
        let detection = adapter.detect(&canonical);
        if !detection.evidence.is_empty() {
            analysis.framework_detections.push(detection.clone());
        }
        if !registry.is_actionable(&detection) {
            if !detection.evidence.is_empty() {
                analysis.diagnostics.push(AnalysisDiagnostic {
                    level: DiagnosticLevel::Info,
                    code: "framework-evidence-insufficient".into(),
                    message: format!(
                        "Framework adapter {} produced evidence, but it did not meet the actionable confidence policy",
                        adapter.id()
                    ),
                    file: detection.evidence.first().map(|item| item.source.clone()),
                });
            }
            continue;
        }
        let contribution = adapter.analyze(&canonical)?;
        merge_contribution_parts(
            &mut technologies,
            &mut components,
            &mut entrypoints,
            &mut dependencies,
            &mut analysis,
            &mut compatibility,
            contribution,
            CompatibilityLevel::Framework,
        );
    }

    for suite in &registry.framework_suites {
        for framework in suite.analyze(&canonical, &components)? {
            if !framework.detection.evidence.is_empty() {
                analysis
                    .framework_detections
                    .push(framework.detection.clone());
            }
            if !registry.is_actionable(&framework.detection) {
                analysis.diagnostics.push(AnalysisDiagnostic {
                    level: DiagnosticLevel::Info,
                    code: "framework-evidence-insufficient".into(),
                    message: format!(
                        "{} was observed by {} but did not meet the actionable confidence policy",
                        framework.detection.name,
                        suite.id()
                    ),
                    file: framework
                        .detection
                        .evidence
                        .first()
                        .map(|item| item.source.clone()),
                });
                continue;
            }
            merge_contribution_parts(
                &mut technologies,
                &mut components,
                &mut entrypoints,
                &mut dependencies,
                &mut analysis,
                &mut compatibility,
                framework.contribution,
                CompatibilityLevel::Framework,
            );
        }
    }

    if has_php
        && !analysis
            .framework_detections
            .iter()
            .any(|item| item.name == "Laravel" && registry.is_actionable(item))
    {
        analysis.diagnostics.push(AnalysisDiagnostic {
            level: DiagnosticLevel::Info,
            code: "generic-php-fallback".into(),
            message: "PHP syntax was analyzed without assuming Laravel".into(),
            file: None,
        });
    }

    if canonical.join("Dockerfile").exists()
        || canonical.join("docker-compose.yml").exists()
        || canonical.join("compose.yml").exists()
    {
        technologies.push(technology("runtime", "Docker", 100));
        analysis.runtime_requirements.push(RuntimeRequirement {
            name: "Docker".into(),
            executable: "docker".into(),
            version_hint: None,
            required_by: canonical.to_string_lossy().into_owned(),
            confidence: 100,
        });
    }

    let parsed_files: HashSet<String> = components
        .iter()
        .map(|component| {
            Path::new(&component.file)
                .to_string_lossy()
                .replace('\\', "/")
        })
        .collect();
    if all_files.len() > DEFAULT_MAX_FALLBACK_FILES {
        analysis.diagnostics.push(AnalysisDiagnostic {
            level: DiagnosticLevel::Warning,
            code: "component-index-limit".into(),
            message: format!(
                "Filesystem fallback components were limited to {} of {} indexed files",
                DEFAULT_MAX_FALLBACK_FILES,
                all_files.len()
            ),
            file: None,
        });
    }
    for file in all_files.iter().take(DEFAULT_MAX_FALLBACK_FILES) {
        let canonical_file = file.to_string_lossy().replace('\\', "/");
        if parsed_files.contains(&canonical_file) {
            continue;
        }
        if fs::metadata(file)
            .map(|m| m.len() > DEFAULT_MAX_SOURCE_SIZE)
            .unwrap_or(false)
        {
            analysis.diagnostics.push(AnalysisDiagnostic {
                level: DiagnosticLevel::Info,
                code: "source-too-large".into(),
                message:
                    "File was indexed but omitted from syntax analysis because it exceeds 2 MiB"
                        .into(),
                file: Some(file.to_string_lossy().into_owned()),
            });
            continue;
        }
        let language = language_from_extension(file);
        if language != "Unknown" || likely_text_file(file) {
            let name = file
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("file")
                .to_string();

            components.push(Component {
                id: format!(
                    "file:{}",
                    file.strip_prefix(&canonical)
                        .unwrap_or(file)
                        .to_string_lossy()
                        .replace('\\', "/")
                ),
                name,
                kind: ComponentKind::File,
                language,
                file: file.to_string_lossy().to_string(),
            });
        }
    }

    attach_technology_evidence(
        &mut technologies,
        &canonical,
        &all_files,
        &analysis,
        &text_index,
    );
    technologies.sort_by(|left, right| {
        left.category
            .cmp(&right.category)
            .then(left.name.cmp(&right.name))
    });
    technologies.dedup_by(|left, right| left.category == right.category && left.name == right.name);

    components.sort_by(|left, right| left.id.cmp(&right.id).then(left.file.cmp(&right.file)));
    components.dedup_by(|left, right| left.id == right.id);
    entrypoints.sort_by(|left, right| left.id.cmp(&right.id));
    entrypoints.dedup_by(|left, right| left.id == right.id);
    dependencies.sort_by(|left, right| {
        (&left.source_id, &left.target_id, left.kind.as_str()).cmp(&(
            &right.source_id,
            &right.target_id,
            right.kind.as_str(),
        ))
    });
    dependencies.dedup();
    for component in &components {
        if !analysis
            .symbols
            .iter()
            .any(|item| item.symbol_id == component.id)
        {
            analysis.symbols.push(SymbolMetadata {
                symbol_id: component.id.clone(),
                qualified_name: None,
                namespace: None,
                framework_kind: None,
                attributes: Vec::new(),
                evidence: vec![source_evidence(&component.file, "Source component", 90)],
                confidence: 90,
            });
        }
    }
    for entrypoint in &entrypoints {
        if !analysis
            .entrypoints
            .iter()
            .any(|item| item.entrypoint_id == entrypoint.id)
        {
            let (method, path) = if entrypoint.kind.as_str() == "http" {
                entrypoint
                    .name
                    .split_once(' ')
                    .map(|(method, path)| (Some(method.to_string()), Some(path.to_string())))
                    .unwrap_or((None, None))
            } else {
                (None, None)
            };
            analysis.entrypoints.push(EntrypointMetadata {
                entrypoint_id: entrypoint.id.clone(),
                method,
                path,
                route_name: None,
                domain: None,
                middleware: Vec::new(),
                controller: None,
                action: None,
                evidence: vec![source_evidence(
                    &entrypoint.file,
                    "Framework entrypoint declaration",
                    95,
                )],
                confidence: 95,
            });
        }
    }
    for dependency in &dependencies {
        if !analysis.dependencies.iter().any(|item| {
            item.source_id == dependency.source_id
                && item.target_id == dependency.target_id
                && item.kind == dependency.kind.as_str()
        }) {
            let source_file = components
                .iter()
                .find(|item| item.id == dependency.source_id)
                .map(|item| item.file.clone())
                .unwrap_or_else(|| canonical.to_string_lossy().into_owned());
            analysis
                .dependencies
                .push(reposlice_core::DependencyMetadata {
                    source_id: dependency.source_id.clone(),
                    target_id: dependency.target_id.clone(),
                    kind: dependency.kind.as_str().into(),
                    evidence: vec![source_evidence(
                        &source_file,
                        "Resolved source reference",
                        85,
                    )],
                    confidence: 85,
                });
        }
    }
    analysis
        .framework_detections
        .sort_by(|left, right| left.name.cmp(&right.name));
    analysis
        .symbols
        .sort_by(|left, right| left.symbol_id.cmp(&right.symbol_id));
    analysis
        .entrypoints
        .sort_by(|left, right| left.entrypoint_id.cmp(&right.entrypoint_id));
    analysis.runtime_requirements.sort_by(|left, right| {
        left.name
            .cmp(&right.name)
            .then(left.required_by.cmp(&right.required_by))
    });
    analysis
        .runtime_requirements
        .dedup_by(|left, right| left.name == right.name && left.required_by == right.required_by);

    let model = ProjectModel {
        root: canonical.to_string_lossy().to_string(),
        name,
        files,
        compatibility,
        technologies,
        components,
        entrypoints,
        dependencies,
        analysis,
    };
    if let Ok(mut cache) = model_cache().lock() {
        if cache.len() >= 32 && !cache.contains_key(&cache_key) {
            if let Some(oldest_key) = cache.keys().next().cloned() {
                cache.remove(&oldest_key);
            }
        }
        cache.insert(cache_key.clone(), (fingerprint, model.clone()));
    }
    if let Some(head) = clean_head {
        if let Ok(mut cache) = clean_git_model_cache().lock() {
            if cache.len() >= 32 && !cache.contains_key(&cache_key) {
                if let Some(oldest_key) = cache.keys().next().cloned() {
                    cache.remove(&oldest_key);
                }
            }
            cache.insert(cache_key, (head, model.clone()));
        }
    }
    Ok(model)
}

fn merge_contribution_parts(
    technologies: &mut Vec<Technology>,
    components: &mut Vec<Component>,
    entrypoints: &mut Vec<reposlice_core::Entrypoint>,
    dependencies: &mut Vec<reposlice_core::Dependency>,
    analysis: &mut AnalysisMetadata,
    compatibility: &mut CompatibilityLevel,
    contribution: AnalysisContribution,
    minimum_level: CompatibilityLevel,
) {
    technologies.extend(contribution.technologies);
    overlay_components(components, contribution.components);
    entrypoints.extend(contribution.entrypoints);
    let has_dependencies = !contribution.dependencies.is_empty();
    dependencies.extend(contribution.dependencies);
    analysis
        .framework_detections
        .extend(contribution.metadata.framework_detections);
    merge_symbols(&mut analysis.symbols, contribution.metadata.symbols);
    analysis
        .entrypoints
        .extend(contribution.metadata.entrypoints);
    analysis
        .dependencies
        .extend(contribution.metadata.dependencies);
    analysis
        .runtime_requirements
        .extend(contribution.metadata.runtime_requirements);
    analysis
        .diagnostics
        .extend(contribution.metadata.diagnostics);
    let level = if matches!(minimum_level, CompatibilityLevel::Syntax) && has_dependencies {
        CompatibilityLevel::Semantic
    } else {
        minimum_level
    };
    raise_compatibility(compatibility, level);
}

fn technology(category: &str, name: &str, confidence: u8) -> Technology {
    let (_canonical_category, classification) = technology_classification(category, name);
    let canonical_name = if matches!(category, "framework" | "data" | "language") {
        reposlice_core::canonical_technology_name(name)
    } else {
        name.to_string()
    };
    let detected_from = match category {
        "language" => vec!["extension".into()],
        "build" => vec!["manifest".into()],
        "runtime" => vec!["infrastructure".into()],
        "framework" => vec!["manifest".into(), "source-code".into()],
        _ => vec!["manifest".into()],
    };
    Technology {
        category: category.to_string(),
        name: canonical_name,
        classification: classification.to_string(),
        confidence,
        evidence: Vec::new(),
        scope: "project-unit".into(),
        repository_id: None,
        project_unit_id: None,
        detected_from,
        detection_kind: "DIRECT".into(),
    }
}

fn attach_technology_evidence(
    technologies: &mut [Technology],
    root: &Path,
    files: &[PathBuf],
    analysis: &AnalysisMetadata,
    text_index: &ScanTextIndex,
) {
    for technology in technologies {
        if technology.category == "framework" || technology.category == "library" {
            if let Some(detection) = analysis
                .framework_detections
                .iter()
                .find(|item| item.name.eq_ignore_ascii_case(&technology.name))
            {
                technology.evidence = detection.evidence.clone();
            }
        }
        if technology.evidence.is_empty() {
            let markers: Vec<&str> = match technology.name.as_str() {
                "Maven" => vec!["pom.xml"],
                "Gradle" => vec!["build.gradle", "build.gradle.kts"],
                "Cargo" => vec!["Cargo.toml", "Cargo.lock"],
                "Composer" => vec!["composer.json", "composer.lock"],
                "npm" => vec!["package.json", "package-lock.json"],
                "pnpm" => vec!["package.json", "pnpm-lock.yaml"],
                "Yarn" => vec!["package.json", "yarn.lock"],
                "Webpack" => vec!["webpack.config"],
                "Vite" => vec!["vite.config"],
                "Docker" => vec!["Dockerfile", "docker-compose.yml", "compose.yml"],
                "Hibernate" => vec!["pom.xml", "build.gradle", "build.gradle.kts"],
                "JPA" => vec!["pom.xml", "build.gradle", "build.gradle.kts"],
                "Prisma" | "TypeORM" | "Sequelize" => vec!["package.json"],
                "PostgreSQL" => vec!["postgresql", "postgres://", "jdbc:postgresql"],
                "MySQL" => vec!["mysql", "jdbc:mysql"],
                "MariaDB" => vec!["mariadb", "jdbc:mariadb"],
                "SQL Server" => vec!["sqlserver", "mssql"],
                "Oracle" => vec!["jdbc:oracle", "ojdbc"],
                "SQLite" => vec!["sqlite", "jdbc:sqlite"],
                "MongoDB" => vec!["mongodb", "mongodb+srv"],
                "Redis" => vec!["redis://", "image: redis"],
                _ => Vec::new(),
            };
            if !markers.is_empty() {
                for file in files {
                    let name = file.file_name().unwrap_or_default().to_string_lossy();
                    let content = text_index.get(file).unwrap_or("");
                    let content_match = technology.category == "data"
                        && markers.iter().any(|marker| content.contains(marker));
                    if markers
                        .iter()
                        .any(|marker| name == *marker || name.starts_with(marker))
                        || content_match
                    {
                        technology.evidence.push(Evidence {
                            kind: if technology.category == "data" {
                                EvidenceKind::Configuration
                            } else {
                                EvidenceKind::Manifest
                            },
                            source: file.to_string_lossy().into_owned(),
                            detail: format!("{} evidence", technology.name),
                            confidence: 95,
                        });
                    }
                }
            }
        }
        if technology.category == "language" {
            for file in files {
                if language_from_extension(file) == technology.name {
                    technology.evidence.push(Evidence {
                        kind: EvidenceKind::Source,
                        source: file.to_string_lossy().into_owned(),
                        detail: format!("{} file extension", technology.name),
                        confidence: 100,
                    });
                    if technology.evidence.len() >= 3 {
                        break;
                    }
                }
            }
        }
        technology
            .evidence
            .sort_by(|left, right| left.source.cmp(&right.source));
        technology
            .evidence
            .dedup_by(|left, right| left.source == right.source && left.detail == right.detail);
        if !technology.evidence.is_empty() {
            technology.detected_from = technology
                .evidence
                .iter()
                .map(|item| item.kind.as_str().to_string())
                .collect();
            technology.detected_from.sort();
            technology.detected_from.dedup();
            if technology
                .evidence
                .iter()
                .all(|item| item.kind == EvidenceKind::Convention)
            {
                technology.detection_kind = "INFERRED".into();
            }
            let independent = technology.evidence.len().min(4) as u8;
            if technology.category == "data" {
                technology.confidence = technology.confidence.max((55 + independent * 11).min(100));
            }
        } else {
            technology.evidence.push(Evidence {
                kind: EvidenceKind::Inference,
                source: root.to_string_lossy().into_owned(),
                detail: "Detected by project scanner".into(),
                confidence: technology.confidence,
            });
            technology.detected_from = vec!["inference".into()];
            technology.detection_kind = "INFERRED".into();
        }
    }
}

fn detect_databases(root: &Path, files: &[PathBuf], text_index: &ScanTextIndex) -> Vec<Technology> {
    let mut result = Vec::new();
    let definitions = [
        (
            "PostgreSQL",
            [
                "org.postgresql",
                "jdbc:postgresql://",
                "postgres://",
                "postgresql://",
                "image: postgres",
            ]
            .as_slice(),
        ),
        (
            "MySQL",
            ["com.mysql", "jdbc:mysql://", "mysql://", "image: mysql"].as_slice(),
        ),
        (
            "MariaDB",
            [
                "org.mariadb",
                "jdbc:mariadb://",
                "mariadb://",
                "image: mariadb",
            ]
            .as_slice(),
        ),
        (
            "SQL Server",
            [
                "mssql-jdbc",
                "jdbc:sqlserver://",
                "sqlserver://",
                "image: mssql",
            ]
            .as_slice(),
        ),
        (
            "Oracle",
            ["ojdbc", "jdbc:oracle:", "oracle://", "image: oracle"].as_slice(),
        ),
        (
            "SQLite",
            ["sqlite-jdbc", "jdbc:sqlite:", "sqlite://", "image: sqlite"].as_slice(),
        ),
        (
            "MongoDB",
            ["mongodb://", "mongodb+srv://", "mongodb"].as_slice(),
        ),
        ("Redis", ["redis://", "redis", "image: redis"].as_slice()),
    ];
    for (name, markers) in definitions {
        let mut matches = 0u8;
        for file in files {
            let file_name = file
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_ascii_lowercase();
            let relevant = file_name.contains("pom")
                || file_name.contains("gradle")
                || file_name.contains("package")
                || file_name.contains("application")
                || file_name.contains("docker")
                || file_name == ".env"
                || file.extension().is_some_and(|ext| {
                    matches!(
                        ext.to_string_lossy().as_ref(),
                        "yml" | "yaml" | "properties" | "json" | "toml"
                    )
                });
            if !relevant {
                continue;
            }
            let content = text_index.get(file).unwrap_or("");
            if markers.iter().any(|marker| content.contains(marker)) {
                matches = matches.saturating_add(1);
            }
        }
        if matches > 0 {
            result.push(technology(
                "data",
                name,
                (65u16 + u16::from(matches.min(3)) * 12).min(100) as u8,
            ));
        }
    }
    let _ = root;
    result
}

fn detect_sql_and_orm(files: &[PathBuf], text_index: &ScanTextIndex) -> Vec<Technology> {
    let mut result = Vec::new();
    let mut sql_evidence = false;
    let mut orm = Vec::new();
    for file in files {
        let name = file
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_ascii_lowercase();
        let content = text_index.get(file).unwrap_or("");
        if file
            .extension()
            .is_some_and(|ext| ext.to_string_lossy().as_ref() == "sql")
            || content.contains("@query(nativequery = true)")
            || content.contains("select ") && content.contains(" from ")
        {
            sql_evidence = true;
        }
        if content.contains("hibernate") || content.contains("org.hibernate") {
            orm.push(("Hibernate", file));
        }
        if content.contains("spring-data-jpa")
            || content.contains("javax.persistence")
            || content.contains("jakarta.persistence")
        {
            orm.push(("JPA", file));
        }
        if content.contains("prisma") {
            orm.push(("Prisma", file));
        }
        if content.contains("typeorm") {
            orm.push(("TypeORM", file));
        }
        if content.contains("sequelize") {
            orm.push(("Sequelize", file));
        }
        let _ = name;
    }
    if sql_evidence {
        result.push(technology("language", "SQL", 90));
    }
    for (name, _) in orm {
        result.push(technology("data", name, 80));
    }
    result
}

fn compatibility_rank(level: &CompatibilityLevel) -> u8 {
    match level {
        CompatibilityLevel::Filesystem => 0,
        CompatibilityLevel::Syntax => 1,
        CompatibilityLevel::Semantic => 2,
        CompatibilityLevel::Framework => 3,
        CompatibilityLevel::Runtime => 4,
        CompatibilityLevel::Capsule => 5,
    }
}

fn raise_compatibility(current: &mut CompatibilityLevel, candidate: CompatibilityLevel) {
    if compatibility_rank(&candidate) > compatibility_rank(current) {
        *current = candidate;
    }
}

fn compatibility_for_model(
    model: &ProjectModel,
    policy: &FrameworkDetectionPolicy,
) -> CompatibilityLevel {
    if model
        .analysis
        .framework_detections
        .iter()
        .any(|detection| detection.actionable_with(policy))
        || model
            .technologies
            .iter()
            .any(|technology| technology.category == "framework" && technology.confidence >= 60)
    {
        CompatibilityLevel::Framework
    } else if !model.dependencies.is_empty() {
        CompatibilityLevel::Semantic
    } else if model
        .components
        .iter()
        .any(|component| component.kind != ComponentKind::File)
    {
        CompatibilityLevel::Syntax
    } else {
        CompatibilityLevel::Filesystem
    }
}

fn model_cache() -> &'static Mutex<BTreeMap<String, (u64, ProjectModel)>> {
    MODEL_CACHE.get_or_init(|| Mutex::new(BTreeMap::new()))
}

fn clean_git_model_cache() -> &'static Mutex<BTreeMap<String, (String, ProjectModel)>> {
    CLEAN_GIT_MODEL_CACHE.get_or_init(|| Mutex::new(BTreeMap::new()))
}

fn clean_git_head(root: &Path) -> Option<String> {
    if !root.join(".git").exists() {
        return None;
    }
    let head = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()?;
    if !head.status.success() {
        return None;
    }
    let status = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["status", "--porcelain=v1", "--untracked-files=normal"])
        .output()
        .ok()?;
    if !status.status.success() || !status.stdout.is_empty() {
        return None;
    }
    let head = String::from_utf8(head.stdout).ok()?.trim().to_string();
    (!head.is_empty()).then_some(head)
}

pub fn clear_analysis_cache() {
    if let Ok(mut cache) = model_cache().lock() {
        cache.clear();
    }
    if let Ok(mut cache) = clean_git_model_cache().lock() {
        cache.clear();
    }
    clear_source_text_cache();
}

fn project_fingerprint(root: &Path, files: &[PathBuf]) -> u64 {
    let mut hash = 14695981039346656037u64;
    for file in files {
        let relative = file
            .strip_prefix(root)
            .unwrap_or(file)
            .to_string_lossy()
            .replace('\\', "/");
        for byte in relative.bytes() {
            hash ^= byte as u64;
            hash = hash.wrapping_mul(1099511628211);
        }
        if let Ok(metadata) = fs::metadata(file) {
            for byte in metadata.len().to_le_bytes() {
                hash ^= byte as u64;
                hash = hash.wrapping_mul(1099511628211);
            }
            if let Ok(modified) = metadata
                .modified()
                .and_then(|value| value.duration_since(UNIX_EPOCH).map_err(io::Error::other))
            {
                for byte in modified.as_nanos().to_le_bytes() {
                    hash ^= byte as u64;
                    hash = hash.wrapping_mul(1099511628211);
                }
            }
        }
    }
    hash
}

fn maven_java_version(content: &str) -> Option<String> {
    for key in [
        "java.version",
        "maven.compiler.release",
        "maven.compiler.source",
    ] {
        let open = format!("<{key}>");
        let close = format!("</{key}>");
        if let Some((_, rest)) = content.split_once(&open) {
            if let Some((value, _)) = rest.split_once(&close) {
                return Some(value.trim().to_string());
            }
        }
    }
    None
}

fn json_string_after(content: &str, key: &str) -> Option<String> {
    let marker = format!("\"{key}\"");
    let rest = content.split_once(&marker)?.1;
    let value = rest.split_once(':')?.1.trim_start().strip_prefix('"')?;
    Some(value.split('"').next()?.to_string())
}

fn source_evidence(file: &str, detail: &str, confidence: u8) -> Evidence {
    Evidence {
        kind: EvidenceKind::Source,
        source: file.into(),
        detail: detail.into(),
        confidence,
    }
}

fn overlay_components(components: &mut Vec<Component>, additions: Vec<Component>) {
    for addition in additions {
        if let Some(existing) = components.iter_mut().find(|item| item.id == addition.id) {
            *existing = addition;
        } else {
            components.push(addition);
        }
    }
}

fn merge_symbols(symbols: &mut Vec<SymbolMetadata>, additions: Vec<SymbolMetadata>) {
    for addition in additions {
        if let Some(existing) = symbols
            .iter_mut()
            .find(|item| item.symbol_id == addition.symbol_id)
        {
            if addition.framework_kind.is_some() {
                existing.framework_kind = addition.framework_kind;
            }
            existing.attributes.extend(addition.attributes);
            existing.evidence.extend(addition.evidence);
            existing.confidence = existing.confidence.max(addition.confidence);
        } else {
            symbols.push(addition);
        }
    }
}

fn collect_all_files(root: &Path) -> io::Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    collect_files(root, root, &mut files)?;
    files.sort();
    Ok(files)
}

fn collect_files(root: &Path, path: &Path, files: &mut Vec<PathBuf>) -> io::Result<()> {
    if skip(root, path) {
        return Ok(());
    }

    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if recoverable_walk_error(&error) => return Ok(()),
        Err(error) => return Err(error),
    };
    if metadata.file_type().is_symlink() {
        return Ok(());
    }

    if metadata.is_file() {
        files.push(path.to_path_buf());
        return Ok(());
    }

    if metadata.is_dir() {
        let entries = match fs::read_dir(path) {
            Ok(entries) => entries,
            Err(error) if recoverable_walk_error(&error) => return Ok(()),
            Err(error) => return Err(error),
        };
        for entry in entries.flatten() {
            collect_files(root, &entry.path(), files)?;
        }
    }

    Ok(())
}

fn recoverable_walk_error(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::PermissionDenied | io::ErrorKind::NotFound
    )
}

fn skip(root: &Path, path: &Path) -> bool {
    ScanPolicy::default().is_excluded_from(root, path)
}

fn language_from_extension(path: &Path) -> String {
    match path.extension().and_then(|value| value.to_str()) {
        Some("java") => "Java",
        Some("ts") | Some("tsx") => "TypeScript",
        Some("js") | Some("jsx") => "JavaScript",
        Some("py") => "Python",
        Some("rs") => "Rust",
        Some("go") => "Go",
        Some("cs") => "C#",
        Some("fs") | Some("fsx") => "F#",
        Some("php") => "PHP",
        Some("kt") | Some("kts") => "Kotlin",
        Some("rb") => "Ruby",
        Some("dart") => "Dart",
        Some("c") | Some("h") => "C",
        Some("cc") | Some("cpp") | Some("cxx") | Some("hpp") => "C++",
        Some("swift") => "Swift",
        Some("scala") => "Scala",
        Some("sh") | Some("bash") | Some("zsh") | Some("ps1") => "Shell",
        Some("sql") => "SQL",
        Some("html") | Some("htm") => "HTML",
        Some("xml") => "XML",
        Some("css") => "CSS",
        Some("scss") => "SCSS",
        Some("graphql") | Some("gql") => "GraphQL",
        Some("vue") => "Vue",
        Some("svelte") => "Svelte",
        Some("astro") => "Astro",
        Some("razor") => "Razor",
        Some("ex") | Some("exs") => "Elixir",
        _ => "Unknown",
    }
    .to_string()
}

fn likely_text_file(path: &Path) -> bool {
    use std::io::Read;
    let Ok(mut file) = fs::File::open(path) else {
        return false;
    };
    let mut buf = [0u8; 8192];
    let Ok(n) = file.read(&mut buf) else {
        return false;
    };
    if n == 0 {
        return true;
    }
    let sample = &buf[..n];
    !sample.contains(&0) && std::str::from_utf8(sample).is_ok()
}

fn discover_manifest_directories(
    root: &Path,
    path: &Path,
    candidates: &mut Vec<PathBuf>,
) -> io::Result<()> {
    if skip(root, path) {
        return Ok(());
    }
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if recoverable_walk_error(&error) => return Ok(()),
        Err(error) => return Err(error),
    };
    if metadata.file_type().is_symlink() {
        return Ok(());
    }
    if metadata.is_dir() {
        let mut has_manifest = false;
        let entries = match fs::read_dir(path) {
            Ok(entries) => entries,
            Err(error) if recoverable_walk_error(&error) => return Ok(()),
            Err(error) => return Err(error),
        };
        for entry in entries.flatten() {
            let entry_path = entry.path();
            if entry_path.is_file() {
                let name = entry_path
                    .file_name()
                    .and_then(|value| value.to_str())
                    .unwrap_or("");
                if UNIT_MANIFESTS.contains(&name)
                    || name.ends_with(".sln")
                    || name.ends_with(".csproj")
                {
                    has_manifest = true;
                }
            } else {
                discover_manifest_directories(root, &entry_path, candidates)?;
            }
        }
        if has_manifest {
            candidates.push(path.to_path_buf());
        }
    }
    Ok(())
}

fn infrastructure_only(root: &Path) -> bool {
    [
        "docker-compose.yml",
        "docker-compose.yaml",
        "compose.yml",
        "compose.yaml",
        "main.tf",
    ]
    .iter()
    .any(|name| root.join(name).is_file())
}

fn is_application_root(root: &Path) -> bool {
    if root.join("artisan").is_file() && root.join("bootstrap/app.php").is_file() {
        return true;
    }
    if root.join("angular.json").is_file()
        || root.join("manage.py").is_file()
        || root.join("bin/rails").is_file()
        || root.join("bin/console").is_file()
    {
        return true;
    }
    if let Ok(entries) = fs::read_dir(root) {
        for path in entries.flatten().map(|entry| entry.path()) {
            if path.extension().and_then(|value| value.to_str()) == Some("csproj") && {
                let project = read_source_text(&path).unwrap_or_default();
                project.contains("Microsoft.NET.Sdk.Web")
                    || project.contains("Microsoft.NET.Sdk.BlazorWebAssembly")
                    || project.contains("Microsoft.AspNetCore.Components.Web")
            } {
                return true;
            }
        }
    }
    let package = read_source_text(root.join("package.json")).unwrap_or_default();
    if (root.join("src").is_dir()
        || root.join("app").is_dir()
        || root.join("pages").is_dir()
        || root.join("server").is_dir()
        || root.join("server.js").is_file()
        || root.join("server.ts").is_file())
        && [
            "@angular/core",
            "react",
            "next",
            "vue",
            "nuxt",
            "svelte",
            "@sveltejs/kit",
            "astro",
            "@nestjs/core",
            "express",
            "fastify",
            "hono",
        ]
        .iter()
        .any(|dependency| package.contains(dependency))
    {
        return true;
    }
    if root.join("go.mod").is_file()
        && (root.join("main.go").is_file() || root.join("cmd").is_dir())
    {
        return true;
    }
    let cargo = read_source_text(root.join("Cargo.toml")).unwrap_or_default();
    if cargo.contains("[package]") && root.join("src").is_dir() {
        return true;
    }
    let pom = read_source_text(root.join("pom.xml")).unwrap_or_default();
    let gradle = read_source_text(root.join("build.gradle")).unwrap_or_default()
        + &read_source_text(root.join("build.gradle.kts")).unwrap_or_default();
    let jvm_web = [
        "spring-boot",
        "io.quarkus",
        "io.micronaut",
        "micronaut-runtime",
        "io.ktor",
    ]
    .iter()
    .any(|marker| pom.contains(marker) || gradle.contains(marker));
    let mix = read_source_text(root.join("mix.exs")).unwrap_or_default();
    (jvm_web && root.join("src/main").is_dir())
        || (mix.contains(":phoenix") && root.join("lib").is_dir())
}

fn package_is_library(root: &Path) -> bool {
    let package = read_source_text(root.join("package.json")).unwrap_or_default();
    package.contains("\"main\"")
        || package.contains("\"exports\"")
        || package.contains("\"types\"")
        || (root.join("src/lib.rs").is_file() && !root.join("src/main.rs").is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestLanguageAnalyzer;
    struct TestFrameworkAdapter;

    impl LanguageAnalyzer for TestLanguageAnalyzer {
        fn id(&self) -> &'static str {
            "test-language"
        }

        fn supports(&self, path: &Path) -> bool {
            path.extension().and_then(|value| value.to_str()) == Some("txt")
        }

        fn analyze(&self, root: &Path) -> io::Result<AnalysisContribution> {
            let file = root.join("custom.txt");
            Ok(AnalysisContribution {
                components: vec![Component {
                    id: "custom:component".into(),
                    name: "custom".into(),
                    kind: ComponentKind::File,
                    language: "Custom".into(),
                    file: file.to_string_lossy().into_owned(),
                }],
                ..Default::default()
            })
        }
    }

    impl FrameworkAdapter for TestFrameworkAdapter {
        fn id(&self) -> &'static str {
            "test-framework"
        }

        fn detect(&self, root: &Path) -> reposlice_core::FrameworkDetection {
            reposlice_core::FrameworkDetection {
                name: "TestFramework".into(),
                confidence: 90,
                evidence: vec![Evidence {
                    kind: EvidenceKind::Configuration,
                    source: root.join("test.framework").to_string_lossy().into_owned(),
                    detail: "explicit test framework marker".into(),
                    confidence: 100,
                }],
            }
        }

        fn analyze(&self, root: &Path) -> io::Result<AnalysisContribution> {
            Ok(AnalysisContribution {
                technologies: vec![technology("framework", "TestFramework", 90)],
                components: vec![Component {
                    id: "test-framework:component".into(),
                    name: "test-framework-component".into(),
                    kind: ComponentKind::Controller,
                    language: "Custom".into(),
                    file: root.join("test.framework").to_string_lossy().into_owned(),
                }],
                ..Default::default()
            })
        }
    }

    #[test]
    fn registry_adds_external_language_analyzers_without_scanner_changes() {
        let root = std::env::temp_dir().join(format!("reposlice-registry-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("custom.txt"), "custom").unwrap();
        let mut registry = AnalyzerRegistry::new();
        registry.register_language(TestLanguageAnalyzer);
        let model = scan_project_with_registry(&root, &registry).unwrap();
        assert!(model
            .components
            .iter()
            .any(|component| component.id == "custom:component"));
        assert_eq!(registry.language_ids(), vec!["test-language"]);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn scans_repository_when_storage_parent_is_reposlice_directory() {
        let temp_root =
            std::env::temp_dir().join(format!("reposlice-storage-{}", std::process::id()));
        let storage = temp_root.join(".reposlice").join("repositories");
        let root = storage.join("project");
        let _ = fs::remove_dir_all(&temp_root);
        fs::create_dir_all(root.join("src")).unwrap();
        fs::create_dir_all(root.join("node_modules/pkg")).unwrap();
        fs::create_dir_all(root.join(".reposlice/cache")).unwrap();
        fs::write(root.join("src/main.rs"), "fn main() {}").unwrap();
        fs::write(root.join("node_modules/pkg/index.js"), "generated").unwrap();
        fs::write(root.join(".reposlice/cache/model.json"), "{}").unwrap();

        let files = collect_all_files(&root).unwrap();
        assert!(files.iter().any(|path| path.ends_with("src/main.rs")));
        assert!(!files
            .iter()
            .any(|path| path.to_string_lossy().contains("node_modules")));
        assert!(!files
            .iter()
            .any(|path| path.to_string_lossy().contains(".reposlice/cache")));

        let _ = fs::remove_dir_all(temp_root);
    }

    #[test]
    fn registry_adds_actionable_framework_adapters_without_scanner_changes() {
        let root = std::env::temp_dir().join(format!(
            "reposlice-framework-registry-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("test.framework"), "enabled=true").unwrap();
        let mut registry = AnalyzerRegistry::new();
        registry.register_framework(TestFrameworkAdapter);
        let model = scan_project_with_registry(&root, &registry).unwrap();
        assert!(model
            .technologies
            .iter()
            .any(|technology| technology.name == "TestFramework"));
        assert!(model
            .components
            .iter()
            .any(|component| component.id == "test-framework:component"));
        assert_eq!(registry.framework_ids(), vec!["test-framework"]);
        let _ = fs::remove_dir_all(root);
    }

    type FrameworkCase<'a> = (&'a str, &'a str, Vec<(&'a str, &'a str)>, Option<&'a str>);

    fn fixture(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/analysis")
            .join(name)
    }

    #[test]
    fn recognizes_universal_languages() {
        assert_eq!(language_from_extension(Path::new("main.swift")), "Swift");
        assert_eq!(language_from_extension(Path::new("view.vue")), "Vue");
        assert_eq!(language_from_extension(Path::new("page.astro")), "Astro");
        assert_eq!(language_from_extension(Path::new("app.ex")), "Elixir");
        assert_eq!(language_from_extension(Path::new("Index.razor")), "Razor");
        assert_eq!(language_from_extension(Path::new("query.sql")), "SQL");
    }

    #[test]
    fn frontend_routes_do_not_turn_react_into_a_backend() {
        clear_analysis_cache();
        let model = scan_project(&fixture("react-basic")).unwrap();
        assert_eq!(classify_project_role(&model), ProjectRole::Frontend);
        let parsed_files: HashSet<&str> = model
            .components
            .iter()
            .filter(|component| component.kind != ComponentKind::File)
            .map(|component| component.file.as_str())
            .collect();
        assert!(!model.components.iter().any(|component| {
            component.kind == ComponentKind::File && parsed_files.contains(component.file.as_str())
        }));
    }

    #[test]
    fn keeps_a_workspace_root_when_it_is_also_an_application() {
        let root = std::env::temp_dir().join(format!("reposlice-root-app-{}", std::process::id()));
        let child = root.join("packages/shared");
        fs::create_dir_all(root.join("src")).unwrap();
        fs::create_dir_all(&child).unwrap();
        fs::write(
            root.join("package.json"),
            r#"{"workspaces":["packages/*"],"dependencies":{"react":"latest"}}"#,
        )
        .unwrap();
        fs::write(
            root.join("src/App.tsx"),
            "export function App() { return null; }",
        )
        .unwrap();
        fs::write(child.join("package.json"), r#"{"name":"shared"}"#).unwrap();
        let units = discover_project_units(&root).unwrap();
        assert_eq!(units.len(), 2);
        assert!(units.contains(&fs::canonicalize(&root).unwrap()));
        assert!(units.contains(&fs::canonicalize(&child).unwrap()));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn discovers_specific_monorepo_units_without_root_duplicate() {
        let root = std::env::temp_dir().join(format!("reposlice-discovery-{}", std::process::id()));
        let web = root.join("apps/web");
        let api = root.join("apps/api");
        fs::create_dir_all(&web).unwrap();
        fs::create_dir_all(&api).unwrap();
        fs::write(root.join("package.json"), "{}").unwrap();
        fs::write(web.join("package.json"), "{}").unwrap();
        fs::write(web.join("app.ts"), "export const app = 1;").unwrap();
        fs::write(api.join("go.mod"), "module api").unwrap();
        fs::write(api.join("main.go"), "package main").unwrap();
        let units = discover_project_units(&root).unwrap();
        assert_eq!(
            units,
            vec![
                fs::canonicalize(api).unwrap(),
                fs::canonicalize(web).unwrap()
            ]
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn analyzes_laravel_routes_components_and_relations() {
        clear_analysis_cache();
        let model = scan_project(&fixture("laravel-basic")).unwrap();
        assert!(model
            .technologies
            .iter()
            .any(|item| item.name == "Laravel" && item.confidence >= 60));
        assert!(model
            .entrypoints
            .iter()
            .any(|item| item.name == "GET /users/{user}"));
        assert!(model
            .components
            .iter()
            .any(|item| item.kind == ComponentKind::Controller));
        assert!(model
            .dependencies
            .iter()
            .any(|item| item.kind == reposlice_core::DependencyKind::EloquentRelation));
        assert!(model
            .entrypoints
            .iter()
            .any(|item| item.kind.as_str() == "cli"));
        assert!(model
            .components
            .iter()
            .any(|item| item.kind == ComponentKind::Migration));
        assert!(model
            .analysis
            .entrypoints
            .iter()
            .any(|item| item.route_name.as_deref() == Some("users.show")));
    }

    #[test]
    fn generic_php_does_not_claim_laravel() {
        clear_analysis_cache();
        let model = scan_project(&fixture("php-generic")).unwrap();
        assert!(model.technologies.iter().any(|item| item.name == "PHP"));
        assert!(!model.technologies.iter().any(|item| item.name == "Laravel"));
        assert!(model
            .analysis
            .diagnostics
            .iter()
            .any(|item| item.code == "generic-php-fallback"));
    }

    #[test]
    fn expands_resource_routes_and_nested_groups() {
        clear_analysis_cache();
        let resources = scan_project(&fixture("laravel-resources")).unwrap();
        assert_eq!(
            resources
                .entrypoints
                .iter()
                .filter(|item| item.name.starts_with("GET /users"))
                .count(),
            2
        );
        assert!(!resources
            .entrypoints
            .iter()
            .any(|item| item.name == "DELETE /posts/comments/{comment}"));
        let groups = scan_project(&fixture("laravel-groups")).unwrap();
        assert!(groups
            .entrypoints
            .iter()
            .any(|item| item.name == "POST /admin/reports"));
        assert!(groups
            .analysis
            .entrypoints
            .iter()
            .any(|item| item.middleware == vec!["auth", "verified"]));
    }

    #[test]
    fn ignores_vendor_and_returns_deterministic_cached_models() {
        clear_analysis_cache();
        let first = scan_project(&fixture("excluded-heavy")).unwrap();
        let second = scan_project(&fixture("excluded-heavy")).unwrap();
        assert_eq!(first, second);
        assert!(first.components.iter().any(|item| item.name == "Visible"));
        assert!(!first.components.iter().any(|item| item.name == "Ignored"));
    }

    #[test]
    fn clean_git_head_is_none_for_non_git_directories() {
        let root = std::env::temp_dir().join(format!("reposlice-non-git-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        assert_eq!(clean_git_head(&root), None);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn ignores_local_generated_and_tool_cache_directories() {
        for directory in [
            ".angular",
            ".svelte-kit",
            ".astro",
            ".turbo",
            ".nx",
            ".output",
            ".vercel",
            ".serverless",
            ".wrangler",
            ".dart_tool",
            ".terraform",
            ".pytest_cache",
            ".mypy_cache",
            ".ruff_cache",
            ".tox",
            "DerivedData",
        ] {
            let root = Path::new("project");
            assert!(skip(root, &root.join(directory).join("generated.ts")));
        }
    }

    #[test]
    fn discovers_mixed_monorepo_units_without_parent_duplication() {
        let units = discover_project_units(&fixture("monorepo-mixed")).unwrap();
        assert_eq!(units.len(), 2);
        assert!(units.iter().any(|path| path.ends_with("apps/api")));
        assert!(units.iter().any(|path| path.ends_with("apps/web")));
    }

    #[test]
    fn malformed_and_oversized_sources_degrade_safely() {
        let malformed = scan_project(&fixture("malformed-project")).unwrap();
        assert!(malformed
            .components
            .iter()
            .any(|item| item.language == "PHP"));
        let root = std::env::temp_dir().join(format!("reposlice-large-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("Huge.php"),
            vec![b'a'; DEFAULT_MAX_SOURCE_SIZE as usize + 1],
        )
        .unwrap();
        let model = scan_project(&root).unwrap();
        assert!(model
            .analysis
            .diagnostics
            .iter()
            .any(|item| item.code == "source-too-large"));
        assert!(!model.components.iter().any(|item| item.name == "Huge"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn analyzes_supported_web_framework_matrix() {
        for (fixture_name, framework, entrypoint) in [
            ("angular-basic", "Angular", "ROUTE /users"),
            ("react-basic", "React", "ROUTE /users"),
            ("nest-basic", "NestJS", "GET /users/:id"),
            ("express-basic", "Express", "GET /health"),
            ("django-basic", "Django", "ANY /api/users"),
            ("fastapi-basic", "FastAPI", "GET /health"),
            ("aspnet-basic", "ASP.NET", "GET /health"),
            ("symfony-basic", "Symfony", "GET /users/{id}"),
            ("rails-basic", "Rails", "GET /users"),
            ("go-web-basic", "net/http", "ANY /health"),
            ("rust-web-basic", "Axum", "GET /health"),
        ] {
            clear_analysis_cache();
            let model = scan_project(&fixture(fixture_name)).unwrap();
            assert!(
                model.technologies.iter().any(|item| item.name == framework),
                "missing {framework}"
            );
            assert!(
                model.entrypoints.iter().any(|item| item.name == entrypoint),
                "missing {entrypoint} in {fixture_name}: {:?}",
                model.entrypoints
            );
            assert!(model
                .analysis
                .framework_detections
                .iter()
                .any(|item| item.name == framework && item.confidence >= 60));
        }
    }

    #[test]
    fn analyzes_spring_annotations_on_compact_java_lines() {
        clear_analysis_cache();
        let model = scan_project(&fixture("spring-basic")).unwrap();
        assert_eq!(model.entrypoints.len(), 1);
        assert_eq!(model.entrypoints[0].name, "GET /api/users/{id}");
    }

    #[test]
    fn analyzes_extended_web_framework_matrix() {
        let cases: Vec<FrameworkCase<'_>> = vec![
            ("next", "Next.js", vec![("package.json", r#"{"dependencies":{"next":"15"}}"#), ("next.config.js", "module.exports = {}"), ("app/users/[id]/page.tsx", "export default function Page() { return null }")], Some("ROUTE /users/{id}")),
            ("vue", "Vue", vec![("package.json", r#"{"dependencies":{"vue":"3"}}"#), ("src/App.vue", "<template><main /></template>"), ("src/router.ts", "createRouter({ routes: [{ path: '/users' }] })")], Some("ROUTE /users")),
            ("nuxt", "Nuxt", vec![("package.json", r#"{"dependencies":{"nuxt":"4"}}"#), ("nuxt.config.ts", "export default defineNuxtConfig({})"), ("pages/users/[id].vue", "<template />")], Some("ROUTE /users/{id}")),
            ("svelte", "Svelte", vec![("package.json", r#"{"dependencies":{"svelte":"5"}}"#), ("svelte.config.js", "export default {}"), ("src/App.svelte", "<svelte:head><title>App</title></svelte:head>")], None),
            ("sveltekit", "SvelteKit", vec![("package.json", r#"{"dependencies":{"@sveltejs/kit":"2","svelte":"5"}}"#), ("svelte.config.js", "export default {}"), ("src/routes/users/[id]/+page.svelte", "<h1>User</h1>")], Some("ROUTE /users/{id}")),
            ("astro", "Astro", vec![("package.json", r#"{"dependencies":{"astro":"5"}}"#), ("astro.config.mjs", "export default {}"), ("src/pages/users/[id].astro", "---\nconst user = Astro.params.id\n---")], Some("ROUTE /users/{id}")),
            ("fastify", "Fastify", vec![("package.json", r#"{"dependencies":{"fastify":"5"}}"#), ("src/app.ts", "import Fastify from 'fastify'; const app = Fastify(); app.get('/health', () => {})")], Some("GET /health")),
            ("hono", "Hono", vec![("package.json", r#"{"dependencies":{"hono":"4"}}"#), ("src/app.ts", "import { Hono } from 'hono'; const app = new Hono().basePath('/api'); app.get('/health', c => c.text('ok'))")], Some("GET /api/health")),
            ("flask", "Flask", vec![("requirements.txt", "flask==3.1"), ("app.py", "from flask import Flask\napp = Flask(__name__)\n@app.get('/health')\ndef health(): pass")], Some("GET /health")),
            ("quarkus", "Quarkus", vec![("pom.xml", "<dependency><groupId>io.quarkus</groupId></dependency>"), ("src/main/java/Users.java", "import jakarta.ws.rs.*; @Path(\"/api\") class Users { @GET @Path(\"/users\") void users() {} }")], Some("GET /api/users")),
            ("micronaut", "Micronaut", vec![("build.gradle", "implementation(\"io.micronaut:micronaut-runtime\")"), ("src/main/java/Users.java", "import io.micronaut.http.annotation.*; @Controller(\"/api\") class Users { @Get(\"/users\") void users() {} }")], Some("GET /api/users")),
            ("ktor", "Ktor", vec![("build.gradle.kts", "implementation(\"io.ktor:ktor-server-core\")"), ("src/main/kotlin/App.kt", "import io.ktor.server.routing.*\nfun routes() { routing { route(\"/api\") { get(\"/health\") {} } } }")], Some("GET /api/health")),
            ("blazor", "Blazor", vec![("App.csproj", "<Project Sdk=\"Microsoft.NET.Sdk.BlazorWebAssembly\"></Project>"), ("Pages/Users.razor", "@page \"/users/{id}\"\n<h1>User</h1>")], Some("ROUTE /users/{id}")),
            ("phoenix", "Phoenix", vec![("mix.exs", "defp deps, do: [{:phoenix, \"~> 1.7\"}]"), ("lib/demo_web/router.ex", "defmodule DemoWeb.Router do\n  use DemoWeb, :router\n  scope \"/api\" do\n    get \"/health\", HealthController, :show\n  end\nend")], Some("GET /api/health")),
        ];

        let root = std::env::temp_dir().join(format!("reposlice-extended-{}", std::process::id()));
        for (name, framework, files, entrypoint) in cases {
            let project = root.join(name);
            for (relative, content) in files {
                let path = project.join(relative);
                fs::create_dir_all(path.parent().unwrap()).unwrap();
                fs::write(path, content).unwrap();
            }
            clear_analysis_cache();
            let model = scan_project(&project).unwrap();
            assert!(
                model.technologies.iter().any(|item| item.name == framework),
                "missing {framework}: {:?}",
                model.technologies
            );
            if let Some(entrypoint) = entrypoint {
                assert!(
                    model.entrypoints.iter().any(|item| item.name == entrypoint),
                    "missing {entrypoint} for {framework}: {:?}",
                    model.entrypoints
                );
            }
        }
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn manifest_name_alone_does_not_activate_node_framework() {
        let root = std::env::temp_dir().join(format!("reposlice-evidence-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("package.json"),
            r#"{"dependencies":{"react":"19"}}"#,
        )
        .unwrap();
        let model = scan_project(&root).unwrap();
        assert!(!model.technologies.iter().any(|item| item.name == "React"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn separates_sql_from_database_evidence_and_keeps_technology_scope() {
        let root =
            std::env::temp_dir().join(format!("reposlice-tech-evidence-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("schema.sql"), "CREATE TABLE users (id integer);").unwrap();
        fs::write(
            root.join("application.properties"),
            "spring.datasource.url=jdbc:postgresql://localhost/app",
        )
        .unwrap();
        let model = scan_project(&root).unwrap();
        let sql = model
            .technologies
            .iter()
            .find(|item| item.name == "SQL")
            .unwrap();
        let postgres = model
            .technologies
            .iter()
            .find(|item| item.name == "PostgreSQL")
            .unwrap();
        assert_eq!(sql.classification, "Query Language");
        assert_eq!(postgres.classification, "Database");
        assert!(!sql.evidence.is_empty());
        assert!(!postgres.evidence.is_empty());
        assert_ne!(sql.detection_kind, "TRANSITIVE");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn detects_database_families_from_explicit_connection_evidence() {
        let root = std::env::temp_dir().join(format!("reposlice-db-matrix-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("application.properties"), "jdbc:postgresql://x\njdbc:mysql://x\njdbc:mariadb://x\njdbc:sqlserver://x\njdbc:oracle:thin\njdbc:sqlite:x\nmongodb://x\nredis://x").unwrap();
        let model = scan_project(&root).unwrap();
        for name in [
            "PostgreSQL",
            "MySQL",
            "MariaDB",
            "SQL Server",
            "Oracle",
            "SQLite",
            "MongoDB",
            "Redis",
        ] {
            assert!(
                model.technologies.iter().any(|item| item.name == name),
                "missing {name}"
            );
        }
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn framework_quality_corpus_meets_thresholds() {
        let corpus = include_str!("../../../quality/framework-ground-truth.tsv");
        let mut true_positive = 0usize;
        let mut false_positive = 0usize;
        let mut false_negative = 0usize;
        let mut cases = 0usize;

        for line in corpus
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
        {
            let (fixture_name, expected_raw) = line.split_once('\t').unwrap();
            clear_analysis_cache();
            let model = scan_project(&fixture(fixture_name)).unwrap();
            let detected = model
                .analysis
                .framework_detections
                .iter()
                .filter(|item| item.actionable())
                .map(|item| item.name.as_str())
                .collect::<std::collections::BTreeSet<_>>();
            let expected = expected_raw
                .split(',')
                .map(str::trim)
                .filter(|item| !item.is_empty() && *item != "-")
                .collect::<std::collections::BTreeSet<_>>();

            true_positive += detected.intersection(&expected).count();
            false_positive += detected.difference(&expected).count();
            false_negative += expected.difference(&detected).count();
            cases += 1;
        }

        let precision = if true_positive + false_positive == 0 {
            1.0
        } else {
            true_positive as f64 / (true_positive + false_positive) as f64
        };
        let recall = if true_positive + false_negative == 0 {
            1.0
        } else {
            true_positive as f64 / (true_positive + false_negative) as f64
        };
        let f1 = if precision + recall == 0.0 {
            0.0
        } else {
            2.0 * precision * recall / (precision + recall)
        };
        println!(
            "framework-quality cases={cases} tp={true_positive} fp={false_positive} fn={false_negative} precision={precision:.4} recall={recall:.4} f1={f1:.4}"
        );
        assert!(
            cases >= 34,
            "framework quality corpus unexpectedly shrank to {cases} cases"
        );
        assert!(
            precision >= 0.90,
            "framework precision regressed to {precision:.4}"
        );
        assert!(recall >= 0.90, "framework recall regressed to {recall:.4}");
        assert!(f1 >= 0.90, "framework F1 regressed to {f1:.4}");
    }
}
