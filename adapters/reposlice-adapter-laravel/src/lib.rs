use reposlice_core::{
    stable_hash, read_source_text, AnalysisContribution, Component, ComponentKind, Dependency, DependencyKind,
    DependencyMetadata, Entrypoint, EntrypointKind, EntrypointMetadata, Evidence, EvidenceKind,
    FrameworkAdapter, FrameworkDetection, RuntimeRequirement, ScanPolicy, SymbolMetadata, DEFAULT_MAX_SOURCE_SIZE,
};
use reposlice_parser_php::{resolve_type_reference, PhpIndex, PhpType};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const ROUTE_METHODS: &[&str] = &["get", "post", "put", "patch", "delete", "options"];
const RELATION_METHODS: &[&str] = &[
    "hasOne",
    "hasMany",
    "belongsTo",
    "belongsToMany",
    "morphOne",
    "morphMany",
    "morphTo",
    "morphToMany",
    "morphedByMany",
    "hasOneThrough",
    "hasManyThrough",
];

#[derive(Clone, Debug, Default)]
pub struct LaravelAnalysis {
    pub components: Vec<Component>,
    pub entrypoints: Vec<Entrypoint>,
    pub dependencies: Vec<Dependency>,
    pub symbols: Vec<SymbolMetadata>,
    pub entrypoint_metadata: Vec<EntrypointMetadata>,
    pub dependency_metadata: Vec<DependencyMetadata>,
    pub runtime_requirements: Vec<RuntimeRequirement>,
}

pub struct LaravelAdapter;

impl FrameworkAdapter for LaravelAdapter {
    fn id(&self) -> &'static str {
        "laravel"
    }
    fn detect(&self, root: &Path) -> FrameworkDetection {
        detect_laravel(root)
    }
    fn analyze(&self, root: &Path) -> io::Result<AnalysisContribution> {
        let php = reposlice_parser_php::parse_php(root)?;
        let result = analyze_laravel(root, &php.index)?;
        Ok(AnalysisContribution {
            technologies: vec![reposlice_core::Technology {
                category: "framework".into(),
                name: "Laravel".into(),
                confidence: detect_laravel(root).confidence,
                classification: "Backend Framework".into(),
                ..reposlice_core::Technology::default()
            }],
            components: result.components,
            entrypoints: result.entrypoints,
            dependencies: result.dependencies,
            metadata: reposlice_core::AnalysisMetadata {
                symbols: result.symbols,
                entrypoints: result.entrypoint_metadata,
                dependencies: result.dependency_metadata,
                runtime_requirements: result.runtime_requirements,
                ..Default::default()
            },
        })
    }
}

#[derive(Clone, Debug, Default)]
struct RouteContext {
    prefix: String,
    middleware: Vec<String>,
    name_prefix: String,
    domain: Option<String>,
    controller: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Token {
    value: String,
    quoted: bool,
}

#[derive(Clone, Debug)]
struct Call {
    method: String,
    args: Vec<Vec<Token>>,
    chains: Vec<(String, Vec<Vec<Token>>)>,
    end: usize,
}

pub fn detect_laravel(root: &Path) -> FrameworkDetection {
    let mut confidence = 0u8;
    let mut evidence = Vec::new();
    let composer = root.join("composer.json");
    let composer_content = read_source_text(&composer).unwrap_or_default();
    if composer_content.contains("laravel/framework") {
        confidence = confidence.saturating_add(50);
        evidence.push(evidence_item(
            EvidenceKind::Manifest,
            &composer,
            "composer requires laravel/framework",
            100,
        ));
    }
    for (relative, weight, detail) in [
        ("artisan", 15, "Laravel artisan executable"),
        ("bootstrap/app.php", 15, "Laravel bootstrap file"),
        ("routes", 10, "Laravel routes directory"),
        ("app/Providers", 10, "Laravel providers directory"),
    ] {
        let path = root.join(relative);
        if path.exists() {
            confidence = confidence.saturating_add(weight);
            evidence.push(evidence_item(EvidenceKind::Convention, &path, detail, 85));
        }
    }
    FrameworkDetection {
        name: "Laravel".into(),
        confidence: confidence.min(100),
        evidence,
    }
}

pub fn analyze_laravel(root: &Path, index: &PhpIndex) -> io::Result<LaravelAnalysis> {
    let mut output = LaravelAnalysis::default();
    let mut by_qualified = BTreeMap::new();
    for item in &index.types {
        by_qualified.insert(
            item.qualified_name.to_ascii_lowercase(),
            item.component_id.clone(),
        );
        output.symbols.push(symbol_metadata(item));
    }
    classify_components(index, &mut output.components);
    for component in &output.components {
        if let Some(symbol) = output
            .symbols
            .iter_mut()
            .find(|item| item.symbol_id == component.id)
        {
            symbol.framework_kind = Some(component.kind.as_str().to_string());
            symbol.confidence = 98;
        }
    }
    add_conventional_entrypoints(
        index,
        &output.components,
        &mut output.entrypoints,
        &mut output.entrypoint_metadata,
    );
    add_migrations(
        root,
        index,
        &mut output.components,
        &mut output.symbols,
        &mut output.dependencies,
        &mut output.dependency_metadata,
    )?;
    analyze_relations(
        index,
        &by_qualified,
        &mut output.dependencies,
        &mut output.dependency_metadata,
    );
    analyze_routes(root, index, &by_qualified, &mut output)?;
    output.runtime_requirements = runtime_requirements(root);
    output.components.sort_by(|a, b| a.id.cmp(&b.id));
    output.components.dedup_by(|a, b| a.id == b.id);
    output.entrypoints.sort_by(|a, b| a.id.cmp(&b.id));
    output.entrypoints.dedup_by(|a, b| a.id == b.id);
    output.dependencies.sort_by(|a, b| {
        (&a.source_id, &a.target_id, a.kind.as_str()).cmp(&(
            &b.source_id,
            &b.target_id,
            b.kind.as_str(),
        ))
    });
    output.dependencies.dedup();
    Ok(output)
}

fn classify_components(index: &PhpIndex, output: &mut Vec<Component>) {
    for item in &index.types {
        let normalized = item.file.replace('\\', "/").to_ascii_lowercase();
        let kind = if normalized.contains("/http/controllers/") {
            ComponentKind::Controller
        } else if normalized.contains("/http/middleware/") {
            ComponentKind::Middleware
        } else if normalized.contains("/console/commands/") {
            ComponentKind::Command
        } else if normalized.contains("/jobs/") {
            ComponentKind::Job
        } else if normalized.contains("/events/") {
            ComponentKind::Event
        } else if normalized.contains("/listeners/") {
            ComponentKind::Listener
        } else if normalized.contains("/models/") {
            ComponentKind::Model
        } else if normalized.contains("/providers/") {
            ComponentKind::Provider
        } else if item
            .extends
            .as_deref()
            .is_some_and(|value| value.ends_with("Controller"))
        {
            ComponentKind::Controller
        } else if item
            .extends
            .as_deref()
            .is_some_and(|value| value.ends_with("Model"))
        {
            ComponentKind::Model
        } else {
            continue;
        };
        output.push(Component {
            id: item.component_id.clone(),
            name: item.name.clone(),
            kind,
            language: "PHP".into(),
            file: item.file.clone(),
        });
    }
}

fn symbol_metadata(item: &PhpType) -> SymbolMetadata {
    SymbolMetadata {
        symbol_id: item.component_id.clone(),
        qualified_name: Some(item.qualified_name.clone()),
        namespace: (!item.namespace.is_empty()).then(|| item.namespace.clone()),
        framework_kind: None,
        attributes: Vec::new(),
        evidence: vec![evidence_item(
            EvidenceKind::Source,
            Path::new(&item.file),
            "PHP declaration",
            100,
        )],
        confidence: 100,
    }
}

fn add_conventional_entrypoints(
    index: &PhpIndex,
    components: &[Component],
    entrypoints: &mut Vec<Entrypoint>,
    metadata: &mut Vec<EntrypointMetadata>,
) {
    for component in components {
        let kind = match component.kind {
            ComponentKind::Command => Some(EntrypointKind::CliCommand),
            ComponentKind::Listener => Some(EntrypointKind::EventConsumer),
            _ => None,
        };
        let Some(kind) = kind else {
            continue;
        };
        if component.kind == ComponentKind::Listener
            && !index
                .types
                .iter()
                .find(|item| item.component_id == component.id)
                .is_some_and(|item| item.methods.iter().any(|method| method.name == "handle"))
        {
            continue;
        }
        let seed = format!("{}\0{}", component.id, kind.as_str());
        let id = format!("laravel:{}:{}", kind.as_str(), stable_hash(&seed));
        let evidence = vec![evidence_item(
            EvidenceKind::Convention,
            Path::new(&component.file),
            &format!("Laravel {} convention", component.kind.as_str()),
            80,
        )];
        entrypoints.push(Entrypoint {
            id: id.clone(),
            name: component.name.clone(),
            kind,
            component_id: component.id.clone(),
            file: component.file.clone(),
        });
        metadata.push(EntrypointMetadata {
            entrypoint_id: id,
            method: None,
            path: None,
            route_name: None,
            domain: None,
            middleware: Vec::new(),
            controller: Some(component.name.clone()),
            action: Some("handle".into()),
            evidence,
            confidence: 80,
        });
    }
}

fn add_migrations(
    root: &Path,
    index: &PhpIndex,
    components: &mut Vec<Component>,
    symbols: &mut Vec<SymbolMetadata>,
    dependencies: &mut Vec<Dependency>,
    dependency_metadata: &mut Vec<DependencyMetadata>,
) -> io::Result<()> {
    let directory = root.join("database/migrations");
    if !directory.is_dir() {
        return Ok(());
    }
    let mut files = Vec::new();
    collect_php(root, &directory, &mut files)?;
    for file in files {
        let relative = file
            .strip_prefix(root)
            .unwrap_or(&file)
            .to_string_lossy()
            .replace('\\', "/");
        let id = format!("laravel:migration:{}", stable_hash(&relative));
        let name = file
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("migration")
            .to_string();
        let content = read_source_text(&file).unwrap_or_default();
        let (table, foreign_tables) = migration_schema(&content);
        components.push(Component {
            id: id.clone(),
            name,
            kind: ComponentKind::Migration,
            language: "PHP".into(),
            file: file.to_string_lossy().into_owned(),
        });
        symbols.push(SymbolMetadata {
            symbol_id: id.clone(),
            qualified_name: None,
            namespace: None,
            framework_kind: Some("migration".into()),
            attributes: {
                let mut values = vec![("relative_path".into(), relative)];
                if let Some(table) = &table {
                    values.push(("table".into(), table.clone()));
                }
                values
            },
            evidence: vec![evidence_item(
                EvidenceKind::Convention,
                &file,
                "database/migrations convention",
                95,
            )],
            confidence: 95,
        });
        for foreign_table in foreign_tables {
            if let Some(target) = index
                .types
                .iter()
                .find(|item| conventional_table(&item.name) == foreign_table)
            {
                dependencies.push(Dependency {
                    source_id: id.clone(),
                    target_id: target.component_id.clone(),
                    kind: DependencyKind::References,
                });
                dependency_metadata.push(DependencyMetadata {
                    source_id: id.clone(),
                    target_id: target.component_id.clone(),
                    kind: "migration-foreign-key".into(),
                    evidence: vec![evidence_item(
                        EvidenceKind::Source,
                        &file,
                        &format!("migration references table {foreign_table}"),
                        80,
                    )],
                    confidence: 80,
                });
            }
        }
    }
    Ok(())
}

fn migration_schema(content: &str) -> (Option<String>, BTreeSet<String>) {
    let tokens = tokenize(content);
    let mut table = None;
    let mut foreign_tables = BTreeSet::new();
    let mut cursor = 0usize;
    while cursor + 3 < tokens.len() {
        if token(&tokens, cursor) == Some("Schema") && token(&tokens, cursor + 1) == Some("::") {
            if let Some(call) = parse_call(&tokens, cursor + 2) {
                if matches!(call.method.as_str(), "create" | "table") {
                    table = argument_string(&call.args, 0).or(table);
                }
                cursor = call.end;
                continue;
            }
        }
        if token(&tokens, cursor) == Some("foreignId") && token(&tokens, cursor + 1) == Some("(") {
            if let Some(call) = parse_call(&tokens, cursor) {
                if let Some((_, args)) = call.chains.iter().find(|(name, _)| name == "constrained")
                {
                    if let Some(value) = argument_string(args, 0) {
                        foreign_tables.insert(value);
                    } else if let Some(column) = argument_string(&call.args, 0) {
                        foreign_tables.insert(format!("{}s", column.trim_end_matches("_id")));
                    }
                }
                cursor = call.end;
                continue;
            }
        }
        cursor += 1;
    }
    (table, foreign_tables)
}

fn conventional_table(name: &str) -> String {
    let mut value = String::new();
    for (index, character) in name.chars().enumerate() {
        if character.is_ascii_uppercase() && index > 0 {
            value.push('_');
        }
        value.push(character.to_ascii_lowercase());
    }
    if value.ends_with('s') {
        value
    } else {
        format!("{value}s")
    }
}

fn analyze_relations(
    index: &PhpIndex,
    by_qualified: &BTreeMap<String, String>,
    dependencies: &mut Vec<Dependency>,
    metadata: &mut Vec<DependencyMetadata>,
) {
    let mut seen = BTreeSet::new();
    for item in &index.types {
        for method in &item.methods {
            for call in &method.calls {
                if !RELATION_METHODS.contains(&call.method.as_str()) {
                    continue;
                }
                let Some(argument) = call.arguments.first() else {
                    continue;
                };
                let reference = argument
                    .split("::")
                    .next()
                    .unwrap_or(argument)
                    .trim()
                    .trim_matches('\\');
                let qualified = resolve_type_reference(item, reference).to_ascii_lowercase();
                let Some(target) = by_qualified.get(&qualified) else {
                    continue;
                };
                let key = (
                    item.component_id.clone(),
                    target.clone(),
                    call.method.clone(),
                );
                if !seen.insert(key) {
                    continue;
                }
                dependencies.push(Dependency {
                    source_id: item.component_id.clone(),
                    target_id: target.clone(),
                    kind: DependencyKind::EloquentRelation,
                });
                metadata.push(DependencyMetadata {
                    source_id: item.component_id.clone(),
                    target_id: target.clone(),
                    kind: call.method.clone(),
                    evidence: vec![evidence_item(
                        EvidenceKind::Source,
                        Path::new(&item.file),
                        &format!("{}() relation in {}", call.method, method.name),
                        95,
                    )],
                    confidence: 95,
                });
            }
        }
    }
}

fn analyze_routes(
    root: &Path,
    index: &PhpIndex,
    controllers: &BTreeMap<String, String>,
    output: &mut LaravelAnalysis,
) -> io::Result<()> {
    let directory = root.join("routes");
    if !directory.is_dir() {
        return Ok(());
    }
    let mut files = Vec::new();
    collect_php(root, &directory, &mut files)?;
    files.sort();
    for file in files {
        if fs::metadata(&file)?.len() > DEFAULT_MAX_SOURCE_SIZE {
            continue;
        }
        let content = read_source_text(&file)?;
        let tokens = tokenize(&content);
        let relative = file
            .strip_prefix(root)
            .unwrap_or(&file)
            .to_string_lossy()
            .replace('\\', "/");
        let route_component = Component {
            id: format!("laravel:routes:{}", stable_hash(&relative)),
            name: relative.clone(),
            kind: ComponentKind::Module,
            language: "PHP".into(),
            file: file.to_string_lossy().into_owned(),
        };
        output.components.push(route_component.clone());
        parse_route_range(
            &tokens,
            0,
            tokens.len(),
            &RouteContext::default(),
            &route_component,
            index,
            controllers,
            output,
        );
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn parse_route_range(
    tokens: &[Token],
    start: usize,
    end: usize,
    context: &RouteContext,
    route_component: &Component,
    index: &PhpIndex,
    controllers: &BTreeMap<String, String>,
    output: &mut LaravelAnalysis,
) {
    let mut cursor = start;
    while cursor + 3 < end {
        if token(tokens, cursor).is_some_and(is_route_facade)
            && token(tokens, cursor + 1) == Some("::")
        {
            if let Some(call) = parse_call(tokens, cursor + 2) {
                let mut nested_context = context.clone();
                apply_modifier(&mut nested_context, &call.method, &call.args);
                for (name, args) in &call.chains {
                    apply_modifier(&mut nested_context, name, args);
                }
                if call.method == "group" || call.chains.iter().any(|(name, _)| name == "group") {
                    if let Some((body_start, body_end)) = closure_body(tokens, cursor, call.end) {
                        parse_route_range(
                            tokens,
                            body_start,
                            body_end,
                            &nested_context,
                            route_component,
                            index,
                            controllers,
                            output,
                        );
                    }
                } else {
                    emit_route(&call, context, route_component, index, controllers, output);
                }
                cursor = call.end.max(cursor + 1);
                continue;
            }
        }
        cursor += 1;
    }
}

fn emit_route(
    call: &Call,
    base: &RouteContext,
    route_component: &Component,
    index: &PhpIndex,
    controllers: &BTreeMap<String, String>,
    output: &mut LaravelAnalysis,
) {
    let mut context = base.clone();
    let direct_name = chain_string(&call.chains, "name");
    for (name, args) in &call.chains {
        apply_modifier(&mut context, name, args);
    }
    match call.method.as_str() {
        method if ROUTE_METHODS.contains(&method) => {
            let Some(path) = argument_string(&call.args, 0) else {
                return;
            };
            let handler = call.args.get(1);
            add_endpoint(
                method.to_ascii_uppercase(),
                path,
                handler,
                direct_name,
                &context,
                route_component,
                index,
                controllers,
                output,
            );
        }
        "any" => {
            let Some(path) = argument_string(&call.args, 0) else {
                return;
            };
            for method in ["GET", "POST", "PUT", "PATCH", "DELETE", "OPTIONS"] {
                add_endpoint(
                    method.into(),
                    path.clone(),
                    call.args.get(1),
                    None,
                    &context,
                    route_component,
                    index,
                    controllers,
                    output,
                );
            }
        }
        "match" => {
            let Some(path) = argument_string(&call.args, 1) else {
                return;
            };
            for method in argument_list(&call.args, 0) {
                add_endpoint(
                    method.to_ascii_uppercase(),
                    path.clone(),
                    call.args.get(2),
                    None,
                    &context,
                    route_component,
                    index,
                    controllers,
                    output,
                );
            }
        }
        "resource" | "apiResource" => {
            emit_resource(call, &context, route_component, index, controllers, output)
        }
        "resources" | "apiResources" => {
            emit_resources(call, &context, route_component, index, controllers, output)
        }
        _ => {}
    }
}

fn emit_resource(
    call: &Call,
    context: &RouteContext,
    route_component: &Component,
    index: &PhpIndex,
    controllers: &BTreeMap<String, String>,
    output: &mut LaravelAnalysis,
) {
    let Some(resource) = argument_string(&call.args, 0) else {
        return;
    };
    let controller = call
        .args
        .get(1)
        .and_then(|tokens| controller_expression(tokens));
    let api = call.method == "apiResource";
    let (only, except) = resource_filters(&call.chains);
    for (action, method, suffix) in resource_actions(api) {
        if !only.is_empty() && !only.contains(action) || except.contains(action) {
            continue;
        }
        let path = resource_path(&resource, suffix);
        let handler = controller.as_ref().map(|value| {
            vec![
                Token {
                    value: value.clone(),
                    quoted: false,
                },
                Token {
                    value: action.into(),
                    quoted: true,
                },
            ]
        });
        add_endpoint(
            method.into(),
            path,
            handler.as_ref(),
            Some(format!("{}.{}", resource.replace('/', "."), action)),
            context,
            route_component,
            index,
            controllers,
            output,
        );
    }
}

fn emit_resources(
    call: &Call,
    context: &RouteContext,
    route_component: &Component,
    index: &PhpIndex,
    controllers: &BTreeMap<String, String>,
    output: &mut LaravelAnalysis,
) {
    let Some(entries) = call.args.first() else {
        return;
    };
    let mut cursor = 0usize;
    while cursor + 2 < entries.len() {
        if entries[cursor].quoted && token(entries, cursor + 1) == Some("=>") {
            let value_start = cursor + 2;
            let value_end = (value_start..entries.len())
                .find(|index| token(entries, *index) == Some(","))
                .unwrap_or(entries.len());
            let subcall = Call {
                method: if call.method == "apiResources" {
                    "apiResource".into()
                } else {
                    "resource".into()
                },
                args: vec![
                    vec![entries[cursor].clone()],
                    entries[value_start..value_end].to_vec(),
                ],
                chains: call.chains.clone(),
                end: 0,
            };
            emit_resource(
                &subcall,
                context,
                route_component,
                index,
                controllers,
                output,
            );
            cursor = value_end.saturating_add(1);
        } else {
            cursor += 1;
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn add_endpoint(
    method: String,
    path: String,
    handler: Option<&Vec<Token>>,
    conventional_name: Option<String>,
    context: &RouteContext,
    route_component: &Component,
    index: &PhpIndex,
    controllers: &BTreeMap<String, String>,
    output: &mut LaravelAnalysis,
) {
    let full_path = join_path(&context.prefix, &path);
    let (controller_name, action) = handler
        .map(|tokens| parse_handler(tokens))
        .unwrap_or_default();
    let controller_name = controller_name.or_else(|| context.controller.clone());
    let component_id = controller_name
        .as_deref()
        .and_then(|name| resolve_controller(index, controllers, name))
        .unwrap_or_else(|| route_component.id.clone());
    let route_name = conventional_name.map(|value| {
        if context.name_prefix.ends_with(&value) {
            context.name_prefix.clone()
        } else {
            format!("{}{}", context.name_prefix, value)
        }
    });
    let seed = format!(
        "{}\0{}\0{}\0{}",
        route_component.id,
        method,
        full_path,
        action.clone().unwrap_or_default()
    );
    let id = format!("laravel:route:{}", stable_hash(&seed));
    output.entrypoints.push(Entrypoint {
        id: id.clone(),
        name: format!("{method} {full_path}"),
        kind: EntrypointKind::HttpEndpoint,
        component_id: component_id.clone(),
        file: route_component.file.clone(),
    });
    let evidence = vec![evidence_item(
        EvidenceKind::Source,
        Path::new(&route_component.file),
        "Laravel Route facade declaration",
        100,
    )];
    output.entrypoint_metadata.push(EntrypointMetadata {
        entrypoint_id: id,
        method: Some(method),
        path: Some(full_path),
        route_name,
        domain: context.domain.clone(),
        middleware: context.middleware.clone(),
        controller: controller_name.clone(),
        action: action.clone(),
        evidence: evidence.clone(),
        confidence: 100,
    });
    if component_id != route_component.id {
        output.dependencies.push(Dependency {
            source_id: route_component.id.clone(),
            target_id: component_id.clone(),
            kind: DependencyKind::Calls,
        });
        output.dependency_metadata.push(DependencyMetadata {
            source_id: route_component.id.clone(),
            target_id: component_id,
            kind: "route-handler".into(),
            evidence,
            confidence: 100,
        });
    }
}

fn resource_actions(api: bool) -> Vec<(&'static str, &'static str, &'static str)> {
    let mut result = vec![
        ("index", "GET", ""),
        ("store", "POST", ""),
        ("show", "GET", "/{resource}"),
        ("update", "PUT", "/{resource}"),
        ("destroy", "DELETE", "/{resource}"),
    ];
    if !api {
        result.extend([
            ("create", "GET", "/create"),
            ("edit", "GET", "/{resource}/edit"),
        ]);
    }
    result
}

fn resource_path(resource: &str, suffix: &str) -> String {
    let parameter = resource
        .rsplit(['.', '/'])
        .next()
        .unwrap_or("resource")
        .trim_end_matches('s');
    format!(
        "/{}{}",
        resource.replace('.', "/"),
        suffix.replace("{resource}", &format!("{{{parameter}}}"))
    )
}

fn resource_filters(chains: &[(String, Vec<Vec<Token>>)]) -> (BTreeSet<String>, BTreeSet<String>) {
    let only = chains
        .iter()
        .find(|(name, _)| name == "only")
        .map(|(_, args)| flatten_strings(args))
        .unwrap_or_default();
    let except = chains
        .iter()
        .find(|(name, _)| name == "except")
        .map(|(_, args)| flatten_strings(args))
        .unwrap_or_default();
    (only, except)
}

fn apply_modifier(context: &mut RouteContext, name: &str, args: &[Vec<Token>]) {
    match name {
        "prefix" => {
            if let Some(value) = argument_string(args, 0) {
                context.prefix = join_path(&context.prefix, &value);
            }
        }
        "middleware" => context.middleware.extend(flatten_strings(args)),
        "name" => {
            if let Some(value) = argument_string(args, 0) {
                context.name_prefix.push_str(&value);
            }
        }
        "domain" => context.domain = argument_string(args, 0),
        "controller" => {
            context.controller = args
                .first()
                .and_then(|tokens| controller_expression(tokens))
        }
        _ => {}
    }
}

fn parse_call(tokens: &[Token], method_index: usize) -> Option<Call> {
    let method = token(tokens, method_index)?.to_string();
    if token(tokens, method_index + 1) != Some("(") {
        return None;
    }
    let close = matching(tokens, method_index + 1, "(", ")")?;
    let args = split_token_arguments(&tokens[method_index + 2..close]);
    let mut chains = Vec::new();
    let mut cursor = close + 1;
    while token(tokens, cursor) == Some("->") {
        let name = token(tokens, cursor + 1)?.to_string();
        if token(tokens, cursor + 2) != Some("(") {
            break;
        }
        let chain_end = matching(tokens, cursor + 2, "(", ")")?;
        chains.push((name, split_token_arguments(&tokens[cursor + 3..chain_end])));
        cursor = chain_end + 1;
    }
    Some(Call {
        method,
        args,
        chains,
        end: cursor,
    })
}

fn split_token_arguments(tokens: &[Token]) -> Vec<Vec<Token>> {
    let mut result = Vec::new();
    let mut current = Vec::new();
    let mut depth = 0i32;
    for item in tokens {
        match item.value.as_str() {
            "(" | "[" | "{" => depth += 1,
            ")" | "]" | "}" => depth -= 1,
            "," if depth == 0 => {
                result.push(current);
                current = Vec::new();
                continue;
            }
            _ => {}
        }
        current.push(item.clone());
    }
    if !current.is_empty() {
        result.push(current);
    }
    result
}

fn closure_body(tokens: &[Token], start: usize, end: usize) -> Option<(usize, usize)> {
    let open = (start..end).find(|index| token(tokens, *index) == Some("{"))?;
    let close = matching(tokens, open, "{", "}")?;
    Some((open + 1, close))
}

fn parse_handler(tokens: &[Token]) -> (Option<String>, Option<String>) {
    let strings: Vec<String> = tokens
        .iter()
        .filter(|item| item.quoted)
        .map(|item| item.value.clone())
        .collect();
    if let Some(value) = strings.first() {
        if let Some((controller, action)) = value.split_once('@') {
            return (Some(controller.into()), Some(action.into()));
        }
        if value.ends_with("Controller") {
            return (Some(value.clone()), Some("__invoke".into()));
        }
    }
    let controller = controller_expression(tokens);
    if controller.is_some() {
        return (controller, strings.last().cloned());
    }
    (None, strings.first().cloned())
}

fn controller_expression(tokens: &[Token]) -> Option<String> {
    for window in tokens.windows(3) {
        if window[1].value == "::" && window[2].value == "class" {
            return Some(window[0].value.clone());
        }
    }
    None
}

fn resolve_controller(
    index: &PhpIndex,
    controllers: &BTreeMap<String, String>,
    name: &str,
) -> Option<String> {
    let clean = name.trim_matches('\\');
    if let Some(value) = controllers.get(&clean.to_ascii_lowercase()) {
        return Some(value.clone());
    }
    index
        .types
        .iter()
        .find(|item| {
            item.name.eq_ignore_ascii_case(clean)
                || item.qualified_name.ends_with(&format!("\\{clean}"))
        })
        .map(|item| item.component_id.clone())
}

fn argument_string(args: &[Vec<Token>], index: usize) -> Option<String> {
    args.get(index)?
        .iter()
        .find(|item| item.quoted)
        .map(|item| item.value.clone())
}

fn argument_list(args: &[Vec<Token>], index: usize) -> Vec<String> {
    args.get(index)
        .map(|tokens| {
            tokens
                .iter()
                .filter(|item| item.quoted)
                .map(|item| item.value.clone())
                .collect()
        })
        .unwrap_or_default()
}

fn flatten_strings(args: &[Vec<Token>]) -> BTreeSet<String> {
    args.iter()
        .flat_map(|tokens| tokens.iter())
        .filter(|item| item.quoted)
        .map(|item| item.value.clone())
        .collect()
}

fn chain_string(chains: &[(String, Vec<Vec<Token>>)], name: &str) -> Option<String> {
    chains
        .iter()
        .find(|(candidate, _)| candidate == name)
        .and_then(|(_, args)| argument_string(args, 0))
}

fn join_path(base: &str, path: &str) -> String {
    let segments = [base, path]
        .into_iter()
        .flat_map(|value| value.split('/'))
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    format!("/{}", segments.join("/"))
}

fn matching(tokens: &[Token], start: usize, open: &str, close: &str) -> Option<usize> {
    let mut depth = 0usize;
    for (index, item) in tokens.iter().enumerate().skip(start) {
        if item.value == open {
            depth += 1;
        }
        if item.value == close {
            depth = depth.saturating_sub(1);
            if depth == 0 {
                return Some(index);
            }
        }
    }
    None
}

fn token(tokens: &[Token], index: usize) -> Option<&str> {
    tokens.get(index).map(|item| item.value.as_str())
}

fn is_route_facade(value: &str) -> bool {
    value == "Route" || value.trim_end_matches('\\').ends_with("\\Route")
}

fn tokenize(content: &str) -> Vec<Token> {
    let chars: Vec<char> = content.chars().collect();
    let mut result = Vec::new();
    let mut index = 0usize;
    while index < chars.len() {
        let current = chars[index];
        let next = chars.get(index + 1).copied();
        if current.is_whitespace() {
            index += 1;
            continue;
        }
        if (current == '/' && next == Some('/')) || current == '#' {
            while index < chars.len() && chars[index] != '\n' {
                index += 1;
            }
            continue;
        }
        if current == '/' && next == Some('*') {
            index += 2;
            while index + 1 < chars.len() && !(chars[index] == '*' && chars[index + 1] == '/') {
                index += 1;
            }
            index = (index + 2).min(chars.len());
            continue;
        }
        if current == '\'' || current == '"' {
            let quote = current;
            index += 1;
            let mut value = String::new();
            let mut escaped = false;
            while index < chars.len() {
                let ch = chars[index];
                index += 1;
                if escaped {
                    value.push(ch);
                    escaped = false;
                } else if ch == '\\' {
                    escaped = true;
                } else if ch == quote {
                    break;
                } else {
                    value.push(ch);
                }
            }
            result.push(Token {
                value,
                quoted: true,
            });
            continue;
        }
        if current.is_alphanumeric() || matches!(current, '_' | '$' | '\\') {
            let start = index;
            index += 1;
            while index < chars.len()
                && (chars[index].is_alphanumeric() || matches!(chars[index], '_' | '$' | '\\'))
            {
                index += 1;
            }
            result.push(Token {
                value: chars[start..index].iter().collect(),
                quoted: false,
            });
            continue;
        }
        let pair = next.map(|value| format!("{current}{value}"));
        if matches!(pair.as_deref(), Some("::" | "->" | "=>")) {
            result.push(Token {
                value: pair.unwrap(),
                quoted: false,
            });
            index += 2;
        } else {
            result.push(Token {
                value: current.to_string(),
                quoted: false,
            });
            index += 1;
        }
    }
    result
}

fn runtime_requirements(root: &Path) -> Vec<RuntimeRequirement> {
    let composer = read_source_text(root.join("composer.json")).unwrap_or_default();
    let version = json_string_after(&composer, "php");
    vec![
        RuntimeRequirement {
            name: "PHP".into(),
            executable: "php".into(),
            version_hint: version,
            required_by: "composer.json".into(),
            confidence: 100,
        },
        RuntimeRequirement {
            name: "Composer".into(),
            executable: "composer".into(),
            version_hint: None,
            required_by: "composer.json".into(),
            confidence: 100,
        },
    ]
}

fn json_string_after(content: &str, key: &str) -> Option<String> {
    let marker = format!("\"{key}\"");
    let rest = content.split_once(&marker)?.1;
    let value = rest.split_once(':')?.1.trim_start();
    let value = value.strip_prefix('"')?;
    Some(value.split('"').next()?.to_string())
}

fn collect_php(root: &Path, path: &Path, files: &mut Vec<PathBuf>) -> io::Result<()> {
    if ScanPolicy::default().is_excluded_from(root, path) {
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
        if path.extension().and_then(|value| value.to_str()) == Some("php")
            && !path
                .file_name()
                .and_then(|value| value.to_str())
                .is_some_and(|name| name.ends_with(".blade.php"))
        {
            files.push(path.to_path_buf());
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
            collect_php(root, &entry.path(), files)?;
        }
    }
    Ok(())
}

fn evidence_item(kind: EvidenceKind, source: &Path, detail: &str, confidence: u8) -> Evidence {
    Evidence {
        kind,
        source: source.to_string_lossy().into_owned(),
        detail: detail.into(),
        confidence,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expands_api_resources_and_filters() {
        let tokens = tokenize(
            "Route::apiResource('users', UserController::class)->only(['index', 'show']);",
        );
        let call = parse_call(&tokens, 2).unwrap();
        assert_eq!(call.method, "apiResource");
        let (only, _) = resource_filters(&call.chains);
        assert_eq!(only, BTreeSet::from(["index".into(), "show".into()]));
    }

    #[test]
    fn nested_path_is_normalized() {
        assert_eq!(join_path("/api/v1/", "/users"), "/api/v1/users");
        assert_eq!(
            resource_path("posts.comments", "/{resource}"),
            "/posts/comments/{comment}"
        );
    }
}
