import { useEffect, useMemo, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { listen } from "@tauri-apps/api/event";
import { addRepository, cancelAnalysis, cloneRepository, createWorkspace, createWorkspaceCapsule, deleteRepository, deleteWorkspace, detectRuntime, listRepositories, listWorkspaceCapsules, listWorkspaces, loadCachedWorkspace, resetAll, scanRepository, scanWorkspace, updateRepository, verifyCapsule } from "./lib/client";
import type { AnalysisProgress, CapsuleSummary, EngineState, RepositoryRecord, RuntimeCapabilities, VerificationReport, View, WorkspaceModel, WorkspaceRecord } from "./types";
import Sidebar from "./components/Sidebar";
import ProjectToolbar from "./components/ProjectToolbar";
import Modal from "./components/Modal";
import OverviewView from "./views/OverviewView";
import ProjectsView from "./views/ProjectsView";
import ArchitectureView from "./views/ArchitectureView";
import AuditView from "./views/AuditView";
import ComponentsView from "./views/ComponentsView";
import EntrypointsView from "./views/EntrypointsView";
import DependenciesView from "./views/DependenciesView";
import CapsulesView from "./views/CapsulesView";
import { translateEngineErrorDetail, useI18n } from "./i18n";

function App() {
  const { t } = useI18n();
  const [workspaces, setWorkspaces] = useState<WorkspaceRecord[]>([]);
  const [workspaceId, setWorkspaceId] = useState("default");
  const [repositories, setRepositories] = useState<RepositoryRecord[]>([]);
  const [repositoryId, setRepositoryId] = useState("");
  const [unitId, setUnitId] = useState("");
  const [workspace, setWorkspace] = useState<WorkspaceModel | null>(null);
  const [view, setView] = useState<View>("overview");
  const [selectedTarget, setSelectedTarget] = useState("");
  const [capsules, setCapsules] = useState<CapsuleSummary[]>([]);
  const [verificationByPath, setVerificationByPath] = useState<Record<string, VerificationReport | undefined>>({});
  const [verifyingPath, setVerifyingPath] = useState("");
  const [runtime, setRuntime] = useState<RuntimeCapabilities | null>(null);
  const [engineState, setEngineState] = useState<EngineState>("checking");
  const [busy, setBusy] = useState(false);
  const [cloneOpen, setCloneOpen] = useState(false);
  const [cloneUrl, setCloneUrl] = useState("");
  const [workspaceCreateOpen, setWorkspaceCreateOpen] = useState(false);
  const [workspaceCreateName, setWorkspaceCreateName] = useState("");
  const [appError, setAppError] = useState("");
  const [analysisProgress, setAnalysisProgress] = useState<AnalysisProgress | null>(null);
  const workspaceLoadRequest = useRef(0);

  function showError(context: string, error: unknown) {
    const detail = error instanceof Error ? error.message : String(error);
    setAppError(`${context}: ${detail ? translateEngineErrorDetail(detail, t) : t("error.unknown")}`);
  }

  useEffect(() => {
    const requestId = ++workspaceLoadRequest.current;
    void (async () => {
      try {
        setRuntime(await detectRuntime());
        setEngineState("ready");
      } catch (error) {
        setEngineState("unavailable");
        showError(t("error.runtime"), error);
      }
      try {
        const items = await listWorkspaces();
        const selected = items.find((item) => item.id === "default")?.id ?? items[0]?.id ?? "default";
        const [repos, workspaceCapsules, cachedWorkspace] = await Promise.all([
          listRepositories(selected),
          listWorkspaceCapsules(selected),
          loadCachedWorkspace(selected)
        ]);
        if (requestId !== workspaceLoadRequest.current) return;
        setWorkspaces(items);
        setWorkspaceId(selected);
        setRepositories(repos);
        setCapsules(workspaceCapsules);
        setWorkspace(cachedWorkspace);
      } catch (error) {
        if (requestId === workspaceLoadRequest.current) showError(t("error.loadWorkspace"), error);
      }
    })();
  }, []);

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void listen<AnalysisProgress>("analysis-progress", (event) => {
      if (!disposed) setAnalysisProgress(event.payload);
    }).then((cleanup) => {
      if (disposed) cleanup();
      else unlisten = cleanup;
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);

  const selectedRepository = workspace?.repositories.find((repository) => repository.id === repositoryId);
  const selectedUnit = selectedRepository?.projectUnits.find((unit) => unit.id === unitId);
  const selectedAnalysis = selectedRepository?.audit.analysis ?? repositories.find((item) => item.id === repositoryId)?.analysis ?? null;
  const model = useMemo(() => {
    if (selectedUnit) return selectedUnit.model;
    if (!workspace) return null;
    if (repositoryId && !selectedRepository) return null;
    const units = workspace.repositories.flatMap((repository) => !repositoryId || repository.id === repositoryId ? repository.projectUnits.map((unit) => ({ repository, unit })) : []);
    const scoped = (repository: string, unit: string, id: string) => `${repository}|${unit}|${id}`;
    const dependencies = units.flatMap(({ repository, unit }) => unit.model.dependencies.map((dependency) => ({ sourceId: scoped(repository.id, unit.id, dependency.sourceId), targetId: scoped(repository.id, unit.id, dependency.targetId), kind: dependency.kind })));
    for (const dependency of workspace.crossProjectDependencies) {
      const targetUnit = workspace.repositories.flatMap((repository) => repository.projectUnits).find((unit) => unit.id === dependency.targetProjectUnitId);
      const targetComponent = dependency.targetComponentId ?? targetUnit?.model.entrypoints.find((entrypoint) => entrypoint.id === dependency.targetEntrypointId)?.componentId;
      if (targetComponent) dependencies.push({ sourceId: scoped(dependency.sourceRepositoryId, dependency.sourceProjectUnitId, dependency.sourceComponentId), targetId: scoped(dependency.targetRepositoryId, dependency.targetProjectUnitId, targetComponent), kind: `${dependency.kind} ${dependency.confidence}%` });
    }
    const technologies = units.flatMap(({ unit }) => unit.model.technologies);
    const compatibility = units.reduce((highest, { unit }) => compatibilityRank(unit.model.compatibility) > compatibilityRank(highest) ? unit.model.compatibility : highest, "L0");
    return { root: workspace.name, name: workspace.name, files: units.reduce((total, item) => total + item.unit.model.files, 0), compatibility, technologies,
      components: units.flatMap(({ repository, unit }) => unit.model.components.map((component) => ({ ...component, id: scoped(repository.id, unit.id, component.id), repositoryName: repository.name, projectUnitName: unit.name }))),
      entrypoints: units.flatMap(({ repository, unit }) => unit.model.entrypoints.map((entrypoint) => ({ ...entrypoint, id: scoped(repository.id, unit.id, entrypoint.id), componentId: scoped(repository.id, unit.id, entrypoint.componentId), repositoryName: repository.name, projectUnitName: unit.name }))), dependencies,
      analysis: {
        frameworkDetections: units.flatMap(({ unit }) => unit.model.analysis.frameworkDetections),
        symbols: units.flatMap(({ repository, unit }) => unit.model.analysis.symbols.map((symbol) => ({ ...symbol, symbolId: scoped(repository.id, unit.id, symbol.symbolId) }))),
        entrypoints: units.flatMap(({ repository, unit }) => unit.model.analysis.entrypoints.map((entrypoint) => ({ ...entrypoint, entrypointId: scoped(repository.id, unit.id, entrypoint.entrypointId) }))),
        dependencies: units.flatMap(({ repository, unit }) => unit.model.analysis.dependencies.map((dependency) => ({ ...dependency, sourceId: scoped(repository.id, unit.id, dependency.sourceId), targetId: scoped(repository.id, unit.id, dependency.targetId) }))),
        runtimeRequirements: units.flatMap(({ unit }) => unit.model.analysis.runtimeRequirements),
        diagnostics: units.flatMap(({ unit }) => unit.model.analysis.diagnostics),
      } };
  }, [workspace, repositoryId, selectedUnit]);
  const targetParts = selectedTarget.split("|");
  const rawTarget = targetParts.slice(2).join("|");
  const localTarget = selectedUnit ? rawTarget : rawTarget.startsWith("component:") ? `component:${targetParts[0]}|${targetParts[1]}|${rawTarget.slice(10)}` : selectedTarget;

  const targets = useMemo(() => {
    if (!workspace) return [];
    return workspace.repositories.flatMap((repository) => repository.projectUnits.flatMap((unit) => [
      ...unit.model.entrypoints.map((entrypoint) => ({ id: `${repository.id}|${unit.id}|${entrypoint.id}`, label: `${repository.name} / ${unit.name} / ${entrypoint.name}`, type: entrypoint.kind })),
      ...unit.model.components.slice(0, 1500).map((component) => ({ id: `${repository.id}|${unit.id}|component:${component.id}`, label: `${repository.name} / ${unit.name} / ${component.name}`, type: component.kind }))
    ]));
  }, [workspace]);

  async function refreshRepositories(id = workspaceId) { const items = await listRepositories(id); setRepositories(items); return items; }

  async function handleWorkspaceChange(id: string) {
    const requestId = ++workspaceLoadRequest.current;
    setAppError("");
    setWorkspaceId(id);
    setWorkspace(null);
    setRepositoryId("");
    setUnitId("");
    setSelectedTarget("");
    setCapsules([]);
    setAnalysisProgress(null);
    try {
      const [repos, workspaceCapsules, cachedWorkspace] = await Promise.all([
        listRepositories(id),
        listWorkspaceCapsules(id),
        loadCachedWorkspace(id)
      ]);
      if (requestId !== workspaceLoadRequest.current) return;
      setRepositories(repos);
      setCapsules(workspaceCapsules);
      setWorkspace(cachedWorkspace);
    } catch (error) {
      if (requestId === workspaceLoadRequest.current) showError(t("error.changeWorkspace"), error);
    }
  }

  function handleRepositoryChange(id: string) {
    setRepositoryId(id);
    setUnitId(id ? workspace?.repositories.find((item) => item.id === id)?.projectUnits[0]?.id ?? "" : "");
    setSelectedTarget("");
  }

  function handleUnitChange(id: string) {
    setUnitId(id);
    setSelectedTarget("");
  }

  async function handlePickRepository() {
    setAppError("");
    setBusy(true);
    try {
      const selected = await open({ directory: true, multiple: false, title: t("app.selectRepository") });
      if (typeof selected !== "string") return;
      const repository = await addRepository(workspaceId, selected);
      await refreshRepositories(); setRepositoryId(repository.id); setWorkspace(null); setView("projects");
    } catch (error) { showError(t("error.addRepository"), error); } finally { setBusy(false); }
  }

  async function handleClone() {
    if (!cloneUrl.trim()) return;
    setAppError("");
    setBusy(true);
    try {
      const repository = await cloneRepository(workspaceId, cloneUrl.trim());
      await refreshRepositories(); setCloneOpen(false); setCloneUrl(""); setRepositoryId(repository.id);
      await handleScan(repository.id);
    } catch (error) { showError(t("error.cloneRepository"), error); } finally { setBusy(false); }
  }

  async function handleCreateWorkspace(name?: string) {
    const workspaceName = name?.trim();
    if (!workspaceName) {
      setWorkspaceCreateName("");
      setWorkspaceCreateOpen(true);
      return;
    }
    setAppError("");
    setBusy(true);
    try {
      const created = await createWorkspace(workspaceName);
      setWorkspaces(await listWorkspaces());
      setWorkspaceCreateOpen(false);
      setWorkspaceCreateName("");
      await handleWorkspaceChange(created.id);
    } catch (error) {
      showError(t("error.createWorkspace"), error);
    } finally {
      setBusy(false);
    }
  }

  async function handleRemoveRepository(id: string) {
    if (!id) return;
    setAppError("");
    setBusy(true);
    try {
      await deleteRepository(workspaceId, id);
      await refreshRepositories();
      setWorkspace(await loadCachedWorkspace(workspaceId));
      if (repositoryId === id) {
        setRepositoryId("");
        setUnitId("");
        setSelectedTarget("");
      }
    } catch (error) { showError(t("error.removeRepository"), error); } finally { setBusy(false); }
  }

  async function handleUpdateRepository(id: string) {
    if (!id) return;
    setAppError("");
    setBusy(true);
    try {
      await updateRepository(workspaceId, id);
      await refreshRepositories();
    } catch (error) {
      showError(t("error.updateRepository"), error);
      setBusy(false);
      return;
    }
    setBusy(false);
    await handleScan(id);
  }

  async function handleCancelAnalysis() {
    try {
      const requested = await cancelAnalysis(workspaceId);
      if (requested) {
        setAnalysisProgress((current) => current ? { ...current, stage: "cancelling" } : current);
      }
    } catch (error) { showError(t("error.cancelAnalysis"), error); }
  }

  async function handleResetWorkspace() {
    setAppError("");
    setBusy(true);
    try {
      await deleteWorkspace(workspaceId);
      const items = await listWorkspaces();
      setWorkspaces(items);
      const selected = items[0]?.id ?? "default";
      setWorkspaceId(selected);
      const repos = await listRepositories(selected);
      setRepositories(repos);
      setWorkspace(null); setRepositoryId(""); setUnitId(""); setSelectedTarget(""); setCapsules([]);
    } catch (error) { showError(t("error.resetWorkspace"), error); } finally { setBusy(false); }
  }

  async function handleResetAll() {
    setAppError("");
    setBusy(true);
    try {
      await resetAll();
      setWorkspaces([]); setWorkspaceId("default"); setRepositories([]); setRepositoryId(""); setUnitId("");
      setWorkspace(null); setSelectedTarget(""); setCapsules([]); setVerificationByPath({});
    } catch (error) { showError(t("error.resetAll"), error); } finally { setBusy(false); }
  }

  async function handleScan(preferredRepositoryId = repositoryId) {
    setAppError("");
    setBusy(true);
    const scanningAll = !preferredRepositoryId;
    try {
      const result = preferredRepositoryId
        ? await scanRepository(workspaceId, preferredRepositoryId)
        : await scanWorkspace(workspaceId);
      setWorkspace(result);
      await refreshRepositories(workspaceId);
      if (scanningAll) {
        setRepositoryId("");
        setUnitId("");
        setSelectedTarget("");
      } else {
        const repository = result.repositories.find((item) => item.id === preferredRepositoryId) ?? result.repositories[0];
        const unit = repository?.projectUnits[0];
        setRepositoryId(repository?.id ?? "");
        setUnitId(unit?.id ?? "");
        const target = unit?.model.entrypoints[0]?.id ?? (unit?.model.components[0] ? `component:${unit.model.components[0].id}` : "");
        setSelectedTarget(unit && repository && target ? `${repository.id}|${unit.id}|${target}` : "");
      }
      setView("overview");
    } catch (error) {
      try { await refreshRepositories(workspaceId); } catch {}
      const detail = error instanceof Error ? error.message : String(error);
      if (!detail.includes("Analysis was cancelled")) showError(t("error.scan"), error);
    } finally { setBusy(false); setAnalysisProgress(null); }
  }

  function handleTargetChange(value: string) {
    setSelectedTarget(value);
    const parts = value.split("|");
    if (parts.length >= 2 && parts[0] && parts[1]) {
      const newRepositoryId = parts[0];
      const newUnitId = parts[1];
      if (newRepositoryId !== repositoryId) {
        setRepositoryId(newRepositoryId);
        const newUnit = workspace?.repositories.find((item) => item.id === newRepositoryId)?.projectUnits.find((u) => u.id === newUnitId);
        setUnitId(newUnit ? newUnitId : workspace?.repositories.find((item) => item.id === newRepositoryId)?.projectUnits[0]?.id ?? "");
      } else {
        setUnitId(newUnitId);
      }
    }
  }

  function handleViewTarget(target: string) {
    if (selectedUnit && selectedRepository) { setSelectedTarget(`${selectedRepository.id}|${selectedUnit.id}|${target}`); return; }
    if (target.startsWith("component:")) {
      const [repository, unit, ...id] = target.slice(10).split("|");
      setSelectedTarget(`${repository}|${unit}|component:${id.join("|")}`);
    } else {
      setSelectedTarget(target);
    }
  }

  async function handleCreateCapsule() {
    const [targetRepository, targetUnit, ...targetParts] = selectedTarget.split("|");
    const target = targetParts.join("|");
    if (!targetRepository || !targetUnit || !target) return;
    setAppError("");
    setBusy(true);
    try { const capsule = await createWorkspaceCapsule(workspaceId, targetRepository, targetUnit, target); setCapsules((items) => [capsule, ...items.filter((item) => item.path !== capsule.path)]); setView("capsules"); } catch (error) { showError(t("error.createCapsule"), error); } finally { setBusy(false); }
  }

  async function handleVerifyCapsule(capsule: CapsuleSummary) {
    setAppError("");
    setVerifyingPath(capsule.path);
    try { const report = await verifyCapsule(capsule.path); setVerificationByPath((current) => ({ ...current, [capsule.path]: report })); } catch (error) { showError(t("error.verifyCapsule"), error); } finally { setVerifyingPath(""); }
  }

  return <div className="app-shell"><Sidebar view={view} engineState={engineState} onChange={setView} /><main className="main-content">
    {appError && <div className="app-error-banner floating-alert" role="alert"><span>{appError}</span><button type="button" aria-label={t("app.closeError")} onClick={() => setAppError("")}>×</button></div>}
    {analysisProgress && analysisProgress.workspaceId === workspaceId && <AnalysisProgressBanner progress={analysisProgress} onCancel={() => void handleCancelAnalysis()} t={t} />}
    {(["overview", "audit", "components", "entrypoints", "dependencies"] as View[]).includes(view) &&
      <ProjectToolbar workspaces={workspaces} workspaceId={workspaceId} repositories={repositories} repositoryId={repositoryId} unitId={unitId} workspace={workspace} selectedTarget={selectedTarget} targets={targets} busy={busy} showTarget={view === "dependencies"} onWorkspaceChange={(id) => void handleWorkspaceChange(id)} onRepositoryChange={handleRepositoryChange} onUnitChange={handleUnitChange} onTargetChange={handleTargetChange} onScan={() => void handleScan()} onCreateWorkspace={handleCreateWorkspace} />}
    <div className="view-container">
      {view === "overview" && <OverviewView model={model} runtime={runtime} workspace={unitId ? null : workspace} analysis={unitId ? selectedAnalysis : null} onOpenAudit={() => setView("audit")} />}
      {view === "projects" && <ProjectsView repositories={repositories} workspace={workspace} currentId={repositoryId} onSelect={handleRepositoryChange} onAdd={handlePickRepository} onClone={() => setCloneOpen(true)} onUpdate={(id) => void handleUpdateRepository(id)} onRemove={(id) => void handleRemoveRepository(id)} onCreateWorkspace={(name) => void handleCreateWorkspace(name)} onResetWorkspace={() => void handleResetWorkspace()} onResetAll={() => void handleResetAll()} busy={busy} />}
      {view === "architecture" && <ArchitectureView workspace={workspace} />}
      {view === "audit" && <AuditView repositories={repositories} workspace={workspace} repositoryId={repositoryId} busy={busy} onScan={() => void handleScan()} />}
      {view === "components" && <ComponentsView model={model} selectedTarget={localTarget} onSelectTarget={handleViewTarget} onCreateCapsule={handleCreateCapsule} canCreateCapsule={Boolean(selectedTarget)} busy={busy} />}
      {view === "entrypoints" && <EntrypointsView model={model} selectedTarget={localTarget} onSelectTarget={handleViewTarget} onCreateCapsule={handleCreateCapsule} canCreateCapsule={Boolean(selectedTarget)} busy={busy} />}
      {view === "dependencies" && <DependenciesView model={model} selectedTarget={localTarget} workspace={workspace} workspaceId={workspaceId} unitId={unitId} onCreateCapsule={handleCreateCapsule} canCreateCapsule={Boolean(selectedTarget)} busy={busy} />}
      {view === "capsules" && <CapsulesView capsules={capsules} verificationByPath={verificationByPath} verifyingPath={verifyingPath} onVerify={(capsule) => void handleVerifyCapsule(capsule)} />}
    </div>
  </main>
  {workspaceCreateOpen && <Modal title={t("projects.newWorkspace")} className="project-modal project-modal--workspace" onClose={() => { if (!busy) setWorkspaceCreateOpen(false); }} footer={<><button className="button secondary" disabled={busy} onClick={() => setWorkspaceCreateOpen(false)}>{t("common.cancel")}</button><button className="button primary" disabled={busy || !workspaceCreateName.trim()} onClick={() => void handleCreateWorkspace(workspaceCreateName)}>{t("common.create")}</button></>}>
    <label>{t("projects.workspaceName")}<input autoFocus value={workspaceCreateName} onChange={(event) => setWorkspaceCreateName(event.target.value)} onKeyDown={(event) => { if (event.key === "Enter" && workspaceCreateName.trim() && !busy) void handleCreateWorkspace(workspaceCreateName); }} /></label>
  </Modal>}
  {cloneOpen && <Modal title={t("app.cloneRepository")} className="project-modal project-modal--clone" onClose={() => { if (!busy) setCloneOpen(false); }} footer={<><button className="button secondary" disabled={busy} onClick={() => setCloneOpen(false)}>{t("common.cancel")}</button><button className="button primary" disabled={busy || !cloneUrl.trim()} onClick={() => void handleClone()}>{busy ? t("app.cloning") : t("app.clone")}</button></>}><label>{t("app.repositoryUrl")}<input autoFocus value={cloneUrl} onChange={(event) => setCloneUrl(event.target.value)} placeholder="https://github.com/org/project.git" /></label></Modal>}
</div>;
}

export default App;

type Translate = (key: string, params?: Record<string, string | number>) => string;

function AnalysisProgressBanner({ progress, onCancel, t }: { progress: AnalysisProgress; onCancel: () => void; t: Translate }) {
  const total = Math.max(progress.totalRepositories, 1);
  const base = Math.round((progress.completedRepositories / total) * 85);
  const percent = progress.stage === "completed" ? 100 : progress.stage === "linking" ? 97 : progress.stage === "validating" ? 92 : Math.min(90, Math.max(4, 5 + base));
  const stageKey = `analysisProgress.${progress.stage}`;
  const stage = t(stageKey, { repository: progress.repositoryName ?? "" });
  return <div className="analysis-progress-layer" role="presentation">
    <div className="analysis-progress-banner" role="status" aria-live="polite">
      <div className="analysis-progress-banner__head">
        <div className="analysis-progress-banner__copy">
          <span className="analysis-progress-banner__pulse" aria-hidden="true" />
          <div><strong>{t("analysisProgress.title")}</strong><span title={stage}>{stage}</span></div>
        </div>
        <span className="analysis-progress-banner__percent">{percent}%</span>
      </div>
      <div className="analysis-progress-banner__track" aria-label={t("analysisProgress.title")} aria-valuemin={0} aria-valuemax={100} aria-valuenow={percent} role="progressbar">
        <span style={{ width: `${percent}%` }} />
      </div>
      {progress.stage !== "completed" && progress.stage !== "cancelling" && <button className="button secondary" type="button" onClick={onCancel}>{t("common.cancel")}</button>}
    </div>
  </div>;
}

function compatibilityRank(value: string): number {
  const match = /^L([0-5])$/.exec(value);
  return match ? Number(match[1]) : 0;
}
