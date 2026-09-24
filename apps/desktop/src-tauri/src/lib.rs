mod focused_analysis;

use reposlice_capsule::{
    create_workspace_capsule, list_capsules, list_capsules_in, CapsuleSummary,
};
use reposlice_core::{ProjectModel, ScopedTarget, WorkspaceModel};
use reposlice_graph::{
    validate_dependency_graph, validate_workspace_graph, workspace_dependency_slice,
    workspace_impact_slice,
};
use reposlice_runtime::detect_runtime;
use reposlice_verifier::{sandbox_validation_plan, verify_capsule};
use reposlice_workspace::{
    add_repository, clone_repository, create_workspace, delete_repository,
    install_local_crash_reporting, latest_analysis_record, list_repositories, list_workspaces,
    load_cached_workspace_model, scan_workspace_controlled, scan_workspace_repository,
    scan_workspace_repository_controlled, update_managed_repository, workspace_root,
    AnalysisRecord, RepositoryRecord, WorkspaceRecord, WorkspaceScanProgress,
};
use serde::Serialize;
use std::collections::{BTreeMap, VecDeque};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{Emitter, Manager};

const DESKTOP_MODEL_CACHE_LIMIT: usize = 6;

#[tauri::command]
async fn analyze_source_command(
    source: String,
) -> Result<focused_analysis::FocusedRepositoryAnalysis, String> {
    let source = source.trim().to_string();
    if source.is_empty() {
        return Err("Select a local project or enter a GitHub repository URL".to_string());
    }

    tauri::async_runtime::spawn_blocking(move || {
        let repository = if source.starts_with("https://github.com/")
            || source.starts_with("http://github.com/")
        {
            clone_repository("default", &source)
        } else {
            let path = Path::new(&source);
            if !path.is_dir() {
                return Err("The selected project folder does not exist".to_string());
            }
            add_repository("default", path)
        }
        .map_err(|error| error.to_string())?;

        let workspace = scan_workspace_repository("default", &repository.id)
            .map_err(|error| error.to_string())?;
        focused_analysis::from_workspace(workspace, &repository.id)
    })
    .await
    .map_err(|error| error.to_string())?
}

#[derive(Default)]
struct DesktopModelCache {
    models: BTreeMap<String, WorkspaceModel>,
    recency: VecDeque<String>,
}

#[derive(Default)]
struct DesktopState {
    analyses: Mutex<DesktopModelCache>,
    cancellations: Mutex<BTreeMap<String, Arc<AtomicBool>>>,
}

impl DesktopState {
    fn store(&self, workspace: WorkspaceModel) -> Result<(), String> {
        let workspace_id = workspace.id.clone();
        let mut cache = self
            .analyses
            .lock()
            .map_err(|_| "Desktop analysis cache is unavailable".to_string())?;
        cache.models.insert(workspace_id.clone(), workspace);
        cache.recency.retain(|id| id != &workspace_id);
        cache.recency.push_back(workspace_id);
        while cache.recency.len() > DESKTOP_MODEL_CACHE_LIMIT {
            if let Some(evicted) = cache.recency.pop_front() {
                cache.models.remove(&evicted);
            }
        }
        Ok(())
    }

    fn load(&self, workspace_id: &str) -> Result<WorkspaceModel, String> {
        {
            let mut cache = self
                .analyses
                .lock()
                .map_err(|_| "Desktop analysis cache is unavailable".to_string())?;
            if let Some(workspace) = cache.models.get(workspace_id).cloned() {
                cache.recency.retain(|id| id != workspace_id);
                cache.recency.push_back(workspace_id.to_string());
                return Ok(workspace);
            }
        }
        let workspace = load_cached_workspace_model(workspace_id)
            .map_err(|error| error.to_string())?
            .ok_or_else(|| {
                "No current analysis is loaded for this workspace. Run Analyze first.".to_string()
            })?;
        self.store(workspace.clone())?;
        Ok(workspace)
    }

    fn invalidate(&self, workspace_id: &str) -> Result<(), String> {
        let mut cache = self
            .analyses
            .lock()
            .map_err(|_| "Desktop analysis cache is unavailable".to_string())?;
        cache.models.remove(workspace_id);
        cache.recency.retain(|id| id != workspace_id);
        Ok(())
    }

    fn clear(&self) -> Result<(), String> {
        let mut cache = self
            .analyses
            .lock()
            .map_err(|_| "Desktop analysis cache is unavailable".to_string())?;
        cache.models.clear();
        cache.recency.clear();
        Ok(())
    }

    fn begin_analysis(&self, workspace_id: &str) -> Result<Arc<AtomicBool>, String> {
        let mut cancellations = self
            .cancellations
            .lock()
            .map_err(|_| "Desktop analysis cancellation state is unavailable".to_string())?;
        if cancellations.contains_key(workspace_id) {
            return Err("An analysis is already running for this workspace".to_string());
        }
        let token = Arc::new(AtomicBool::new(false));
        cancellations.insert(workspace_id.to_string(), Arc::clone(&token));
        Ok(token)
    }

    fn finish_analysis(&self, workspace_id: &str) -> Result<(), String> {
        self.cancellations
            .lock()
            .map_err(|_| "Desktop analysis cancellation state is unavailable".to_string())?
            .remove(workspace_id);
        Ok(())
    }

    fn cancel_analysis(&self, workspace_id: &str) -> Result<bool, String> {
        let cancellations = self
            .cancellations
            .lock()
            .map_err(|_| "Desktop analysis cancellation state is unavailable".to_string())?;
        if let Some(token) = cancellations.get(workspace_id) {
            token.store(true, Ordering::Relaxed);
            return Ok(true);
        }
        Ok(false)
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WorkspaceRecordDto {
    id: String,
    name: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RepositoryRecordDto {
    id: String,
    workspace_id: String,
    name: String,
    path: String,
    origin: String,
    analysis: Option<AnalysisRecordDto>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct AnalysisProgressDto {
    workspace_id: String,
    repository_id: Option<String>,
    repository_name: Option<String>,
    completed_repositories: usize,
    total_repositories: usize,
    stage: String,
    reused_cache: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GitMetadataDto {
    is_git_repository: bool,
    branch: Option<String>,
    commit: Option<String>,
    remote: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct HttpCallDto {
    method: String,
    path: String,
    origin: Option<String>,
    component_id: String,
    file: String,
    evidence: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ProjectUnitDto {
    id: String,
    repository_id: String,
    root: String,
    name: String,
    role: String,
    model: ProjectModelDto,
    http_calls: Vec<HttpCallDto>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RepositoryDto {
    id: String,
    name: String,
    root: String,
    origin: String,
    git: Option<GitMetadataDto>,
    project_units: Vec<ProjectUnitDto>,
    audit: RepositoryAuditDto,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CrossProjectDependencyDto {
    source_repository_id: String,
    source_project_unit_id: String,
    source_component_id: String,
    target_repository_id: String,
    target_project_unit_id: String,
    target_entrypoint_id: Option<String>,
    target_component_id: Option<String>,
    kind: String,
    evidence: String,
    confidence: u8,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WorkspaceModelDto {
    id: String,
    name: String,
    repositories: Vec<RepositoryDto>,
    cross_project_dependencies: Vec<CrossProjectDependencyDto>,
    graph_integrity: WorkspaceGraphIntegrityDto,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TechnologyDto {
    category: String,
    name: String,
    classification: String,
    confidence: u8,
    evidence: Vec<EvidenceDto>,
    scope: String,
    repository_id: Option<String>,
    project_unit_id: Option<String>,
    detected_from: Vec<String>,
    detection_kind: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ComponentDto {
    id: String,
    name: String,
    kind: String,
    language: String,
    file: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct EntrypointDto {
    id: String,
    name: String,
    kind: String,
    component_id: String,
    file: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DependencyDto {
    source_id: String,
    target_id: String,
    kind: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DependencySliceDto {
    source_id: String,
    target_id: String,
    kind: String,
    evidence: String,
    confidence: u8,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct EvidenceDto {
    kind: String,
    source: String,
    detail: String,
    confidence: u8,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FrameworkDetectionDto {
    name: String,
    confidence: u8,
    evidence: Vec<EvidenceDto>,
    actionable: bool,
    direct_evidence: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RuntimeRequirementDto {
    name: String,
    executable: String,
    version_hint: Option<String>,
    required_by: String,
    confidence: u8,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SymbolMetadataDto {
    symbol_id: String,
    qualified_name: Option<String>,
    namespace: Option<String>,
    framework_kind: Option<String>,
    attributes: Vec<(String, String)>,
    evidence: Vec<EvidenceDto>,
    confidence: u8,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct EntrypointMetadataDto {
    entrypoint_id: String,
    method: Option<String>,
    path: Option<String>,
    route_name: Option<String>,
    domain: Option<String>,
    middleware: Vec<String>,
    controller: Option<String>,
    action: Option<String>,
    evidence: Vec<EvidenceDto>,
    confidence: u8,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DependencyMetadataDto {
    source_id: String,
    target_id: String,
    kind: String,
    evidence: Vec<EvidenceDto>,
    confidence: u8,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AnalysisDiagnosticDto {
    level: String,
    code: String,
    message: String,
    file: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AnalysisMetadataDto {
    framework_detections: Vec<FrameworkDetectionDto>,
    symbols: Vec<SymbolMetadataDto>,
    entrypoints: Vec<EntrypointMetadataDto>,
    dependencies: Vec<DependencyMetadataDto>,
    runtime_requirements: Vec<RuntimeRequirementDto>,
    diagnostics: Vec<AnalysisDiagnosticDto>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ProjectModelDto {
    root: String,
    name: String,
    files: usize,
    compatibility: String,
    technologies: Vec<TechnologyDto>,
    components: Vec<ComponentDto>,
    entrypoints: Vec<EntrypointDto>,
    dependencies: Vec<DependencyDto>,
    analysis: AnalysisMetadataDto,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AnalysisRecordDto {
    analysis_id: String,
    status: String,
    started_at_ms: u64,
    completed_at_ms: u64,
    duration_ms: u64,
    engine_version: String,
    registry_signature: String,
    branch: Option<String>,
    commit: Option<String>,
    model_fingerprint: String,
    files: usize,
    project_units: usize,
    components: usize,
    entrypoints: usize,
    dependencies: usize,
    cross_project_dependencies: usize,
    graph_passed: bool,
    graph_clean: bool,
    missing_sources: usize,
    missing_targets: usize,
    self_dependencies: usize,
    cycles: usize,
    diagnostics_info: usize,
    diagnostics_warning: usize,
    diagnostics_error: usize,
    framework_detections: usize,
    actionable_frameworks: usize,
    message: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct UnitGraphAuditDto {
    project_unit_id: String,
    project_unit_name: String,
    passed: bool,
    clean: bool,
    missing_sources: Vec<String>,
    missing_targets: Vec<String>,
    self_dependencies: usize,
    cycles: Vec<Vec<String>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RepositoryAuditDto {
    analysis: Option<AnalysisRecordDto>,
    units: Vec<UnitGraphAuditDto>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WorkspaceGraphIntegrityDto {
    passed: bool,
    error_count: usize,
    missing_source_units: Vec<String>,
    missing_target_units: Vec<String>,
    missing_source_components: Vec<String>,
    missing_target_components: Vec<String>,
    missing_target_entrypoints: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CapsuleSummaryDto {
    path: String,
    target: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct VerificationCheckDto {
    name: String,
    passed: bool,
    detail: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SandboxValidationPlanDto {
    runtime: String,
    manifest: String,
    suggested_command: String,
    network_required: bool,
    note: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct VerificationReportDto {
    passed: bool,
    kind: String,
    readiness_score: u8,
    checks: Vec<VerificationCheckDto>,
    sandbox_validation_plan: Vec<SandboxValidationPlanDto>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RuntimeCapabilitiesDto {
    docker: bool,
    java: bool,
    node: bool,
    python: bool,
    php: bool,
    composer: bool,
    tools: Vec<RuntimeToolDto>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RuntimeToolDto {
    name: String,
    executable: String,
    available: bool,
    version: Option<String>,
}

async fn run_blocking<T, F>(operation: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, String> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(operation)
        .await
        .map_err(|error| format!("Background operation failed: {error}"))?
}

#[tauri::command]
fn list_workspaces_command() -> Result<Vec<WorkspaceRecordDto>, String> {
    Ok(list_workspaces()
        .map_err(|error| error.to_string())?
        .into_iter()
        .map(workspace_record_dto)
        .collect())
}

#[tauri::command]
fn create_workspace_command(name: String) -> Result<WorkspaceRecordDto, String> {
    Ok(workspace_record_dto(
        create_workspace(&name).map_err(|error| error.to_string())?,
    ))
}

#[tauri::command]
fn add_repository_command(
    workspace_id: String,
    path: String,
    state: tauri::State<'_, DesktopState>,
) -> Result<RepositoryRecordDto, String> {
    let repository =
        add_repository(&workspace_id, Path::new(&path)).map_err(|error| error.to_string())?;
    state.invalidate(&workspace_id)?;
    Ok(repository_record_dto(repository))
}

#[tauri::command]
fn clone_repository_command(
    workspace_id: String,
    url: String,
    state: tauri::State<'_, DesktopState>,
) -> Result<RepositoryRecordDto, String> {
    let repository = clone_repository(&workspace_id, &url).map_err(|error| error.to_string())?;
    state.invalidate(&workspace_id)?;
    Ok(repository_record_dto(repository))
}

#[tauri::command]
fn list_repositories_command(workspace_id: String) -> Result<Vec<RepositoryRecordDto>, String> {
    Ok(list_repositories(&workspace_id)
        .map_err(|error| error.to_string())?
        .into_iter()
        .map(repository_record_dto)
        .collect())
}

#[tauri::command]
fn delete_repository_command(
    workspace_id: String,
    repository_id: String,
    state: tauri::State<'_, DesktopState>,
) -> Result<(), String> {
    delete_repository(&workspace_id, &repository_id).map_err(|error| error.to_string())?;
    state.invalidate(&workspace_id)
}

#[tauri::command]
async fn update_repository_command(
    workspace_id: String,
    repository_id: String,
    state: tauri::State<'_, DesktopState>,
) -> Result<RepositoryRecordDto, String> {
    let update_workspace_id = workspace_id.clone();
    let repository = run_blocking(move || {
        update_managed_repository(&update_workspace_id, &repository_id)
            .map_err(|error| error.to_string())
    })
    .await?;
    state.invalidate(&workspace_id)?;
    Ok(repository_record_dto(repository))
}

#[tauri::command]
async fn load_cached_workspace_command(
    workspace_id: String,
    state: tauri::State<'_, DesktopState>,
) -> Result<Option<WorkspaceModelDto>, String> {
    let lookup_id = workspace_id.clone();
    let workspace = run_blocking(move || {
        load_cached_workspace_model(&lookup_id).map_err(|error| error.to_string())
    })
    .await?;
    if let Some(workspace) = workspace {
        state.store(workspace.clone())?;
        return Ok(Some(workspace_model_dto(workspace)));
    }
    Ok(None)
}

#[tauri::command]
async fn scan_workspace_command(
    workspace_id: String,
    app: tauri::AppHandle,
    state: tauri::State<'_, DesktopState>,
) -> Result<WorkspaceModelDto, String> {
    let cancellation = state.begin_analysis(&workspace_id)?;
    let job_workspace_id = workspace_id.clone();
    let app_handle = app.clone();
    let result = run_blocking(move || {
        scan_workspace_controlled(
            &job_workspace_id,
            |progress| {
                let _ = app_handle.emit("analysis-progress", analysis_progress_dto(progress));
            },
            || cancellation.load(Ordering::Relaxed),
        )
        .map_err(|error| error.to_string())
    })
    .await;
    state.finish_analysis(&workspace_id)?;
    let workspace = result?;
    state.store(workspace.clone())?;
    Ok(workspace_model_dto(workspace))
}

#[tauri::command]
async fn scan_repository_command(
    workspace_id: String,
    repository_id: String,
    app: tauri::AppHandle,
    state: tauri::State<'_, DesktopState>,
) -> Result<WorkspaceModelDto, String> {
    let cancellation = state.begin_analysis(&workspace_id)?;
    let job_workspace_id = workspace_id.clone();
    let job_repository_id = repository_id.clone();
    let app_handle = app.clone();
    let result = run_blocking(move || {
        scan_workspace_repository_controlled(
            &job_workspace_id,
            &job_repository_id,
            |progress| {
                let _ = app_handle.emit("analysis-progress", analysis_progress_dto(progress));
            },
            || cancellation.load(Ordering::Relaxed),
        )
        .map_err(|error| error.to_string())
    })
    .await;
    state.finish_analysis(&workspace_id)?;
    let workspace = result?;
    state.store(workspace.clone())?;
    Ok(workspace_model_dto(workspace))
}

#[tauri::command]
fn cancel_analysis_command(
    workspace_id: String,
    state: tauri::State<'_, DesktopState>,
) -> Result<bool, String> {
    state.cancel_analysis(&workspace_id)
}

#[tauri::command]
async fn slice_workspace_dependencies_command(
    workspace_id: String,
    project_unit_id: String,
    target: String,
    state: tauri::State<'_, DesktopState>,
) -> Result<Vec<DependencySliceDto>, String> {
    let workspace = state.load(&workspace_id)?;
    run_blocking(move || slice_workspace_dependencies(workspace, project_unit_id, target)).await
}

fn slice_workspace_dependencies(
    workspace: WorkspaceModel,
    project_unit_id: String,
    target: String,
) -> Result<Vec<DependencySliceDto>, String> {
    let (unit_id, component_ids) =
        resolve_workspace_target_components(&workspace, &project_unit_id, &target)?;
    let starts = component_ids
        .into_iter()
        .map(|component_id| (unit_id.clone(), component_id))
        .collect::<Vec<_>>();
    Ok(workspace_dependency_slice(&workspace, &starts)
        .dependencies
        .into_iter()
        .map(dependency_slice_dto)
        .collect())
}

#[tauri::command]
async fn impact_workspace_dependencies_command(
    workspace_id: String,
    project_unit_id: String,
    target: String,
    state: tauri::State<'_, DesktopState>,
) -> Result<Vec<DependencySliceDto>, String> {
    let workspace = state.load(&workspace_id)?;
    run_blocking(move || impact_workspace_dependencies(workspace, project_unit_id, target)).await
}

fn impact_workspace_dependencies(
    workspace: WorkspaceModel,
    project_unit_id: String,
    target: String,
) -> Result<Vec<DependencySliceDto>, String> {
    let (unit_id, component_ids) =
        resolve_workspace_target_components(&workspace, &project_unit_id, &target)?;
    let starts = component_ids
        .into_iter()
        .map(|component_id| (unit_id.clone(), component_id))
        .collect::<Vec<_>>();
    Ok(workspace_impact_slice(&workspace, &starts)
        .dependencies
        .into_iter()
        .map(dependency_slice_dto)
        .collect())
}

fn resolve_workspace_target_components(
    workspace: &WorkspaceModel,
    project_unit_id: &str,
    target: &str,
) -> Result<(String, Vec<String>), String> {
    let unit = workspace
        .repositories
        .iter()
        .flat_map(|repository| &repository.project_units)
        .find(|unit| unit.id == project_unit_id)
        .ok_or_else(|| "Target project unit was not found".to_string())?;
    let component_ids = if target == format!("project-unit:{}", unit.id) {
        unit.model
            .components
            .iter()
            .map(|component| component.id.clone())
            .collect::<Vec<_>>()
    } else if let Some(component_id) = target.strip_prefix("component:") {
        unit.model
            .resolve_target_component_id(&format!("component:{component_id}"))
            .map(|component| vec![component.to_string()])
            .ok_or_else(|| "Target component was not found".to_string())?
    } else {
        unit.model
            .resolve_target_component_id(target)
            .map(|component| vec![component.to_string()])
            .ok_or_else(|| "Target entrypoint was not found".to_string())?
    };
    if component_ids.is_empty() {
        return Err("Target project unit has no components".to_string());
    }
    if component_ids.iter().any(|component_id| {
        !unit
            .model
            .components
            .iter()
            .any(|item| item.id == *component_id)
    }) {
        return Err("Target references a missing component".to_string());
    }
    Ok((unit.id.clone(), component_ids))
}

fn dependency_slice_dto(dependency: reposlice_graph::ScopedDependency) -> DependencySliceDto {
    DependencySliceDto {
        source_id: format!(
            "{}|{}",
            dependency.source_project_unit_id, dependency.source_component_id
        ),
        target_id: format!(
            "{}|{}",
            dependency.target_project_unit_id, dependency.target_component_id
        ),
        kind: dependency.kind,
        evidence: dependency.evidence,
        confidence: dependency.confidence,
    }
}

#[tauri::command]
async fn create_workspace_capsule_command(
    workspace_id: String,
    repository_id: String,
    project_unit_id: String,
    target: String,
    state: tauri::State<'_, DesktopState>,
) -> Result<CapsuleSummaryDto, String> {
    let workspace = state.load(&workspace_id)?;
    run_blocking(move || {
        create_workspace_capsule_for_target(
            workspace,
            workspace_id,
            repository_id,
            project_unit_id,
            target,
        )
    })
    .await
}

fn create_workspace_capsule_for_target(
    workspace: WorkspaceModel,
    workspace_id: String,
    repository_id: String,
    project_unit_id: String,
    target: String,
) -> Result<CapsuleSummaryDto, String> {
    let output = workspace_root().join("capsules").join(&workspace_id);
    let scoped = ScopedTarget {
        repository_id,
        project_unit_id,
        target_id: target.clone(),
    };
    let capsule = create_workspace_capsule(&workspace, &scoped, &output)
        .map_err(|error| error.to_string())?;
    Ok(CapsuleSummaryDto {
        path: capsule.to_string_lossy().to_string(),
        target,
    })
}

fn project_model_dto(model: ProjectModel) -> ProjectModelDto {
    let analysis = model.analysis;
    ProjectModelDto {
        root: display_path(&model.root),
        name: model.name,
        files: model.files,
        compatibility: model.compatibility.as_str().to_string(),
        technologies: model
            .technologies
            .into_iter()
            .map(|technology| TechnologyDto {
                category: technology.category,
                name: technology.name,
                classification: technology.classification,
                confidence: technology.confidence,
                evidence: evidence_dtos(technology.evidence),
                scope: technology.scope,
                repository_id: technology.repository_id,
                project_unit_id: technology.project_unit_id,
                detected_from: technology
                    .detected_from
                    .into_iter()
                    .map(|value| display_path(&value))
                    .collect(),
                detection_kind: technology.detection_kind,
            })
            .collect(),
        components: model
            .components
            .into_iter()
            .map(|component| ComponentDto {
                id: component.id,
                name: component.name,
                kind: component.kind.as_str().to_string(),
                language: component.language,
                file: display_path(&component.file),
            })
            .collect(),
        entrypoints: model
            .entrypoints
            .into_iter()
            .map(|entrypoint| EntrypointDto {
                id: entrypoint.id,
                name: entrypoint.name,
                kind: entrypoint.kind.as_str().to_string(),
                component_id: entrypoint.component_id,
                file: display_path(&entrypoint.file),
            })
            .collect(),
        dependencies: model
            .dependencies
            .into_iter()
            .map(|dependency| DependencyDto {
                source_id: dependency.source_id,
                target_id: dependency.target_id,
                kind: dependency.kind.as_str().to_string(),
            })
            .collect(),
        analysis: AnalysisMetadataDto {
            framework_detections: analysis
                .framework_detections
                .into_iter()
                .map(|item| {
                    let actionable = item.actionable();
                    let direct_evidence = item.has_direct_evidence();
                    FrameworkDetectionDto {
                        name: item.name,
                        confidence: item.confidence,
                        evidence: evidence_dtos(item.evidence),
                        actionable,
                        direct_evidence,
                    }
                })
                .collect(),
            symbols: analysis
                .symbols
                .into_iter()
                .map(|item| SymbolMetadataDto {
                    symbol_id: item.symbol_id,
                    qualified_name: item.qualified_name,
                    namespace: item.namespace,
                    framework_kind: item.framework_kind,
                    attributes: item.attributes,
                    evidence: evidence_dtos(item.evidence),
                    confidence: item.confidence,
                })
                .collect(),
            entrypoints: analysis
                .entrypoints
                .into_iter()
                .map(|item| EntrypointMetadataDto {
                    entrypoint_id: item.entrypoint_id,
                    method: item.method,
                    path: item.path,
                    route_name: item.route_name,
                    domain: item.domain,
                    middleware: item.middleware,
                    controller: item.controller,
                    action: item.action,
                    evidence: evidence_dtos(item.evidence),
                    confidence: item.confidence,
                })
                .collect(),
            dependencies: analysis
                .dependencies
                .into_iter()
                .map(|item| DependencyMetadataDto {
                    source_id: item.source_id,
                    target_id: item.target_id,
                    kind: item.kind,
                    evidence: evidence_dtos(item.evidence),
                    confidence: item.confidence,
                })
                .collect(),
            runtime_requirements: analysis
                .runtime_requirements
                .into_iter()
                .map(|item| RuntimeRequirementDto {
                    name: item.name,
                    executable: item.executable,
                    version_hint: item.version_hint,
                    required_by: item.required_by,
                    confidence: item.confidence,
                })
                .collect(),
            diagnostics: analysis
                .diagnostics
                .into_iter()
                .map(|item| AnalysisDiagnosticDto {
                    level: item.level.as_str().into(),
                    code: item.code,
                    message: item.message,
                    file: item.file.map(|value| display_path(&value)),
                })
                .collect(),
        },
    }
}

fn evidence_dtos(values: Vec<reposlice_core::Evidence>) -> Vec<EvidenceDto> {
    values
        .into_iter()
        .map(|item| EvidenceDto {
            kind: item.kind.as_str().into(),
            source: display_path(&item.source),
            detail: item.detail,
            confidence: item.confidence,
        })
        .collect()
}

#[tauri::command]
fn list_workspace_capsules_command(workspace_id: String) -> Result<Vec<CapsuleSummaryDto>, String> {
    let repositories = list_repositories(&workspace_id).map_err(|error| error.to_string())?;
    let mut capsules = list_capsules_in(&workspace_root().join("capsules").join(&workspace_id))
        .map_err(|error| error.to_string())?;
    for repository in repositories {
        capsules
            .extend(list_capsules(Path::new(&repository.path)).map_err(|error| error.to_string())?);
    }
    capsules.sort_by(|left, right| left.path.cmp(&right.path));
    capsules.dedup_by(|left, right| left.path == right.path);
    Ok(capsules.into_iter().map(capsule_dto).collect())
}

#[tauri::command]
async fn verify_capsule_command(path: String) -> Result<VerificationReportDto, String> {
    run_blocking(move || {
        let report = verify_capsule(Path::new(&path)).map_err(|error| error.to_string())?;
        let readiness_score = report.readiness_score();
        let passed = report.passed();
        let sandbox_validation_plan = sandbox_validation_plan(Path::new(&path))
            .map_err(|error| error.to_string())?
            .into_iter()
            .map(|plan| SandboxValidationPlanDto {
                runtime: plan.runtime,
                manifest: plan.manifest,
                suggested_command: plan.suggested_command,
                network_required: plan.network_required,
                note: plan.note,
            })
            .collect();

        Ok(VerificationReportDto {
            passed,
            kind: report.kind,
            readiness_score,
            checks: report
                .checks
                .into_iter()
                .map(|check| VerificationCheckDto {
                    name: check.name,
                    passed: check.passed,
                    detail: check.detail,
                })
                .collect(),
            sandbox_validation_plan,
        })
    })
    .await
}

#[tauri::command]
fn delete_workspace_command(
    workspace_id: String,
    state: tauri::State<'_, DesktopState>,
) -> Result<(), String> {
    reposlice_workspace::delete_workspace(&workspace_id).map_err(|e| e.to_string())?;
    state.invalidate(&workspace_id)
}

#[tauri::command]
fn reset_all_command(state: tauri::State<'_, DesktopState>) -> Result<(), String> {
    reposlice_workspace::reset_all().map_err(|e| e.to_string())?;
    state.clear()
}

#[tauri::command]
fn detect_runtime_command() -> Result<RuntimeCapabilitiesDto, String> {
    let runtime = detect_runtime();

    Ok(RuntimeCapabilitiesDto {
        docker: runtime.docker,
        java: runtime.java,
        node: runtime.node,
        python: runtime.python,
        php: runtime.php,
        composer: runtime.composer,
        tools: runtime
            .tools
            .into_iter()
            .map(|tool| RuntimeToolDto {
                name: tool.name,
                executable: tool.executable,
                available: tool.available,
                version: tool.version,
            })
            .collect(),
    })
}

fn capsule_dto(capsule: CapsuleSummary) -> CapsuleSummaryDto {
    CapsuleSummaryDto {
        path: capsule.path,
        target: capsule.target,
    }
}

fn analysis_progress_dto(progress: &WorkspaceScanProgress) -> AnalysisProgressDto {
    AnalysisProgressDto {
        workspace_id: progress.workspace_id.clone(),
        repository_id: progress.repository_id.clone(),
        repository_name: progress.repository_name.clone(),
        completed_repositories: progress.completed_repositories,
        total_repositories: progress.total_repositories,
        stage: progress.stage.clone(),
        reused_cache: progress.reused_cache,
    }
}

fn workspace_record_dto(workspace: WorkspaceRecord) -> WorkspaceRecordDto {
    WorkspaceRecordDto {
        id: workspace.id,
        name: workspace.name,
    }
}
fn repository_record_dto(repository: RepositoryRecord) -> RepositoryRecordDto {
    let origin = repository_origin(&repository.path);
    let analysis = latest_analysis_record(&repository.workspace_id, &repository.id)
        .ok()
        .flatten()
        .map(analysis_record_dto);
    RepositoryRecordDto {
        id: repository.id,
        workspace_id: repository.workspace_id,
        name: repository.name,
        path: display_path(&repository.path),
        origin,
        analysis,
    }
}

fn analysis_record_dto(record: AnalysisRecord) -> AnalysisRecordDto {
    AnalysisRecordDto {
        analysis_id: record.analysis_id,
        status: record.status.as_str().to_string(),
        started_at_ms: record.started_at_ms,
        completed_at_ms: record.completed_at_ms,
        duration_ms: record.duration_ms,
        engine_version: record.engine_version,
        registry_signature: record.registry_signature,
        branch: record.branch,
        commit: record.commit,
        model_fingerprint: record.model_fingerprint,
        files: record.files,
        project_units: record.project_units,
        components: record.components,
        entrypoints: record.entrypoints,
        dependencies: record.dependencies,
        cross_project_dependencies: record.cross_project_dependencies,
        graph_passed: record.graph_passed,
        graph_clean: record.graph_clean,
        missing_sources: record.missing_sources,
        missing_targets: record.missing_targets,
        self_dependencies: record.self_dependencies,
        cycles: record.cycles,
        diagnostics_info: record.diagnostics_info,
        diagnostics_warning: record.diagnostics_warning,
        diagnostics_error: record.diagnostics_error,
        framework_detections: record.framework_detections,
        actionable_frameworks: record.actionable_frameworks,
        message: record.message,
    }
}

fn repository_dto(repository: reposlice_core::Repository, workspace_id: &str) -> RepositoryDto {
    let audit_units = repository
        .project_units
        .iter()
        .map(|unit| {
            let report = validate_dependency_graph(&unit.model);
            UnitGraphAuditDto {
                project_unit_id: unit.id.clone(),
                project_unit_name: unit.name.clone(),
                passed: report.passed(),
                clean: report.is_clean(),
                missing_sources: report.missing_sources.into_iter().collect(),
                missing_targets: report.missing_targets.into_iter().collect(),
                self_dependencies: report.self_dependencies.len(),
                cycles: report.cycles,
            }
        })
        .collect::<Vec<_>>();
    let analysis = latest_analysis_record(workspace_id, &repository.id)
        .ok()
        .flatten()
        .map(analysis_record_dto);
    let origin = repository_origin(&repository.root);
    RepositoryDto {
        id: repository.id,
        name: repository.name,
        root: display_path(&repository.root),
        origin,
        git: repository.git.map(|git| GitMetadataDto {
            is_git_repository: git.is_git_repository,
            branch: git.branch,
            commit: git.commit,
            remote: git.remote,
        }),
        project_units: repository
            .project_units
            .into_iter()
            .map(|unit| ProjectUnitDto {
                id: unit.id,
                repository_id: unit.repository_id,
                root: display_path(&unit.root),
                name: unit.name,
                role: unit.role.as_str().to_string(),
                model: project_model_dto(unit.model),
                http_calls: unit
                    .http_calls
                    .into_iter()
                    .map(|call| HttpCallDto {
                        method: call.method,
                        path: call.path,
                        origin: call.origin,
                        component_id: call.component_id,
                        file: display_path(&call.file),
                        evidence: call.evidence,
                    })
                    .collect(),
            })
            .collect(),
        audit: RepositoryAuditDto {
            analysis,
            units: audit_units,
        },
    }
}

fn workspace_model_dto(workspace: WorkspaceModel) -> WorkspaceModelDto {
    let graph = validate_workspace_graph(&workspace);
    let WorkspaceModel {
        id,
        name,
        repositories,
        cross_project_dependencies,
    } = workspace;
    let repositories = repositories
        .into_iter()
        .map(|repository| repository_dto(repository, &id))
        .collect();
    WorkspaceModelDto {
        id,
        name,
        repositories,
        cross_project_dependencies: cross_project_dependencies
            .into_iter()
            .map(|dependency| CrossProjectDependencyDto {
                source_repository_id: dependency.source_repository_id,
                source_project_unit_id: dependency.source_project_unit_id,
                source_component_id: dependency.source_component_id,
                target_repository_id: dependency.target_repository_id,
                target_project_unit_id: dependency.target_project_unit_id,
                target_entrypoint_id: dependency.target_entrypoint_id,
                target_component_id: dependency.target_component_id,
                kind: dependency.kind.as_str().to_string(),
                evidence: dependency.evidence,
                confidence: dependency.confidence,
            })
            .collect(),
        graph_integrity: WorkspaceGraphIntegrityDto {
            passed: graph.passed(),
            error_count: graph.error_count(),
            missing_source_units: graph.missing_source_units.into_iter().collect(),
            missing_target_units: graph.missing_target_units.into_iter().collect(),
            missing_source_components: graph.missing_source_components.into_iter().collect(),
            missing_target_components: graph.missing_target_components.into_iter().collect(),
            missing_target_entrypoints: graph.missing_target_entrypoints.into_iter().collect(),
        },
    }
}

fn repository_origin(path: &str) -> String {
    let managed_root = workspace_root().join("repositories");
    let candidate = Path::new(path);
    let managed = fs_canonical_or_original(&managed_root);
    let target = fs_canonical_or_original(candidate);
    if target.starts_with(&managed) {
        "clone".to_string()
    } else {
        "local".to_string()
    }
}

fn fs_canonical_or_original(path: &Path) -> std::path::PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn display_path(value: &str) -> String {
    if let Some(rest) = value.strip_prefix(r"\\?\UNC\") {
        return format!(r"\\{}", rest);
    }
    value.strip_prefix(r"\\?\").unwrap_or(value).to_string()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    install_local_crash_reporting();
    tauri::Builder::default()
        .manage(DesktopState::default())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            if let Some(window) = app.get_webview_window("main") {
                let icon_bytes = include_bytes!("../icons/Logo_Desktop.png");
                let decoder = png::Decoder::new(std::io::Cursor::new(icon_bytes));
                if let Ok(mut reader) = decoder.read_info() {
                    let mut buf = vec![0; reader.output_buffer_size()];
                    if reader.next_frame(&mut buf).is_ok() {
                        let info = reader.info();
                        let _ = window.set_icon(tauri::image::Image::new_owned(
                            buf,
                            info.width,
                            info.height,
                        ));
                    }
                }
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            analyze_source_command,
            verify_capsule_command,
            detect_runtime_command,
            list_workspaces_command,
            create_workspace_command,
            delete_workspace_command,
            add_repository_command,
            clone_repository_command,
            list_repositories_command,
            delete_repository_command,
            update_repository_command,
            load_cached_workspace_command,
            scan_workspace_command,
            scan_repository_command,
            cancel_analysis_command,
            slice_workspace_dependencies_command,
            impact_workspace_dependencies_command,
            create_workspace_capsule_command,
            list_workspace_capsules_command,
            reset_all_command
        ])
        .run(tauri::generate_context!())
        .expect("RepoSlice desktop runtime failed");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn workspace(id: &str) -> WorkspaceModel {
        WorkspaceModel {
            id: id.to_string(),
            name: "Test Workspace".to_string(),
            repositories: Vec::new(),
            cross_project_dependencies: Vec::new(),
        }
    }

    #[test]
    fn desktop_state_stores_loads_and_invalidates_analysis() {
        let state = DesktopState::default();
        state.store(workspace("demo")).unwrap();
        assert_eq!(state.load("demo").unwrap().id, "demo");
        state.invalidate("demo").unwrap();
        assert!(state.load("demo").is_err());
    }

    #[test]
    fn desktop_state_bounds_in_memory_workspace_models() {
        let state = DesktopState::default();
        for index in 0..(DESKTOP_MODEL_CACHE_LIMIT + 2) {
            state
                .store(workspace(&format!("workspace-{index}")))
                .unwrap();
        }
        let cache = state.analyses.lock().unwrap();
        assert_eq!(cache.models.len(), DESKTOP_MODEL_CACHE_LIMIT);
        assert!(!cache.models.contains_key("workspace-0"));
        assert!(!cache.models.contains_key("workspace-1"));
        assert!(cache.models.contains_key("workspace-7"));
    }

    #[test]
    fn display_path_removes_windows_extended_length_prefix() {
        assert_eq!(display_path(r"\\?\C:\repos\demo"), r"C:\repos\demo");
        assert_eq!(
            display_path(r"\\?\UNC\server\share\demo"),
            r"\\server\share\demo"
        );
    }
}
