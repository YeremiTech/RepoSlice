use reposlice_core::{
    read_source_text, CrossProjectDependency, CrossProjectDependencyKind, Dependency, ProjectModel,
    ProjectUnit, WorkspaceModel, DEFAULT_MAX_SOURCE_SIZE,
};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
struct IndexedHttpEndpoint {
    repository_id: String,
    project_unit_id: String,
    entrypoint_id: String,
    component_id: String,
    path: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScopedDependency {
    pub source_project_unit_id: String,
    pub source_component_id: String,
    pub target_project_unit_id: String,
    pub target_component_id: String,
    pub kind: String,
    pub evidence: String,
    pub confidence: u8,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WorkspaceDependencySlice {
    pub nodes: BTreeSet<String>,
    pub dependencies: Vec<ScopedDependency>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DependencySlice {
    pub nodes: BTreeSet<String>,
    pub dependencies: Vec<Dependency>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GraphIntegrityReport {
    pub missing_sources: BTreeSet<String>,
    pub missing_targets: BTreeSet<String>,
    pub self_dependencies: Vec<Dependency>,
    pub cycles: Vec<Vec<String>>,
}

impl GraphIntegrityReport {
    pub fn passed(&self) -> bool {
        self.missing_sources.is_empty() && self.missing_targets.is_empty()
    }

    pub fn is_clean(&self) -> bool {
        self.passed() && self.self_dependencies.is_empty() && self.cycles.is_empty()
    }

    pub fn warning_count(&self) -> usize {
        self.self_dependencies.len() + self.cycles.len()
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WorkspaceGraphIntegrityReport {
    pub missing_source_units: BTreeSet<String>,
    pub missing_target_units: BTreeSet<String>,
    pub missing_source_components: BTreeSet<String>,
    pub missing_target_components: BTreeSet<String>,
    pub missing_target_entrypoints: BTreeSet<String>,
}

impl WorkspaceGraphIntegrityReport {
    pub fn passed(&self) -> bool {
        self.missing_source_units.is_empty()
            && self.missing_target_units.is_empty()
            && self.missing_source_components.is_empty()
            && self.missing_target_components.is_empty()
            && self.missing_target_entrypoints.is_empty()
    }

    pub fn error_count(&self) -> usize {
        self.missing_source_units.len()
            + self.missing_target_units.len()
            + self.missing_source_components.len()
            + self.missing_target_components.len()
            + self.missing_target_entrypoints.len()
    }
}

pub fn validate_workspace_graph(workspace: &WorkspaceModel) -> WorkspaceGraphIntegrityReport {
    let mut report = WorkspaceGraphIntegrityReport::default();
    for dependency in &workspace.cross_project_dependencies {
        let source_unit = workspace
            .repositories
            .iter()
            .find(|repository| repository.id == dependency.source_repository_id)
            .and_then(|repository| {
                repository
                    .project_units
                    .iter()
                    .find(|unit| unit.id == dependency.source_project_unit_id)
            });
        let target_unit = workspace
            .repositories
            .iter()
            .find(|repository| repository.id == dependency.target_repository_id)
            .and_then(|repository| {
                repository
                    .project_units
                    .iter()
                    .find(|unit| unit.id == dependency.target_project_unit_id)
            });

        let Some(source_unit) = source_unit else {
            report
                .missing_source_units
                .insert(dependency.source_project_unit_id.clone());
            continue;
        };
        if !source_unit
            .model
            .components
            .iter()
            .any(|component| component.id == dependency.source_component_id)
        {
            report.missing_source_components.insert(format!(
                "{}|{}",
                dependency.source_project_unit_id, dependency.source_component_id
            ));
        }

        let Some(target_unit) = target_unit else {
            report
                .missing_target_units
                .insert(dependency.target_project_unit_id.clone());
            continue;
        };
        if let Some(component_id) = &dependency.target_component_id {
            if !target_unit
                .model
                .components
                .iter()
                .any(|component| component.id == *component_id)
            {
                report.missing_target_components.insert(format!(
                    "{}|{}",
                    dependency.target_project_unit_id, component_id
                ));
            }
        }
        if let Some(entrypoint_id) = &dependency.target_entrypoint_id {
            if !target_unit
                .model
                .entrypoints
                .iter()
                .any(|entrypoint| entrypoint.id == *entrypoint_id)
            {
                report.missing_target_entrypoints.insert(format!(
                    "{}|{}",
                    dependency.target_project_unit_id, entrypoint_id
                ));
            }
        }
    }
    report
}

pub fn validate_dependency_graph(model: &ProjectModel) -> GraphIntegrityReport {
    let component_ids = model
        .components
        .iter()
        .map(|component| component.id.as_str())
        .collect::<BTreeSet<_>>();
    let mut report = GraphIntegrityReport::default();
    for dependency in &model.dependencies {
        if !component_ids.contains(dependency.source_id.as_str()) {
            report.missing_sources.insert(dependency.source_id.clone());
        }
        if !component_ids.contains(dependency.target_id.as_str()) {
            report.missing_targets.insert(dependency.target_id.clone());
        }
        if dependency.source_id == dependency.target_id {
            report.self_dependencies.push(dependency.clone());
        }
    }
    report.cycles = dependency_cycles(model);
    report
}

pub fn dependency_cycles(model: &ProjectModel) -> Vec<Vec<String>> {
    let adjacency = model.dependencies.iter().fold(
        BTreeMap::<String, Vec<String>>::new(),
        |mut index, dependency| {
            if dependency.source_id != dependency.target_id {
                index
                    .entry(dependency.source_id.clone())
                    .or_default()
                    .push(dependency.target_id.clone());
            }
            index
        },
    );
    let nodes = model
        .components
        .iter()
        .map(|component| component.id.clone())
        .collect::<BTreeSet<_>>();
    let mut color = BTreeMap::<String, u8>::new();
    let mut stack = Vec::<String>::new();
    let mut cycles = BTreeSet::<Vec<String>>::new();

    fn visit(
        node: &str,
        adjacency: &BTreeMap<String, Vec<String>>,
        nodes: &BTreeSet<String>,
        color: &mut BTreeMap<String, u8>,
        stack: &mut Vec<String>,
        cycles: &mut BTreeSet<Vec<String>>,
    ) {
        color.insert(node.to_string(), 1);
        stack.push(node.to_string());
        for target in adjacency.get(node).into_iter().flatten() {
            if !nodes.contains(target) {
                continue;
            }
            match color.get(target).copied().unwrap_or(0) {
                0 => visit(target, adjacency, nodes, color, stack, cycles),
                1 => {
                    if let Some(start) = stack.iter().position(|item| item == target) {
                        let mut cycle = stack[start..].to_vec();
                        if cycle.len() > 1 {
                            canonicalize_cycle(&mut cycle);
                            cycles.insert(cycle);
                        }
                    }
                }
                _ => {}
            }
        }
        stack.pop();
        color.insert(node.to_string(), 2);
    }

    fn canonicalize_cycle(cycle: &mut [String]) {
        if cycle.is_empty() {
            return;
        }
        if let Some((index, _)) = cycle.iter().enumerate().min_by(|(_, a), (_, b)| a.cmp(b)) {
            cycle.rotate_left(index);
        }
    }

    for node in &nodes {
        if color.get(node).copied().unwrap_or(0) == 0 {
            visit(
                node,
                &adjacency,
                &nodes,
                &mut color,
                &mut stack,
                &mut cycles,
            );
        }
    }
    cycles.into_iter().collect()
}

pub fn dependency_closure(model: &ProjectModel, start_id: &str) -> BTreeSet<String> {
    let adjacency = model.dependencies.iter().fold(
        BTreeMap::<&str, Vec<&str>>::new(),
        |mut index, dependency| {
            index
                .entry(dependency.source_id.as_str())
                .or_default()
                .push(dependency.target_id.as_str());
            index
        },
    );
    let mut result = BTreeSet::new();
    let mut queue = VecDeque::from([start_id.to_string()]);

    while let Some(current) = queue.pop_front() {
        if !result.insert(current.clone()) {
            continue;
        }

        if let Some(targets) = adjacency.get(current.as_str()) {
            for target in targets {
                if !result.contains(*target) {
                    queue.push_back((*target).to_string());
                }
            }
        }
    }

    result
}

pub fn dependency_slice(model: &ProjectModel, start_id: &str) -> DependencySlice {
    let nodes = dependency_closure(model, start_id);
    let dependencies = model
        .dependencies
        .iter()
        .filter(|dependency| {
            nodes.contains(&dependency.source_id) && nodes.contains(&dependency.target_id)
        })
        .cloned()
        .collect();
    DependencySlice {
        nodes,
        dependencies,
    }
}

pub fn reverse_dependency_closure(model: &ProjectModel, start_id: &str) -> BTreeSet<String> {
    impact_slice(model, start_id).nodes
}

pub fn impact_slice(model: &ProjectModel, start_id: &str) -> DependencySlice {
    let reverse = model.dependencies.iter().fold(
        BTreeMap::<&str, Vec<&Dependency>>::new(),
        |mut index, dependency| {
            index
                .entry(dependency.target_id.as_str())
                .or_default()
                .push(dependency);
            index
        },
    );
    let mut result = DependencySlice::default();
    let mut queue = VecDeque::from([start_id.to_string()]);
    while let Some(current) = queue.pop_front() {
        if !result.nodes.insert(current.clone()) {
            continue;
        }
        for dependency in reverse.get(current.as_str()).into_iter().flatten() {
            result.dependencies.push((*dependency).clone());
            if !result.nodes.contains(&dependency.source_id) {
                queue.push_back(dependency.source_id.clone());
            }
        }
    }
    result.dependencies.sort_by(|left, right| {
        (&left.source_id, &left.target_id, left.kind.as_str()).cmp(&(
            &right.source_id,
            &right.target_id,
            right.kind.as_str(),
        ))
    });
    result.dependencies.dedup();
    result
}

pub fn normalize_http_path(value: &str) -> String {
    let without_query = value.split(['?', '#']).next().unwrap_or(value).trim();
    let path = if let Some(scheme) = without_query.find("://") {
        without_query[scheme + 3..]
            .find('/')
            .map(|index| &without_query[scheme + 3 + index..])
            .unwrap_or("/")
    } else {
        without_query
    };
    let mut segments = Vec::new();
    for segment in path.split('/').filter(|segment| !segment.is_empty()) {
        let dynamic = is_dynamic_segment(segment);
        segments.push(if dynamic {
            "{}".to_string()
        } else {
            segment.to_ascii_lowercase()
        });
    }
    format!("/{}", segments.join("/"))
}

pub fn match_http_dependencies(workspace: &WorkspaceModel) -> Vec<CrossProjectDependency> {
    let mut endpoint_index: BTreeMap<String, Vec<IndexedHttpEndpoint>> = BTreeMap::new();
    let internal_hosts = workspace
        .repositories
        .iter()
        .flat_map(|repository| {
            std::iter::once(repository.name.as_str()).chain(
                repository
                    .project_units
                    .iter()
                    .map(|unit| unit.name.as_str()),
            )
        })
        .flat_map(host_aliases)
        .collect::<BTreeSet<_>>();
    for repository in &workspace.repositories {
        for unit in &repository.project_units {
            for entrypoint in &unit.model.entrypoints {
                if entrypoint.kind.as_str() != "http" {
                    continue;
                }
                let Some((method, path)) = entrypoint.name.split_once(' ') else {
                    continue;
                };
                endpoint_index
                    .entry(method.to_ascii_uppercase())
                    .or_default()
                    .push(IndexedHttpEndpoint {
                        repository_id: repository.id.clone(),
                        project_unit_id: unit.id.clone(),
                        entrypoint_id: entrypoint.id.clone(),
                        component_id: entrypoint.component_id.clone(),
                        path: path.to_string(),
                    });
            }
        }
    }

    let mut result = Vec::new();
    for source_repository in &workspace.repositories {
        for source_unit in &source_repository.project_units {
            for call in &source_unit.http_calls {
                if call.origin.as_deref().is_some_and(|origin| {
                    origin != "dynamic://unresolved"
                        && !is_local_origin(origin)
                        && !internal_hosts.contains(&origin_host(origin))
                }) {
                    continue;
                }
                let mut candidates = Vec::new();
                let method = call.method.to_ascii_uppercase();
                for key in [method.as_str(), "ANY"] {
                    for endpoint in endpoint_index.get(key).into_iter().flatten() {
                        // This function only models cross-project relationships.
                        // Calls within a project unit belong to its internal graph.
                        if source_unit.id == endpoint.project_unit_id {
                            continue;
                        }
                        if http_paths_match(&endpoint.path, &call.path) {
                            let mut confidence = if canonical_http_path(&endpoint.path)
                                == canonical_http_path(&call.path)
                            {
                                100
                            } else {
                                90
                            };
                            if let Some(origin) = call.origin.as_deref() {
                                confidence = confidence.min(if origin == "dynamic://unresolved" {
                                    70
                                } else if is_local_origin(origin) {
                                    85
                                } else {
                                    80
                                });
                            }
                            candidates.push((endpoint, confidence));
                        }
                    }
                }
                candidates.sort_by(|(left, _), (right, _)| {
                    (
                        &left.repository_id,
                        &left.project_unit_id,
                        &left.entrypoint_id,
                    )
                        .cmp(&(
                            &right.repository_id,
                            &right.project_unit_id,
                            &right.entrypoint_id,
                        ))
                });
                candidates.dedup_by(|(left, _), (right, _)| {
                    left.repository_id == right.repository_id
                        && left.project_unit_id == right.project_unit_id
                        && left.entrypoint_id == right.entrypoint_id
                });
                if candidates.len() == 1 {
                    let (endpoint, confidence) = candidates[0];
                    result.push(CrossProjectDependency {
                        source_repository_id: source_repository.id.clone(),
                        source_project_unit_id: source_unit.id.clone(),
                        source_component_id: call.component_id.clone(),
                        target_repository_id: endpoint.repository_id.clone(),
                        target_project_unit_id: endpoint.project_unit_id.clone(),
                        target_entrypoint_id: Some(endpoint.entrypoint_id.clone()),
                        target_component_id: Some(endpoint.component_id.clone()),
                        kind: CrossProjectDependencyKind::Http,
                        evidence: call.evidence.clone(),
                        confidence,
                    });
                }
            }
        }
    }
    result
}

pub fn match_cross_project_dependencies(workspace: &WorkspaceModel) -> Vec<CrossProjectDependency> {
    let mut dependencies = match_http_dependencies(workspace);
    dependencies.extend(match_workspace_package_dependencies(workspace));
    dependencies.sort_by(|left, right| {
        (
            &left.source_project_unit_id,
            &left.source_component_id,
            &left.target_project_unit_id,
            &left.target_component_id,
            left.kind.as_str(),
        )
            .cmp(&(
                &right.source_project_unit_id,
                &right.source_component_id,
                &right.target_project_unit_id,
                &right.target_component_id,
                right.kind.as_str(),
            ))
    });
    dependencies.dedup_by(|left, right| {
        left.source_project_unit_id == right.source_project_unit_id
            && left.source_component_id == right.source_component_id
            && left.target_project_unit_id == right.target_project_unit_id
            && left.target_component_id == right.target_component_id
            && left.kind == right.kind
    });
    dependencies
}

pub fn match_workspace_package_dependencies(
    workspace: &WorkspaceModel,
) -> Vec<CrossProjectDependency> {
    #[derive(Clone)]
    struct PackageTarget {
        repository_id: String,
        project_unit_id: String,
        name: String,
        component_id: String,
    }

    let targets = workspace
        .repositories
        .iter()
        .flat_map(|repository| {
            repository.project_units.iter().filter_map(|unit| {
                let (name, _) = package_identity(Path::new(&unit.root))?;
                let component_id = representative_component(unit)?.to_string();
                Some(PackageTarget {
                    repository_id: repository.id.clone(),
                    project_unit_id: unit.id.clone(),
                    name,
                    component_id,
                })
            })
        })
        .collect::<Vec<_>>();

    let target_name_counts =
        targets
            .iter()
            .fold(BTreeMap::<&str, usize>::new(), |mut counts, target| {
                *counts.entry(target.name.as_str()).or_default() += 1;
                counts
            });

    let mut result = Vec::new();
    for repository in &workspace.repositories {
        for unit in &repository.project_units {
            for target in &targets {
                if target_name_counts
                    .get(target.name.as_str())
                    .copied()
                    .unwrap_or(0)
                    != 1
                {
                    continue;
                }
                if repository.id == target.repository_id && unit.id == target.project_unit_id {
                    continue;
                }
                let Some(manifest) =
                    declared_workspace_dependency(Path::new(&unit.root), &target.name)
                else {
                    continue;
                };
                let imported_by = importing_component(unit, &target.name);
                let source_component_id = imported_by
                    .clone()
                    .or_else(|| component_for_file(unit, &manifest))
                    .or_else(|| representative_component(unit).map(str::to_string));
                let Some(source_component_id) = source_component_id else {
                    continue;
                };
                result.push(CrossProjectDependency {
                    source_repository_id: repository.id.clone(),
                    source_project_unit_id: unit.id.clone(),
                    source_component_id,
                    target_repository_id: target.repository_id.clone(),
                    target_project_unit_id: target.project_unit_id.clone(),
                    target_entrypoint_id: None,
                    target_component_id: Some(target.component_id.clone()),
                    kind: CrossProjectDependencyKind::WorkspacePackage,
                    evidence: format!(
                        "Workspace package {} declared in {}",
                        target.name,
                        manifest.to_string_lossy()
                    ),
                    confidence: if imported_by.is_some() { 100 } else { 90 },
                });
            }
        }
    }
    result
}

fn package_identity(root: &Path) -> Option<(String, PathBuf)> {
    for name in ["package.json", "composer.json"] {
        let path = root.join(name);
        if let Ok(content) = read_source_text(&path) {
            if let Some(value) = json_string_field(&content, "name") {
                return Some((value, path));
            }
        }
    }
    for (name, section) in [("Cargo.toml", "package"), ("pyproject.toml", "project")] {
        let path = root.join(name);
        if !path.is_file() {
            continue;
        }
        let Ok(content) = read_source_text(&path) else {
            continue;
        };
        if let Some(value) = toml_section_value(&content, section, "name") {
            return Some((value, path));
        }
    }
    let pom = root.join("pom.xml");
    if pom.is_file() {
        let content = read_source_text(&pom).ok()?;
        if let Some(value) = xml_tag_value(&content, "artifactId") {
            return Some((value, pom));
        }
    }
    None
}

fn declared_workspace_dependency(root: &Path, target_name: &str) -> Option<PathBuf> {
    let package = root.join("package.json");
    if let Ok(content) = read_source_text(&package) {
        if [
            "dependencies",
            "devDependencies",
            "peerDependencies",
            "optionalDependencies",
        ]
        .iter()
        .any(|section| json_object_contains_key(&content, section, target_name))
        {
            return Some(package);
        }
    }

    let composer = root.join("composer.json");
    if let Ok(content) = read_source_text(&composer) {
        if ["require", "require-dev"]
            .iter()
            .any(|section| json_object_contains_key(&content, section, target_name))
        {
            return Some(composer);
        }
    }

    let cargo = root.join("Cargo.toml");
    if let Ok(content) = read_source_text(&cargo) {
        if toml_dependency_contains(&content, target_name) {
            return Some(cargo);
        }
    }

    let pyproject = root.join("pyproject.toml");
    if let Ok(content) = read_source_text(&pyproject) {
        if toml_dependency_contains(&content, target_name)
            || python_project_dependency_contains(&content, target_name)
        {
            return Some(pyproject);
        }
    }

    let pom = root.join("pom.xml");
    if let Ok(content) = read_source_text(&pom) {
        if xml_dependency_contains(&content, target_name) {
            return Some(pom);
        }
    }
    None
}

fn json_object_contains_key(content: &str, section: &str, key: &str) -> bool {
    let marker = format!("\"{section}\"");
    let Some((_, rest)) = content.split_once(&marker) else {
        return false;
    };
    let Some(colon) = rest.find(':') else {
        return false;
    };
    let rest = &rest[colon + 1..];
    let Some(open) = rest.find('{') else {
        return false;
    };
    let bytes = rest.as_bytes();
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    let mut close = None;
    for (offset, byte) in bytes[open..].iter().enumerate() {
        if in_string {
            if escaped {
                escaped = false;
            } else if *byte == b'\\' {
                escaped = true;
            } else if *byte == b'"' {
                in_string = false;
            }
            continue;
        }
        match *byte {
            b'"' => in_string = true,
            b'{' => depth += 1,
            b'}' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    close = Some(open + offset);
                    break;
                }
            }
            _ => {}
        }
    }
    let Some(close) = close else {
        return false;
    };
    let object = &rest[open + 1..close];
    object.contains(&format!("\"{key}\""))
}

fn toml_dependency_contains(content: &str, target_name: &str) -> bool {
    let mut dependency_section = false;
    for line in content.lines() {
        let line = line.trim();
        if line.starts_with('[') && line.ends_with(']') {
            let section = line.trim_matches(['[', ']']).to_ascii_lowercase();
            dependency_section = section.contains("dependencies");
            continue;
        }
        if dependency_section {
            let Some((name, _)) = line.split_once('=') else {
                continue;
            };
            if name.trim().trim_matches(['\'', '"']) == target_name {
                return true;
            }
        }
    }
    false
}

fn python_project_dependency_contains(content: &str, target_name: &str) -> bool {
    let normalized = target_name.to_ascii_lowercase().replace('_', "-");
    let mut in_project = false;
    let mut in_dependencies = false;
    for line in content.lines() {
        let line = line.trim();
        if line.starts_with('[') && line.ends_with(']') {
            in_project = line == "[project]";
            in_dependencies = false;
            continue;
        }
        if !in_project {
            continue;
        }
        if line.starts_with("dependencies") && line.contains('[') {
            in_dependencies = true;
        }
        if in_dependencies {
            let lowercase = line.to_ascii_lowercase().replace('_', "-");
            if lowercase.contains(&format!("\"{normalized}"))
                || lowercase.contains(&format!("'{normalized}"))
            {
                return true;
            }
            if line.contains(']') {
                in_dependencies = false;
            }
        }
    }
    false
}

fn xml_dependency_contains(content: &str, target_name: &str) -> bool {
    let mut rest = content;
    while let Some((_, after_open)) = rest.split_once("<dependency>") {
        let Some((block, after_close)) = after_open.split_once("</dependency>") else {
            break;
        };
        if xml_tag_value(block, "artifactId").as_deref() == Some(target_name) {
            return true;
        }
        rest = after_close;
    }
    false
}

fn importing_component(unit: &ProjectUnit, target_name: &str) -> Option<String> {
    let files = unit
        .model
        .components
        .iter()
        .map(|component| component.file.as_str())
        .collect::<BTreeSet<_>>();
    for file in files.iter() {
        let path = Path::new(file);
        let Ok(metadata) = fs::metadata(path) else {
            continue;
        };
        if metadata.len() > DEFAULT_MAX_SOURCE_SIZE {
            continue;
        }
        let Ok(content) = read_source_text(path) else {
            continue;
        };
        if !content.contains(target_name) {
            continue;
        }
        let import_evidence = content.lines().any(|line| {
            let line = line.trim();
            line.contains(target_name)
                && (line.starts_with("import ")
                    || line.starts_with("use ")
                    || line.contains(" from ")
                    || line.contains("require(")
                    || line.contains("require "))
        });
        if import_evidence {
            if let Some(component) = unit
                .model
                .components
                .iter()
                .find(|component| component.file == *file && component.kind.as_str() != "file")
                .or_else(|| {
                    unit.model
                        .components
                        .iter()
                        .find(|component| component.file == *file)
                })
            {
                return Some(component.id.clone());
            }
        }
    }
    None
}

fn representative_component(unit: &ProjectUnit) -> Option<&str> {
    unit.model
        .entrypoints
        .first()
        .map(|entrypoint| entrypoint.component_id.as_str())
        .or_else(|| {
            unit.model
                .components
                .iter()
                .find(|component| component.kind.as_str() != "file")
                .map(|component| component.id.as_str())
        })
        .or_else(|| {
            unit.model
                .components
                .first()
                .map(|component| component.id.as_str())
        })
}

fn component_for_file(unit: &ProjectUnit, file: &Path) -> Option<String> {
    let canonical = fs::canonicalize(file).unwrap_or_else(|_| file.to_path_buf());
    unit.model
        .components
        .iter()
        .find(|component| {
            fs::canonicalize(&component.file).unwrap_or_else(|_| PathBuf::from(&component.file))
                == canonical
        })
        .map(|component| component.id.clone())
}

fn json_string_field(content: &str, key: &str) -> Option<String> {
    let marker = format!("\"{key}\"");
    let rest = content
        .split_once(&marker)?
        .1
        .split_once(':')?
        .1
        .trim_start();
    let rest = rest.strip_prefix('"')?;
    Some(rest.split('"').next()?.to_string())
}

fn toml_section_value(content: &str, section: &str, key: &str) -> Option<String> {
    let mut active = false;
    for line in content.lines() {
        let line = line.trim();
        if line.starts_with('[') && line.ends_with(']') {
            active = line == format!("[{section}]");
            continue;
        }
        if active {
            if let Some(value) = line.strip_prefix(&format!("{key} =")) {
                return Some(value.trim().trim_matches(['\'', '"']).to_string());
            }
        }
    }
    None
}

fn xml_tag_value(content: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let rest = content.split_once(&open)?.1;
    Some(rest.split_once(&close)?.0.trim().to_string())
}

fn resolve_cross_target_component(
    dependency: &CrossProjectDependency,
    unit: &ProjectUnit,
) -> Option<String> {
    dependency.target_component_id.clone().or_else(|| {
        dependency
            .target_entrypoint_id
            .as_deref()
            .and_then(|entrypoint_id| {
                unit.model
                    .entrypoints
                    .iter()
                    .find(|entrypoint| entrypoint.id == entrypoint_id)
                    .map(|entrypoint| entrypoint.component_id.clone())
            })
    })
}

fn canonical_http_path(value: &str) -> String {
    normalize_http_path(value)
}

fn http_paths_match(endpoint: &str, call: &str) -> bool {
    let endpoint = path_segments(endpoint);
    let call = path_segments(call);
    let mut index = 0;
    while index < endpoint.len() {
        let expected = endpoint[index];
        if is_catch_all_segment(expected) && index + 1 == endpoint.len() {
            return call.len() >= index;
        }
        let Some(actual) = call.get(index) else {
            return false;
        };
        if !is_dynamic_segment(expected)
            && !is_dynamic_segment(actual)
            && !expected.eq_ignore_ascii_case(actual)
        {
            return false;
        }
        index += 1;
    }
    call.len() == endpoint.len()
}

fn path_segments(value: &str) -> Vec<&str> {
    let without_query = value.split(['?', '#']).next().unwrap_or(value).trim();
    let path = if let Some(scheme) = without_query.find("://") {
        without_query[scheme + 3..]
            .find('/')
            .map(|index| &without_query[scheme + 3 + index..])
            .unwrap_or("/")
    } else {
        without_query
    };
    path.split('/')
        .filter(|segment| !segment.is_empty())
        .collect()
}

fn is_dynamic_segment(segment: &str) -> bool {
    (segment.starts_with('{') && segment.ends_with('}'))
        || segment.starts_with(':')
        || segment.contains("${")
        || (segment.starts_with('<') && segment.ends_with('>'))
        || segment.starts_with('*')
}

fn is_catch_all_segment(segment: &str) -> bool {
    segment.contains('*') || segment.contains("path:")
}

fn is_local_origin(origin: &str) -> bool {
    let host = origin_host(origin);
    matches!(host.as_str(), "localhost" | "127.0.0.1" | "[::1]") || host.ends_with(".localhost")
}

fn origin_host(origin: &str) -> String {
    let authority = origin
        .split_once("://")
        .map(|(_, value)| value)
        .unwrap_or(origin)
        .split('/')
        .next()
        .unwrap_or("")
        .rsplit('@')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    if authority.starts_with('[') {
        authority
            .find(']')
            .map(|end| &authority[..=end])
            .unwrap_or(&authority)
    } else {
        authority.split(':').next().unwrap_or(&authority)
    }
    .trim_end_matches('.')
    .to_string()
}

fn host_aliases(value: &str) -> Vec<String> {
    let lower = value.trim().to_ascii_lowercase();
    let dashed = lower
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '-' {
                character
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string();
    let compact = dashed.replace('-', "");
    let mut aliases = vec![lower, dashed];
    if !compact.is_empty() {
        aliases.push(compact);
    }
    aliases.retain(|alias| !alias.is_empty());
    aliases.sort();
    aliases.dedup();
    aliases
}

pub fn workspace_dependency_closure(
    workspace: &WorkspaceModel,
    start_unit_id: &str,
    start_id: &str,
) -> BTreeSet<String> {
    workspace_dependency_slice(
        workspace,
        &[(start_unit_id.to_string(), start_id.to_string())],
    )
    .nodes
}

pub fn workspace_dependency_slice(
    workspace: &WorkspaceModel,
    starts: &[(String, String)],
) -> WorkspaceDependencySlice {
    let units = workspace
        .repositories
        .iter()
        .flat_map(|repository| &repository.project_units)
        .map(|unit| (unit.id.as_str(), unit))
        .collect::<BTreeMap<_, _>>();
    let cross_dependencies = workspace.cross_project_dependencies.iter().fold(
        BTreeMap::<(&str, &str), Vec<_>>::new(),
        |mut index, item| {
            index
                .entry((
                    item.source_project_unit_id.as_str(),
                    item.source_component_id.as_str(),
                ))
                .or_default()
                .push(item);
            index
        },
    );
    let local_dependencies = units.values().fold(
        BTreeMap::<(&str, &str), Vec<&reposlice_core::Dependency>>::new(),
        |mut index, unit| {
            for dependency in &unit.model.dependencies {
                index
                    .entry((unit.id.as_str(), dependency.source_id.as_str()))
                    .or_default()
                    .push(dependency);
            }
            index
        },
    );
    let mut result = WorkspaceDependencySlice::default();
    let mut queue = VecDeque::from_iter(starts.iter().cloned());
    while let Some((unit_id, current)) = queue.pop_front() {
        if !result.nodes.insert(scoped_node(&unit_id, &current)) {
            continue;
        }
        if let Some(unit) = units.get(unit_id.as_str()) {
            for dependency in local_dependencies
                .get(&(unit.id.as_str(), current.as_str()))
                .into_iter()
                .flatten()
            {
                let metadata = unit.model.analysis.dependencies.iter().find(|item| {
                    item.source_id == dependency.source_id
                        && item.target_id == dependency.target_id
                        && item.kind == dependency.kind.as_str()
                });
                result.dependencies.push(ScopedDependency {
                    source_project_unit_id: unit.id.clone(),
                    source_component_id: dependency.source_id.clone(),
                    target_project_unit_id: unit.id.clone(),
                    target_component_id: dependency.target_id.clone(),
                    kind: dependency.kind.as_str().to_string(),
                    evidence: metadata
                        .and_then(|item| item.evidence.first())
                        .map(|item| item.detail.clone())
                        .unwrap_or_else(|| "Resolved internal dependency".to_string()),
                    confidence: metadata.map(|item| item.confidence).unwrap_or(85),
                });
                queue.push_back((unit.id.clone(), dependency.target_id.clone()));
            }
        }
        for dependency in cross_dependencies
            .get(&(unit_id.as_str(), current.as_str()))
            .into_iter()
            .flatten()
        {
            if let Some(unit) = units.get(dependency.target_project_unit_id.as_str()) {
                if let Some(target_component_id) = resolve_cross_target_component(dependency, unit)
                {
                    result.dependencies.push(ScopedDependency {
                        source_project_unit_id: unit_id.clone(),
                        source_component_id: current.clone(),
                        target_project_unit_id: unit.id.clone(),
                        target_component_id: target_component_id.clone(),
                        kind: dependency.kind.as_str().to_string(),
                        evidence: dependency.evidence.clone(),
                        confidence: dependency.confidence,
                    });
                    queue.push_back((unit.id.clone(), target_component_id));
                }
            }
        }
    }
    result.dependencies.sort_by(|left, right| {
        (
            &left.source_project_unit_id,
            &left.source_component_id,
            &left.target_project_unit_id,
            &left.target_component_id,
            &left.kind,
        )
            .cmp(&(
                &right.source_project_unit_id,
                &right.source_component_id,
                &right.target_project_unit_id,
                &right.target_component_id,
                &right.kind,
            ))
    });
    result.dependencies.dedup_by(|left, right| {
        left.source_project_unit_id == right.source_project_unit_id
            && left.source_component_id == right.source_component_id
            && left.target_project_unit_id == right.target_project_unit_id
            && left.target_component_id == right.target_component_id
            && left.kind == right.kind
    });
    result
}

pub fn workspace_impact_slice(
    workspace: &WorkspaceModel,
    starts: &[(String, String)],
) -> WorkspaceDependencySlice {
    let units = workspace
        .repositories
        .iter()
        .flat_map(|repository| &repository.project_units)
        .map(|unit| (unit.id.as_str(), unit))
        .collect::<BTreeMap<_, _>>();
    let mut reverse = BTreeMap::<(String, String), Vec<ScopedDependency>>::new();

    for unit in units.values() {
        for dependency in &unit.model.dependencies {
            let metadata = unit.model.analysis.dependencies.iter().find(|item| {
                item.source_id == dependency.source_id
                    && item.target_id == dependency.target_id
                    && item.kind == dependency.kind.as_str()
            });
            reverse
                .entry((unit.id.clone(), dependency.target_id.clone()))
                .or_default()
                .push(ScopedDependency {
                    source_project_unit_id: unit.id.clone(),
                    source_component_id: dependency.source_id.clone(),
                    target_project_unit_id: unit.id.clone(),
                    target_component_id: dependency.target_id.clone(),
                    kind: dependency.kind.as_str().to_string(),
                    evidence: metadata
                        .and_then(|item| item.evidence.first())
                        .map(|item| item.detail.clone())
                        .unwrap_or_else(|| "Resolved internal dependency".to_string()),
                    confidence: metadata.map(|item| item.confidence).unwrap_or(85),
                });
        }
    }

    for dependency in &workspace.cross_project_dependencies {
        let Some(target_unit) = units.get(dependency.target_project_unit_id.as_str()) else {
            continue;
        };
        let Some(target_component_id) = resolve_cross_target_component(dependency, target_unit)
        else {
            continue;
        };
        reverse
            .entry((target_unit.id.clone(), target_component_id.clone()))
            .or_default()
            .push(ScopedDependency {
                source_project_unit_id: dependency.source_project_unit_id.clone(),
                source_component_id: dependency.source_component_id.clone(),
                target_project_unit_id: dependency.target_project_unit_id.clone(),
                target_component_id,
                kind: dependency.kind.as_str().to_string(),
                evidence: dependency.evidence.clone(),
                confidence: dependency.confidence,
            });
    }

    let mut result = WorkspaceDependencySlice::default();
    let mut queue = VecDeque::from_iter(starts.iter().cloned());
    while let Some((unit_id, current)) = queue.pop_front() {
        if !result.nodes.insert(scoped_node(&unit_id, &current)) {
            continue;
        }
        for dependency in reverse
            .get(&(unit_id.clone(), current.clone()))
            .into_iter()
            .flatten()
        {
            result.dependencies.push(dependency.clone());
            queue.push_back((
                dependency.source_project_unit_id.clone(),
                dependency.source_component_id.clone(),
            ));
        }
    }
    result.dependencies.sort_by(|left, right| {
        (
            &left.source_project_unit_id,
            &left.source_component_id,
            &left.target_project_unit_id,
            &left.target_component_id,
            &left.kind,
        )
            .cmp(&(
                &right.source_project_unit_id,
                &right.source_component_id,
                &right.target_project_unit_id,
                &right.target_component_id,
                &right.kind,
            ))
    });
    result.dependencies.dedup();
    result
}

pub fn scoped_node(project_unit_id: &str, component_id: &str) -> String {
    format!("{project_unit_id}\0{component_id}")
}

pub fn export_workspace_mermaid(model: &WorkspaceModel) -> String {
    let mut lines = vec!["flowchart LR".to_string()];
    for repository in &model.repositories {
        lines.push(format!(
            "  {}[\"{}\"]",
            export_mermaid_id(&repository.id),
            export_escape_mermaid(&repository.name)
        ));
    }
    let mut seen = BTreeSet::new();
    for dependency in &model.cross_project_dependencies {
        let edge = format!(
            "{}:{}:{}",
            dependency.source_repository_id,
            dependency.target_repository_id,
            dependency.kind.as_str()
        );
        if seen.insert(edge) {
            lines.push(format!(
                "  {} -->|\"{} {}%\"| {}",
                export_mermaid_id(&dependency.source_repository_id),
                dependency.kind.as_str(),
                dependency.confidence,
                export_mermaid_id(&dependency.target_repository_id)
            ));
        }
    }
    lines.join("\n")
}

pub fn export_workspace_graphml(model: &WorkspaceModel) -> String {
    let mut xml = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<graphml xmlns=\"http://graphml.graphdrawing.org/xmlns\">\n  <graph id=\"reposlice\" edgedefault=\"directed\">\n");
    for repository in &model.repositories {
        xml.push_str(&format!(
            "    <node id=\"{}\"><data key=\"label\">{}</data></node>\n",
            export_xml_escape(&repository.id),
            export_xml_escape(&repository.name)
        ));
    }
    for (index, dependency) in model.cross_project_dependencies.iter().enumerate() {
        xml.push_str(&format!("    <edge id=\"e{}\" source=\"{}\" target=\"{}\"><data key=\"kind\">{}</data><data key=\"confidence\">{}</data></edge>\n", index, export_xml_escape(&dependency.source_repository_id), export_xml_escape(&dependency.target_repository_id), dependency.kind.as_str(), dependency.confidence));
    }
    xml.push_str("  </graph>\n</graphml>\n");
    xml
}

fn export_mermaid_id(value: &str) -> String {
    format!(
        "n_{}",
        value
            .chars()
            .map(|character| if character.is_ascii_alphanumeric() {
                character
            } else {
                '_'
            })
            .collect::<String>()
    )
}
fn export_escape_mermaid(value: &str) -> String {
    value.replace('"', "'").replace('\n', " ")
}
fn export_xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use reposlice_core::{
        CompatibilityLevel, Component, ComponentKind, Dependency, DependencyKind, Entrypoint,
        EntrypointKind, HttpCall, ProjectRole, ProjectUnit, Repository,
    };

    #[test]
    fn normalizes_http_parameters_and_queries() {
        assert_eq!(
            normalize_http_path("/api/users/123?full=true"),
            "/api/users/123"
        );
        assert_eq!(normalize_http_path("/api/users/:id"), "/api/users/{}");
        assert_eq!(normalize_http_path("/api/users/{id}"), "/api/users/{}");
        assert_eq!(normalize_http_path("/api/users/${id}"), "/api/users/{}");
    }

    #[test]
    fn detects_dependency_cycles_without_treating_them_as_missing_nodes() {
        let model = ProjectModel {
            root: ".".into(),
            name: "cycles".into(),
            files: 3,
            compatibility: CompatibilityLevel::Semantic,
            technologies: vec![],
            components: vec![component("a"), component("b"), component("c")],
            entrypoints: vec![],
            dependencies: vec![
                Dependency {
                    source_id: "a".into(),
                    target_id: "b".into(),
                    kind: DependencyKind::Calls,
                },
                Dependency {
                    source_id: "b".into(),
                    target_id: "c".into(),
                    kind: DependencyKind::Calls,
                },
                Dependency {
                    source_id: "c".into(),
                    target_id: "a".into(),
                    kind: DependencyKind::Calls,
                },
            ],
            analysis: Default::default(),
        };
        let report = validate_dependency_graph(&model);
        assert!(report.passed());
        assert!(!report.is_clean());
        assert_eq!(
            report.cycles,
            vec![vec!["a".to_string(), "b".to_string(), "c".to_string()]]
        );
    }

    #[test]
    fn computes_reverse_impact_and_validates_graph_integrity() {
        let model = ProjectModel {
            root: ".".into(),
            name: "impact".into(),
            files: 3,
            compatibility: CompatibilityLevel::Semantic,
            technologies: vec![],
            components: vec![
                component("controller"),
                component("service"),
                component("repository"),
            ],
            entrypoints: vec![],
            dependencies: vec![
                Dependency {
                    source_id: "controller".into(),
                    target_id: "service".into(),
                    kind: DependencyKind::Calls,
                },
                Dependency {
                    source_id: "service".into(),
                    target_id: "repository".into(),
                    kind: DependencyKind::Calls,
                },
            ],
            analysis: Default::default(),
        };
        let impact = reverse_dependency_closure(&model, "repository");
        assert!(impact.contains("repository"));
        assert!(impact.contains("service"));
        assert!(impact.contains("controller"));
        assert!(validate_dependency_graph(&model).passed());

        let mut invalid = model.clone();
        invalid.dependencies.push(Dependency {
            source_id: "missing".into(),
            target_id: "repository".into(),
            kind: DependencyKind::Calls,
        });
        assert!(!validate_dependency_graph(&invalid).passed());
    }

    #[test]
    fn matches_http_and_crosses_the_workspace_graph() {
        let frontend = unit(
            "front",
            "front-component",
            vec![],
            vec![],
            vec![HttpCall {
                method: "GET".into(),
                path: "/api/users/42".into(),
                origin: None,
                component_id: "front-component".into(),
                file: "front.ts".into(),
                evidence: "fetch".into(),
            }],
        );
        let backend = unit(
            "back",
            "controller",
            vec![Entrypoint {
                id: "users-get".into(),
                name: "GET /api/users/{id}".into(),
                kind: EntrypointKind::HttpEndpoint,
                component_id: "controller".into(),
                file: "Controller.java".into(),
            }],
            vec![Dependency {
                source_id: "controller".into(),
                target_id: "service".into(),
                kind: DependencyKind::Calls,
            }],
            vec![],
        );
        let mut workspace = WorkspaceModel {
            id: "ws".into(),
            name: "ws".into(),
            repositories: vec![
                repository("front-repo", frontend),
                repository("back-repo", backend),
            ],
            cross_project_dependencies: vec![],
        };
        workspace.cross_project_dependencies = match_http_dependencies(&workspace);
        assert_eq!(workspace.cross_project_dependencies.len(), 1);
        assert_eq!(workspace.cross_project_dependencies[0].confidence, 90);
        let closure = workspace_dependency_closure(&workspace, "front", "front-component");
        assert!(closure.contains(&scoped_node("back", "controller")));
        assert!(closure.contains(&scoped_node("back", "service")));
        let impact =
            workspace_impact_slice(&workspace, &[("back".to_string(), "service".to_string())]);
        assert!(impact.nodes.contains(&scoped_node("back", "controller")));
        assert!(impact
            .nodes
            .contains(&scoped_node("front", "front-component")));
        assert!(impact
            .dependencies
            .iter()
            .any(|dependency| dependency.kind.eq_ignore_ascii_case("http")));
    }

    #[test]
    fn ignores_external_origins_and_same_unit_calls() {
        let mut source = unit(
            "app",
            "client",
            vec![Entrypoint {
                id: "users".into(),
                name: "ANY /api/users/{id}".into(),
                kind: EntrypointKind::HttpEndpoint,
                component_id: "controller".into(),
                file: "server.ts".into(),
            }],
            vec![],
            vec![HttpCall {
                method: "GET".into(),
                path: "/api/users/1".into(),
                origin: Some("https://third-party.example".into()),
                component_id: "client".into(),
                file: "client.ts".into(),
                evidence: "fetch".into(),
            }],
        );
        source.model.components.push(Component {
            id: "controller".into(),
            name: "controller".into(),
            kind: ComponentKind::Controller,
            language: "TypeScript".into(),
            file: "server.ts".into(),
        });
        let mut workspace = WorkspaceModel {
            id: "ws".into(),
            name: "ws".into(),
            repositories: vec![repository("repo", source)],
            cross_project_dependencies: vec![],
        };
        assert!(match_http_dependencies(&workspace).is_empty());
        workspace.repositories[0].project_units[0].http_calls[0].origin =
            Some("http://localhost:3000".into());
        assert!(match_http_dependencies(&workspace).is_empty());
    }

    #[test]
    fn matches_dynamic_and_named_internal_origins_across_units() {
        let caller = unit(
            "front",
            "client",
            vec![],
            vec![],
            vec![HttpCall {
                method: "GET".into(),
                path: "/health".into(),
                origin: Some("dynamic://unresolved".into()),
                component_id: "client".into(),
                file: "client.ts".into(),
                evidence: "fetch(environment.apiUrl + '/health')".into(),
            }],
        );
        let backend = unit(
            "backend",
            "controller",
            vec![Entrypoint {
                id: "health".into(),
                name: "GET /health".into(),
                kind: EntrypointKind::HttpEndpoint,
                component_id: "controller".into(),
                file: "server.ts".into(),
            }],
            vec![],
            vec![],
        );
        let mut workspace = WorkspaceModel {
            id: "ws".into(),
            name: "ws".into(),
            repositories: vec![
                repository("frontend", caller),
                repository("backend", backend),
            ],
            cross_project_dependencies: vec![],
        };
        let dynamic = match_http_dependencies(&workspace);
        assert_eq!(dynamic.len(), 1);
        assert_eq!(dynamic[0].confidence, 70);

        workspace.repositories[0].project_units[0].http_calls[0].origin =
            Some("http://backend:8080".into());
        let named = match_http_dependencies(&workspace);
        assert_eq!(named.len(), 1);
        assert_eq!(named[0].confidence, 80);
    }

    #[test]
    fn refuses_ambiguous_http_targets() {
        let caller = unit(
            "caller",
            "client",
            vec![],
            vec![],
            vec![HttpCall {
                method: "GET".into(),
                path: "/health".into(),
                origin: None,
                component_id: "client".into(),
                file: "client.ts".into(),
                evidence: "fetch('/health')".into(),
            }],
        );
        let endpoint = || Entrypoint {
            id: "health".into(),
            name: "GET /health".into(),
            kind: EntrypointKind::HttpEndpoint,
            component_id: "controller".into(),
            file: "server.ts".into(),
        };
        let workspace = WorkspaceModel {
            id: "ws".into(),
            name: "ws".into(),
            repositories: vec![
                repository("caller-repo", caller),
                repository(
                    "api-a",
                    unit("api-a-unit", "controller", vec![endpoint()], vec![], vec![]),
                ),
                repository(
                    "api-b",
                    unit("api-b-unit", "controller", vec![endpoint()], vec![], vec![]),
                ),
            ],
            cross_project_dependencies: vec![],
        };
        assert!(match_http_dependencies(&workspace).is_empty());
    }

    fn component(id: &str) -> Component {
        Component {
            id: id.into(),
            name: id.into(),
            kind: ComponentKind::Class,
            language: "Rust".into(),
            file: format!("{id}.rs"),
        }
    }

    #[test]
    fn matches_workspace_package_dependencies_and_traverses_them() {
        let root = std::env::temp_dir().join(format!(
            "reposlice-graph-workspace-package-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let app_root = root.join("app");
        let shared_root = root.join("shared");
        std::fs::create_dir_all(app_root.join("src")).unwrap();
        std::fs::create_dir_all(shared_root.join("src")).unwrap();
        std::fs::write(
            app_root.join("package.json"),
            r#"{"name":"@acme/app","dependencies":{"@acme/shared":"workspace:*"}}"#,
        )
        .unwrap();
        std::fs::write(
            shared_root.join("package.json"),
            r#"{"name":"@acme/shared","version":"1.0.0"}"#,
        )
        .unwrap();
        let app_file = app_root.join("src/app.ts");
        let shared_file = shared_root.join("src/index.ts");
        std::fs::write(
            &app_file,
            "import { value } from '@acme/shared';\nconsole.log(value);\n",
        )
        .unwrap();
        std::fs::write(&shared_file, "export const value = 1;\n").unwrap();

        let mut app = unit("app-unit", "app-component", vec![], vec![], vec![]);
        app.repository_id = "repo-app".into();
        app.root = app_root.to_string_lossy().into_owned();
        app.model.root = app.root.clone();
        app.model.components[0].file = app_file.to_string_lossy().into_owned();
        let mut shared = unit("shared-unit", "shared-component", vec![], vec![], vec![]);
        shared.repository_id = "repo-shared".into();
        shared.root = shared_root.to_string_lossy().into_owned();
        shared.model.root = shared.root.clone();
        shared.model.components[0].file = shared_file.to_string_lossy().into_owned();
        app.model
            .components
            .retain(|component| component.id == "app-component");
        shared
            .model
            .components
            .retain(|component| component.id == "shared-component");

        let mut workspace = WorkspaceModel {
            id: "ws-packages".into(),
            name: "ws-packages".into(),
            repositories: vec![
                Repository {
                    id: "repo-app".into(),
                    name: "app".into(),
                    root: app_root.to_string_lossy().into_owned(),
                    git: None,
                    project_units: vec![app],
                },
                Repository {
                    id: "repo-shared".into(),
                    name: "shared".into(),
                    root: shared_root.to_string_lossy().into_owned(),
                    git: None,
                    project_units: vec![shared],
                },
            ],
            cross_project_dependencies: vec![],
        };
        workspace.cross_project_dependencies = match_cross_project_dependencies(&workspace);
        let package_dependency = workspace
            .cross_project_dependencies
            .iter()
            .find(|dependency| dependency.kind == CrossProjectDependencyKind::WorkspacePackage)
            .unwrap();
        assert_eq!(package_dependency.source_project_unit_id, "app-unit");
        assert_eq!(package_dependency.target_project_unit_id, "shared-unit");
        assert_eq!(
            package_dependency.target_component_id.as_deref(),
            Some("shared-component")
        );
        assert_eq!(package_dependency.confidence, 100);

        let slice = workspace_dependency_slice(
            &workspace,
            &[("app-unit".to_string(), "app-component".to_string())],
        );
        assert!(slice
            .nodes
            .contains(&scoped_node("shared-unit", "shared-component")));
        let impact = workspace_impact_slice(
            &workspace,
            &[("shared-unit".to_string(), "shared-component".to_string())],
        );
        assert!(impact
            .nodes
            .contains(&scoped_node("app-unit", "app-component")));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn workspace_package_matching_supports_cargo_dependencies() {
        let root = std::env::temp_dir().join(format!(
            "reposlice-graph-cargo-package-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let app_root = root.join("app");
        let shared_root = root.join("shared");
        std::fs::create_dir_all(app_root.join("src")).unwrap();
        std::fs::create_dir_all(shared_root.join("src")).unwrap();
        std::fs::write(
            app_root.join("Cargo.toml"),
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n[dependencies]\nshared = { path = \"../shared\" }\n",
        )
        .unwrap();
        std::fs::write(
            shared_root.join("Cargo.toml"),
            "[package]\nname = \"shared\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        let app_file = app_root.join("src/main.rs");
        let shared_file = shared_root.join("src/lib.rs");
        std::fs::write(&app_file, "use shared::value;\nfn main() { value(); }\n").unwrap();
        std::fs::write(&shared_file, "pub fn value() {}\n").unwrap();

        let mut app = unit("cargo-app-unit", "cargo-app", vec![], vec![], vec![]);
        app.repository_id = "cargo-app-repo".into();
        app.root = app_root.to_string_lossy().into_owned();
        app.model.root = app.root.clone();
        app.model.components[0].file = app_file.to_string_lossy().into_owned();
        let mut shared = unit("cargo-shared-unit", "cargo-shared", vec![], vec![], vec![]);
        shared.repository_id = "cargo-shared-repo".into();
        shared.root = shared_root.to_string_lossy().into_owned();
        shared.model.root = shared.root.clone();
        shared.model.components[0].file = shared_file.to_string_lossy().into_owned();

        let workspace = WorkspaceModel {
            id: "cargo-workspace".into(),
            name: "cargo-workspace".into(),
            repositories: vec![
                Repository {
                    id: "cargo-app-repo".into(),
                    name: "app".into(),
                    root: app_root.to_string_lossy().into_owned(),
                    git: None,
                    project_units: vec![app],
                },
                Repository {
                    id: "cargo-shared-repo".into(),
                    name: "shared".into(),
                    root: shared_root.to_string_lossy().into_owned(),
                    git: None,
                    project_units: vec![shared],
                },
            ],
            cross_project_dependencies: vec![],
        };
        let dependencies = match_workspace_package_dependencies(&workspace);
        assert_eq!(dependencies.len(), 1);
        assert_eq!(
            dependencies[0].kind,
            CrossProjectDependencyKind::WorkspacePackage
        );
        assert_eq!(dependencies[0].confidence, 100);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn ambiguous_workspace_package_names_are_not_linked() {
        let root = std::env::temp_dir().join(format!(
            "reposlice-graph-ambiguous-package-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let app_root = root.join("app");
        let shared_a_root = root.join("shared-a");
        let shared_b_root = root.join("shared-b");
        for directory in [&app_root, &shared_a_root, &shared_b_root] {
            std::fs::create_dir_all(directory.join("src")).unwrap();
        }
        std::fs::write(
            app_root.join("package.json"),
            r#"{"name":"app","dependencies":{"shared":"workspace:*"}}"#,
        )
        .unwrap();
        for directory in [&shared_a_root, &shared_b_root] {
            std::fs::write(
                directory.join("package.json"),
                r#"{"name":"shared","version":"1.0.0"}"#,
            )
            .unwrap();
        }
        let app_file = app_root.join("src/app.ts");
        std::fs::write(&app_file, "import value from 'shared';\n").unwrap();

        let mut app = unit("ambiguous-app", "app", vec![], vec![], vec![]);
        app.repository_id = "app-repo".into();
        app.root = app_root.to_string_lossy().into_owned();
        app.model.root = app.root.clone();
        app.model.components[0].file = app_file.to_string_lossy().into_owned();

        let make_shared = |unit_id: &str, repository_id: &str, directory: &Path| {
            let source = directory.join("src/index.ts");
            std::fs::write(&source, "export default 1;\n").unwrap();
            let mut unit = unit(unit_id, "shared", vec![], vec![], vec![]);
            unit.repository_id = repository_id.into();
            unit.root = directory.to_string_lossy().into_owned();
            unit.model.root = unit.root.clone();
            unit.model.components[0].file = source.to_string_lossy().into_owned();
            unit
        };
        let shared_a = make_shared("shared-a", "shared-a-repo", &shared_a_root);
        let shared_b = make_shared("shared-b", "shared-b-repo", &shared_b_root);
        let workspace = WorkspaceModel {
            id: "ambiguous".into(),
            name: "ambiguous".into(),
            repositories: vec![
                Repository {
                    id: "app-repo".into(),
                    name: "app".into(),
                    root: app_root.to_string_lossy().into_owned(),
                    git: None,
                    project_units: vec![app],
                },
                Repository {
                    id: "shared-a-repo".into(),
                    name: "shared-a".into(),
                    root: shared_a_root.to_string_lossy().into_owned(),
                    git: None,
                    project_units: vec![shared_a],
                },
                Repository {
                    id: "shared-b-repo".into(),
                    name: "shared-b".into(),
                    root: shared_b_root.to_string_lossy().into_owned(),
                    git: None,
                    project_units: vec![shared_b],
                },
            ],
            cross_project_dependencies: vec![],
        };
        assert!(match_workspace_package_dependencies(&workspace).is_empty());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn validates_workspace_cross_project_integrity() {
        let source = unit("source-unit", "source-component", vec![], vec![], vec![]);
        let target = unit("target-unit", "target-component", vec![], vec![], vec![]);
        let valid_dependency = CrossProjectDependency {
            source_repository_id: "source-repo".into(),
            source_project_unit_id: "source-unit".into(),
            source_component_id: "source-component".into(),
            target_repository_id: "target-repo".into(),
            target_project_unit_id: "target-unit".into(),
            target_entrypoint_id: None,
            target_component_id: Some("target-component".into()),
            kind: CrossProjectDependencyKind::WorkspacePackage,
            evidence: "workspace package".into(),
            confidence: 100,
        };
        let mut workspace = WorkspaceModel {
            id: "workspace".into(),
            name: "workspace".into(),
            repositories: vec![
                repository("source-repo", source),
                repository("target-repo", target),
            ],
            cross_project_dependencies: vec![valid_dependency.clone()],
        };

        let valid = validate_workspace_graph(&workspace);
        assert!(valid.passed());
        assert_eq!(valid.error_count(), 0);

        let mut broken = valid_dependency;
        broken.target_component_id = Some("missing-component".into());
        workspace.cross_project_dependencies = vec![broken];
        let invalid = validate_workspace_graph(&workspace);
        assert!(!invalid.passed());
        assert_eq!(invalid.error_count(), 1);
        assert!(invalid
            .missing_target_components
            .contains("target-unit|missing-component"));
    }

    fn unit(
        id: &str,
        component: &str,
        entrypoints: Vec<Entrypoint>,
        dependencies: Vec<Dependency>,
        http_calls: Vec<HttpCall>,
    ) -> ProjectUnit {
        ProjectUnit {
            id: id.into(),
            repository_id: format!("{id}-repo"),
            root: id.into(),
            name: id.into(),
            role: ProjectRole::Unknown,
            model: ProjectModel {
                root: id.into(),
                name: id.into(),
                files: 1,
                compatibility: CompatibilityLevel::Filesystem,
                technologies: vec![],
                components: vec![
                    Component {
                        id: component.into(),
                        name: component.into(),
                        kind: ComponentKind::File,
                        language: "Unknown".into(),
                        file: component.into(),
                    },
                    Component {
                        id: "service".into(),
                        name: "service".into(),
                        kind: ComponentKind::Service,
                        language: "Java".into(),
                        file: "service".into(),
                    },
                ],
                entrypoints,
                dependencies,
                analysis: Default::default(),
            },
            http_calls,
        }
    }

    fn repository(id: &str, unit: ProjectUnit) -> Repository {
        Repository {
            id: id.into(),
            name: id.into(),
            root: id.into(),
            git: None,
            project_units: vec![unit],
        }
    }
}
