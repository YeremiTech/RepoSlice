use reposlice_core::{
    sha256_hex, slugify, stable_hash, GitMetadata, ProjectUnit, Repository, ScanPolicy, WorkspaceModel,
};
use reposlice_graph::{
    match_cross_project_dependencies, validate_dependency_graph, validate_workspace_graph,
};
use reposlice_scanner::{
    classify_project_role, discover_http_calls_for_unit, discover_project_units,
    scan_project_excluding_with_registry, AnalyzerRegistry,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Component as PathComponent, Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const DEFAULT_WORKSPACE_ID: &str = "default";

const WORKSPACE_MODEL_CACHE_SCHEMA: u32 = 3;

#[derive(Clone, Debug, Serialize, Deserialize)]
struct WorkspaceModelCache {
    schema: u32,
    engine_version: String,
    registry_signature: String,
    repository_states: BTreeMap<String, String>,
    model_sha256: String,
    model: WorkspaceModel,
}


#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceScanProgress {
    pub workspace_id: String,
    pub repository_id: Option<String>,
    pub repository_name: Option<String>,
    pub completed_repositories: usize,
    pub total_repositories: usize,
    pub stage: String,
    pub reused_cache: bool,
}

static REGISTRY_WRITE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
static ATOMIC_WRITE_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceProject {
    pub name: String,
    pub path: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceRecord {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RepositoryRecord {
    pub id: String,
    pub workspace_id: String,
    pub name: String,
    pub path: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AnalysisStatus {
    Completed,
    CompletedWithWarnings,
    Partial,
    Failed,
}

impl AnalysisStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Completed => "COMPLETED",
            Self::CompletedWithWarnings => "COMPLETED_WITH_WARNINGS",
            Self::Partial => "PARTIAL",
            Self::Failed => "FAILED",
        }
    }

    fn from_str(value: &str) -> Self {
        match value {
            "COMPLETED" => Self::Completed,
            "COMPLETED_WITH_WARNINGS" => Self::CompletedWithWarnings,
            "PARTIAL" => Self::Partial,
            _ => Self::Failed,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AnalysisRecord {
    pub workspace_id: String,
    pub repository_id: String,
    pub analysis_id: String,
    pub status: AnalysisStatus,
    pub started_at_ms: u64,
    pub completed_at_ms: u64,
    pub duration_ms: u64,
    pub engine_version: String,
    pub registry_signature: String,
    pub branch: Option<String>,
    pub commit: Option<String>,
    pub model_fingerprint: String,
    pub files: usize,
    pub project_units: usize,
    pub components: usize,
    pub entrypoints: usize,
    pub dependencies: usize,
    pub cross_project_dependencies: usize,
    pub graph_passed: bool,
    pub graph_clean: bool,
    pub missing_sources: usize,
    pub missing_targets: usize,
    pub self_dependencies: usize,
    pub cycles: usize,
    pub diagnostics_info: usize,
    pub diagnostics_warning: usize,
    pub diagnostics_error: usize,
    pub framework_detections: usize,
    pub actionable_frameworks: usize,
    pub message: String,
}


static CRASH_REPORTING_INSTALLED: OnceLock<()> = OnceLock::new();

pub fn install_local_crash_reporting() {
    if std::env::var("REPOSLICE_CRASH_REPORTING").ok().as_deref() != Some("local") {
        return;
    }
    CRASH_REPORTING_INSTALLED.get_or_init(|| {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let payload = info.payload().downcast_ref::<&str>().copied()
                .or_else(|| info.payload().downcast_ref::<String>().map(String::as_str))
                .unwrap_or("panic");
            let location = info.location().map(|location| format!("{}:{}:{}", location.file(), location.line(), location.column())).unwrap_or_else(|| "unknown".to_string());
            let report = serde_json::json!({
                "timestampMs": epoch_millis(),
                "version": env!("CARGO_PKG_VERSION"),
                "os": std::env::consts::OS,
                "arch": std::env::consts::ARCH,
                "location": location,
                "message": payload,
                "privacy": "local-only; repository source and file contents are not collected"
            });
            let directory = workspace_root().join("crash-reports");
            if fs::create_dir_all(&directory).is_ok() {
                let path = directory.join(format!("crash-{}.json", epoch_millis()));
                if let Ok(content) = serde_json::to_string_pretty(&report) {
                    let _ = fs::write(path, content);
                }
            }
            previous(info);
        }));
    });
}

pub fn workspace_root() -> PathBuf {
    if let Ok(value) = std::env::var("REPOSLICE_HOME") {
        return PathBuf::from(value);
    }
    if let Ok(value) = std::env::var("USERPROFILE") {
        return PathBuf::from(value).join(".reposlice");
    }
    if let Ok(value) = std::env::var("HOME") {
        return PathBuf::from(value).join(".reposlice");
    }
    PathBuf::from(".reposlice")
}

pub fn workspace_data_dir(workspace_id: &str) -> io::Result<PathBuf> {
    ensure_workspace(workspace_id)?;
    Ok(workspace_root().join("workspaces").join(workspace_id))
}

pub fn create_workspace(name: &str) -> io::Result<WorkspaceRecord> {
    let name = name.trim();
    if name.is_empty() || name.contains(['\t', '\n', '\r']) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Workspace name is invalid",
        ));
    }
    let _guard = acquire_registry_lock()?;
    let mut workspaces = read_workspaces()?;
    let base = slugify(name);
    if base.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Workspace name must contain a letter or number",
        ));
    }
    let mut id = base.clone();
    let mut suffix = 2;
    while id == DEFAULT_WORKSPACE_ID || workspaces.iter().any(|workspace| workspace.id == id) {
        id = format!("{base}-{suffix}");
        suffix += 1;
    }
    let workspace = WorkspaceRecord {
        id,
        name: name.to_string(),
    };
    workspaces.push(workspace.clone());
    write_workspaces(&workspaces)?;
    Ok(workspace)
}

pub fn list_workspaces() -> io::Result<Vec<WorkspaceRecord>> {
    let mut workspaces = read_workspaces()?;
    if let Some(default) = workspaces
        .iter_mut()
        .find(|workspace| workspace.id == DEFAULT_WORKSPACE_ID)
    {
        default.name = "Local Workspace".to_string();
    } else {
        workspaces.push(WorkspaceRecord {
            id: DEFAULT_WORKSPACE_ID.to_string(),
            name: "Local Workspace".to_string(),
        });
    }
    workspaces.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(workspaces)
}

pub fn add_repository(workspace_id: &str, path: &Path) -> io::Result<RepositoryRecord> {
    ensure_workspace(workspace_id)?;
    let selected = fs::canonicalize(path)?;
    let canonical = git_output(&selected, &["rev-parse", "--show-toplevel"])
        .and_then(|root| fs::canonicalize(root).ok())
        .unwrap_or(selected);
    if !canonical.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Repository path must be a directory",
        ));
    }
    let name = canonical
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("repository")
        .to_string();
    let canonical_string = canonical.to_string_lossy().to_string();
    let _guard = acquire_registry_lock()?;
    let mut repositories = read_repositories(workspace_id)?;
    if let Some(existing) = repositories
        .iter()
        .find(|repository| repository.path == canonical_string)
    {
        return Ok(existing.clone());
    }
    let record = RepositoryRecord {
        id: scoped_id(workspace_id, &canonical_string),
        workspace_id: workspace_id.to_string(),
        name,
        path: canonical_string,
    };
    repositories.push(record.clone());
    write_repositories(workspace_id, &repositories)?;
    invalidate_workspace_model_cache(workspace_id)?;
    Ok(record)
}

pub fn list_repositories(workspace_id: &str) -> io::Result<Vec<RepositoryRecord>> {
    ensure_workspace(workspace_id)?;
    let mut repositories = read_repositories(workspace_id)?;
    if workspace_id == DEFAULT_WORKSPACE_ID && repositories.is_empty() {
        let legacy = workspace_root().join("projects/registry.tsv");
        if legacy.is_file() {
            repositories = fs::read_to_string(legacy)?
                .lines()
                .filter_map(|line| {
                    let (name, path) = line.split_once('\t')?;
                    Path::new(path).is_dir().then(|| RepositoryRecord {
                        id: scoped_id(DEFAULT_WORKSPACE_ID, path),
                        workspace_id: DEFAULT_WORKSPACE_ID.to_string(),
                        name: name.to_string(),
                        path: path.to_string(),
                    })
                })
                .collect();
        }
    }
    let original_len = repositories.len();
    repositories.retain(|repository| Path::new(&repository.path).is_dir());
    if repositories.len() != original_len {
        let _guard = acquire_registry_lock()?;
        let mut latest = read_repositories(workspace_id)?;
        latest.retain(|repository| Path::new(&repository.path).is_dir());
        write_repositories(workspace_id, &latest)?;
        repositories = latest;
    }
    repositories.sort_by(|left, right| left.name.cmp(&right.name).then(left.path.cmp(&right.path)));
    Ok(repositories)
}

pub fn clone_repository(workspace_id: &str, url: &str) -> io::Result<RepositoryRecord> {
    ensure_workspace(workspace_id)?;
    let url = url.trim();
    if url.is_empty() || url.contains(['\n', '\r', '\0']) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Repository URL is invalid",
        ));
    }
    let safe_remote = sanitize_remote_url(url);
    let base_name = repository_name_from_url(&safe_remote);
    let repositories_root = workspace_root().join("repositories");
    fs::create_dir_all(&repositories_root)?;
    let mut destination = repositories_root.join(&base_name);
    let mut suffix = 2;
    while destination.exists() {
        destination = repositories_root.join(format!("{base_name}-{suffix}"));
        suffix += 1;
    }
    let output = Command::new("git")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GCM_INTERACTIVE", "Never")
        .arg("clone")
        .arg("--")
        .arg(url)
        .arg(&destination)
        .output()?;
    if !output.status.success() {
        remove_failed_clone(&repositories_root, &destination);
        // Do not surface Git's stderr: credential-bearing URLs can be echoed
        // in transport errors in encoded or normalized forms that are unsafe
        // to redact reliably.
        return Err(io::Error::other(format!(
            "Git clone failed (exit code {})",
            output.status.code().unwrap_or(-1)
        )));
    }
    // Git stores the source as `remote.origin.url`. Authentication can be used
    // for the transfer, but credentials and query tokens must never remain on disk.
    if safe_remote != url {
        let sanitized = Command::new("git")
            .arg("-C")
            .arg(&destination)
            .args(["remote", "set-url", "origin", "--"])
            .arg(&safe_remote)
            .output()?;
        if !sanitized.status.success() {
            remove_failed_clone(&repositories_root, &destination);
            return Err(io::Error::other(
                "Repository cloned, but its authenticated remote could not be sanitized",
            ));
        }
    }
    add_repository(workspace_id, &destination)
}

pub fn git_metadata(path: &Path) -> Option<GitMetadata> {
    let top = git_output(path, &["rev-parse", "--show-toplevel"])?;
    if top.is_empty() {
        return None;
    }
    Some(GitMetadata {
        is_git_repository: true,
        branch: git_output(path, &["branch", "--show-current"]).filter(|value| !value.is_empty()),
        commit: git_output(path, &["rev-parse", "HEAD"]).filter(|value| !value.is_empty()),
        remote: git_output(path, &["remote", "get-url", "origin"])
            .filter(|value| !value.is_empty())
            .map(|value| sanitize_remote_url(&value)),
    })
}

pub fn scan_repository(record: &RepositoryRecord) -> anyhow::Result<Repository> {
    let registry = AnalyzerRegistry::with_defaults();
    scan_repository_with_registry(record, &registry)
}

pub fn scan_repository_with_registry(
    repository_record: &RepositoryRecord,
    registry: &AnalyzerRegistry,
) -> anyhow::Result<Repository> {
    let started_at_ms = epoch_millis();
    let timer = Instant::now();
    match scan_repository_untracked(repository_record, registry) {
        Ok(repository) => {
            let completed_at_ms = epoch_millis();
            let record = analysis_record_from_repository(
                repository_record,
                &repository,
                registry,
                started_at_ms,
                completed_at_ms,
                timer.elapsed().as_millis().min(u64::MAX as u128) as u64,
            );
            write_analysis_record(&record)?;
            Ok(repository)
        }
        Err(error) => {
            let completed_at_ms = epoch_millis();
            let record = failed_analysis_record(
                repository_record,
                registry,
                started_at_ms,
                completed_at_ms,
                timer.elapsed().as_millis().min(u64::MAX as u128) as u64,
                &error.to_string(),
            );
            let _ = write_analysis_record(&record);
            Err(error)
        }
    }
}

fn scan_repository_untracked(
    record: &RepositoryRecord,
    registry: &AnalyzerRegistry,
) -> anyhow::Result<Repository> {
    let root = fs::canonicalize(&record.path)?;
    let mut project_units = Vec::new();
    let unit_roots = discover_project_units(&root)?;
    for unit_root in &unit_roots {
        let descendants: Vec<PathBuf> = unit_roots
            .iter()
            .filter(|other| *other != unit_root && other.starts_with(unit_root))
            .cloned()
            .collect();
        let mut model = scan_project_excluding_with_registry(unit_root, &descendants, registry)?;
        let role = classify_project_role(&model);
        let http_calls = discover_http_calls_for_unit(unit_root, &model.components)?;
        let relative = unit_root
            .strip_prefix(&root)
            .unwrap_or(unit_root)
            .to_string_lossy();
        let unit_key = if relative.is_empty() {
            "."
        } else {
            relative.as_ref()
        };
        let project_unit_id = scoped_id(&record.id, unit_key);
        for technology in &mut model.technologies {
            technology.repository_id = Some(record.id.clone());
            technology.project_unit_id = Some(project_unit_id.clone());
            technology.scope = "project-unit".into();
            if technology.evidence.is_empty() {
                technology.evidence.push(reposlice_core::Evidence {
                    kind: reposlice_core::EvidenceKind::Inference,
                    source: model.root.clone(),
                    detail: format!("Detected in project unit {}", model.name),
                    confidence: technology.confidence,
                });
            }
        }
        project_units.push(ProjectUnit {
            id: project_unit_id,
            repository_id: record.id.clone(),
            root: model.root.clone(),
            name: model.name.clone(),
            role,
            model,
            http_calls,
        });
    }
    Ok(Repository {
        id: record.id.clone(),
        name: record.name.clone(),
        root: root.to_string_lossy().to_string(),
        git: git_metadata(&root),
        project_units,
    })
}

pub fn scan_workspace(workspace_id: &str) -> anyhow::Result<WorkspaceModel> {
    let registry = AnalyzerRegistry::with_defaults();
    scan_workspace_with_registry(workspace_id, &registry)
}

pub fn scan_workspace_with_registry(
    workspace_id: &str,
    registry: &AnalyzerRegistry,
) -> anyhow::Result<WorkspaceModel> {
    scan_workspace_incremental(workspace_id, None, registry, |_| {}, || false)
}

pub fn scan_workspace_controlled<F, C>(
    workspace_id: &str,
    on_progress: F,
    should_cancel: C,
) -> anyhow::Result<WorkspaceModel>
where
    F: FnMut(&WorkspaceScanProgress),
    C: Fn() -> bool,
{
    let registry = AnalyzerRegistry::with_defaults();
    scan_workspace_incremental(workspace_id, None, &registry, on_progress, should_cancel)
}

pub fn scan_workspace_repository(
    workspace_id: &str,
    repository_id: &str,
) -> anyhow::Result<WorkspaceModel> {
    let registry = AnalyzerRegistry::with_defaults();
    scan_workspace_repository_with_registry(workspace_id, repository_id, &registry)
}

pub fn scan_workspace_repository_with_registry(
    workspace_id: &str,
    repository_id: &str,
    registry: &AnalyzerRegistry,
) -> anyhow::Result<WorkspaceModel> {
    scan_workspace_incremental(workspace_id, Some(repository_id), registry, |_| {}, || false)
}

pub fn scan_workspace_repository_controlled<F, C>(
    workspace_id: &str,
    repository_id: &str,
    on_progress: F,
    should_cancel: C,
) -> anyhow::Result<WorkspaceModel>
where
    F: FnMut(&WorkspaceScanProgress),
    C: Fn() -> bool,
{
    let registry = AnalyzerRegistry::with_defaults();
    scan_workspace_incremental(
        workspace_id,
        Some(repository_id),
        &registry,
        on_progress,
        should_cancel,
    )
}

/// Loads a persisted workspace model when it still matches the registered repositories,
/// analyzer registry, engine version, and current repository state. Stale caches are ignored.
pub fn load_cached_workspace_model(workspace_id: &str) -> io::Result<Option<WorkspaceModel>> {
    let registry = AnalyzerRegistry::with_defaults();
    load_cached_workspace_model_with_registry(workspace_id, &registry)
}

pub fn load_cached_workspace_model_with_registry(
    workspace_id: &str,
    registry: &AnalyzerRegistry,
) -> io::Result<Option<WorkspaceModel>> {
    let workspace = workspace_record(workspace_id)?;
    let records = list_repositories(workspace_id)?;
    let Some(cache) = read_workspace_model_cache(workspace_id)? else {
        return Ok(None);
    };
    if !cache_metadata_matches(&cache, &workspace, &records, registry) {
        return Ok(None);
    }
    for record in &records {
        let Some(cached_state) = cache.repository_states.get(&record.id) else {
            return Ok(None);
        };
        if *cached_state != repository_state_signature(record)? {
            return Ok(None);
        }
    }
    Ok(Some(cache.model))
}

fn scan_workspace_incremental<F, C>(
    workspace_id: &str,
    force_repository_id: Option<&str>,
    registry: &AnalyzerRegistry,
    mut on_progress: F,
    should_cancel: C,
) -> anyhow::Result<WorkspaceModel>
where
    F: FnMut(&WorkspaceScanProgress),
    C: Fn() -> bool,
{
    let workspace = workspace_record(workspace_id)?;
    let records = list_repositories(workspace_id)?;
    if let Some(repository_id) = force_repository_id {
        if !records.iter().any(|record| record.id == repository_id) {
            return Err(io::Error::new(io::ErrorKind::NotFound, "Repository was not found").into());
        }
    }

    let total_repositories = records.len();
    on_progress(&WorkspaceScanProgress {
        workspace_id: workspace_id.to_string(),
        repository_id: None,
        repository_name: None,
        completed_repositories: 0,
        total_repositories,
        stage: "preparing".to_string(),
        reused_cache: false,
    });

    let cache = read_workspace_model_cache(workspace_id)?
        .filter(|cache| cache_metadata_matches(cache, &workspace, &records, registry));
    let cached_repositories = cache
        .as_ref()
        .map(|cache| {
            cache
                .model
                .repositories
                .iter()
                .map(|repository| (repository.id.clone(), repository.clone()))
                .collect::<BTreeMap<_, _>>()
        })
        .unwrap_or_default();

    let mut repository_states = BTreeMap::new();
    let mut repositories = Vec::with_capacity(records.len());
    for (index, record) in records.iter().enumerate() {
        if should_cancel() {
            return Err(io::Error::new(io::ErrorKind::Interrupted, "Analysis was cancelled").into());
        }
        let current_state = repository_state_signature(record)?;
        let force = force_repository_id == Some(record.id.as_str());
        let cached_is_current = !force
            && cache
                .as_ref()
                .and_then(|cache| cache.repository_states.get(&record.id))
                .is_some_and(|state| state == &current_state);

        on_progress(&WorkspaceScanProgress {
            workspace_id: workspace_id.to_string(),
            repository_id: Some(record.id.clone()),
            repository_name: Some(record.name.clone()),
            completed_repositories: index,
            total_repositories,
            stage: if cached_is_current { "cache" } else { "scanning" }.to_string(),
            reused_cache: cached_is_current,
        });
        let repository = if cached_is_current {
            if let Some(cached) = cached_repositories.get(&record.id) {
                cached.clone()
            } else {
                scan_repository_with_registry(record, registry)?
            }
        } else {
            scan_repository_with_registry(record, registry)?
        };
        if should_cancel() {
            return Err(io::Error::new(io::ErrorKind::Interrupted, "Analysis was cancelled").into());
        }
        repository_states.insert(record.id.clone(), repository_state_signature(record)?);
        repositories.push(repository);
        on_progress(&WorkspaceScanProgress {
            workspace_id: workspace_id.to_string(),
            repository_id: Some(record.id.clone()),
            repository_name: Some(record.name.clone()),
            completed_repositories: index + 1,
            total_repositories,
            stage: "repository-complete".to_string(),
            reused_cache: cached_is_current,
        });
    }

    if should_cancel() {
        return Err(io::Error::new(io::ErrorKind::Interrupted, "Analysis was cancelled").into());
    }
    on_progress(&WorkspaceScanProgress {
        workspace_id: workspace_id.to_string(),
        repository_id: None,
        repository_name: None,
        completed_repositories: total_repositories,
        total_repositories,
        stage: "validating".to_string(),
        reused_cache: false,
    });
    ensure_workspace_snapshot_unchanged(workspace_id, &records, &repository_states)?;
    if should_cancel() {
        return Err(io::Error::new(io::ErrorKind::Interrupted, "Analysis was cancelled").into());
    }
    on_progress(&WorkspaceScanProgress {
        workspace_id: workspace_id.to_string(),
        repository_id: None,
        repository_name: None,
        completed_repositories: total_repositories,
        total_repositories,
        stage: "linking".to_string(),
        reused_cache: false,
    });
    let mut model = WorkspaceModel {
        id: workspace.id,
        name: workspace.name,
        repositories,
        cross_project_dependencies: Vec::new(),
    };
    model.cross_project_dependencies = match_cross_project_dependencies(&model);
    let workspace_integrity = validate_workspace_graph(&model);
    if !workspace_integrity.passed() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "Workspace graph integrity validation failed with {} unresolved cross-project reference(s)",
                workspace_integrity.error_count()
            ),
        )
        .into());
    }
    ensure_workspace_snapshot_unchanged(workspace_id, &records, &repository_states)?;
    persist_cross_project_counts(&model)?;
    persist_workspace_model_cache(&model, registry, repository_states)?;
    on_progress(&WorkspaceScanProgress {
        workspace_id: workspace_id.to_string(),
        repository_id: None,
        repository_name: None,
        completed_repositories: total_repositories,
        total_repositories,
        stage: "completed".to_string(),
        reused_cache: false,
    });
    Ok(model)
}

fn workspace_record(workspace_id: &str) -> io::Result<WorkspaceRecord> {
    list_workspaces()?
        .into_iter()
        .find(|workspace| workspace.id == workspace_id)
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Workspace was not found"))
}

fn workspace_model_cache_path(workspace_id: &str) -> PathBuf {
    workspace_root()
        .join("workspaces")
        .join(workspace_id)
        .join("model-cache.json")
}

fn read_workspace_model_cache(workspace_id: &str) -> io::Result<Option<WorkspaceModelCache>> {
    let path = workspace_model_cache_path(workspace_id);
    let Some(content) = read_registry_content(&path)? else {
        return Ok(None);
    };
    match serde_json::from_str(&content) {
        Ok(cache) => Ok(Some(cache)),
        Err(_) => {
            quarantine_corrupt_cache(&path);
            Ok(None)
        }
    }
}

fn persist_workspace_model_cache(
    model: &WorkspaceModel,
    registry: &AnalyzerRegistry,
    repository_states: BTreeMap<String, String>,
) -> io::Result<()> {
    let _guard = acquire_registry_lock()?;
    let cache = WorkspaceModelCache {
        schema: WORKSPACE_MODEL_CACHE_SCHEMA,
        engine_version: env!("CARGO_PKG_VERSION").to_string(),
        registry_signature: stable_hash(&registry.signature()),
        repository_states,
        model_sha256: workspace_model_sha256(model)?,
        model: model.clone(),
    };
    let content = serde_json::to_string(&cache).map_err(|error| {
        io::Error::other(format!(
            "Workspace model cache could not be serialized: {error}"
        ))
    })?;
    atomic_write(&workspace_model_cache_path(&model.id), &content)
}

fn cache_metadata_matches(
    cache: &WorkspaceModelCache,
    workspace: &WorkspaceRecord,
    records: &[RepositoryRecord],
    registry: &AnalyzerRegistry,
) -> bool {
    if cache.schema != WORKSPACE_MODEL_CACHE_SCHEMA
        || cache.engine_version != env!("CARGO_PKG_VERSION")
        || cache.registry_signature != stable_hash(&registry.signature())
        || cache.model.id != workspace.id
        || cache.model.name != workspace.name
        || cache.model.repositories.len() != records.len()
    {
        return false;
    }
    if workspace_model_sha256(&cache.model).ok().as_deref() != Some(cache.model_sha256.as_str())
        || !validate_workspace_graph(&cache.model).passed()
    {
        return false;
    }

    records.iter().all(|record| {
        cache.model.repositories.iter().any(|repository| {
            repository.id == record.id && paths_equivalent(&repository.root, &record.path)
        })
    })
}

fn paths_equivalent(left: &str, right: &str) -> bool {
    let left = fs::canonicalize(left).unwrap_or_else(|_| PathBuf::from(left));
    let right = fs::canonicalize(right).unwrap_or_else(|_| PathBuf::from(right));
    left == right
}

fn repository_state_signature(record: &RepositoryRecord) -> io::Result<String> {
    let root = Path::new(&record.path);
    if let Some(commit) = git_output(root, &["rev-parse", "HEAD"]) {
        let status = git_output(root, &["status", "--porcelain=v1", "--untracked-files=all"])
            .unwrap_or_default();
        if status.is_empty() {
            return Ok(stable_hash(&format!("git-clean\n{commit}")));
        }

        // A porcelain status line only describes *which* paths are dirty. Its shape can stay
        // identical while the actual contents continue changing, which would make a persisted
        // workspace cache stale. Include Git's content diff plus the filesystem signature so
        // tracked, staged and untracked edits all invalidate the repository model deterministically.
        let diff = git_output(
            root,
            &["diff", "--no-ext-diff", "--no-textconv", "--binary", "HEAD", "--"],
        )
        .unwrap_or_default();
        let filesystem = filesystem_state_signature(root)?;
        return Ok(stable_hash(&format!(
            "git-dirty\n{commit}\n{status}\n{}\n{filesystem}",
            stable_hash(&diff)
        )));
    }
    filesystem_state_signature(root)
}

fn filesystem_state_signature(root: &Path) -> io::Result<String> {
    let canonical = fs::canonicalize(root)?;
    let policy = ScanPolicy::default();
    let mut rows = Vec::new();
    collect_filesystem_state(&canonical, &canonical, &policy, &mut rows)?;
    rows.sort();
    Ok(stable_hash(&rows.join("\n")))
}

fn collect_filesystem_state(
    root: &Path,
    directory: &Path,
    policy: &ScanPolicy,
    rows: &mut Vec<String>,
) -> io::Result<()> {
    let mut entries = match fs::read_dir(directory) {
        Ok(entries) => entries.filter_map(Result::ok).collect::<Vec<_>>(),
        Err(error) if error.kind() == io::ErrorKind::PermissionDenied => return Ok(()),
        Err(error) => return Err(error),
    };
    entries.sort_by_key(|entry| entry.path());
    for entry in entries {
        let path = entry.path();
        if policy.is_excluded_from(root, &path) {
            continue;
        }
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(_) => continue,
        };
        if metadata.file_type().is_symlink() {
            continue;
        }
        if metadata.is_dir() {
            collect_filesystem_state(root, &path, policy, rows)?;
            continue;
        }
        if !metadata.is_file() || !policy.accepts_source_from(root, &path, metadata.len()) {
            continue;
        }
        let modified = metadata
            .modified()
            .ok()
            .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
            .map(|value| value.as_nanos())
            .unwrap_or(0);
        let relative = path.strip_prefix(root).unwrap_or(&path).to_string_lossy();
        rows.push(format!("{relative}\t{}\t{modified}", metadata.len()));
    }
    Ok(())
}

fn workspace_model_sha256(model: &WorkspaceModel) -> io::Result<String> {
    let bytes = serde_json::to_vec(model).map_err(|error| {
        io::Error::other(format!("Workspace model could not be serialized for integrity validation: {error}"))
    })?;
    Ok(format!("sha256:{}", sha256_hex(&bytes)))
}

fn ensure_workspace_snapshot_unchanged(
    workspace_id: &str,
    expected_records: &[RepositoryRecord],
    expected_states: &BTreeMap<String, String>,
) -> io::Result<()> {
    let current_records = list_repositories(workspace_id)?;
    if current_records != expected_records {
        return Err(io::Error::new(
            io::ErrorKind::Interrupted,
            "Workspace repositories changed while analysis was running; rerun the analysis to produce a coherent snapshot",
        ));
    }
    for record in expected_records {
        let Some(expected_state) = expected_states.get(&record.id) else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Workspace analysis is missing a repository state signature",
            ));
        };
        if repository_state_signature(record)? != *expected_state {
            return Err(io::Error::new(
                io::ErrorKind::Interrupted,
                format!(
                    "Repository '{}' changed while analysis was running; rerun the analysis to avoid mixing source revisions",
                    record.name
                ),
            ));
        }
    }
    Ok(())
}

fn quarantine_corrupt_cache(path: &Path) {
    if !path.is_file() {
        return;
    }
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("model-cache.json");
    let quarantine = path
        .parent()
        .unwrap_or(Path::new("."))
        .join(format!(".{file_name}.corrupt.{}", epoch_millis()));
    let _ = fs::rename(path, quarantine);
}

pub fn list_analysis_records(workspace_id: &str) -> io::Result<Vec<AnalysisRecord>> {
    ensure_workspace(workspace_id)?;
    let directory = analysis_records_dir(workspace_id);
    if !directory.is_dir() {
        return Ok(Vec::new());
    }
    let mut records = fs::read_dir(directory)?
        .filter_map(Result::ok)
        .filter_map(|entry| read_analysis_record_file(&entry.path()).ok().flatten())
        .collect::<Vec<_>>();
    records.sort_by(|left, right| right.completed_at_ms.cmp(&left.completed_at_ms));
    Ok(records)
}

pub fn latest_analysis_record(
    workspace_id: &str,
    repository_id: &str,
) -> io::Result<Option<AnalysisRecord>> {
    ensure_workspace(workspace_id)?;
    read_analysis_record_file(&analysis_record_path(workspace_id, repository_id))
}

fn analysis_record_from_repository(
    repository_record: &RepositoryRecord,
    repository: &Repository,
    registry: &AnalyzerRegistry,
    started_at_ms: u64,
    completed_at_ms: u64,
    duration_ms: u64,
) -> AnalysisRecord {
    let reports = repository
        .project_units
        .iter()
        .map(|unit| validate_dependency_graph(&unit.model))
        .collect::<Vec<_>>();
    let graph_passed = reports.iter().all(|report| report.passed());
    let graph_clean = reports.iter().all(|report| report.is_clean());
    let missing_sources = reports.iter().map(|report| report.missing_sources.len()).sum();
    let missing_targets = reports.iter().map(|report| report.missing_targets.len()).sum();
    let self_dependencies = reports.iter().map(|report| report.self_dependencies.len()).sum();
    let cycles = reports.iter().map(|report| report.cycles.len()).sum();
    let diagnostics_info = repository.project_units.iter().map(|unit| {
        unit.model.analysis.diagnostics.iter().filter(|item| item.level.as_str() == "info").count()
    }).sum();
    let diagnostics_warning = repository.project_units.iter().map(|unit| {
        unit.model.analysis.diagnostics.iter().filter(|item| item.level.as_str() == "warning").count()
    }).sum();
    let diagnostics_error = repository.project_units.iter().map(|unit| {
        unit.model.analysis.diagnostics.iter().filter(|item| item.level.as_str() == "error").count()
    }).sum();
    let framework_detections = repository.project_units.iter().map(|unit| unit.model.analysis.framework_detections.len()).sum();
    let actionable_frameworks = repository.project_units.iter().map(|unit| {
        unit.model.analysis.framework_detections.iter().filter(|item| item.actionable_with(registry.detection_policy())).count()
    }).sum();
    let status = if !graph_passed || diagnostics_error > 0 {
        AnalysisStatus::Partial
    } else if !graph_clean || diagnostics_warning > 0 {
        AnalysisStatus::CompletedWithWarnings
    } else {
        AnalysisStatus::Completed
    };
    let message = match status {
        AnalysisStatus::Completed => "Analysis completed and graph integrity is clean",
        AnalysisStatus::CompletedWithWarnings => "Analysis completed with non-blocking warnings",
        AnalysisStatus::Partial => "Analysis completed with integrity errors or error diagnostics",
        AnalysisStatus::Failed => "Analysis failed",
    }.to_string();
    AnalysisRecord {
        workspace_id: repository_record.workspace_id.clone(),
        repository_id: repository_record.id.clone(),
        analysis_id: analysis_id(&repository_record.id, completed_at_ms),
        status,
        started_at_ms,
        completed_at_ms,
        duration_ms,
        engine_version: env!("CARGO_PKG_VERSION").to_string(),
        registry_signature: stable_hash(&registry.signature()),
        branch: repository.git.as_ref().and_then(|git| git.branch.clone()),
        commit: repository.git.as_ref().and_then(|git| git.commit.clone()),
        model_fingerprint: repository_fingerprint(repository),
        files: repository.project_units.iter().map(|unit| unit.model.files).sum(),
        project_units: repository.project_units.len(),
        components: repository.project_units.iter().map(|unit| unit.model.components.len()).sum(),
        entrypoints: repository.project_units.iter().map(|unit| unit.model.entrypoints.len()).sum(),
        dependencies: repository.project_units.iter().map(|unit| unit.model.dependencies.len()).sum(),
        cross_project_dependencies: 0,
        graph_passed,
        graph_clean,
        missing_sources,
        missing_targets,
        self_dependencies,
        cycles,
        diagnostics_info,
        diagnostics_warning,
        diagnostics_error,
        framework_detections,
        actionable_frameworks,
        message,
    }
}

fn failed_analysis_record(
    repository_record: &RepositoryRecord,
    registry: &AnalyzerRegistry,
    started_at_ms: u64,
    completed_at_ms: u64,
    duration_ms: u64,
    error: &str,
) -> AnalysisRecord {
    let git = git_metadata(Path::new(&repository_record.path));
    AnalysisRecord {
        workspace_id: repository_record.workspace_id.clone(),
        repository_id: repository_record.id.clone(),
        analysis_id: analysis_id(&repository_record.id, completed_at_ms),
        status: AnalysisStatus::Failed,
        started_at_ms,
        completed_at_ms,
        duration_ms,
        engine_version: env!("CARGO_PKG_VERSION").to_string(),
        registry_signature: stable_hash(&registry.signature()),
        branch: git.as_ref().and_then(|metadata| metadata.branch.clone()),
        commit: git.as_ref().and_then(|metadata| metadata.commit.clone()),
        model_fingerprint: String::new(),
        files: 0,
        project_units: 0,
        components: 0,
        entrypoints: 0,
        dependencies: 0,
        cross_project_dependencies: 0,
        graph_passed: false,
        graph_clean: false,
        missing_sources: 0,
        missing_targets: 0,
        self_dependencies: 0,
        cycles: 0,
        diagnostics_info: 0,
        diagnostics_warning: 0,
        diagnostics_error: 1,
        framework_detections: 0,
        actionable_frameworks: 0,
        message: normalize_record_value(error),
    }
}

fn persist_cross_project_counts(workspace: &WorkspaceModel) -> io::Result<()> {
    for repository in &workspace.repositories {
        let Some(mut record) = latest_analysis_record(&workspace.id, &repository.id)? else {
            continue;
        };
        record.cross_project_dependencies = workspace
            .cross_project_dependencies
            .iter()
            .filter(|dependency| {
                dependency.source_repository_id == repository.id
                    || dependency.target_repository_id == repository.id
            })
            .count();
        write_analysis_record(&record)?;
    }
    Ok(())
}

fn repository_fingerprint(repository: &Repository) -> String {
    let mut rows = Vec::new();
    for unit in &repository.project_units {
        rows.push(format!("unit:{}:{}:{}", unit.id, unit.model.files, unit.model.compatibility.as_str()));
        for technology in &unit.model.technologies {
            rows.push(format!("technology:{}:{}:{}", technology.category, technology.name, technology.confidence));
        }
        for component in &unit.model.components {
            rows.push(format!("component:{}:{}:{}", component.id, component.kind.as_str(), component.file));
        }
        for entrypoint in &unit.model.entrypoints {
            rows.push(format!("entrypoint:{}:{}:{}", entrypoint.id, entrypoint.kind.as_str(), entrypoint.component_id));
        }
        for dependency in &unit.model.dependencies {
            rows.push(format!("dependency:{}:{}:{}", dependency.source_id, dependency.target_id, dependency.kind.as_str()));
        }
    }
    rows.sort();
    stable_hash(&rows.join("\n"))
}

fn analysis_id(repository_id: &str, completed_at_ms: u64) -> String {
    let seed = format!("{repository_id}:{completed_at_ms}");
    format!("ANL-{completed_at_ms}-{}", &stable_hash(&seed)[..8])
}

fn epoch_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u64::MAX as u128) as u64
}

fn analysis_records_dir(workspace_id: &str) -> PathBuf {
    workspace_root().join("workspaces").join(workspace_id).join("analysis")
}

fn analysis_record_path(workspace_id: &str, repository_id: &str) -> PathBuf {
    analysis_records_dir(workspace_id).join(format!("{}.audit", stable_hash(repository_id)))
}

fn write_analysis_record(record: &AnalysisRecord) -> io::Result<()> {
    let _guard = acquire_registry_lock()?;
    let path = analysis_record_path(&record.workspace_id, &record.repository_id);
    fs::create_dir_all(path.parent().unwrap_or(Path::new(".")))?;
    let values = [
        ("workspace_id", record.workspace_id.clone()),
        ("repository_id", record.repository_id.clone()),
        ("analysis_id", record.analysis_id.clone()),
        ("status", record.status.as_str().to_string()),
        ("started_at_ms", record.started_at_ms.to_string()),
        ("completed_at_ms", record.completed_at_ms.to_string()),
        ("duration_ms", record.duration_ms.to_string()),
        ("engine_version", record.engine_version.clone()),
        ("registry_signature", record.registry_signature.clone()),
        ("branch", record.branch.clone().unwrap_or_default()),
        ("commit", record.commit.clone().unwrap_or_default()),
        ("model_fingerprint", record.model_fingerprint.clone()),
        ("files", record.files.to_string()),
        ("project_units", record.project_units.to_string()),
        ("components", record.components.to_string()),
        ("entrypoints", record.entrypoints.to_string()),
        ("dependencies", record.dependencies.to_string()),
        ("cross_project_dependencies", record.cross_project_dependencies.to_string()),
        ("graph_passed", record.graph_passed.to_string()),
        ("graph_clean", record.graph_clean.to_string()),
        ("missing_sources", record.missing_sources.to_string()),
        ("missing_targets", record.missing_targets.to_string()),
        ("self_dependencies", record.self_dependencies.to_string()),
        ("cycles", record.cycles.to_string()),
        ("diagnostics_info", record.diagnostics_info.to_string()),
        ("diagnostics_warning", record.diagnostics_warning.to_string()),
        ("diagnostics_error", record.diagnostics_error.to_string()),
        ("framework_detections", record.framework_detections.to_string()),
        ("actionable_frameworks", record.actionable_frameworks.to_string()),
        ("message", record.message.clone()),
    ];
    atomic_write(&path, &values.into_iter().map(|(key, value)| {
        format!("{key}={}", normalize_record_value(&value))
    }).collect::<Vec<_>>().join("\n"))
}

fn read_analysis_record_file(path: &Path) -> io::Result<Option<AnalysisRecord>> {
    let content = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    let fields = content.lines().filter_map(|line| line.split_once('=')).collect::<std::collections::BTreeMap<_, _>>();
    let value = |key: &str| fields.get(key).copied().unwrap_or("");
    let parse_u64 = |key: &str| value(key).parse::<u64>().unwrap_or(0);
    let parse_usize = |key: &str| value(key).parse::<usize>().unwrap_or(0);
    let parse_bool = |key: &str| value(key).parse::<bool>().unwrap_or(false);
    let repository_id = value("repository_id").to_string();
    if repository_id.is_empty() {
        return Ok(None);
    }
    Ok(Some(AnalysisRecord {
        workspace_id: value("workspace_id").to_string(),
        repository_id,
        analysis_id: value("analysis_id").to_string(),
        status: AnalysisStatus::from_str(value("status")),
        started_at_ms: parse_u64("started_at_ms"),
        completed_at_ms: parse_u64("completed_at_ms"),
        duration_ms: parse_u64("duration_ms"),
        engine_version: value("engine_version").to_string(),
        registry_signature: value("registry_signature").to_string(),
        branch: optional_record_value(value("branch")),
        commit: optional_record_value(value("commit")),
        model_fingerprint: value("model_fingerprint").to_string(),
        files: parse_usize("files"),
        project_units: parse_usize("project_units"),
        components: parse_usize("components"),
        entrypoints: parse_usize("entrypoints"),
        dependencies: parse_usize("dependencies"),
        cross_project_dependencies: parse_usize("cross_project_dependencies"),
        graph_passed: parse_bool("graph_passed"),
        graph_clean: parse_bool("graph_clean"),
        missing_sources: parse_usize("missing_sources"),
        missing_targets: parse_usize("missing_targets"),
        self_dependencies: parse_usize("self_dependencies"),
        cycles: parse_usize("cycles"),
        diagnostics_info: parse_usize("diagnostics_info"),
        diagnostics_warning: parse_usize("diagnostics_warning"),
        diagnostics_error: parse_usize("diagnostics_error"),
        framework_detections: parse_usize("framework_detections"),
        actionable_frameworks: parse_usize("actionable_frameworks"),
        message: value("message").to_string(),
    }))
}

fn normalize_record_value(value: &str) -> String {
    value.replace('\n', " ").replace('\r', " ").replace('=', ":").trim().to_string()
}

fn optional_record_value(value: &str) -> Option<String> {
    (!value.is_empty()).then(|| value.to_string())
}

pub fn add_project(path: &Path) -> io::Result<WorkspaceProject> {
    let repository = add_repository(DEFAULT_WORKSPACE_ID, path)?;
    Ok(WorkspaceProject {
        name: repository.name,
        path: repository.path,
    })
}

pub fn list_projects() -> io::Result<Vec<WorkspaceProject>> {
    Ok(list_repositories(DEFAULT_WORKSPACE_ID)?
        .into_iter()
        .map(|repository| WorkspaceProject {
            name: repository.name,
            path: repository.path,
        })
        .collect())
}

fn ensure_workspace(id: &str) -> io::Result<()> {
    if !valid_storage_id(id) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Workspace id is invalid",
        ));
    }
    if list_workspaces()?
        .iter()
        .any(|workspace| workspace.id == id)
    {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::NotFound,
            "Workspace was not found",
        ))
    }
}

fn valid_storage_id(value: &str) -> bool {
    if value.contains(['/', '\\']) {
        return false;
    }

    let mut components = Path::new(value).components();
    matches!(components.next(), Some(PathComponent::Normal(_))) && components.next().is_none()
}

fn git_output(path: &Path, arguments: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(path)
        .args(arguments)
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn workspaces_registry() -> PathBuf {
    workspace_root().join("workspaces/registry.tsv")
}
fn repositories_registry(workspace_id: &str) -> PathBuf {
    workspace_root()
        .join("workspaces")
        .join(workspace_id)
        .join("repositories.tsv")
}

fn read_workspaces() -> io::Result<Vec<WorkspaceRecord>> {
    let path = workspaces_registry();
    let Some(content) = read_registry_content(&path)? else {
        return Ok(Vec::new());
    };
    Ok(content
        .lines()
        .filter_map(|line| {
            let (id, name) = line.split_once('\t')?;
            (!id.is_empty() && !name.is_empty() && valid_storage_id(id)).then(|| WorkspaceRecord {
                id: id.to_string(),
                name: name.to_string(),
            })
        })
        .collect())
}

fn write_workspaces(workspaces: &[WorkspaceRecord]) -> io::Result<()> {
    let path = workspaces_registry();
    fs::create_dir_all(path.parent().unwrap_or(Path::new(".")))?;
    atomic_write(
        &path,
        &workspaces
            .iter()
            .map(|workspace| format!("{}\t{}", workspace.id, workspace.name))
            .collect::<Vec<_>>()
            .join("\n"),
    )
}

fn read_repositories(workspace_id: &str) -> io::Result<Vec<RepositoryRecord>> {
    let path = repositories_registry(workspace_id);
    let Some(content) = read_registry_content(&path)? else {
        return Ok(Vec::new());
    };
    Ok(content
        .lines()
        .filter_map(|line| {
            let mut parts = line.splitn(3, '\t');
            let id = parts.next().filter(|value| !value.is_empty())?;
            let name = parts.next().filter(|value| !value.is_empty())?;
            let path = parts.next().filter(|value| !value.is_empty())?;
            Some(RepositoryRecord {
                id: id.to_string(),
                workspace_id: workspace_id.to_string(),
                name: name.to_string(),
                path: path.to_string(),
            })
        })
        .collect())
}

fn write_repositories(workspace_id: &str, repositories: &[RepositoryRecord]) -> io::Result<()> {
    let path = repositories_registry(workspace_id);
    fs::create_dir_all(path.parent().unwrap_or(Path::new(".")))?;
    atomic_write(
        &path,
        &repositories
            .iter()
            .map(|repository| {
                format!(
                    "{}\t{}\t{}",
                    repository.id, repository.name, repository.path
                )
            })
            .collect::<Vec<_>>()
            .join("\n"),
    )
}

fn atomic_write(path: &Path, content: &str) -> io::Result<()> {
    let parent = path.parent().unwrap_or(Path::new("."));
    fs::create_dir_all(parent)?;
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("registry");
    let sequence = ATOMIC_WRITE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let temporary = parent.join(format!(
        ".{file_name}.{}.{}.{}.tmp",
        std::process::id(),
        sequence,
        &stable_hash(content)[..8]
    ));
    let write_result = (|| -> io::Result<()> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        file.write_all(content.as_bytes())?;
        file.sync_all()?;
        Ok(())
    })();
    if let Err(error) = write_result {
        let _ = fs::remove_file(&temporary);
        return Err(error);
    }

    if !path.exists() {
        return fs::rename(temporary, path);
    }

    let backup = parent.join(format!(".{file_name}.backup"));
    if backup.exists() {
        fs::remove_file(&backup)?;
    }
    fs::rename(path, &backup)?;
    if let Err(error) = fs::rename(&temporary, path) {
        let _ = fs::rename(&backup, path);
        let _ = fs::remove_file(&temporary);
        return Err(error);
    }
    let _ = fs::remove_file(backup);
    Ok(())
}

fn read_registry_content(path: &Path) -> io::Result<Option<String>> {
    match fs::read_to_string(path) {
        Ok(content) => Ok(Some(content)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            let file_name = path
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("registry");
            let backup = path
                .parent()
                .unwrap_or(Path::new("."))
                .join(format!(".{file_name}.backup"));
            match fs::read_to_string(&backup) {
                Ok(content) => {
                    fs::rename(&backup, path)?;
                    Ok(Some(content))
                }
                Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
                Err(error) => Err(error),
            }
        }
        Err(error) => Err(error),
    }
}

struct RegistryGuard {
    _process_guard: MutexGuard<'static, ()>,
    lock_path: PathBuf,
}

impl Drop for RegistryGuard {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.lock_path);
    }
}

fn acquire_registry_lock() -> io::Result<RegistryGuard> {
    let process_guard = REGISTRY_WRITE_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .map_err(|_| io::Error::other("Registry write lock was poisoned"))?;
    let root = workspace_root();
    fs::create_dir_all(&root)?;
    let lock_path = root.join(".registry.lock");
    for _ in 0..200 {
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&lock_path)
        {
            Ok(_) => {
                return Ok(RegistryGuard {
                    _process_guard: process_guard,
                    lock_path,
                });
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                let stale = fs::metadata(&lock_path)
                    .and_then(|metadata| metadata.modified())
                    .ok()
                    .and_then(|modified| SystemTime::now().duration_since(modified).ok())
                    .is_some_and(|age| age > Duration::from_secs(30));
                if stale {
                    let _ = fs::remove_file(&lock_path);
                } else {
                    std::thread::sleep(Duration::from_millis(10));
                }
            }
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::WouldBlock,
        "Workspace registry is busy",
    ))
}

fn remove_failed_clone(repositories_root: &Path, destination: &Path) {
    let Ok(root) = fs::canonicalize(repositories_root) else {
        return;
    };
    let Ok(target) = fs::canonicalize(destination) else {
        return;
    };
    if target != root && target.starts_with(&root) {
        let _ = fs::remove_dir_all(target);
    }
}

fn sanitize_remote_url(value: &str) -> String {
    let value = value.trim();
    if let Some((scheme, rest)) = value.split_once("://") {
        let authority_end = rest.find('/').unwrap_or(rest.len());
        let authority = &rest[..authority_end];
        let safe_authority = authority.rsplit('@').next().unwrap_or(authority);
        let suffix = rest[authority_end..].split(['?', '#']).next().unwrap_or("");
        return format!("{scheme}://{safe_authority}{suffix}");
    }
    let without_suffix = value.split(['?', '#']).next().unwrap_or(value);
    if let Some((credentials, destination)) = without_suffix.split_once('@') {
        if credentials.contains(':') && credentials != "git" {
            return destination.to_string();
        }
    }
    without_suffix.to_string()
}

fn scoped_id(scope: &str, value: &str) -> String {
    let hash = stable_hash(&format!("{scope}\0{value}"));
    format!("{}:{hash}", slugify(scope))
}

fn repository_name_from_url(url: &str) -> String {
    let safe = sanitize_remote_url(url);
    let tail = safe
        .trim_end_matches('/')
        .rsplit(['/', ':'])
        .next()
        .unwrap_or("repository")
        .trim_end_matches(".git");
    let value = slugify(tail);
    if value.is_empty() {
        "repository".to_string()
    } else {
        value
    }
}

pub fn delete_workspace(id: &str) -> io::Result<()> {
    if !valid_storage_id(id) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Workspace id is invalid",
        ));
    }
    let _guard = acquire_registry_lock()?;
    let mut workspaces = read_workspaces()?;
    if id != DEFAULT_WORKSPACE_ID && !workspaces.iter().any(|workspace| workspace.id == id) {
        return Ok(());
    }

    let repositories = read_repositories(id)?;
    for repository in repositories {
        remove_managed_repository(&repository.path)?;
    }

    if id != DEFAULT_WORKSPACE_ID {
        workspaces.retain(|workspace| workspace.id != id);
        write_workspaces(&workspaces)?;
    }

    let root = workspace_root();
    remove_dir_if_exists(&root.join("workspaces").join(id))?;
    remove_dir_if_exists(&root.join("capsules").join(id))?;
    if id == DEFAULT_WORKSPACE_ID {
        remove_dir_if_exists(&root.join("projects"))?;
    }
    Ok(())
}

pub fn delete_repository(workspace_id: &str, repository_id: &str) -> io::Result<()> {
    ensure_workspace(workspace_id)?;
    let _guard = acquire_registry_lock()?;
    let path = repositories_registry(workspace_id);
    if !path.is_file() {
        return Ok(());
    }
    let entries = read_repositories(workspace_id)?;
    let mut kept = Vec::new();
    let mut removed_path: Option<String> = None;
    for entry in entries {
        if entry.id == repository_id {
            removed_path = Some(entry.path.clone());
        } else {
            kept.push(entry);
        }
    }
    write_repositories(workspace_id, &kept)?;
    invalidate_workspace_model_cache(workspace_id)?;
    let analysis_path = analysis_record_path(workspace_id, repository_id);
    if analysis_path.is_file() {
        fs::remove_file(analysis_path)?;
    }
    if let Some(repo_path) = removed_path {
        remove_managed_repository(&repo_path)?;
    }
    Ok(())
}

pub fn invalidate_workspace_model_cache(workspace_id: &str) -> io::Result<()> {
    let path = workspace_model_cache_path(workspace_id);
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

/// Updates a repository cloned and managed by RepoSlice using a fast-forward-only pull.
/// Local repositories are intentionally never modified by this operation.
pub fn update_managed_repository(
    workspace_id: &str,
    repository_id: &str,
) -> io::Result<RepositoryRecord> {
    ensure_workspace(workspace_id)?;
    let record = list_repositories(workspace_id)?
        .into_iter()
        .find(|repository| repository.id == repository_id)
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Repository was not found"))?;
    let repositories_root = workspace_root().join("repositories");
    let managed_root = fs::canonicalize(&repositories_root).map_err(|_| {
        io::Error::new(io::ErrorKind::InvalidInput, "Managed repository root was not found")
    })?;
    let target = fs::canonicalize(&record.path)?;
    if target == managed_root || !target.starts_with(&managed_root) {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "Only repositories cloned and managed by RepoSlice can be updated",
        ));
    }
    if git_output(&target, &["rev-parse", "--is-inside-work-tree"]).as_deref() != Some("true") {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "Repository is not a Git worktree"));
    }
    let status = git_output(&target, &["status", "--porcelain=v1", "--untracked-files=all"])
        .unwrap_or_default();
    if !status.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Managed repository has local changes; commit, stash, or discard them before updating",
        ));
    }
    let output = Command::new("git")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GCM_INTERACTIVE", "Never")
        .arg("-C")
        .arg(&target)
        .args(["pull", "--ff-only"])
        .output()?;
    if !output.status.success() {
        return Err(io::Error::other(format!(
            "Git update failed (exit code {})",
            output.status.code().unwrap_or(-1)
        )));
    }
    invalidate_workspace_model_cache(workspace_id)?;
    Ok(record)
}

pub fn reset_all() -> io::Result<()> {
    let _guard = acquire_registry_lock()?;
    let root = workspace_root();
    let dirs_to_clean = ["workspaces", "repositories", "capsules", "projects"];
    for dir in &dirs_to_clean {
        remove_dir_if_exists(&root.join(dir))?;
    }
    Ok(())
}

fn remove_managed_repository(repo_path: &str) -> io::Result<()> {
    let path = Path::new(repo_path);
    if path.is_dir() {
        let repositories_root = workspace_root().join("repositories");
        if let (Ok(root), Ok(target)) =
            (fs::canonicalize(&repositories_root), fs::canonicalize(path))
        {
            if target != root && target.starts_with(&root) {
                fs::remove_dir_all(target)?;
            }
        }
    }
    Ok(())
}

fn remove_dir_if_exists(path: &Path) -> io::Result<()> {
    match fs::remove_dir_all(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    static TEST_ENV_LOCK: Mutex<()> = Mutex::new(());

    struct TestHome {
        root: PathBuf,
        previous: Option<std::ffi::OsString>,
    }

    impl TestHome {
        fn new(label: &str) -> Self {
            let root = std::env::temp_dir().join(format!(
                "reposlice-{label}-{}-{}",
                std::process::id(),
                epoch_millis()
            ));
            let _ = fs::remove_dir_all(&root);
            fs::create_dir_all(&root).unwrap();
            let previous = std::env::var_os("REPOSLICE_HOME");
            std::env::set_var("REPOSLICE_HOME", &root);
            Self { root, previous }
        }
    }

    impl Drop for TestHome {
        fn drop(&mut self) {
            if let Some(value) = self.previous.take() {
                std::env::set_var("REPOSLICE_HOME", value);
            } else {
                std::env::remove_var("REPOSLICE_HOME");
            }
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn parses_repository_names() {
        assert_eq!(
            repository_name_from_url("https://github.com/org/project.git"),
            "project"
        );
        assert_eq!(
            repository_name_from_url("git@github.com:org/project.git"),
            "project"
        );
        assert_eq!(
            repository_name_from_url(
                "https://token@example.com/org/project.git?access_token=secret"
            ),
            "project"
        );
    }

    #[test]
    fn removes_credentials_and_query_data_from_remote_urls() {
        assert_eq!(
            sanitize_remote_url("https://user:secret@github.com/org/project.git?token=x"),
            "https://github.com/org/project.git"
        );
        assert_eq!(
            sanitize_remote_url("git@github.com:org/project.git"),
            "git@github.com:org/project.git"
        );
        assert_eq!(
            sanitize_remote_url("oauth2:secret@gitlab.com:org/project.git"),
            "gitlab.com:org/project.git"
        );
        assert_eq!(
            sanitize_remote_url("ssh://user:secret@example.com/org/project.git"),
            "ssh://example.com/org/project.git"
        );
    }

    #[test]
    fn rejects_workspace_ids_that_can_escape_storage() {
        assert!(valid_storage_id("default"));
        assert!(valid_storage_id("my-workspace"));
        assert!(!valid_storage_id("../outside"));
        assert!(!valid_storage_id("nested/workspace"));
        assert!(!valid_storage_id(r"nested\workspace"));
        assert_eq!(
            delete_workspace("../outside").unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
    }

    #[test]
    fn reads_persisted_analysis_records() {
        let path = std::env::temp_dir().join(format!(
            "reposlice-analysis-record-{}-{}.audit",
            std::process::id(),
            epoch_millis()
        ));
        let content = [
            "workspace_id=default",
            "repository_id=repo-1",
            "analysis_id=ANL-1-abc",
            "status=COMPLETED_WITH_WARNINGS",
            "started_at_ms=10",
            "completed_at_ms=20",
            "duration_ms=10",
            "engine_version=0.3.0",
            "registry_signature=registry123",
            "branch=main",
            "commit=abc123",
            "model_fingerprint=fingerprint",
            "files=12",
            "project_units=1",
            "components=8",
            "entrypoints=2",
            "dependencies=6",
            "cross_project_dependencies=1",
            "graph_passed=true",
            "graph_clean=false",
            "missing_sources=0",
            "missing_targets=0",
            "self_dependencies=0",
            "cycles=1",
            "diagnostics_info=2",
            "diagnostics_warning=1",
            "diagnostics_error=0",
            "framework_detections=1",
            "actionable_frameworks=1",
            "message=Analysis completed with warnings",
        ]
        .join("\n");
        fs::write(&path, content).unwrap();
        let record = read_analysis_record_file(&path).unwrap().unwrap();
        let _ = fs::remove_file(path);

        assert_eq!(record.repository_id, "repo-1");
        assert_eq!(record.status, AnalysisStatus::CompletedWithWarnings);
        assert_eq!(record.duration_ms, 10);
        assert_eq!(record.registry_signature, "registry123");
        assert_eq!(record.components, 8);
        assert_eq!(record.cross_project_dependencies, 1);
        assert!(record.graph_passed);
        assert!(!record.graph_clean);
        assert_eq!(record.cycles, 1);
        assert_eq!(record.diagnostics_warning, 1);
    }

    #[test]
    fn deletes_virtual_default_workspace_storage() {
        let _lock = TEST_ENV_LOCK.lock().unwrap();
        let home = TestHome::new("default-delete");
        fs::create_dir_all(home.root.join("workspaces/default")).unwrap();
        fs::create_dir_all(home.root.join("capsules/default")).unwrap();
        fs::create_dir_all(home.root.join("projects")).unwrap();

        delete_workspace(DEFAULT_WORKSPACE_ID).unwrap();

        assert!(!home.root.join("workspaces/default").exists());
        assert!(!home.root.join("capsules/default").exists());
        assert!(!home.root.join("projects").exists());
    }

    #[test]
    fn single_repository_refresh_preserves_the_complete_workspace_model() {
        let _lock = TEST_ENV_LOCK.lock().unwrap();
        let home = TestHome::new("single-refresh");
        let repo_a = home.root.join("repo-a");
        let repo_b = home.root.join("repo-b");
        fs::create_dir_all(&repo_a).unwrap();
        fs::create_dir_all(&repo_b).unwrap();
        fs::write(repo_a.join("README.md"), "repo a").unwrap();
        fs::write(repo_b.join("README.md"), "repo b").unwrap();

        let workspace = create_workspace("Refresh Workspace").unwrap();
        let registered_a = add_repository(&workspace.id, &repo_a).unwrap();
        let registered_b = add_repository(&workspace.id, &repo_b).unwrap();

        let initial = scan_workspace(&workspace.id).unwrap();
        assert_eq!(initial.repositories.len(), 2);

        fs::write(repo_a.join("changed.txt"), "changed").unwrap();
        let refreshed = scan_workspace_repository(&workspace.id, &registered_a.id).unwrap();
        assert_eq!(refreshed.repositories.len(), 2);
        assert!(refreshed.repositories.iter().any(|repo| repo.id == registered_a.id));
        assert!(refreshed.repositories.iter().any(|repo| repo.id == registered_b.id));

        let cached = load_cached_workspace_model(&workspace.id)
            .unwrap()
            .expect("workspace cache should remain valid after a single repository refresh");
        assert_eq!(cached.repositories.len(), 2);
    }

    #[test]
    fn cache_is_invalidated_when_repository_contents_change() {
        let _lock = TEST_ENV_LOCK.lock().unwrap();
        let home = TestHome::new("cache-invalidation");
        let repo = home.root.join("repo");
        fs::create_dir_all(&repo).unwrap();
        fs::write(repo.join("README.md"), "v1").unwrap();

        let workspace = create_workspace("Cache Workspace").unwrap();
        add_repository(&workspace.id, &repo).unwrap();
        scan_workspace(&workspace.id).unwrap();
        assert!(load_cached_workspace_model(&workspace.id).unwrap().is_some());

        fs::write(repo.join("README.md"), "a longer v2 payload").unwrap();
        assert!(load_cached_workspace_model(&workspace.id).unwrap().is_none());
    }

    #[test]
    fn dirty_git_state_signature_tracks_content_changes_not_only_status_shape() {
        if Command::new("git").arg("--version").output().is_err() {
            return;
        }
        let _lock = TEST_ENV_LOCK.lock().unwrap();
        let home = TestHome::new("dirty-git-signature");
        let repo = home.root.join("git-repo");
        fs::create_dir_all(&repo).unwrap();

        for args in [
            vec!["init", "-q"],
            vec!["config", "user.email", "reposlice@example.invalid"],
            vec!["config", "user.name", "RepoSlice Test"],
        ] {
            let status = Command::new("git")
                .arg("-C")
                .arg(&repo)
                .args(args)
                .status()
                .unwrap();
            assert!(status.success());
        }
        fs::write(repo.join("tracked.txt"), "version-one").unwrap();
        assert!(Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args(["add", "."])
            .status()
            .unwrap()
            .success());
        assert!(Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args(["commit", "-qm", "initial"])
            .status()
            .unwrap()
            .success());

        let record = RepositoryRecord {
            id: "repo".into(),
            workspace_id: "default".into(),
            name: "repo".into(),
            path: repo.to_string_lossy().to_string(),
        };
        let clean = repository_state_signature(&record).unwrap();
        fs::write(repo.join("tracked.txt"), "version-two").unwrap();
        let dirty_two = repository_state_signature(&record).unwrap();
        fs::write(repo.join("tracked.txt"), "version-three").unwrap();
        let dirty_three = repository_state_signature(&record).unwrap();

        assert_ne!(clean, dirty_two);
        assert_ne!(dirty_two, dirty_three);
    }

    #[test]
    fn controlled_scan_honors_cancellation_before_mutating_the_cache() {
        let _lock = TEST_ENV_LOCK.lock().unwrap();
        let home = TestHome::new("cancelled-scan");
        let repo = home.root.join("repo");
        fs::create_dir_all(&repo).unwrap();
        fs::write(repo.join("README.md"), "repo").unwrap();

        let workspace = create_workspace("Cancelled Workspace").unwrap();
        add_repository(&workspace.id, &repo).unwrap();
        let error = scan_workspace_controlled(&workspace.id, |_| {}, || true).unwrap_err();
        assert!(error.to_string().contains("Analysis was cancelled"));
        assert!(load_cached_workspace_model(&workspace.id).unwrap().is_none());
    }

    #[test]
    fn scan_rejects_a_workspace_snapshot_that_changes_mid_analysis() {
        let _lock = TEST_ENV_LOCK.lock().unwrap();
        let home = TestHome::new("snapshot-drift");
        let repo = home.root.join("repo");
        fs::create_dir_all(&repo).unwrap();
        fs::write(repo.join("README.md"), "initial").unwrap();

        let workspace = create_workspace("Snapshot Workspace").unwrap();
        add_repository(&workspace.id, &repo).unwrap();
        let mut mutated = false;
        let error = scan_workspace_controlled(
            &workspace.id,
            |progress| {
                if progress.stage == "repository-complete" && !mutated {
                    fs::write(repo.join("README.md"), "changed during scan").unwrap();
                    mutated = true;
                }
            },
            || false,
        )
        .unwrap_err();

        assert!(error.to_string().contains("changed while analysis was running"));
        assert!(load_cached_workspace_model(&workspace.id).unwrap().is_none());
    }

    #[test]
    fn cache_integrity_hash_rejects_tampered_workspace_models() {
        let _lock = TEST_ENV_LOCK.lock().unwrap();
        let home = TestHome::new("cache-integrity");
        let repo = home.root.join("repo");
        fs::create_dir_all(&repo).unwrap();
        fs::write(repo.join("README.md"), "repo").unwrap();

        let workspace = create_workspace("Integrity Workspace").unwrap();
        add_repository(&workspace.id, &repo).unwrap();
        scan_workspace(&workspace.id).unwrap();

        let path = workspace_model_cache_path(&workspace.id);
        let mut cache: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        cache["model"]["repositories"][0]["name"] =
            serde_json::Value::String("tampered".into());
        fs::write(&path, serde_json::to_string(&cache).unwrap()).unwrap();

        assert!(load_cached_workspace_model(&workspace.id).unwrap().is_none());
    }

    #[test]
    fn cache_reader_recovers_an_atomic_write_backup_after_interruption() {
        let _lock = TEST_ENV_LOCK.lock().unwrap();
        let home = TestHome::new("cache-backup-recovery");
        let repo = home.root.join("repo");
        fs::create_dir_all(&repo).unwrap();
        fs::write(repo.join("README.md"), "repo").unwrap();

        let workspace = create_workspace("Backup Workspace").unwrap();
        add_repository(&workspace.id, &repo).unwrap();
        scan_workspace(&workspace.id).unwrap();

        let path = workspace_model_cache_path(&workspace.id);
        let backup = path.parent().unwrap().join(".model-cache.json.backup");
        fs::rename(&path, &backup).unwrap();
        assert!(!path.exists());

        let cached = load_cached_workspace_model(&workspace.id).unwrap();
        assert!(cached.is_some());
        assert!(path.is_file());
        assert!(!backup.exists());
    }

    #[test]
    fn malformed_cache_is_quarantined_instead_of_reused() {
        let _lock = TEST_ENV_LOCK.lock().unwrap();
        let home = TestHome::new("cache-quarantine");
        let workspace = create_workspace("Quarantine Workspace").unwrap();
        let path = workspace_model_cache_path(&workspace.id);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "{not-json").unwrap();

        assert!(load_cached_workspace_model(&workspace.id).unwrap().is_none());
        assert!(!path.exists());
        let quarantined = fs::read_dir(path.parent().unwrap())
            .unwrap()
            .flatten()
            .any(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".model-cache.json.corrupt.")
            });
        assert!(quarantined);
    }

    #[test]
    fn removing_external_repository_never_deletes_the_source_directory() {
        let _lock = TEST_ENV_LOCK.lock().unwrap();
        let home = TestHome::new("external-remove");
        let external = home.root.join("external-source");
        fs::create_dir_all(&external).unwrap();
        fs::write(external.join("README.md"), "keep me").unwrap();

        let workspace = create_workspace("External Repository Workspace").unwrap();
        let repository = add_repository(&workspace.id, &external).unwrap();
        delete_repository(&workspace.id, &repository.id).unwrap();

        assert!(external.is_dir());
        assert!(external.join("README.md").is_file());
        assert!(list_repositories(&workspace.id).unwrap().is_empty());
    }

    #[test]
    fn updating_external_repository_is_rejected() {
        let _lock = TEST_ENV_LOCK.lock().unwrap();
        let home = TestHome::new("external-update");
        fs::create_dir_all(home.root.join("repositories")).unwrap();
        let external = home.root.join("external-source");
        fs::create_dir_all(&external).unwrap();
        fs::write(external.join("README.md"), "keep me").unwrap();

        let workspace = create_workspace("External Update Workspace").unwrap();
        let repository = add_repository(&workspace.id, &external).unwrap();
        let error = update_managed_repository(&workspace.id, &repository.id).unwrap_err();

        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
        assert!(external.is_dir());
    }

    #[test]
    fn workspace_data_dir_rejects_unregistered_or_unsafe_ids() {
        let _guard = TEST_ENV_LOCK.lock().unwrap();
        let _home = TestHome::new("workspace-data-dir");
        let workspace = create_workspace("Safe Workspace").unwrap();
        assert!(workspace_data_dir(&workspace.id).unwrap().ends_with(&workspace.id));
        assert_eq!(workspace_data_dir("../outside").unwrap_err().kind(), io::ErrorKind::InvalidInput);
        assert_eq!(workspace_data_dir("missing-workspace").unwrap_err().kind(), io::ErrorKind::NotFound);
    }

}
