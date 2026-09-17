use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::UNIX_EPOCH;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompatibilityLevel {
    Filesystem,
    Syntax,
    Semantic,
    Framework,
    Runtime,
    Capsule,
}

impl CompatibilityLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Filesystem => "L0",
            Self::Syntax => "L1",
            Self::Semantic => "L2",
            Self::Framework => "L3",
            Self::Runtime => "L4",
            Self::Capsule => "L5",
        }
    }
}


pub const DEFAULT_MAX_SOURCE_SIZE: u64 = 2 * 1024 * 1024;
pub const DEFAULT_MAX_INDEXED_TEXT_BYTES: usize = 64 * 1024 * 1024;
pub const DEFAULT_MAX_FALLBACK_FILES: usize = 50_000;

const DEFAULT_EXCLUDED_DIRECTORIES: &[&str] = &[
    ".git", ".reposlice", "node_modules", "target", "dist", "build", "out", "bin", "obj",
    "vendor", ".venv", "venv", ".gradle", "coverage", ".idea", ".vscode", ".next", ".nuxt",
    ".angular", ".svelte-kit", ".astro", ".turbo", ".nx", ".output", ".vercel", ".serverless",
    ".wrangler", ".dart_tool", ".terraform", ".pytest_cache", ".mypy_cache", ".ruff_cache", ".tox",
    "__pycache__", ".cache", "_build", "deps", "DerivedData", "storage",
];

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScanPolicy {
    pub max_source_size: u64,
    pub max_indexed_text_bytes: usize,
    pub max_fallback_files: usize,
}

impl Default for ScanPolicy {
    fn default() -> Self {
        Self {
            max_source_size: DEFAULT_MAX_SOURCE_SIZE,
            max_indexed_text_bytes: DEFAULT_MAX_INDEXED_TEXT_BYTES,
            max_fallback_files: DEFAULT_MAX_FALLBACK_FILES,
        }
    }
}

impl ScanPolicy {
    pub fn is_excluded(&self, path: &std::path::Path) -> bool {
        let mut previous: Option<&str> = None;
        for component in path.components() {
            let Some(name) = component.as_os_str().to_str() else {
                previous = None;
                continue;
            };
            if DEFAULT_EXCLUDED_DIRECTORIES.contains(&name)
                || (previous == Some("bootstrap") && name == "cache")
            {
                return true;
            }
            previous = Some(name);
        }
        false
    }

    pub fn is_excluded_from(
        &self,
        root: &std::path::Path,
        path: &std::path::Path,
    ) -> bool {
        let relative = path.strip_prefix(root).unwrap_or(path);
        self.is_excluded(relative)
    }

    pub fn accepts_source(&self, path: &std::path::Path, size: u64) -> bool {
        !self.is_excluded(path) && size <= self.max_source_size
    }

    pub fn accepts_source_from(
        &self,
        root: &std::path::Path,
        path: &std::path::Path,
        size: u64,
    ) -> bool {
        !self.is_excluded_from(root, path) && size <= self.max_source_size
    }
}


#[derive(Clone)]
struct CachedSourceText {
    size: u64,
    modified_ns: u128,
    text: String,
}

#[derive(Default)]
struct SharedSourceTextCache {
    entries: HashMap<PathBuf, CachedSourceText>,
    bytes: usize,
}

static SOURCE_TEXT_CACHE: OnceLock<Mutex<SharedSourceTextCache>> = OnceLock::new();

pub fn read_source_text(path: impl AsRef<Path>) -> io::Result<String> {
    let path = path.as_ref();
    let metadata = fs::metadata(path)?;
    let modified_ns = metadata
        .modified()
        .ok()
        .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
        .map(|value| value.as_nanos())
        .unwrap_or(0);
    let key = path.to_path_buf();
    if let Ok(cache) = source_text_cache().lock() {
        if let Some(cached) = cache.entries.get(&key) {
            if cached.size == metadata.len() && cached.modified_ns == modified_ns {
                return Ok(cached.text.clone());
            }
        }
    }

    let text = String::from_utf8_lossy(&fs::read(path)?).into_owned();
    if metadata.len() <= DEFAULT_MAX_SOURCE_SIZE && text.len() <= DEFAULT_MAX_INDEXED_TEXT_BYTES {
        if let Ok(mut cache) = source_text_cache().lock() {
            if let Some(previous) = cache.entries.remove(&key) {
                cache.bytes = cache.bytes.saturating_sub(previous.text.len());
            }
            if cache.bytes.saturating_add(text.len()) > DEFAULT_MAX_INDEXED_TEXT_BYTES {
                cache.entries.clear();
                cache.bytes = 0;
            }
            cache.bytes = cache.bytes.saturating_add(text.len());
            cache.entries.insert(
                key,
                CachedSourceText {
                    size: metadata.len(),
                    modified_ns,
                    text: text.clone(),
                },
            );
        }
    }
    Ok(text)
}

pub fn clear_source_text_cache() {
    if let Ok(mut cache) = source_text_cache().lock() {
        cache.entries.clear();
        cache.bytes = 0;
    }
}

pub fn source_text_cache_bytes() -> usize {
    source_text_cache().lock().map(|cache| cache.bytes).unwrap_or(0)
}

fn source_text_cache() -> &'static Mutex<SharedSourceTextCache> {
    SOURCE_TEXT_CACHE.get_or_init(|| Mutex::new(SharedSourceTextCache::default()))
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Technology {
    pub category: String,
    pub name: String,
    pub classification: String,
    pub confidence: u8,
    pub evidence: Vec<Evidence>,
    pub scope: String,
    pub repository_id: Option<String>,
    pub project_unit_id: Option<String>,
    pub detected_from: Vec<String>,
    pub detection_kind: String,
}

impl Default for Technology {
    fn default() -> Self {
        Self {
            category: String::new(), name: String::new(), classification: String::new(),
            confidence: 0, evidence: Vec::new(), scope: "project-unit".into(),
            repository_id: None, project_unit_id: None, detected_from: Vec::new(),
            detection_kind: "DIRECT".into(),
        }
    }
}

pub fn technology_classification(category: &str, name: &str) -> (&'static str, &'static str) {
    let normalized = name.to_ascii_lowercase().replace(['.', '-', '_', ' '], "");
    match (category.to_ascii_lowercase().as_str(), normalized.as_str()) {
        ("language", "sql") | ("language", "graphql") => ("Language", "Query Language"),
        ("language", "html") | ("language", "xml") => ("Language", "Markup Language"),
        ("language", "css") | ("language", "scss") => ("Language", "Style Language"),
        ("language", _) => ("Language", "Programming Language"),
        ("framework", n) if ["angular", "vue", "nuxt", "svelte", "sveltekit", "nextjs"].contains(&n) => ("Framework", "Frontend Framework"),
        ("framework", _) => ("Framework", "Backend Framework"),
        ("library", n) if n == "react" => ("Library", "UI Library"),
        ("data", n) if ["hibernate", "prisma", "typeorm", "sequelize", "jpa"].contains(&n) => ("Data", "ORM"),
        ("data", _) => ("Data", "Database"),
        ("build", n) if ["npm", "pnpm", "yarn", "composer"].contains(&n) => ("Build", "Package Manager"),
        ("build", n) if n == "webpack" => ("Build", "Bundler"),
        ("build", _) => ("Build", "Build Tool"),
        ("runtime", _) => ("Runtime", "Runtime"),
        ("infrastructure", n) if n == "docker" => ("Infrastructure", "Containerization"),
        _ => ("Tooling", "Tool"),
    }
}

pub fn canonical_technology_name(name: &str) -> String {
    let normalized = name.to_ascii_lowercase().replace(['.', '-', '_', ' '], "");
    match normalized.as_str() {
        "postgres" | "postgresql" | "orgpostgresql" => "PostgreSQL",
        "spring" | "springboot" | "orgspringframeworkboot" => "Spring Boot",
        "node" | "nodejs" => "Node.js",
        "typescript" => "TypeScript",
        "javascript" => "JavaScript",
        "dotnet" | "aspnet" | "aspnetcore" => "ASP.NET Core",
        "reactjs" => "React",
        _ => name,
    }
    .into()
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EvidenceKind {
    Manifest,
    Configuration,
    Source,
    Convention,
    Inference,
}

impl EvidenceKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Manifest => "manifest",
            Self::Configuration => "configuration",
            Self::Source => "source",
            Self::Convention => "convention",
            Self::Inference => "inference",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Evidence {
    pub kind: EvidenceKind,
    pub source: String,
    pub detail: String,
    pub confidence: u8,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiagnosticLevel {
    Info,
    Warning,
    Error,
}

impl DiagnosticLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Warning => "warning",
            Self::Error => "error",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnalysisDiagnostic {
    pub level: DiagnosticLevel,
    pub code: String,
    pub message: String,
    pub file: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrameworkDetectionPolicy {
    pub minimum_confidence: u8,
    pub conditional_confidence: u8,
    pub strong_source_confidence: u8,
}

impl Default for FrameworkDetectionPolicy {
    fn default() -> Self {
        Self {
            minimum_confidence: 70,
            conditional_confidence: 60,
            strong_source_confidence: 90,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrameworkDetection {
    pub name: String,
    pub confidence: u8,
    pub evidence: Vec<Evidence>,
}

impl Default for FrameworkDetection {
    fn default() -> Self {
        Self { name: String::new(), confidence: 0, evidence: Vec::new() }
    }
}

impl FrameworkDetection {
    pub fn has_direct_evidence(&self) -> bool {
        self.evidence.iter().any(|evidence| {
            matches!(
                evidence.kind,
                EvidenceKind::Manifest | EvidenceKind::Configuration | EvidenceKind::Source
            )
        })
    }

    pub fn actionable(&self) -> bool {
        self.actionable_with(&FrameworkDetectionPolicy::default())
    }

    pub fn actionable_with(&self, policy: &FrameworkDetectionPolicy) -> bool {
        if !self.has_direct_evidence() {
            return false;
        }
        let strong_source = self.evidence.iter().any(|evidence| {
            evidence.kind == EvidenceKind::Source
                && evidence.confidence >= policy.strong_source_confidence
        });
        self.confidence >= policy.minimum_confidence
            || (self.confidence >= policy.conditional_confidence && strong_source)
    }

    pub fn strongest_evidence_confidence(&self) -> u8 {
        self.evidence
            .iter()
            .map(|evidence| evidence.confidence)
            .max()
            .unwrap_or(0)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeRequirement {
    pub name: String,
    pub executable: String,
    pub version_hint: Option<String>,
    pub required_by: String,
    pub confidence: u8,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SymbolMetadata {
    pub symbol_id: String,
    pub qualified_name: Option<String>,
    pub namespace: Option<String>,
    pub framework_kind: Option<String>,
    pub attributes: Vec<(String, String)>,
    pub evidence: Vec<Evidence>,
    pub confidence: u8,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntrypointMetadata {
    pub entrypoint_id: String,
    pub method: Option<String>,
    pub path: Option<String>,
    pub route_name: Option<String>,
    pub domain: Option<String>,
    pub middleware: Vec<String>,
    pub controller: Option<String>,
    pub action: Option<String>,
    pub evidence: Vec<Evidence>,
    pub confidence: u8,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DependencyMetadata {
    pub source_id: String,
    pub target_id: String,
    pub kind: String,
    pub evidence: Vec<Evidence>,
    pub confidence: u8,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnalysisMetadata {
    pub framework_detections: Vec<FrameworkDetection>,
    pub symbols: Vec<SymbolMetadata>,
    pub entrypoints: Vec<EntrypointMetadata>,
    pub dependencies: Vec<DependencyMetadata>,
    pub runtime_requirements: Vec<RuntimeRequirement>,
    pub diagnostics: Vec<AnalysisDiagnostic>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnalysisContribution {
    pub technologies: Vec<Technology>,
    pub components: Vec<Component>,
    pub entrypoints: Vec<Entrypoint>,
    pub dependencies: Vec<Dependency>,
    pub metadata: AnalysisMetadata,
}

pub trait LanguageAnalyzer: Send + Sync {
    fn id(&self) -> &'static str;
    fn supports(&self, path: &std::path::Path) -> bool;
    fn analyze(&self, root: &std::path::Path) -> std::io::Result<AnalysisContribution>;
}

pub trait FrameworkAdapter: Send + Sync {
    fn id(&self) -> &'static str;
    fn detect(&self, root: &std::path::Path) -> FrameworkDetection;
    fn analyze(&self, root: &std::path::Path) -> std::io::Result<AnalysisContribution>;
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrameworkAnalysis {
    pub detection: FrameworkDetection,
    pub contribution: AnalysisContribution,
}

pub trait FrameworkSuite: Send + Sync {
    fn id(&self) -> &'static str;
    fn analyze(
        &self,
        root: &std::path::Path,
        base: &[Component],
    ) -> std::io::Result<Vec<FrameworkAnalysis>>;
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ComponentKind {
    File,
    Class,
    Interface,
    Record,
    Enum,
    Function,
    Module,
    Service,
    Controller,
    Repository,
    Entity,
    Trait,
    Middleware,
    Command,
    Job,
    Event,
    Listener,
    Model,
    Migration,
    Provider,
    FrontendComponent,
    Directive,
    Pipe,
    Guard,
    View,
    Handler,
    Unknown,
}

impl ComponentKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::File => "file",
            Self::Class => "class",
            Self::Interface => "interface",
            Self::Record => "record",
            Self::Enum => "enum",
            Self::Function => "function",
            Self::Module => "module",
            Self::Service => "service",
            Self::Controller => "controller",
            Self::Repository => "repository",
            Self::Entity => "entity",
            Self::Trait => "trait",
            Self::Middleware => "middleware",
            Self::Command => "command",
            Self::Job => "job",
            Self::Event => "event",
            Self::Listener => "listener",
            Self::Model => "model",
            Self::Migration => "migration",
            Self::Provider => "provider",
            Self::FrontendComponent => "frontend-component",
            Self::Directive => "directive",
            Self::Pipe => "pipe",
            Self::Guard => "guard",
            Self::View => "view",
            Self::Handler => "handler",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Component {
    pub id: String,
    pub name: String,
    pub kind: ComponentKind,
    pub language: String,
    pub file: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EntrypointKind {
    HttpEndpoint,
    CliCommand,
    EventConsumer,
    ScheduledTask,
    PublicFunction,
    PublicMethod,
    GraphqlOperation,
    FrontendRoute,
    Test,
    Unknown,
}

impl EntrypointKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::HttpEndpoint => "http",
            Self::CliCommand => "cli",
            Self::EventConsumer => "event",
            Self::ScheduledTask => "scheduled",
            Self::PublicFunction => "function",
            Self::PublicMethod => "method",
            Self::GraphqlOperation => "graphql",
            Self::FrontendRoute => "route",
            Self::Test => "test",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entrypoint {
    pub id: String,
    pub name: String,
    pub kind: EntrypointKind,
    pub component_id: String,
    pub file: String,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum DependencyKind {
    Imports,
    References,
    Calls,
    Requires,
    Injects,
    Extends,
    Implements,
    UsesTrait,
    EloquentRelation,
    Dispatches,
    Listens,
}

impl DependencyKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Imports => "imports",
            Self::References => "references",
            Self::Calls => "calls",
            Self::Requires => "requires",
            Self::Injects => "injects",
            Self::Extends => "extends",
            Self::Implements => "implements",
            Self::UsesTrait => "uses-trait",
            Self::EloquentRelation => "eloquent-relation",
            Self::Dispatches => "dispatches",
            Self::Listens => "listens",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Dependency {
    pub source_id: String,
    pub target_id: String,
    pub kind: DependencyKind,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectModel {
    pub root: String,
    pub name: String,
    pub files: usize,
    pub compatibility: CompatibilityLevel,
    pub technologies: Vec<Technology>,
    pub components: Vec<Component>,
    pub entrypoints: Vec<Entrypoint>,
    pub dependencies: Vec<Dependency>,
    pub analysis: AnalysisMetadata,
}

impl ProjectModel {
    pub fn resolve_target_component_id<'a>(&'a self, target: &str) -> Option<&'a str> {
        if let Some(entrypoint) = self
            .entrypoints
            .iter()
            .find(|entrypoint| entrypoint.id == target)
        {
            return Some(&entrypoint.component_id);
        }
        let component_id = target.strip_prefix("component:").unwrap_or(target);
        self.components
            .iter()
            .find(|component| component.id == component_id)
            .map(|component| component.id.as_str())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProjectRole {
    Frontend,
    Backend,
    Fullstack,
    Library,
    Infrastructure,
    Unknown,
}

impl ProjectRole {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Frontend => "Frontend",
            Self::Backend => "Backend",
            Self::Fullstack => "Fullstack",
            Self::Library => "Library",
            Self::Infrastructure => "Infrastructure",
            Self::Unknown => "Unknown",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HttpCall {
    pub method: String,
    pub path: String,
    /// Absolute request origin when it is statically known. Relative calls keep this empty.
    pub origin: Option<String>,
    pub component_id: String,
    pub file: String,
    pub evidence: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitMetadata {
    pub is_git_repository: bool,
    pub branch: Option<String>,
    pub commit: Option<String>,
    pub remote: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectUnit {
    pub id: String,
    pub repository_id: String,
    pub root: String,
    pub name: String,
    pub role: ProjectRole,
    pub model: ProjectModel,
    pub http_calls: Vec<HttpCall>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Repository {
    pub id: String,
    pub name: String,
    pub root: String,
    pub git: Option<GitMetadata>,
    pub project_units: Vec<ProjectUnit>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CrossProjectDependencyKind {
    Http,
    WorkspacePackage,
}

impl CrossProjectDependencyKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Http => "Http",
            Self::WorkspacePackage => "WorkspacePackage",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CrossProjectDependency {
    pub source_repository_id: String,
    pub source_project_unit_id: String,
    pub source_component_id: String,
    pub target_repository_id: String,
    pub target_project_unit_id: String,
    pub target_entrypoint_id: Option<String>,
    pub target_component_id: Option<String>,
    pub kind: CrossProjectDependencyKind,
    pub evidence: String,
    pub confidence: u8,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceModel {
    pub id: String,
    pub name: String,
    pub repositories: Vec<Repository>,
    pub cross_project_dependencies: Vec<CrossProjectDependency>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScopedTarget {
    pub repository_id: String,
    pub project_unit_id: String,
    pub target_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapsuleManifest {
    pub name: String,
    pub version: String,
    pub source_root: String,
    pub git_commit: Option<String>,
    pub target: String,
    pub files: usize,
}

impl CapsuleManifest {
    pub fn to_toml(&self) -> String {
        let mut content = format!(
            "[capsule]\nname = \"{}\"\nversion = \"{}\"\n\n[source]\nroot = \"{}\"\n",
            escape(&self.name),
            escape(&self.version),
            escape(&self.source_root),
        );
        if let Some(commit) = &self.git_commit {
            content.push_str(&format!("commit = \"{}\"\n", escape(commit)));
        }
        content.push_str(&format!(
            "\n[target]\nvalue = \"{}\"\n\n[metrics]\nfiles = {}\n",
            escape(&self.target),
            self.files
        ));
        content
    }
}

fn escape(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

pub fn slugify(value: &str) -> String {
    let mut result = String::new();
    for character in value.chars() {
        if character.is_ascii_alphanumeric() {
            result.push(character.to_ascii_lowercase());
        } else if !result.ends_with('-') {
            result.push('-');
        }
    }
    result.trim_matches('-').to_string()
}

pub fn stable_hash(value: &str) -> String {
    let mut hash = 14695981039346656037u64;
    for byte in value.replace('\\', "/").bytes() {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(1099511628211);
    }
    format!("{hash:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_scan_policy_excludes_generated_and_dependency_trees() {
        let policy = ScanPolicy::default();
        for path in [
            "project/node_modules/pkg/index.js",
            "project/target/debug/app",
            "project/vendor/pkg/file.php",
            "project/bootstrap/cache/routes.php",
            "project/.next/server/app.js",
        ] {
            assert!(policy.is_excluded(std::path::Path::new(path)), "{path}");
        }
        assert!(!policy.is_excluded(std::path::Path::new("project/src/main.rs")));
        let cloned_root = std::path::Path::new("/home/user/.reposlice/repositories/project");
        assert!(!policy.is_excluded_from(
            cloned_root,
            &cloned_root.join("src/main.rs")
        ));
        assert!(policy.is_excluded_from(
            cloned_root,
            &cloned_root.join(".reposlice/cache/model.json")
        ));
        assert!(policy.is_excluded_from(
            cloned_root,
            &cloned_root.join("node_modules/pkg/index.js")
        ));
        assert!(policy.accepts_source_from(
            cloned_root,
            &cloned_root.join("src/main.rs"),
            DEFAULT_MAX_SOURCE_SIZE
        ));
        assert!(policy.accepts_source(
            std::path::Path::new("project/src/main.rs"),
            DEFAULT_MAX_SOURCE_SIZE
        ));
    }

    #[test]
    fn shared_source_text_cache_is_bounded_and_clearable() {
        let root = std::env::temp_dir().join(format!(
            "reposlice-core-cache-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let source = root.join("source.rs");
        std::fs::write(&source, "fn main() {}\n").unwrap();

        clear_source_text_cache();
        assert_eq!(read_source_text(&source).unwrap(), "fn main() {}\n");
        assert!(source_text_cache_bytes() > 0);
        assert_eq!(read_source_text(&source).unwrap(), "fn main() {}\n");
        clear_source_text_cache();
        assert_eq!(source_text_cache_bytes(), 0);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn classifies_common_technology_taxonomy() {
        assert_eq!(technology_classification("language", "SQL"), ("Language", "Query Language"));
        assert_eq!(technology_classification("framework", "Angular"), ("Framework", "Frontend Framework"));
        assert_eq!(technology_classification("library", "React"), ("Library", "UI Library"));
        assert_eq!(technology_classification("build", "Webpack"), ("Build", "Bundler"));
    }

    #[test]
    fn normalizes_technology_aliases() {
        assert_eq!(canonical_technology_name("org.postgresql"), "PostgreSQL");
        assert_eq!(canonical_technology_name("nodejs"), "Node.js");
        assert_eq!(canonical_technology_name("spring-boot"), "Spring Boot");
    }

    #[test]
    fn framework_detection_requires_direct_evidence_to_be_actionable() {
        let inferred = FrameworkDetection {
            name: "Example".into(),
            confidence: 95,
            evidence: vec![Evidence {
                kind: EvidenceKind::Inference,
                source: "project".into(),
                detail: "heuristic".into(),
                confidence: 95,
            }],
        };
        assert!(!inferred.actionable());

        let direct = FrameworkDetection {
            name: "Example".into(),
            confidence: 80,
            evidence: vec![Evidence {
                kind: EvidenceKind::Manifest,
                source: "package.json".into(),
                detail: "dependency".into(),
                confidence: 100,
            }],
        };
        assert!(direct.actionable());
        assert_eq!(direct.strongest_evidence_confidence(), 100);
    }
}

/// Incremental SHA-256 hasher used for capsule integrity without relying on external services.
/// It is intended for deterministic content integrity, not as a digital signature primitive.
#[derive(Clone, Debug)]
pub struct Sha256Hasher {
    state: [u32; 8],
    buffer: [u8; 64],
    buffer_len: usize,
    length_bytes: u64,
}

impl Default for Sha256Hasher {
    fn default() -> Self {
        Self::new()
    }
}

impl Sha256Hasher {
    pub fn new() -> Self {
        Self {
            state: [
                0x6a09e667,
                0xbb67ae85,
                0x3c6ef372,
                0xa54ff53a,
                0x510e527f,
                0x9b05688c,
                0x1f83d9ab,
                0x5be0cd19,
            ],
            buffer: [0; 64],
            buffer_len: 0,
            length_bytes: 0,
        }
    }

    pub fn update(&mut self, mut data: &[u8]) {
        self.length_bytes = self.length_bytes.wrapping_add(data.len() as u64);
        if self.buffer_len > 0 {
            let take = (64 - self.buffer_len).min(data.len());
            self.buffer[self.buffer_len..self.buffer_len + take].copy_from_slice(&data[..take]);
            self.buffer_len += take;
            data = &data[take..];
            if self.buffer_len == 64 {
                let block = self.buffer;
                self.process_block(&block);
                self.buffer_len = 0;
            }
        }
        while data.len() >= 64 {
            let mut block = [0u8; 64];
            block.copy_from_slice(&data[..64]);
            self.process_block(&block);
            data = &data[64..];
        }
        if !data.is_empty() {
            self.buffer[..data.len()].copy_from_slice(data);
            self.buffer_len = data.len();
        }
    }

    pub fn finalize_hex(mut self) -> String {
        let bit_length = self.length_bytes.wrapping_mul(8);
        self.buffer[self.buffer_len] = 0x80;
        self.buffer_len += 1;
        if self.buffer_len > 56 {
            self.buffer[self.buffer_len..].fill(0);
            let block = self.buffer;
            self.process_block(&block);
            self.buffer = [0; 64];
            self.buffer_len = 0;
        }
        self.buffer[self.buffer_len..56].fill(0);
        self.buffer[56..64].copy_from_slice(&bit_length.to_be_bytes());
        let block = self.buffer;
        self.process_block(&block);
        self.state
            .iter()
            .map(|word| format!("{word:08x}"))
            .collect::<String>()
    }

    fn process_block(&mut self, block: &[u8; 64]) {
        const K: [u32; 64] = [
            0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1,
            0x923f82a4, 0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3,
            0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786,
            0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
            0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147,
            0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13,
            0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
            0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
            0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a,
            0x5b9cca4f, 0x682e6ff3, 0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208,
            0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
        ];
        let mut w = [0u32; 64];
        for (index, chunk) in block.chunks_exact(4).take(16).enumerate() {
            w[index] = u32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        }
        for index in 16..64 {
            let s0 = w[index - 15].rotate_right(7)
                ^ w[index - 15].rotate_right(18)
                ^ (w[index - 15] >> 3);
            let s1 = w[index - 2].rotate_right(17)
                ^ w[index - 2].rotate_right(19)
                ^ (w[index - 2] >> 10);
            w[index] = w[index - 16]
                .wrapping_add(s0)
                .wrapping_add(w[index - 7])
                .wrapping_add(s1);
        }
        let mut a = self.state[0];
        let mut b = self.state[1];
        let mut c = self.state[2];
        let mut d = self.state[3];
        let mut e = self.state[4];
        let mut f = self.state[5];
        let mut g = self.state[6];
        let mut h = self.state[7];
        for index in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let choice = (e & f) ^ ((!e) & g);
            let temp1 = h
                .wrapping_add(s1)
                .wrapping_add(choice)
                .wrapping_add(K[index])
                .wrapping_add(w[index]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let majority = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(majority);
            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }
        self.state[0] = self.state[0].wrapping_add(a);
        self.state[1] = self.state[1].wrapping_add(b);
        self.state[2] = self.state[2].wrapping_add(c);
        self.state[3] = self.state[3].wrapping_add(d);
        self.state[4] = self.state[4].wrapping_add(e);
        self.state[5] = self.state[5].wrapping_add(f);
        self.state[6] = self.state[6].wrapping_add(g);
        self.state[7] = self.state[7].wrapping_add(h);
    }
}

pub fn sha256_hex(data: &[u8]) -> String {
    let mut hasher = Sha256Hasher::new();
    hasher.update(data);
    hasher.finalize_hex()
}

#[cfg(test)]
mod sha256_tests {
    use super::*;

    #[test]
    fn sha256_matches_known_vectors() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
