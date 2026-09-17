use reposlice_capsule::{capsules_root, create_capsule, create_workspace_capsule};
use reposlice_core::{
    FrameworkAdapter, FrameworkDetectionPolicy, FrameworkSuite, LanguageAnalyzer, ProjectModel,
    ScopedTarget, WorkspaceModel,
};
use reposlice_graph::{
    dependency_slice, export_workspace_graphml, export_workspace_mermaid, impact_slice, validate_dependency_graph, validate_workspace_graph,
    workspace_dependency_slice, workspace_impact_slice, DependencySlice, GraphIntegrityReport,
    WorkspaceDependencySlice, WorkspaceGraphIntegrityReport,
};
use reposlice_runtime::{detect_project_runtime, RuntimeCapabilities};
use reposlice_scanner::{scan_project_with_registry, AnalyzerRegistry};
use reposlice_verifier::{verify_capsule, VerificationReport};
use reposlice_workspace::{
    add_repository, clone_repository, create_workspace, delete_repository, delete_workspace,
    latest_analysis_record, list_analysis_records, list_repositories, list_workspaces,
    load_cached_workspace_model_with_registry, scan_workspace_repository_with_registry,
    scan_workspace_with_registry, update_managed_repository, AnalysisRecord, RepositoryRecord,
    WorkspaceRecord,
};
use std::io;
use std::path::{Path, PathBuf};

#[derive(Default)]
pub struct RepoSlice {
    analyzers: AnalyzerRegistry,
}

impl RepoSlice {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_registry(analyzers: AnalyzerRegistry) -> Self {
        Self { analyzers }
    }

    pub fn without_builtins() -> Self {
        Self::with_registry(AnalyzerRegistry::empty())
    }

    pub fn with_detection_policy(mut self, policy: FrameworkDetectionPolicy) -> Self {
        self.analyzers.set_detection_policy(policy);
        self
    }

    pub fn register_language<A>(&mut self, analyzer: A)
    where
        A: LanguageAnalyzer + 'static,
    {
        self.analyzers.register_language(analyzer);
    }

    pub fn register_framework<A>(&mut self, adapter: A)
    where
        A: FrameworkAdapter + 'static,
    {
        self.analyzers.register_framework(adapter);
    }

    pub fn register_framework_suite<A>(&mut self, suite: A)
    where
        A: FrameworkSuite + 'static,
    {
        self.analyzers.register_framework_suite(suite);
    }

    pub fn analyze(&self, root: impl AsRef<Path>) -> io::Result<ProjectModel> {
        scan_project_with_registry(root.as_ref(), &self.analyzers)
    }

    pub fn slice(&self, model: &ProjectModel, target: &str) -> io::Result<DependencySlice> {
        let component_id = resolve_target(model, target)?;
        Ok(dependency_slice(model, component_id))
    }

    pub fn impact(&self, model: &ProjectModel, target: &str) -> io::Result<DependencySlice> {
        let component_id = resolve_target(model, target)?;
        Ok(impact_slice(model, component_id))
    }

    pub fn validate_graph(&self, model: &ProjectModel) -> GraphIntegrityReport {
        validate_dependency_graph(model)
    }

    pub fn runtime(&self, model: &ProjectModel) -> RuntimeCapabilities {
        detect_project_runtime(&model.analysis.runtime_requirements)
    }

    pub fn create_capsule(
        &self,
        model: &ProjectModel,
        target: &str,
        output_root: Option<&Path>,
    ) -> io::Result<PathBuf> {
        let default_root;
        let output_root = match output_root {
            Some(path) => path,
            None => {
                default_root = capsules_root(Path::new(&model.root));
                &default_root
            }
        };
        create_capsule(model, target, output_root)
    }

    pub fn create_workspace(&self, name: &str) -> io::Result<WorkspaceRecord> {
        create_workspace(name)
    }

    pub fn list_workspaces(&self) -> io::Result<Vec<WorkspaceRecord>> {
        list_workspaces()
    }

    pub fn add_repository(
        &self,
        workspace_id: &str,
        path: impl AsRef<Path>,
    ) -> io::Result<RepositoryRecord> {
        add_repository(workspace_id, path.as_ref())
    }

    pub fn clone_repository(&self, workspace_id: &str, url: &str) -> io::Result<RepositoryRecord> {
        clone_repository(workspace_id, url)
    }

    pub fn list_repositories(&self, workspace_id: &str) -> io::Result<Vec<RepositoryRecord>> {
        list_repositories(workspace_id)
    }

    pub fn list_analysis_records(&self, workspace_id: &str) -> io::Result<Vec<AnalysisRecord>> {
        list_analysis_records(workspace_id)
    }

    pub fn latest_analysis(
        &self,
        workspace_id: &str,
        repository_id: &str,
    ) -> io::Result<Option<AnalysisRecord>> {
        latest_analysis_record(workspace_id, repository_id)
    }

    pub fn delete_workspace(&self, workspace_id: &str) -> io::Result<()> {
        delete_workspace(workspace_id)
    }

    pub fn delete_repository(&self, workspace_id: &str, repository_id: &str) -> io::Result<()> {
        delete_repository(workspace_id, repository_id)
    }

    pub fn update_managed_repository(
        &self,
        workspace_id: &str,
        repository_id: &str,
    ) -> io::Result<RepositoryRecord> {
        update_managed_repository(workspace_id, repository_id)
    }

    pub fn load_cached_workspace(&self, workspace_id: &str) -> io::Result<Option<WorkspaceModel>> {
        load_cached_workspace_model_with_registry(workspace_id, &self.analyzers)
    }

    pub fn analyze_workspace_repository(
        &self,
        workspace_id: &str,
        repository_id: &str,
    ) -> io::Result<WorkspaceModel> {
        scan_workspace_repository_with_registry(workspace_id, repository_id, &self.analyzers)
            .map_err(|error| io::Error::other(error.to_string()))
    }

    pub fn analyze_workspace(&self, workspace_id: &str) -> io::Result<WorkspaceModel> {
        scan_workspace_with_registry(workspace_id, &self.analyzers)
            .map_err(|error| io::Error::other(error.to_string()))
    }

    pub fn workspace_slice(
        &self,
        workspace: &WorkspaceModel,
        targets: &[ScopedTarget],
    ) -> io::Result<WorkspaceDependencySlice> {
        let starts = resolve_workspace_targets(workspace, targets)?;
        Ok(workspace_dependency_slice(workspace, &starts))
    }

    pub fn workspace_impact(
        &self,
        workspace: &WorkspaceModel,
        targets: &[ScopedTarget],
    ) -> io::Result<WorkspaceDependencySlice> {
        let starts = resolve_workspace_targets(workspace, targets)?;
        Ok(workspace_impact_slice(workspace, &starts))
    }

    pub fn validate_workspace_graph(
        &self,
        workspace: &WorkspaceModel,
    ) -> WorkspaceGraphIntegrityReport {
        validate_workspace_graph(workspace)
    }

    pub fn export_workspace(&self, workspace: &WorkspaceModel, format: &str) -> io::Result<String> {
        match format.to_ascii_lowercase().as_str() {
            "json" => serde_json::to_string_pretty(workspace)
                .map_err(|error| io::Error::other(format!("Workspace model could not be serialized: {error}"))),
            "mermaid" | "mmd" => Ok(export_workspace_mermaid(workspace)),
            "graphml" => Ok(export_workspace_graphml(workspace)),
            other => Err(io::Error::new(io::ErrorKind::InvalidInput, format!("Unsupported export format: {other}"))),
        }
    }

    pub fn create_workspace_capsule(
        &self,
        workspace: &WorkspaceModel,
        target: &ScopedTarget,
        output_root: impl AsRef<Path>,
    ) -> io::Result<PathBuf> {
        create_workspace_capsule(workspace, target, output_root.as_ref())
    }

    pub fn verify(&self, capsule_root: impl AsRef<Path>) -> io::Result<VerificationReport> {
        verify_capsule(capsule_root.as_ref())
    }
}

fn resolve_workspace_targets(
    workspace: &WorkspaceModel,
    targets: &[ScopedTarget],
) -> io::Result<Vec<(String, String)>> {
    targets
        .iter()
        .map(|target| {
            let repository = workspace
                .repositories
                .iter()
                .find(|repository| repository.id == target.repository_id)
                .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Repository was not found"))?;
            let unit = repository
                .project_units
                .iter()
                .find(|unit| unit.id == target.project_unit_id)
                .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Project unit was not found"))?;
            let component = unit
                .model
                .resolve_target_component_id(&target.target_id)
                .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Target was not found"))?;
            Ok((unit.id.clone(), component.to_string()))
        })
        .collect()
}

fn resolve_target<'a>(model: &'a ProjectModel, target: &str) -> io::Result<&'a str> {
    model.resolve_target_component_id(target).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            format!("Target was not found in the project model: {target}"),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use reposlice_core::{
        AnalysisMetadata, CompatibilityLevel, Component, ComponentKind, Dependency,
        DependencyKind,
    };

    fn model() -> ProjectModel {
        ProjectModel {
            root: ".".into(),
            name: "sdk".into(),
            files: 3,
            compatibility: CompatibilityLevel::Semantic,
            technologies: vec![],
            components: vec![
                Component {
                    id: "controller".into(),
                    name: "controller".into(),
                    kind: ComponentKind::Controller,
                    language: "Rust".into(),
                    file: "controller.rs".into(),
                },
                Component {
                    id: "service".into(),
                    name: "service".into(),
                    kind: ComponentKind::Service,
                    language: "Rust".into(),
                    file: "service.rs".into(),
                },
                Component {
                    id: "repository".into(),
                    name: "repository".into(),
                    kind: ComponentKind::Repository,
                    language: "Rust".into(),
                    file: "repository.rs".into(),
                },
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
            analysis: AnalysisMetadata::default(),
        }
    }

    #[test]
    fn facade_exposes_forward_and_reverse_slices() {
        let sdk = RepoSlice::new();
        let model = model();
        let slice = sdk.slice(&model, "controller").unwrap();
        assert_eq!(slice.nodes.len(), 3);
        let impact = sdk.impact(&model, "repository").unwrap();
        assert_eq!(impact.nodes.len(), 3);
        assert!(sdk.validate_graph(&model).passed());
    }
}
