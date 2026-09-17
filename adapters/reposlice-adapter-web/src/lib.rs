use reposlice_core::{
    stable_hash, read_source_text, AnalysisContribution, AnalysisDiagnostic, Component, ComponentKind, FrameworkAnalysis, FrameworkSuite,
    DiagnosticLevel, Entrypoint, EntrypointKind, EntrypointMetadata, Evidence, EvidenceKind,
    FrameworkDetection, RuntimeRequirement, ScanPolicy, SymbolMetadata, Technology, DEFAULT_MAX_SOURCE_SIZE,
};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

pub type DetectedFramework = FrameworkAnalysis;

pub struct WebFrameworkSuite;

impl FrameworkSuite for WebFrameworkSuite {
    fn id(&self) -> &'static str { "web-frameworks" }

    fn analyze(&self, root: &Path, base: &[Component]) -> io::Result<Vec<FrameworkAnalysis>> {
        analyze_web_frameworks(root, base)
    }
}

#[derive(Clone, Debug)]
struct SourceFile {
    path: PathBuf,
    relative: String,
    content: String,
}

pub fn analyze_web_frameworks(
    root: &Path,
    base: &[Component],
) -> io::Result<Vec<DetectedFramework>> {
    let files = source_files(root)?;
    let package = json(root.join("package.json"));
    let composer = json(root.join("composer.json"));
    let python_manifest = read_many(root, &["requirements.txt", "pyproject.toml", "Pipfile"]);
    let gemfile = read_source_text(root.join("Gemfile")).unwrap_or_default();
    let cargo = read_source_text(root.join("Cargo.toml")).unwrap_or_default();
    let go_mod = read_source_text(root.join("go.mod")).unwrap_or_default();
    let jvm_manifest = read_many(root, &["pom.xml", "build.gradle", "build.gradle.kts"]);
    let mix = read_source_text(root.join("mix.exs")).unwrap_or_default();
    let mut result = Vec::new();

    let specifications = [
        detect_node(
            root,
            &files,
            &package,
            "Angular",
            "@angular/core",
            &["angular.json"],
            &["@Component", "bootstrapApplication"],
        ),
        detect_node(
            root,
            &files,
            &package,
            "React",
            "react",
            &[],
            &["react-dom", "from 'react'", "from \"react\""],
        ),
        detect_node(
            root,
            &files,
            &package,
            "Next.js",
            "next",
            &["next.config.js", "next.config.mjs", "next.config.ts"],
            &["next/", "nextConfig"],
        ),
        detect_node(
            root,
            &files,
            &package,
            "Vue",
            "vue",
            &["vue.config.js"],
            &["createApp(", "<template"],
        ),
        detect_node(
            root,
            &files,
            &package,
            "Nuxt",
            "nuxt",
            &["nuxt.config.ts", "nuxt.config.js"],
            &["defineNuxtConfig", "useNuxtApp("],
        ),
        detect_node(
            root,
            &files,
            &package,
            "Svelte",
            "svelte",
            &["svelte.config.js", "svelte.config.ts"],
            &["<svelte:", "from 'svelte'"],
        ),
        detect_node(
            root,
            &files,
            &package,
            "SvelteKit",
            "@sveltejs/kit",
            &["svelte.config.js", "svelte.config.ts"],
            &["@sveltejs/kit", "+page"],
        ),
        detect_node(
            root,
            &files,
            &package,
            "Astro",
            "astro",
            &["astro.config.mjs", "astro.config.ts"],
            &["Astro.", "astro:"],
        ),
        detect_node(
            root,
            &files,
            &package,
            "NestJS",
            "@nestjs/core",
            &[],
            &["@Controller", "NestFactory"],
        ),
        detect_node(
            root,
            &files,
            &package,
            "Express",
            "express",
            &[],
            &["express()", "Router()"],
        ),
        detect_node(
            root,
            &files,
            &package,
            "Fastify",
            "fastify",
            &[],
            &["fastify(", "Fastify(", ".route("],
        ),
        detect_node(
            root,
            &files,
            &package,
            "Hono",
            "hono",
            &[],
            &["new Hono", "from 'hono'", "from \"hono\""],
        ),
        detect_python(
            root,
            &files,
            &python_manifest,
            "Django",
            "django",
            &["manage.py", "settings.py"],
            &["django.urls", "urlpatterns"],
        ),
        detect_python(
            root,
            &files,
            &python_manifest,
            "FastAPI",
            "fastapi",
            &[],
            &["FastAPI(", "APIRouter("],
        ),
        detect_python(
            root,
            &files,
            &python_manifest,
            "Flask",
            "flask",
            &[],
            &["Flask(", "Blueprint("],
        ),
        detect_aspnet(root, &files),
        detect_blazor(root, &files),
        detect_composer(
            root,
            &files,
            &composer,
            "Symfony",
            "symfony/framework-bundle",
            &["bin/console", "config/bundles.php"],
            &["Symfony\\", "#[Route("],
        ),
        detect_rails(root, &files, &gemfile),
        detect_go_web(root, &files, &go_mod),
        detect_rust_web(root, &files, &cargo),
        detect_jvm(
            root,
            &files,
            &jvm_manifest,
            "Quarkus",
            &["io.quarkus"],
            &["jakarta.ws.rs", "javax.ws.rs", "@Path("],
        ),
        detect_jvm(
            root,
            &files,
            &jvm_manifest,
            "Micronaut",
            &["io.micronaut", "micronaut-runtime"],
            &["io.micronaut.http", "@Controller("],
        ),
        detect_jvm(
            root,
            &files,
            &jvm_manifest,
            "Ktor",
            &["io.ktor"],
            &["io.ktor.server", "embeddedServer(", "routing {"],
        ),
        detect_phoenix(root, &files, &mix),
    ];

    for detection in specifications
        .into_iter()
        .filter(|item| item.confidence >= 60)
    {
        let contribution = analyze_framework(root, &files, base, &detection)?;
        result.push(DetectedFramework {
            detection,
            contribution,
        });
    }
    result.sort_by(|left, right| left.detection.name.cmp(&right.detection.name));
    Ok(result)
}

fn analyze_framework(
    root: &Path,
    files: &[SourceFile],
    base: &[Component],
    detection: &FrameworkDetection,
) -> io::Result<AnalysisContribution> {
    let mut output = AnalysisContribution::default();
    let category = if detection.name == "React" { "library" } else { "framework" };
    let detection_kind = if detection.evidence.iter().all(|item| item.kind == EvidenceKind::Convention) {
        "INFERRED"
    } else {
        "DIRECT"
    };
    output.technologies.push(Technology {
        category: category.into(),
        name: detection.name.clone(),
        confidence: detection.confidence,
        classification: reposlice_core::technology_classification(category, &detection.name).1.into(),
        evidence: detection.evidence.clone(),
        detected_from: detection.evidence.iter().map(|item| item.kind.as_str().to_string()).collect(),
        detection_kind: detection_kind.into(),
        ..Technology::default()
    });
    output.metadata.framework_detections.push(detection.clone());
    match detection.name.as_str() {
        "Angular" => analyze_angular(files, base, &mut output),
        "React" => analyze_react(files, base, &mut output),
        "Next.js" => analyze_next(files, base, &mut output),
        "Vue" => analyze_vue(files, base, &mut output),
        "Nuxt" => analyze_nuxt(files, base, &mut output),
        "Svelte" => analyze_svelte(files, base, &mut output),
        "SvelteKit" => analyze_sveltekit(files, base, &mut output),
        "Astro" => analyze_astro(files, base, &mut output),
        "NestJS" => analyze_nest(files, base, &mut output),
        "Express" => analyze_express(files, base, &mut output),
        "Fastify" => analyze_fastify(files, base, &mut output),
        "Hono" => analyze_hono(files, base, &mut output),
        "Django" => analyze_django(files, base, &mut output),
        "FastAPI" => analyze_fastapi(files, base, &mut output),
        "Flask" => analyze_flask(files, base, &mut output),
        "ASP.NET" => analyze_aspnet(files, base, &mut output),
        "Blazor" => analyze_blazor(files, base, &mut output),
        "Symfony" => analyze_symfony(files, base, &mut output),
        "Rails" => analyze_rails(files, base, &mut output),
        "Gin" | "Echo" | "Fiber" | "Chi" | "net/http" => analyze_go(files, base, &mut output),
        "Axum" | "Actix Web" | "Rocket" | "Warp" => analyze_rust(files, base, &mut output),
        "Quarkus" => analyze_quarkus(files, base, &mut output),
        "Micronaut" => analyze_micronaut(files, base, &mut output),
        "Ktor" => analyze_ktor(files, base, &mut output),
        "Phoenix" => analyze_phoenix(files, base, &mut output),
        _ => {}
    }
    output
        .metadata
        .runtime_requirements
        .extend(runtime_for(root, &detection.name));
    if output.entrypoints.is_empty() {
        output.metadata.diagnostics.push(AnalysisDiagnostic {
            level: DiagnosticLevel::Info,
            code: "framework-no-static-entrypoints".into(),
            message: format!(
                "{} was detected, but no statically resolvable entrypoints were found",
                detection.name
            ),
            file: None,
        });
    }
    output
        .components
        .sort_by(|left, right| left.id.cmp(&right.id));
    output
        .components
        .dedup_by(|left, right| left.id == right.id);
    output
        .entrypoints
        .sort_by(|left, right| left.id.cmp(&right.id));
    output
        .entrypoints
        .dedup_by(|left, right| left.id == right.id);
    output
        .metadata
        .entrypoints
        .sort_by(|left, right| left.entrypoint_id.cmp(&right.entrypoint_id));
    output
        .metadata
        .entrypoints
        .dedup_by(|left, right| left.entrypoint_id == right.entrypoint_id);
    Ok(output)
}

fn analyze_angular(files: &[SourceFile], base: &[Component], output: &mut AnalysisContribution) {
    for file in files
        .iter()
        .filter(|file| matches!(extension(&file.path), Some("ts" | "tsx")))
    {
        let kind = if file.relative.ends_with(".component.ts") {
            Some(ComponentKind::FrontendComponent)
        } else if file.relative.ends_with(".service.ts") {
            Some(ComponentKind::Service)
        } else if file.relative.ends_with(".directive.ts") {
            Some(ComponentKind::Directive)
        } else if file.relative.ends_with(".pipe.ts") {
            Some(ComponentKind::Pipe)
        } else if file.relative.ends_with(".guard.ts") {
            Some(ComponentKind::Guard)
        } else if file.relative.ends_with(".module.ts") {
            Some(ComponentKind::Module)
        } else {
            None
        };
        if let Some(kind) = kind {
            classify_file(file, base, kind, "Angular convention", output);
        }
        if file.content.contains("Routes")
            || file.content.contains("RouterModule")
            || file.relative.contains("routing")
        {
            for line in file.content.lines().filter(|line| line.contains("path")) {
                if let Some(path) = value_after_key(line, "path") {
                    add_route(
                        "Angular",
                        "ROUTE",
                        &normalize_frontend_path(&path),
                        EntrypointKind::FrontendRoute,
                        file,
                        component_for_file(base, &file.path),
                        90,
                        output,
                    );
                }
            }
        }
    }
}

fn analyze_react(files: &[SourceFile], base: &[Component], output: &mut AnalysisContribution) {
    for file in files
        .iter()
        .filter(|file| matches!(extension(&file.path), Some("jsx" | "tsx" | "js")))
    {
        if matches!(extension(&file.path), Some("jsx" | "tsx")) {
            classify_file(
                file,
                base,
                ComponentKind::FrontendComponent,
                "JSX/TSX component",
                output,
            );
        }
        if file.content.contains("react-router")
            || file.content.contains("<Route")
            || file.content.contains("createBrowserRouter")
        {
            for line in file.content.lines().filter(|line| line.contains("path")) {
                let path = attribute_value(line, "path").or_else(|| value_after_key(line, "path"));
                if let Some(path) = path {
                    add_route(
                        "React",
                        "ROUTE",
                        &normalize_frontend_path(&path),
                        EntrypointKind::FrontendRoute,
                        file,
                        component_for_file(base, &file.path),
                        88,
                        output,
                    );
                }
            }
        }
    }
}

fn analyze_next(files: &[SourceFile], base: &[Component], output: &mut AnalysisContribution) {
    for file in files
        .iter()
        .filter(|file| matches!(extension(&file.path), Some("js" | "jsx" | "ts" | "tsx")))
    {
        let app_page = route_from_file(&file.relative, &["app/", "src/app/"], &["/page"]);
        let pages_route = route_from_file(&file.relative, &["pages/", "src/pages/"], &[]);
        let app_endpoint = route_from_file(&file.relative, &["app/", "src/app/"], &["/route"]);
        if app_page.is_some() || pages_route.is_some() {
            classify_file(
                file,
                base,
                ComponentKind::View,
                "Next.js file-system route",
                output,
            );
        }
        if let Some(path) = app_page.or(pages_route
            .clone()
            .filter(|_| !file.relative.contains("/api/")))
        {
            add_route(
                "Next.js",
                "ROUTE",
                &path,
                EntrypointKind::FrontendRoute,
                file,
                component_for_file(base, &file.path),
                98,
                output,
            );
        }
        if let Some(path) = app_endpoint.or(pages_route.filter(|_| file.relative.contains("/api/")))
        {
            let methods = exported_http_methods(&file.content);
            if methods.is_empty() {
                add_route(
                    "Next.js",
                    "ANY",
                    &path,
                    EntrypointKind::HttpEndpoint,
                    file,
                    component_for_file(base, &file.path),
                    88,
                    output,
                );
            } else {
                for method in methods {
                    add_route(
                        "Next.js",
                        method,
                        &path,
                        EntrypointKind::HttpEndpoint,
                        file,
                        component_for_file(base, &file.path),
                        98,
                        output,
                    );
                }
            }
        }
    }
}

fn analyze_vue(files: &[SourceFile], base: &[Component], output: &mut AnalysisContribution) {
    for file in files {
        if extension(&file.path) == Some("vue") {
            classify_file(
                file,
                base,
                ComponentKind::FrontendComponent,
                "Vue single-file component",
                output,
            );
        }
        if file.content.contains("createRouter") || file.content.contains("vue-router") {
            for line in file.content.lines().filter(|line| line.contains("path")) {
                if let Some(path) = value_after_key(line, "path") {
                    add_route(
                        "Vue",
                        "ROUTE",
                        &path,
                        EntrypointKind::FrontendRoute,
                        file,
                        component_for_file(base, &file.path),
                        92,
                        output,
                    );
                }
            }
        }
    }
}

fn analyze_nuxt(files: &[SourceFile], base: &[Component], output: &mut AnalysisContribution) {
    for file in files {
        if extension(&file.path) == Some("vue") {
            classify_file(
                file,
                base,
                ComponentKind::FrontendComponent,
                "Nuxt Vue component",
                output,
            );
        }
        if let Some(path) = route_from_file(&file.relative, &["pages/"], &[]) {
            add_route(
                "Nuxt",
                "ROUTE",
                &path,
                EntrypointKind::FrontendRoute,
                file,
                component_for_file(base, &file.path),
                98,
                output,
            );
        }
        if let Some(path) = route_from_file(&file.relative, &["server/api/", "server/routes/"], &[])
        {
            let method = method_from_filename(&file.relative).unwrap_or("ANY");
            let path = if file.relative.starts_with("server/api/") {
                join_path("/api", &path)
            } else {
                path
            };
            add_route(
                "Nuxt",
                method,
                &path,
                EntrypointKind::HttpEndpoint,
                file,
                component_for_file(base, &file.path),
                95,
                output,
            );
        }
    }
}

fn analyze_svelte(files: &[SourceFile], base: &[Component], output: &mut AnalysisContribution) {
    for file in files
        .iter()
        .filter(|file| extension(&file.path) == Some("svelte"))
    {
        classify_file(
            file,
            base,
            ComponentKind::FrontendComponent,
            "Svelte component",
            output,
        );
    }
}

fn analyze_sveltekit(files: &[SourceFile], base: &[Component], output: &mut AnalysisContribution) {
    for file in files {
        if extension(&file.path) == Some("svelte") {
            classify_file(
                file,
                base,
                ComponentKind::FrontendComponent,
                "SvelteKit component",
                output,
            );
        }
        if let Some(path) = route_from_file(
            &file.relative,
            &["src/routes/"],
            &["/+page", "/+page.svelte", "/+layout", "/+layout.svelte"],
        ) {
            if file.relative.contains("+page") {
                add_route(
                    "SvelteKit",
                    "ROUTE",
                    &path,
                    EntrypointKind::FrontendRoute,
                    file,
                    component_for_file(base, &file.path),
                    98,
                    output,
                );
            }
        }
        if file.relative.contains("+server.") {
            if let Some(path) = route_from_file(&file.relative, &["src/routes/"], &["/+server"]) {
                for method in exported_http_methods(&file.content) {
                    add_route(
                        "SvelteKit",
                        method,
                        &path,
                        EntrypointKind::HttpEndpoint,
                        file,
                        component_for_file(base, &file.path),
                        98,
                        output,
                    );
                }
            }
        }
    }
}

fn analyze_astro(files: &[SourceFile], base: &[Component], output: &mut AnalysisContribution) {
    for file in files {
        if extension(&file.path) == Some("astro") {
            classify_file(
                file,
                base,
                ComponentKind::FrontendComponent,
                "Astro component/page",
                output,
            );
        }
        let Some(path) = route_from_file(&file.relative, &["src/pages/"], &[]) else {
            continue;
        };
        if extension(&file.path) == Some("astro") {
            add_route(
                "Astro",
                "ROUTE",
                &path,
                EntrypointKind::FrontendRoute,
                file,
                component_for_file(base, &file.path),
                98,
                output,
            );
        } else {
            let methods = exported_http_methods(&file.content);
            for method in methods {
                add_route(
                    "Astro",
                    method,
                    &path,
                    EntrypointKind::HttpEndpoint,
                    file,
                    component_for_file(base, &file.path),
                    96,
                    output,
                );
            }
        }
    }
}

fn analyze_nest(files: &[SourceFile], base: &[Component], output: &mut AnalysisContribution) {
    for file in files
        .iter()
        .filter(|file| matches!(extension(&file.path), Some("ts")))
    {
        let kind = if file.content.contains("@Controller") {
            Some(ComponentKind::Controller)
        } else if file.content.contains("@Injectable") || file.relative.ends_with(".service.ts") {
            Some(ComponentKind::Service)
        } else if file.content.contains("@Module") {
            Some(ComponentKind::Module)
        } else if file.relative.ends_with(".guard.ts") {
            Some(ComponentKind::Guard)
        } else {
            None
        };
        if let Some(kind) = kind {
            classify_file(file, base, kind, "NestJS decorator", output);
        }
        let prefix = decorator_argument(&file.content, "Controller").unwrap_or_default();
        for (decorator, method) in [
            ("Get", "GET"),
            ("Post", "POST"),
            ("Put", "PUT"),
            ("Patch", "PATCH"),
            ("Delete", "DELETE"),
            ("Options", "OPTIONS"),
            ("Head", "HEAD"),
        ] {
            for path in decorator_arguments(&file.content, decorator) {
                add_route(
                    "NestJS",
                    method,
                    &join_path(&prefix, &path),
                    EntrypointKind::HttpEndpoint,
                    file,
                    component_for_file(base, &file.path),
                    98,
                    output,
                );
            }
        }
    }
}

fn analyze_express(files: &[SourceFile], base: &[Component], output: &mut AnalysisContribution) {
    for file in files
        .iter()
        .filter(|file| matches!(extension(&file.path), Some("js" | "jsx" | "ts" | "tsx")))
    {
        if !(file.content.contains("express") || file.content.contains("Router(")) {
            continue;
        }
        classify_file(
            file,
            base,
            ComponentKind::Module,
            "Express application/router",
            output,
        );
        for receiver in express_receivers(&file.content) {
            for (verb, method) in [
                ("get", "GET"),
                ("post", "POST"),
                ("put", "PUT"),
                ("patch", "PATCH"),
                ("delete", "DELETE"),
                ("options", "OPTIONS"),
                ("all", "ANY"),
            ] {
                for path in quoted_arguments_after(&file.content, &format!("{receiver}.{verb}(")) {
                    add_route(
                        "Express",
                        method,
                        &path,
                        EntrypointKind::HttpEndpoint,
                        file,
                        component_for_file(base, &file.path),
                        94,
                        output,
                    );
                }
            }
        }
    }
}

fn analyze_fastify(files: &[SourceFile], base: &[Component], output: &mut AnalysisContribution) {
    for file in files
        .iter()
        .filter(|file| matches!(extension(&file.path), Some("js" | "jsx" | "ts" | "tsx")))
    {
        if !(file.content.contains("fastify") || file.content.contains("Fastify")) {
            continue;
        }
        classify_file(
            file,
            base,
            ComponentKind::Handler,
            "Fastify plugin/handler",
            output,
        );
        for (verb, method) in [
            ("get", "GET"),
            ("post", "POST"),
            ("put", "PUT"),
            ("patch", "PATCH"),
            ("delete", "DELETE"),
            ("options", "OPTIONS"),
            ("head", "HEAD"),
            ("all", "ANY"),
        ] {
            for path in quoted_arguments_after(&file.content, &format!(".{verb}(")) {
                add_route(
                    "Fastify",
                    method,
                    &path,
                    EntrypointKind::HttpEndpoint,
                    file,
                    component_for_file(base, &file.path),
                    95,
                    output,
                );
            }
        }
        for expression in balanced_expressions(&file.content, ".route(") {
            let Some(path) = value_after_key(&expression, "url") else {
                continue;
            };
            let method = value_after_key(&expression, "method").unwrap_or_else(|| "ANY".into());
            add_route(
                "Fastify",
                &method.to_ascii_uppercase(),
                &path,
                EntrypointKind::HttpEndpoint,
                file,
                component_for_file(base, &file.path),
                96,
                output,
            );
        }
    }
}

fn analyze_hono(files: &[SourceFile], base: &[Component], output: &mut AnalysisContribution) {
    for file in files
        .iter()
        .filter(|file| matches!(extension(&file.path), Some("js" | "jsx" | "ts" | "tsx")))
    {
        if !(file.content.contains("Hono") || file.content.contains("hono/")) {
            continue;
        }
        classify_file(
            file,
            base,
            ComponentKind::Handler,
            "Hono application/router",
            output,
        );
        let prefix = quoted_arguments_after(&file.content, ".basePath(")
            .into_iter()
            .next()
            .unwrap_or_default();
        for (verb, method) in [
            ("get", "GET"),
            ("post", "POST"),
            ("put", "PUT"),
            ("patch", "PATCH"),
            ("delete", "DELETE"),
            ("options", "OPTIONS"),
            ("all", "ANY"),
        ] {
            for path in quoted_arguments_after(&file.content, &format!(".{verb}(")) {
                add_route(
                    "Hono",
                    method,
                    &join_path(&prefix, &path),
                    EntrypointKind::HttpEndpoint,
                    file,
                    component_for_file(base, &file.path),
                    96,
                    output,
                );
            }
        }
    }
}

fn analyze_django(files: &[SourceFile], base: &[Component], output: &mut AnalysisContribution) {
    let mut prefixes = BTreeMap::new();
    for file in files
        .iter()
        .filter(|file| file.relative.ends_with("urls.py"))
    {
        for line in file
            .content
            .lines()
            .filter(|line| line.contains("include("))
        {
            let values = all_quoted(line);
            if values.len() >= 2 {
                let target = values[1].replace('.', "/");
                if let Some(included) = files
                    .iter()
                    .find(|candidate| candidate.relative.ends_with(&format!("{target}.py")))
                {
                    prefixes.insert(included.relative.clone(), values[0].clone());
                }
            }
        }
    }
    for file in files
        .iter()
        .filter(|file| extension(&file.path) == Some("py"))
    {
        if file.relative.ends_with("views.py") {
            classify_file(
                file,
                base,
                ComponentKind::View,
                "Django views module",
                output,
            );
        }
        if file.relative.ends_with("urls.py") || file.content.contains("urlpatterns") {
            let prefix = prefixes.get(&file.relative).cloned().unwrap_or_default();
            for line in file
                .content
                .lines()
                .filter(|line| !line.contains("include("))
            {
                for call in ["path(", "re_path("] {
                    for path in quoted_arguments_after(line, call) {
                        add_route(
                            "Django",
                            "ANY",
                            &join_path(&prefix, &path),
                            EntrypointKind::HttpEndpoint,
                            file,
                            component_for_file(base, &file.path),
                            94,
                            output,
                        );
                    }
                }
            }
        }
    }
}

fn analyze_fastapi(files: &[SourceFile], base: &[Component], output: &mut AnalysisContribution) {
    for file in files
        .iter()
        .filter(|file| extension(&file.path) == Some("py"))
    {
        if !(file.content.contains("FastAPI")
            || file.content.contains("APIRouter")
            || file.content.contains("@app.")
            || file.content.contains("@router."))
        {
            continue;
        }
        classify_file(
            file,
            base,
            ComponentKind::Handler,
            "FastAPI router/handler",
            output,
        );
        for line in file
            .content
            .lines()
            .map(str::trim)
            .filter(|line| line.starts_with('@'))
        {
            for (decorator, method) in [
                (".get(", "GET"),
                (".post(", "POST"),
                (".put(", "PUT"),
                (".patch(", "PATCH"),
                (".delete(", "DELETE"),
                (".options(", "OPTIONS"),
            ] {
                for path in quoted_arguments_after(line, decorator) {
                    add_route(
                        "FastAPI",
                        method,
                        &path,
                        EntrypointKind::HttpEndpoint,
                        file,
                        component_for_file(base, &file.path),
                        98,
                        output,
                    );
                }
            }
        }
    }
}

fn analyze_flask(files: &[SourceFile], base: &[Component], output: &mut AnalysisContribution) {
    for file in files
        .iter()
        .filter(|file| extension(&file.path) == Some("py"))
    {
        if !(file.content.contains("Flask(")
            || file.content.contains("Blueprint(")
            || file.content.contains("@app.")
            || file.content.contains("@bp."))
        {
            continue;
        }
        classify_file(
            file,
            base,
            ComponentKind::Handler,
            "Flask view/blueprint",
            output,
        );
        let prefix = balanced_expressions(&file.content, "Blueprint(")
            .into_iter()
            .find_map(|value| value_after_key(&value, "url_prefix"))
            .unwrap_or_default();
        for line in file
            .content
            .lines()
            .map(str::trim)
            .filter(|line| line.starts_with('@'))
        {
            for (marker, method) in [
                (".get(", "GET"),
                (".post(", "POST"),
                (".put(", "PUT"),
                (".patch(", "PATCH"),
                (".delete(", "DELETE"),
            ] {
                for path in quoted_arguments_after(line, marker) {
                    add_route(
                        "Flask",
                        method,
                        &join_path(&prefix, &path),
                        EntrypointKind::HttpEndpoint,
                        file,
                        component_for_file(base, &file.path),
                        98,
                        output,
                    );
                }
            }
            for expression in balanced_expressions(line, ".route(") {
                let Some(path) = first_quoted(&expression) else {
                    continue;
                };
                let methods = list_after_key(&expression, "methods");
                if methods.is_empty() {
                    add_route(
                        "Flask",
                        "GET",
                        &join_path(&prefix, &path),
                        EntrypointKind::HttpEndpoint,
                        file,
                        component_for_file(base, &file.path),
                        94,
                        output,
                    );
                } else {
                    for method in methods {
                        add_route(
                            "Flask",
                            &method.to_ascii_uppercase(),
                            &join_path(&prefix, &path),
                            EntrypointKind::HttpEndpoint,
                            file,
                            component_for_file(base, &file.path),
                            98,
                            output,
                        );
                    }
                }
            }
        }
    }
}

fn analyze_aspnet(files: &[SourceFile], base: &[Component], output: &mut AnalysisContribution) {
    for file in files
        .iter()
        .filter(|file| extension(&file.path) == Some("cs"))
    {
        let is_controller =
            file.relative.contains("Controllers/") || file.content.contains("ControllerBase");
        if is_controller {
            classify_file(
                file,
                base,
                ComponentKind::Controller,
                "ASP.NET controller",
                output,
            );
        }
        let prefix = bracket_attribute(&file.content, "Route")
            .unwrap_or_default()
            .replace(
                "[controller]",
                file.path
                    .file_stem()
                    .and_then(|value| value.to_str())
                    .unwrap_or("")
                    .trim_end_matches("Controller"),
            );
        for (attribute, method) in [
            ("HttpGet", "GET"),
            ("HttpPost", "POST"),
            ("HttpPut", "PUT"),
            ("HttpPatch", "PATCH"),
            ("HttpDelete", "DELETE"),
        ] {
            for path in bracket_attributes(&file.content, attribute) {
                add_route(
                    "ASP.NET",
                    method,
                    &join_path(&prefix, &path),
                    EntrypointKind::HttpEndpoint,
                    file,
                    component_for_file(base, &file.path),
                    96,
                    output,
                );
            }
        }
        for (call, method) in [
            ("MapGet(", "GET"),
            ("MapPost(", "POST"),
            ("MapPut(", "PUT"),
            ("MapPatch(", "PATCH"),
            ("MapDelete(", "DELETE"),
        ] {
            for path in quoted_arguments_after(&file.content, call) {
                add_route(
                    "ASP.NET",
                    method,
                    &path,
                    EntrypointKind::HttpEndpoint,
                    file,
                    component_for_file(base, &file.path),
                    98,
                    output,
                );
            }
        }
    }
}

fn analyze_blazor(files: &[SourceFile], base: &[Component], output: &mut AnalysisContribution) {
    for file in files
        .iter()
        .filter(|file| extension(&file.path) == Some("razor"))
    {
        classify_file(
            file,
            base,
            ComponentKind::FrontendComponent,
            "Blazor Razor component",
            output,
        );
        for line in file
            .content
            .lines()
            .map(str::trim)
            .filter(|line| line.starts_with("@page"))
        {
            if let Some(path) = first_quoted(line) {
                add_route(
                    "Blazor",
                    "ROUTE",
                    &path,
                    EntrypointKind::FrontendRoute,
                    file,
                    component_for_file(base, &file.path),
                    99,
                    output,
                );
            }
        }
    }
}

fn analyze_symfony(files: &[SourceFile], base: &[Component], output: &mut AnalysisContribution) {
    for file in files
        .iter()
        .filter(|file| extension(&file.path) == Some("php"))
    {
        if file.relative.contains("Controller/") || file.content.contains("AbstractController") {
            classify_file(
                file,
                base,
                ComponentKind::Controller,
                "Symfony controller",
                output,
            );
        }
        for expression in attribute_expressions(&file.content, "Route") {
            let Some(path) = first_quoted(&expression) else {
                continue;
            };
            let methods = list_after_key(&expression, "methods");
            if methods.is_empty() {
                add_route(
                    "Symfony",
                    "ANY",
                    &path,
                    EntrypointKind::HttpEndpoint,
                    file,
                    component_for_file(base, &file.path),
                    96,
                    output,
                );
            } else {
                for method in methods {
                    add_route(
                        "Symfony",
                        &method.to_ascii_uppercase(),
                        &path,
                        EntrypointKind::HttpEndpoint,
                        file,
                        component_for_file(base, &file.path),
                        98,
                        output,
                    );
                }
            }
        }
    }
}

fn analyze_rails(files: &[SourceFile], base: &[Component], output: &mut AnalysisContribution) {
    for file in files
        .iter()
        .filter(|file| extension(&file.path) == Some("rb"))
    {
        if file.relative.contains("app/controllers/") {
            classify_file(
                file,
                base,
                ComponentKind::Controller,
                "Rails controller",
                output,
            );
        }
        if !file.relative.ends_with("config/routes.rb") {
            continue;
        }
        for raw in file.content.lines() {
            let line = raw.trim();
            for (verb, method) in [
                ("get ", "GET"),
                ("post ", "POST"),
                ("put ", "PUT"),
                ("patch ", "PATCH"),
                ("delete ", "DELETE"),
            ] {
                if let Some(rest) = line.strip_prefix(verb) {
                    if let Some(path) = first_quoted(rest) {
                        add_route(
                            "Rails",
                            method,
                            &path,
                            EntrypointKind::HttpEndpoint,
                            file,
                            component_for_file(base, &file.path),
                            92,
                            output,
                        );
                    }
                }
            }
            if let Some(resource) = line
                .strip_prefix("resources ")
                .and_then(first_symbol_or_quoted)
            {
                emit_rest_resource(
                    "Rails",
                    &resource,
                    file,
                    component_for_file(base, &file.path),
                    output,
                );
            }
        }
    }
}

fn analyze_go(files: &[SourceFile], base: &[Component], output: &mut AnalysisContribution) {
    for file in files
        .iter()
        .filter(|file| extension(&file.path) == Some("go"))
    {
        if !(file.content.contains("net/http")
            || file.content.contains("gin-gonic")
            || file.content.contains("labstack/echo")
            || file.content.contains("gofiber")
            || file.content.contains("go-chi"))
        {
            continue;
        }
        classify_file(
            file,
            base,
            ComponentKind::Handler,
            "Go HTTP handler",
            output,
        );
        for (call, method) in [
            ("HandleFunc(", "ANY"),
            (".GET(", "GET"),
            (".POST(", "POST"),
            (".PUT(", "PUT"),
            (".PATCH(", "PATCH"),
            (".DELETE(", "DELETE"),
            (".Get(", "GET"),
            (".Post(", "POST"),
            (".Put(", "PUT"),
            (".Delete(", "DELETE"),
        ] {
            for path in quoted_arguments_after(&file.content, call) {
                add_route(
                    "Go Web",
                    method,
                    &path,
                    EntrypointKind::HttpEndpoint,
                    file,
                    component_for_file(base, &file.path),
                    90,
                    output,
                );
            }
        }
    }
}

fn analyze_rust(files: &[SourceFile], base: &[Component], output: &mut AnalysisContribution) {
    for file in files
        .iter()
        .filter(|file| extension(&file.path) == Some("rs"))
    {
        if !(file.content.contains("axum")
            || file.content.contains("actix_web")
            || file.content.contains("rocket")
            || file.content.contains("warp"))
        {
            continue;
        }
        classify_file(
            file,
            base,
            ComponentKind::Handler,
            "Rust web handler",
            output,
        );
        for (attribute, method) in [
            ("#[get(", "GET"),
            ("#[post(", "POST"),
            ("#[put(", "PUT"),
            ("#[patch(", "PATCH"),
            ("#[delete(", "DELETE"),
        ] {
            for path in quoted_arguments_after(&file.content, attribute) {
                add_route(
                    "Rust Web",
                    method,
                    &path,
                    EntrypointKind::HttpEndpoint,
                    file,
                    component_for_file(base, &file.path),
                    96,
                    output,
                );
            }
        }
        for path in quoted_arguments_after(&file.content, ".route(") {
            let method = route_method_near(&file.content, &path).unwrap_or("ANY");
            add_route(
                "Rust Web",
                method,
                &path,
                EntrypointKind::HttpEndpoint,
                file,
                component_for_file(base, &file.path),
                90,
                output,
            );
        }
    }
}

fn analyze_quarkus(files: &[SourceFile], base: &[Component], output: &mut AnalysisContribution) {
    for file in files
        .iter()
        .filter(|file| matches!(extension(&file.path), Some("java" | "kt" | "kts")))
    {
        if file.content.contains("@ApplicationScoped") || file.content.contains("@Singleton") {
            classify_file(
                file,
                base,
                ComponentKind::Service,
                "Quarkus CDI bean",
                output,
            );
        }
        if !file.content.contains("@Path(") {
            continue;
        }
        classify_file(
            file,
            base,
            ComponentKind::Controller,
            "Quarkus JAX-RS resource",
            output,
        );
        analyze_jvm_annotations(
            "Quarkus",
            file,
            base,
            output,
            "Path",
            &[
                ("GET", "GET"),
                ("POST", "POST"),
                ("PUT", "PUT"),
                ("PATCH", "PATCH"),
                ("DELETE", "DELETE"),
            ],
        );
    }
}

fn analyze_micronaut(files: &[SourceFile], base: &[Component], output: &mut AnalysisContribution) {
    for file in files
        .iter()
        .filter(|file| matches!(extension(&file.path), Some("java" | "kt" | "kts")))
    {
        if file.content.contains("@Singleton") {
            classify_file(
                file,
                base,
                ComponentKind::Service,
                "Micronaut singleton",
                output,
            );
        }
        if !file.content.contains("@Controller") {
            continue;
        }
        classify_file(
            file,
            base,
            ComponentKind::Controller,
            "Micronaut controller",
            output,
        );
        analyze_jvm_annotations(
            "Micronaut",
            file,
            base,
            output,
            "Controller",
            &[
                ("Get", "GET"),
                ("Post", "POST"),
                ("Put", "PUT"),
                ("Patch", "PATCH"),
                ("Delete", "DELETE"),
                ("Options", "OPTIONS"),
            ],
        );
    }
}

fn analyze_ktor(files: &[SourceFile], base: &[Component], output: &mut AnalysisContribution) {
    for file in files
        .iter()
        .filter(|file| matches!(extension(&file.path), Some("kt" | "kts")))
    {
        if !(file.content.contains("io.ktor")
            || file.content.contains("routing {")
            || file.content.contains("Route."))
        {
            continue;
        }
        classify_file(
            file,
            base,
            ComponentKind::Handler,
            "Ktor routing module",
            output,
        );
        let mut scopes: Vec<(String, i32)> = Vec::new();
        let mut depth = 0i32;
        for raw in file.content.lines() {
            let line = raw.trim();
            while scopes
                .last()
                .is_some_and(|(_, scope_depth)| *scope_depth > depth)
            {
                scopes.pop();
            }
            if let Some(path) = call_first_argument(line, "route(") {
                scopes.push((path, depth + line.matches('{').count() as i32));
            }
            let prefix = scopes
                .iter()
                .fold(String::new(), |value, (part, _)| join_path(&value, part));
            for (call, method) in [
                ("get(", "GET"),
                ("post(", "POST"),
                ("put(", "PUT"),
                ("patch(", "PATCH"),
                ("delete(", "DELETE"),
                ("options(", "OPTIONS"),
                ("head(", "HEAD"),
            ] {
                if let Some(path) = call_first_argument(line, call) {
                    add_route(
                        "Ktor",
                        method,
                        &join_path(&prefix, &path),
                        EntrypointKind::HttpEndpoint,
                        file,
                        component_for_file(base, &file.path),
                        95,
                        output,
                    );
                }
            }
            depth += line.matches('{').count() as i32 - line.matches('}').count() as i32;
        }
    }
}

fn analyze_phoenix(files: &[SourceFile], base: &[Component], output: &mut AnalysisContribution) {
    for file in files
        .iter()
        .filter(|file| matches!(extension(&file.path), Some("ex" | "exs")))
    {
        if file.relative.contains("_web/controllers/") {
            classify_file(
                file,
                base,
                ComponentKind::Controller,
                "Phoenix controller",
                output,
            );
        }
        if file.relative.contains("_web/live/") {
            classify_file(file, base, ComponentKind::View, "Phoenix LiveView", output);
        }
        if !file.relative.ends_with("router.ex") {
            continue;
        }
        classify_file(file, base, ComponentKind::Module, "Phoenix router", output);
        let mut scopes: Vec<(String, i32)> = Vec::new();
        let mut depth = 0i32;
        for raw in file.content.lines() {
            let line = raw.trim();
            while scopes
                .last()
                .is_some_and(|(_, scope_depth)| *scope_depth > depth)
            {
                scopes.pop();
            }
            if line.starts_with("scope ") {
                if let Some(path) = first_quoted(line) {
                    scopes.push((path, depth + 1));
                }
            }
            let prefix = scopes
                .iter()
                .fold(String::new(), |value, (part, _)| join_path(&value, part));
            for (verb, method) in [
                ("get ", "GET"),
                ("post ", "POST"),
                ("put ", "PUT"),
                ("patch ", "PATCH"),
                ("delete ", "DELETE"),
                ("live ", "GET"),
            ] {
                if let Some(path) = line.strip_prefix(verb).and_then(first_quoted) {
                    add_route(
                        "Phoenix",
                        method,
                        &join_path(&prefix, &path),
                        EntrypointKind::HttpEndpoint,
                        file,
                        component_for_file(base, &file.path),
                        96,
                        output,
                    );
                }
            }
            if let Some(resource) = line.strip_prefix("resources ").and_then(first_quoted) {
                let full = join_path(&prefix, &resource);
                emit_rest_resource(
                    "Phoenix",
                    full.trim_start_matches('/'),
                    file,
                    component_for_file(base, &file.path),
                    output,
                );
            }
            depth += if line.ends_with(" do") { 1 } else { 0 };
            if line == "end" {
                depth -= 1;
            }
        }
    }
}

fn analyze_jvm_annotations(
    framework: &str,
    file: &SourceFile,
    base: &[Component],
    output: &mut AnalysisContribution,
    prefix_annotation: &str,
    verbs: &[(&str, &str)],
) {
    let prefix = annotation_arguments(&file.content, prefix_annotation)
        .into_iter()
        .next()
        .unwrap_or_default();
    for (annotation, method) in verbs {
        for occurrence in annotation_occurrences(&file.content, annotation) {
            add_route(
                framework,
                method,
                &join_path(&prefix, &occurrence),
                EntrypointKind::HttpEndpoint,
                file,
                component_for_file(base, &file.path),
                96,
                output,
            );
        }
    }
}

fn classify_file(
    file: &SourceFile,
    base: &[Component],
    kind: ComponentKind,
    detail: &str,
    output: &mut AnalysisContribution,
) {
    let candidates: Vec<&Component> = base
        .iter()
        .filter(|item| same_path(&item.file, &file.path))
        .collect();
    let selected: Vec<&Component> = {
        let declarations: Vec<&Component> = candidates
            .iter()
            .copied()
            .filter(|item| item.kind != ComponentKind::Module && item.kind != ComponentKind::File)
            .collect();
        if declarations.is_empty() {
            candidates.into_iter().take(1).collect()
        } else {
            declarations
        }
    };
    for component in selected {
        let mut classified = component.clone();
        classified.kind = kind.clone();
        output.components.push(classified.clone());
        output.metadata.symbols.push(SymbolMetadata {
            symbol_id: classified.id,
            qualified_name: None,
            namespace: None,
            framework_kind: Some(kind.as_str().into()),
            attributes: Vec::new(),
            evidence: vec![evidence(EvidenceKind::Convention, &file.path, detail, 90)],
            confidence: 90,
        });
    }
}

#[allow(clippy::too_many_arguments)]
fn add_route(
    framework: &str,
    method: &str,
    path: &str,
    kind: EntrypointKind,
    file: &SourceFile,
    component_id: Option<String>,
    confidence: u8,
    output: &mut AnalysisContribution,
) {
    let normalized = if kind == EntrypointKind::FrontendRoute {
        normalize_frontend_path(path)
    } else {
        normalize_http_path(path)
    };
    let component_id = component_id.unwrap_or_else(|| {
        format!(
            "{}:module:{}",
            framework.to_ascii_lowercase().replace(['.', ' '], "-"),
            stable_hash(&file.relative)
        )
    });
    let seed = format!("{framework}\0{}\0{method}\0{normalized}", file.relative);
    let id = format!(
        "{}:entrypoint:{}",
        framework.to_ascii_lowercase().replace(['.', ' '], "-"),
        stable_hash(&seed)
    );
    let name = format!("{method} {normalized}");
    output.entrypoints.push(Entrypoint {
        id: id.clone(),
        name,
        kind,
        component_id,
        file: file.path.to_string_lossy().into_owned(),
    });
    output.metadata.entrypoints.push(EntrypointMetadata {
        entrypoint_id: id,
        method: (method != "ROUTE").then(|| method.into()),
        path: Some(normalized),
        route_name: None,
        domain: None,
        middleware: Vec::new(),
        controller: None,
        action: None,
        evidence: vec![evidence(
            EvidenceKind::Source,
            &file.path,
            &format!("{framework} route declaration"),
            confidence,
        )],
        confidence,
    });
}

fn component_for_file(base: &[Component], path: &Path) -> Option<String> {
    base.iter()
        .find(|item| {
            same_path(&item.file, path)
                && item.kind != ComponentKind::Module
                && item.kind != ComponentKind::File
        })
        .or_else(|| base.iter().find(|item| same_path(&item.file, path)))
        .map(|item| item.id.clone())
}

fn emit_rest_resource(
    framework: &str,
    resource: &str,
    file: &SourceFile,
    component: Option<String>,
    output: &mut AnalysisContribution,
) {
    let resource = resource.trim_start_matches(':').replace('_', "-");
    let singular = resource.trim_end_matches('s');
    for (method, suffix) in [
        ("GET", ""),
        ("POST", ""),
        ("GET", "/new"),
        ("GET", &format!("/{{{singular}}}")),
        ("PATCH", &format!("/{{{singular}}}")),
        ("DELETE", &format!("/{{{singular}}}")),
        ("GET", &format!("/{{{singular}}}/edit")),
    ] {
        add_route(
            framework,
            method,
            &format!("/{resource}{suffix}"),
            EntrypointKind::HttpEndpoint,
            file,
            component.clone(),
            85,
            output,
        );
    }
}

fn detect_node(
    root: &Path,
    files: &[SourceFile],
    package: &Option<Value>,
    name: &str,
    dependency: &str,
    markers: &[&str],
    source_markers: &[&str],
) -> FrameworkDetection {
    let mut score = 0u8;
    let mut items = Vec::new();
    if has_package(package, dependency) {
        score += 55;
        items.push(evidence(
            EvidenceKind::Manifest,
            &root.join("package.json"),
            &format!("package dependency {dependency}"),
            100,
        ));
    }
    for marker in markers {
        if root.join(marker).exists() {
            score = score.saturating_add(20);
            items.push(evidence(
                EvidenceKind::Configuration,
                &root.join(marker),
                &format!("{name} configuration"),
                95,
            ));
        }
    }
    if let Some(file) = files.iter().find(|file| {
        source_markers
            .iter()
            .any(|marker| file.content.contains(marker))
    }) {
        score = score.saturating_add(20);
        items.push(evidence(
            EvidenceKind::Source,
            &file.path,
            &format!("{name} source API"),
            90,
        ));
    }
    FrameworkDetection {
        name: name.into(),
        confidence: score.min(100),
        evidence: items,
    }
}

fn detect_python(
    root: &Path,
    files: &[SourceFile],
    manifest: &str,
    name: &str,
    dependency: &str,
    markers: &[&str],
    source_markers: &[&str],
) -> FrameworkDetection {
    let mut score = 0u8;
    let mut items = Vec::new();
    if python_dependency_present(manifest, dependency) {
        score += 55;
        items.push(evidence(
            EvidenceKind::Manifest,
            root,
            &format!("Python dependency {dependency}"),
            100,
        ));
    }
    for marker in markers {
        if files.iter().any(|file| file.relative.ends_with(marker)) || root.join(marker).exists() {
            score = score.saturating_add(10);
            items.push(evidence(
                EvidenceKind::Convention,
                &root.join(marker),
                &format!("{name} project convention"),
                85,
            ));
        }
    }
    if let Some(file) = files.iter().find(|file| {
        source_markers
            .iter()
            .any(|marker| file.content.contains(marker))
    }) {
        score = score.saturating_add(20);
        items.push(evidence(
            EvidenceKind::Source,
            &file.path,
            &format!("{name} source API"),
            90,
        ));
    }
    FrameworkDetection {
        name: name.into(),
        confidence: score.min(100),
        evidence: items,
    }
}

fn detect_jvm(
    root: &Path,
    files: &[SourceFile],
    manifest: &str,
    name: &str,
    dependencies: &[&str],
    source_markers: &[&str],
) -> FrameworkDetection {
    let mut score = 0u8;
    let mut items = Vec::new();
    if dependencies
        .iter()
        .any(|dependency| manifest.contains(dependency))
    {
        score += 65;
        items.push(evidence(
            EvidenceKind::Manifest,
            root,
            &format!("{name} JVM dependency/plugin"),
            100,
        ));
    }
    if let Some(file) = files.iter().find(|file| {
        source_markers
            .iter()
            .any(|marker| file.content.contains(marker))
    }) {
        score = score.saturating_add(30);
        items.push(evidence(
            EvidenceKind::Source,
            &file.path,
            &format!("{name} source API"),
            95,
        ));
    }
    FrameworkDetection {
        name: name.into(),
        confidence: score.min(100),
        evidence: items,
    }
}

fn detect_blazor(root: &Path, files: &[SourceFile]) -> FrameworkDetection {
    let project = find_file(root, |path| {
        path.extension().and_then(|value| value.to_str()) == Some("csproj")
    });
    let mut score = 0u8;
    let mut items = Vec::new();
    if let Some(path) = project {
        let content = read_source_text(&path).unwrap_or_default();
        if content.contains("Microsoft.NET.Sdk.BlazorWebAssembly")
            || content.contains("Microsoft.AspNetCore.Components.Web")
        {
            score += 70;
            items.push(evidence(
                EvidenceKind::Manifest,
                &path,
                "Blazor project SDK/package",
                100,
            ));
        }
    }
    if let Some(file) = files.iter().find(|file| {
        extension(&file.path) == Some("razor")
            && (file.content.contains("@page") || file.content.contains("@code"))
    }) {
        score = score.saturating_add(25);
        items.push(evidence(
            EvidenceKind::Source,
            &file.path,
            "Blazor Razor component",
            95,
        ));
    }
    FrameworkDetection {
        name: "Blazor".into(),
        confidence: score.min(100),
        evidence: items,
    }
}

fn detect_phoenix(root: &Path, files: &[SourceFile], mix: &str) -> FrameworkDetection {
    let mut score = 0u8;
    let mut items = Vec::new();
    if mix.contains(":phoenix") {
        score += 70;
        items.push(evidence(
            EvidenceKind::Manifest,
            &root.join("mix.exs"),
            "Phoenix Mix dependency",
            100,
        ));
    }
    if let Some(file) = files.iter().find(|file| {
        file.relative.ends_with("router.ex")
            && (file.content.contains(":router") || file.content.contains("Phoenix.Router"))
    }) {
        score = score.saturating_add(25);
        items.push(evidence(
            EvidenceKind::Source,
            &file.path,
            "Phoenix router",
            95,
        ));
    }
    FrameworkDetection {
        name: "Phoenix".into(),
        confidence: score.min(100),
        evidence: items,
    }
}

fn detect_aspnet(root: &Path, files: &[SourceFile]) -> FrameworkDetection {
    let project = find_file(root, |path| {
        path.extension().and_then(|value| value.to_str()) == Some("csproj")
    });
    let mut score = 0u8;
    let mut items = Vec::new();
    if let Some(path) = project {
        let content = read_source_text(&path).unwrap_or_default();
        if content.contains("Microsoft.NET.Sdk.Web") || content.contains("Microsoft.AspNetCore") {
            score += 70;
            items.push(evidence(
                EvidenceKind::Manifest,
                &path,
                "ASP.NET web SDK",
                100,
            ));
        }
    }
    if let Some(file) = files.iter().find(|file| {
        file.content.contains("WebApplication.CreateBuilder")
            || file.content.contains("ControllerBase")
    }) {
        score = score.saturating_add(25);
        items.push(evidence(
            EvidenceKind::Source,
            &file.path,
            "ASP.NET source API",
            95,
        ));
    }
    FrameworkDetection {
        name: "ASP.NET".into(),
        confidence: score.min(100),
        evidence: items,
    }
}

fn detect_composer(
    root: &Path,
    files: &[SourceFile],
    composer: &Option<Value>,
    name: &str,
    dependency: &str,
    markers: &[&str],
    source_markers: &[&str],
) -> FrameworkDetection {
    let mut score = 0u8;
    let mut items = Vec::new();
    if has_package(composer, dependency) {
        score += 55;
        items.push(evidence(
            EvidenceKind::Manifest,
            &root.join("composer.json"),
            &format!("Composer dependency {dependency}"),
            100,
        ));
    }
    for marker in markers {
        if root.join(marker).exists() {
            score = score.saturating_add(10);
            items.push(evidence(
                EvidenceKind::Convention,
                &root.join(marker),
                &format!("{name} project convention"),
                90,
            ));
        }
    }
    if let Some(file) = files.iter().find(|file| {
        source_markers
            .iter()
            .any(|marker| file.content.contains(marker))
    }) {
        score = score.saturating_add(20);
        items.push(evidence(
            EvidenceKind::Source,
            &file.path,
            &format!("{name} source API"),
            90,
        ));
    }
    FrameworkDetection {
        name: name.into(),
        confidence: score.min(100),
        evidence: items,
    }
}

fn detect_rails(root: &Path, files: &[SourceFile], gemfile: &str) -> FrameworkDetection {
    let mut score = 0u8;
    let mut items = Vec::new();
    if ruby_gem_present(gemfile, "rails") {
        score += 55;
        items.push(evidence(
            EvidenceKind::Manifest,
            &root.join("Gemfile"),
            "Rails gem dependency",
            100,
        ));
    }
    for marker in ["bin/rails", "config/routes.rb", "config/application.rb"] {
        if root.join(marker).exists() {
            score = score.saturating_add(10);
            items.push(evidence(
                EvidenceKind::Convention,
                &root.join(marker),
                "Rails project convention",
                90,
            ));
        }
    }
    if let Some(file) = files
        .iter()
        .find(|file| file.content.contains("Rails.application"))
    {
        score = score.saturating_add(15);
        items.push(evidence(
            EvidenceKind::Source,
            &file.path,
            "Rails application API",
            90,
        ));
    }
    FrameworkDetection {
        name: "Rails".into(),
        confidence: score.min(100),
        evidence: items,
    }
}

fn detect_go_web(root: &Path, files: &[SourceFile], go_mod: &str) -> FrameworkDetection {
    let dependencies = [
        ("github.com/gin-gonic/gin", "Gin"),
        ("github.com/labstack/echo", "Echo"),
        ("github.com/gofiber/fiber", "Fiber"),
        ("github.com/go-chi/chi", "Chi"),
    ];
    let mut score = 0u8;
    let mut items = Vec::new();
    let mut detected_name = "net/http";
    if let Some((dependency, framework)) = dependencies
        .iter()
        .find(|(dependency, _)| go_mod.contains(dependency))
    {
        score += 70;
        detected_name = framework;
        items.push(evidence(
            EvidenceKind::Manifest,
            &root.join("go.mod"),
            &format!("Go web dependency {dependency}"),
            100,
        ));
    }
    if let Some(file) = files.iter().find(|file| {
        file.content.contains("net/http")
            || file.content.contains("HandleFunc(")
            || file.content.contains("gin.Default(")
    }) {
        score = score.saturating_add(60);
        items.push(evidence(
            EvidenceKind::Source,
            &file.path,
            &format!("Go web API ({detected_name})"),
            90,
        ));
    }
    FrameworkDetection {
        name: detected_name.into(),
        confidence: score.min(100),
        evidence: items,
    }
}

fn detect_rust_web(root: &Path, files: &[SourceFile], cargo: &str) -> FrameworkDetection {
    let dependencies = ["actix-web", "axum", "rocket", "warp"];
    let mut score = 0u8;
    let mut items = Vec::new();
    if let Some(dependency) = dependencies.iter().find(|dependency| {
        cargo
            .lines()
            .any(|line| line.trim_start().starts_with(*dependency))
    }) {
        score += 70;
        items.push(evidence(
            EvidenceKind::Manifest,
            &root.join("Cargo.toml"),
            &format!("Rust web dependency {dependency}"),
            100,
        ));
    }
    if let Some(file) = files.iter().find(|file| {
        dependencies
            .iter()
            .any(|dependency| file.content.contains(&dependency.replace('-', "_")))
    }) {
        score = score.saturating_add(25);
        items.push(evidence(
            EvidenceKind::Source,
            &file.path,
            "Rust web framework API",
            90,
        ));
    }
    let detected_name = dependencies
        .iter()
        .find(|dependency| cargo.lines().any(|line| line.trim_start().starts_with(*dependency)))
        .map(|dependency| match *dependency {
            "actix-web" => "Actix Web",
            "axum" => "Axum",
            "rocket" => "Rocket",
            "warp" => "Warp",
            _ => "Rust",
        })
        .unwrap_or("Rust");
    FrameworkDetection {
        name: detected_name.into(),
        confidence: score.min(100),
        evidence: items,
    }
}

fn runtime_for(root: &Path, framework: &str) -> Vec<RuntimeRequirement> {
    let (name, executable, manifest) = match framework {
        "Angular" | "React" | "Next.js" | "Vue" | "Nuxt" | "Svelte" | "SvelteKit" | "Astro"
        | "NestJS" | "Express" | "Fastify" | "Hono" => ("Node.js", "node", "package.json"),
        "Django" | "FastAPI" | "Flask" => ("Python", "python", "pyproject.toml/requirements.txt"),
        "ASP.NET" | "Blazor" => (".NET", "dotnet", "*.csproj"),
        "Symfony" => ("PHP", "php", "composer.json"),
        "Rails" => ("Ruby", "ruby", "Gemfile"),
        "Gin" | "Echo" | "Fiber" | "Chi" | "net/http" => ("Go", "go", "go.mod"),
        "Axum" | "Actix Web" | "Rocket" | "Warp" => ("Rust", "rustc", "Cargo.toml"),
        "Quarkus" | "Micronaut" | "Ktor" => ("Java", "java", "pom.xml/build.gradle"),
        "Phoenix" => ("Elixir", "mix", "mix.exs"),
        _ => return Vec::new(),
    };
    vec![RuntimeRequirement {
        name: name.into(),
        executable: executable.into(),
        version_hint: None,
        required_by: root.join(manifest).to_string_lossy().into_owned(),
        confidence: 95,
    }]
}

fn express_receivers(content: &str) -> BTreeSet<String> {
    let mut result = BTreeSet::new();
    for line in content.lines() {
        if !(line.contains("express()")
            || line.contains("Router()")
            || line.contains("express.Router()"))
        {
            continue;
        }
        if let Some((left, _)) = line.split_once('=') {
            if let Some(name) = left.split_whitespace().last() {
                let clean = name.trim().trim_matches(|character: char| {
                    !character.is_alphanumeric() && character != '_' && character != '$'
                });
                if !clean.is_empty() {
                    result.insert(clean.into());
                }
            }
        }
    }
    if result.is_empty() {
        result.extend(["app".into(), "router".into()]);
    }
    result
}

fn source_files(root: &Path) -> io::Result<Vec<SourceFile>> {
    let mut paths = Vec::new();
    collect(root, root, &mut paths)?;
    paths.sort();
    let mut result = Vec::new();
    for path in paths {
        let metadata = match fs::metadata(&path) {
            Ok(m) => m,
            Err(_) => continue,
        };
        if metadata.len() > DEFAULT_MAX_SOURCE_SIZE {
            continue;
        }
        let content = match read_source_text(&path) {
            Ok(content) => content,
            Err(_) => continue,
        };
        let content = mask_source_comments(&content, extension(&path).unwrap_or(""));
        let relative = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        result.push(SourceFile {
            path,
            relative,
            content,
        });
    }
    Ok(result)
}

fn mask_source_comments(content: &str, extension: &str) -> String {
    let c_style = matches!(
        extension,
        "ts" | "tsx"
            | "js"
            | "jsx"
            | "mjs"
            | "cjs"
            | "cs"
            | "java"
            | "kt"
            | "kts"
            | "go"
            | "rs"
            | "php"
            | "vue"
            | "svelte"
            | "astro"
            | "razor"
    );
    let hash_style = matches!(extension, "py" | "rb" | "ex" | "exs");
    let chars = content.chars().collect::<Vec<_>>();
    let mut output = String::with_capacity(content.len());
    let mut index = 0usize;
    let mut quote = None;
    let mut escaped = false;
    while index < chars.len() {
        let current = chars[index];
        let next = chars.get(index + 1).copied();
        if let Some(delimiter) = quote {
            output.push(current);
            if escaped {
                escaped = false;
            } else if current == '\\' {
                escaped = true;
            } else if current == delimiter {
                quote = None;
            }
            index += 1;
            continue;
        }
        if matches!(current, '\'' | '"' | '`') {
            quote = Some(current);
            output.push(current);
            index += 1;
            continue;
        }
        let line_comment =
            (c_style && current == '/' && next == Some('/')) || (hash_style && current == '#');
        if line_comment {
            output.push(' ');
            if current == '/' {
                output.push(' ');
                index += 1;
            }
            index += 1;
            while index < chars.len() && chars[index] != '\n' {
                output.push(' ');
                index += 1;
            }
            continue;
        }
        if c_style && current == '/' && next == Some('*') {
            output.push_str("  ");
            index += 2;
            while index < chars.len() {
                if chars[index] == '*' && chars.get(index + 1) == Some(&'/') {
                    output.push_str("  ");
                    index += 2;
                    break;
                }
                output.push(if chars[index] == '\n' { '\n' } else { ' ' });
                index += 1;
            }
            continue;
        }
        output.push(current);
        index += 1;
    }
    output
}
fn collect(root: &Path, path: &Path, files: &mut Vec<PathBuf>) -> io::Result<()> {
    if excluded(root, path) {
        return Ok(());
    }
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error)
            if matches!(
                error.kind(),
                io::ErrorKind::PermissionDenied | io::ErrorKind::NotFound
            ) =>
        {
            return Ok(())
        }
        Err(error) => return Err(error),
    };
    if metadata.file_type().is_symlink() {
        return Ok(());
    }
    if metadata.is_file() {
        if metadata.len() <= DEFAULT_MAX_SOURCE_SIZE
            && !path
                .file_name()
                .and_then(|value| value.to_str())
                .is_some_and(|name| name.ends_with(".blade.php"))
            && matches!(
                extension(path),
                Some(
                    "ts" | "tsx"
                        | "js"
                        | "jsx"
                        | "mjs"
                        | "cjs"
                        | "py"
                        | "cs"
                        | "php"
                        | "rb"
                        | "go"
                        | "rs"
                        | "java"
                        | "kt"
                        | "kts"
                        | "ex"
                        | "exs"
                        | "vue"
                        | "svelte"
                        | "astro"
                        | "razor"
                )
            )
        {
            files.push(path.into());
        }
    } else if metadata.is_dir() {
        let entries = match fs::read_dir(path) {
            Ok(entries) => entries,
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::PermissionDenied | io::ErrorKind::NotFound
                ) =>
            {
                return Ok(())
            }
            Err(error) => return Err(error),
        };
        for entry in entries.flatten() {
            collect(root, &entry.path(), files)?;
        }
    }
    Ok(())
}
fn excluded(root: &Path, path: &Path) -> bool {
    ScanPolicy::default().is_excluded_from(root, path)
}
fn extension(path: &Path) -> Option<&str> {
    path.extension().and_then(|value| value.to_str())
}
fn json(path: PathBuf) -> Option<Value> {
    serde_json::from_str(&read_source_text(path).ok()?).ok()
}
fn has_package(manifest: &Option<Value>, package: &str) -> bool {
    ["dependencies", "devDependencies", "require", "require-dev"]
        .iter()
        .any(|section| {
            manifest
                .as_ref()
                .and_then(|value| value.get(section))
                .and_then(Value::as_object)
                .is_some_and(|values| values.contains_key(package))
        })
}

fn python_dependency_present(manifest: &str, dependency: &str) -> bool {
    manifest.lines().any(|line| {
        let value = line.trim().trim_matches(['\'', '"', ',']);
        let package = value
            .split(|character: char| {
                character.is_whitespace()
                    || matches!(
                        character,
                        '=' | '<' | '>' | '~' | '!' | '[' | ']' | ':' | ','
                    )
            })
            .find(|item| !item.is_empty())
            .unwrap_or("")
            .trim_matches(['\'', '"']);
        package.eq_ignore_ascii_case(dependency)
            || value.starts_with(&format!("{dependency} ="))
            || value.starts_with(&format!("\"{dependency}\""))
    })
}

fn ruby_gem_present(gemfile: &str, dependency: &str) -> bool {
    gemfile.lines().any(|line| {
        let line = line.trim();
        line.strip_prefix("gem ")
            .and_then(first_quoted)
            .is_some_and(|name| name == dependency)
    })
}
fn read_many(root: &Path, names: &[&str]) -> String {
    names
        .iter()
        .map(|name| read_source_text(root.join(name)).unwrap_or_default())
        .collect::<Vec<_>>()
        .join("\n")
}
fn find_file(root: &Path, predicate: impl Fn(&Path) -> bool + Copy) -> Option<PathBuf> {
    let mut files = Vec::new();
    collect_any(root, root, &mut files, predicate).ok()?;
    files.into_iter().next()
}
fn collect_any(
    root: &Path,
    path: &Path,
    files: &mut Vec<PathBuf>,
    predicate: impl Fn(&Path) -> bool + Copy,
) -> io::Result<()> {
    if excluded(root, path) {
        return Ok(());
    }
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error)
            if matches!(
                error.kind(),
                io::ErrorKind::PermissionDenied | io::ErrorKind::NotFound
            ) =>
        {
            return Ok(())
        }
        Err(error) => return Err(error),
    };
    if metadata.file_type().is_symlink() {
        return Ok(());
    }
    if metadata.is_file() {
        if predicate(path) {
            files.push(path.into());
        }
    } else if metadata.is_dir() {
        let entries = match fs::read_dir(path) {
            Ok(entries) => entries,
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::PermissionDenied | io::ErrorKind::NotFound
                ) =>
            {
                return Ok(())
            }
            Err(error) => return Err(error),
        };
        for entry in entries.flatten() {
            collect_any(root, &entry.path(), files, predicate)?;
        }
    }
    Ok(())
}
fn evidence(kind: EvidenceKind, source: &Path, detail: &str, confidence: u8) -> Evidence {
    Evidence {
        kind,
        source: source.to_string_lossy().into_owned(),
        detail: detail.into(),
        confidence,
    }
}
fn same_path(left: &str, right: &Path) -> bool {
    let left_normalized = left.replace('\\', "/");
    let right_normalized = right.to_string_lossy().replace('\\', "/");
    left_normalized == right_normalized
        || left_normalized.trim_end_matches('/') == right_normalized.trim_end_matches('/')
}
fn normalize_http_path(value: &str) -> String {
    let parts = value
        .trim()
        .split('/')
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    format!("/{}", parts.join("/"))
}
fn normalize_frontend_path(value: &str) -> String {
    if value.is_empty() {
        "/".into()
    } else {
        normalize_http_path(value)
    }
}
fn join_path(left: &str, right: &str) -> String {
    normalize_http_path(&format!(
        "{}/{}",
        left.trim_matches('/'),
        right.trim_matches('/')
    ))
}
fn first_quoted(value: &str) -> Option<String> {
    let start = value.find(['\'', '"'])?;
    let quote = value.as_bytes()[start] as char;
    let end = value[start + 1..].find(quote)? + start + 1;
    Some(value[start + 1..end].into())
}
fn quoted_arguments_after(content: &str, marker: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut offset = 0;
    while let Some(index) = content[offset..].find(marker) {
        let start = offset + index + marker.len();
        let tail = &content[start..];
        let boundary = tail.find([')', '\n']).unwrap_or(tail.len().min(512));
        if let Some(value) = first_quoted(&tail[..boundary]) {
            result.push(value);
        }
        offset = start;
    }
    result
}
fn value_after_key(line: &str, key: &str) -> Option<String> {
    let index = line.find(key)? + key.len();
    let rest = line[index..].trim_start();
    let rest = rest.strip_prefix(':')?.trim_start();
    first_quoted(rest)
}
fn attribute_value(line: &str, key: &str) -> Option<String> {
    let index = line.find(key)? + key.len();
    let rest = line[index..].trim_start().strip_prefix('=')?.trim_start();
    first_quoted(rest)
}
fn decorator_argument(content: &str, name: &str) -> Option<String> {
    decorator_arguments(content, name).into_iter().next()
}
fn decorator_arguments(content: &str, name: &str) -> Vec<String> {
    balanced_expressions(content, &format!("@{name}("))
        .into_iter()
        .map(|value| first_quoted(&value).unwrap_or_default())
        .collect()
}
fn bracket_attribute(content: &str, name: &str) -> Option<String> {
    bracket_attributes(content, name).into_iter().next()
}
fn bracket_attributes(content: &str, name: &str) -> Vec<String> {
    let marker = format!("[{name}");
    let mut values = quoted_arguments_after(content, &marker);
    if values.is_empty() && content.contains(&format!("[{name}]")) {
        values.push(String::new());
    }
    values
}
fn attribute_expressions(content: &str, name: &str) -> Vec<String> {
    let marker = format!("#[{name}(");
    balanced_expressions(content, &marker)
}
fn balanced_expressions(content: &str, marker: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut offset = 0;
    while let Some(relative) = content[offset..].find(marker) {
        let start = offset + relative + marker.len();
        let mut depth = 1i32;
        let mut quote = None;
        let mut escaped = false;
        for (delta, ch) in content[start..].char_indices() {
            if let Some(active) = quote {
                if escaped {
                    escaped = false;
                } else if ch == '\\' {
                    escaped = true;
                } else if ch == active {
                    quote = None;
                }
            } else if ch == '\'' || ch == '"' {
                quote = Some(ch);
            } else if ch == '(' {
                depth += 1;
            } else if ch == ')' {
                depth -= 1;
                if depth == 0 {
                    result.push(content[start..start + delta].into());
                    offset = start + delta + 1;
                    break;
                }
            }
        }
        if depth != 0 {
            break;
        }
    }
    result
}
fn list_after_key(expression: &str, key: &str) -> Vec<String> {
    let Some(index) = expression.find(key) else {
        return Vec::new();
    };
    let rest = &expression[index + key.len()..];
    let Some(open) = rest.find('[') else {
        return Vec::new();
    };
    let list = &rest[open + 1..];
    let end = list.find(']').unwrap_or(list.len());
    all_quoted(&list[..end])
}
fn all_quoted(value: &str) -> Vec<String> {
    let chars: Vec<char> = value.chars().collect();
    let mut result = Vec::new();
    let mut index = 0;
    while index < chars.len() {
        if chars[index] == '\'' || chars[index] == '"' {
            let quote = chars[index];
            index += 1;
            let start = index;
            while index < chars.len() && chars[index] != quote {
                index += 1;
            }
            result.push(chars[start..index].iter().collect());
        }
        index += 1;
    }
    result
}
fn first_symbol_or_quoted(value: &str) -> Option<String> {
    first_quoted(value).or_else(|| {
        value
            .trim()
            .strip_prefix(':')
            .map(|value| {
                value
                    .split(|ch: char| !ch.is_alphanumeric() && ch != '_')
                    .next()
                    .unwrap_or("")
                    .into()
            })
            .filter(|value: &String| !value.is_empty())
    })
}
fn route_method_near(content: &str, path: &str) -> Option<&'static str> {
    let index = content.find(path)?;
    let tail = &content[index..content.len().min(index + 160)];
    [
        ("get(", "GET"),
        ("post(", "POST"),
        ("put(", "PUT"),
        ("patch(", "PATCH"),
        ("delete(", "DELETE"),
    ]
    .into_iter()
    .find(|(call, _)| tail.contains(call))
    .map(|(_, method)| method)
}

fn route_from_file(relative: &str, roots: &[&str], terminal_names: &[&str]) -> Option<String> {
    let relative = relative.replace('\\', "/");
    let mut route = roots
        .iter()
        .find_map(|root| relative.strip_prefix(root))?
        .to_string();
    if let Some((stem, _)) = route.rsplit_once('.') {
        route = stem.into();
    }
    if route.split('/').any(|part| part.starts_with('_')) {
        return None;
    }
    if !terminal_names.is_empty() {
        let terminal = terminal_names.iter().find_map(|name| {
            let clean = name
                .trim_start_matches('/')
                .split('.')
                .next()
                .unwrap_or(name);
            (route == clean).then_some(route.len()).or_else(|| {
                route
                    .ends_with(&format!("/{clean}"))
                    .then_some(clean.len() + 1)
            })
        })?;
        route.truncate(route.len().saturating_sub(terminal));
    }
    for suffix in [
        ".get", ".post", ".put", ".patch", ".delete", ".options", ".head",
    ] {
        if route.ends_with(suffix) {
            route.truncate(route.len() - suffix.len());
            break;
        }
    }
    if route == "index" || route.ends_with("/index") {
        route.truncate(
            route
                .len()
                .saturating_sub(if route == "index" { 5 } else { 6 }),
        );
    }
    let mut parts = Vec::new();
    for part in route.split('/').filter(|part| !part.is_empty()) {
        if part.starts_with('(') && part.ends_with(')') {
            continue;
        }
        let converted = if part.starts_with("[[...") && part.ends_with("]]") {
            format!("{{{}*}}", &part[5..part.len() - 2])
        } else if part.starts_with("[...") && part.ends_with(']') {
            format!("{{{}*}}", &part[4..part.len() - 1])
        } else if part.starts_with('[') && part.ends_with(']') {
            format!("{{{}}}", &part[1..part.len() - 1])
        } else {
            part.into()
        };
        parts.push(converted);
    }
    Some(format!("/{}", parts.join("/")))
}

fn exported_http_methods(content: &str) -> Vec<&'static str> {
    ["GET", "POST", "PUT", "PATCH", "DELETE", "OPTIONS", "HEAD"]
        .into_iter()
        .filter(|method| {
            content.contains(&format!("export const {method}"))
                || content.contains(&format!("export function {method}"))
                || content.contains(&format!("export async function {method}"))
        })
        .collect()
}

fn method_from_filename(relative: &str) -> Option<&'static str> {
    let lower = relative.to_ascii_lowercase();
    [
        (".get.", "GET"),
        (".post.", "POST"),
        (".put.", "PUT"),
        (".patch.", "PATCH"),
        (".delete.", "DELETE"),
        (".options.", "OPTIONS"),
        (".head.", "HEAD"),
    ]
    .into_iter()
    .find(|(marker, _)| lower.contains(marker))
    .map(|(_, method)| method)
}

fn call_first_argument(line: &str, call: &str) -> Option<String> {
    let index = line.find(call)? + call.len();
    first_quoted(&line[index..])
}

fn annotation_arguments(content: &str, name: &str) -> Vec<String> {
    balanced_expressions(content, &format!("@{name}("))
        .into_iter()
        .map(|expression| first_quoted(&expression).unwrap_or_default())
        .collect()
}

fn annotation_occurrences(content: &str, name: &str) -> Vec<String> {
    let lines = content.lines().collect::<Vec<_>>();
    let marker = format!("@{name}");
    let mut result = Vec::new();
    for (index, line) in lines
        .iter()
        .enumerate()
        .filter(|(_, line)| line.contains(&marker))
    {
        if let Some(path) = call_first_argument(line, &format!("@{name}(")) {
            result.push(path);
            continue;
        }
        let same_line = line
            .find(&marker)
            .and_then(|offset| call_first_argument(&line[offset + marker.len()..], "@Path("));
        let path = same_line
            .or_else(|| {
                lines
                    .iter()
                    .skip(index + 1)
                    .take(4)
                    .find_map(|candidate| call_first_argument(candidate, "@Path("))
            })
            .or_else(|| {
                lines[..index]
                    .iter()
                    .rev()
                    .take(2)
                    .find_map(|candidate| call_first_argument(candidate, "@Path("))
            })
            .unwrap_or_default();
        result.push(path);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reads_balanced_attributes() {
        assert_eq!(
            attribute_expressions("#[Route('/x', methods: ['GET'])]", "Route"),
            vec!["'/x', methods: ['GET']"]
        );
    }
    #[test]
    fn detects_exact_manifest_dependency() {
        let manifest: Option<Value> =
            serde_json::from_str(r#"{"dependencies":{"react":"18"}}"#).ok();
        assert!(has_package(&manifest, "react"));
        assert!(!has_package(&manifest, "preact"));
    }

    #[test]
    fn derives_file_routes_and_dynamic_segments() {
        assert_eq!(
            route_from_file(
                "src/app/(admin)/users/[id]/page.tsx",
                &["src/app/"],
                &["page"]
            ),
            Some("/users/{id}".into())
        );
        assert_eq!(
            route_from_file(
                "src/routes/docs/[...slug]/+page.svelte",
                &["src/routes/"],
                &["+page"]
            ),
            Some("/docs/{slug*}".into())
        );
        assert_eq!(
            route_from_file("server/api/users/[id].get.ts", &["server/api/"], &[]),
            Some("/users/{id}".into())
        );
    }
}
