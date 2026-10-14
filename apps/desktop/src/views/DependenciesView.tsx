import { useEffect, useMemo, useState } from "react";
import type { Dependency, ProjectModel, WorkspaceModel } from "../types";
import { extractName } from "../lib/format";
import { translateEngineErrorDetail, useI18n } from "../i18n";
import { impactWorkspaceDependencies, sliceWorkspaceDependencies } from "../lib/client";
import FilterToolbar from "../components/FilterToolbar";
import Pagination from "../components/Pagination";
import SectionHeader from "../components/SectionHeader";
import { CapsuleIcon, DependenciesIcon, DependencyNodeIcon } from "../components/Icons";

type DependenciesViewProps = {
  model: ProjectModel | null;
  selectedTarget: string;
  workspace?: WorkspaceModel | null;
  workspaceId: string;
  unitId?: string;
  onCreateCapsule: () => void;
  canCreateCapsule: boolean;
  busy: boolean;
};

type DependencyMode = "slice" | "impact";
type DisplayMode = "list" | "graph";

export default function DependenciesView({ model, selectedTarget, workspace, workspaceId, unitId, onCreateCapsule, canCreateCapsule, busy }: DependenciesViewProps) {
  const { t } = useI18n();
  const [query, setQuery] = useState("");
  const [mode, setMode] = useState<DependencyMode>("slice");
  const [displayMode, setDisplayMode] = useState<DisplayMode>("list");
  const [engineDependencies, setEngineDependencies] = useState<Dependency[] | null>(null);
  const [analysisError, setAnalysisError] = useState("");
  const [page, setPage] = useState(1);

  useEffect(() => {
    let active = true;
    setAnalysisError("");
    if (!workspace || !unitId || !selectedTarget) {
      setEngineDependencies(null);
      return () => { active = false; };
    }
    setEngineDependencies(null);
    const request = mode === "impact" ? impactWorkspaceDependencies : sliceWorkspaceDependencies;
    void request(workspaceId, unitId, selectedTarget)
      .then((dependencies) => { if (active) setEngineDependencies(dependencies); })
      .catch((error) => {
        if (active) {
          setEngineDependencies([]);
          setAnalysisError(translateEngineErrorDetail(String(error), t));
        }
      });
    return () => { active = false; };
  }, [workspace, workspaceId, unitId, selectedTarget, mode, t]);

  const targetInfo = useMemo(() => {
    if (!model || !selectedTarget) return null;
    if (selectedTarget.startsWith("component:")) {
      const componentId = selectedTarget.slice("component:".length);
      const component = model.components.find((item) => item.id === componentId);
      return component ? { rootId: component.id, label: component.name, kind: component.kind } : null;
    }
    const entrypoint = model.entrypoints.find((item) => item.id === selectedTarget);
    return entrypoint ? { rootId: entrypoint.componentId, label: entrypoint.name, kind: entrypoint.kind } : null;
  }, [model, selectedTarget]);

  const displayedDependencies = useMemo(() => {
    if (!model) return [];
    if (workspace && unitId && selectedTarget) return engineDependencies ?? [];
    return deduplicateDependencies(model.dependencies);
  }, [model, workspace, unitId, selectedTarget, engineDependencies]);

  const filtered = useMemo(() => {
    const normalized = query.trim().toLowerCase();
    return displayedDependencies.filter((dependency) =>
      !normalized || dependency.sourceId.toLowerCase().includes(normalized) || dependency.targetId.toLowerCase().includes(normalized) || dependency.kind.toLowerCase().includes(normalized)
    );
  }, [displayedDependencies, query]);

  const pageSize = 80;
  const totalPages = Math.max(1, Math.ceil(filtered.length / pageSize));
  const safePage = Math.min(page, totalPages);
  const pageItems = filtered.slice((safePage - 1) * pageSize, safePage * pageSize);

  const unitAudit = useMemo(() => {
    if (!workspace || !unitId) return null;
    for (const repository of workspace.repositories) {
      const audit = repository.audit.units.find((item) => item.projectUnitId === unitId);
      if (audit) return audit;
    }
    return null;
  }, [workspace, unitId]);

  const relationKinds = useMemo(() => new Set(displayedDependencies.map((item) => item.kind)).size, [displayedDependencies]);
  const graphNodes = useMemo(() => uniqueGraphNodes(filtered, targetInfo?.rootId ?? ""), [filtered, targetInfo]);

  if (!model) {
    return (
      <div className="empty-state large dependencies-empty">
        <div className="view-empty-icon dependencies-empty__icon"><DependenciesIcon size={30} /></div>
        <strong>{t("dependencies.empty")}</strong>
      </div>
    );
  }

  const targetAnalysisAvailable = Boolean(workspace && unitId && selectedTarget);
  const loading = targetAnalysisAvailable && engineDependencies === null;

  return (
    <section className="panel table-panel dependencies-panel">
      <SectionHeader
        title={t("dependencies.title")}
        icon={<DependenciesIcon size={26} />}
        className="view-section-header"
        right={<div className="view-header-actions">
          {unitAudit && <span className={`audit-check ${unitAudit.passed ? "is-pass" : "is-fail"}`} title={`${unitAudit.missingSources.length + unitAudit.missingTargets.length} referencias faltantes · ${unitAudit.cycles.length} ciclos`}>{t("common.graph")} {unitAudit.passed ? "PASS" : "FAIL"}</span>}
          {targetAnalysisAvailable && (
            <div className="dependency-analysis-toggle" role="group" aria-label={t("dependencies.mode")}>
              <button className={`button button-sm ${mode === "slice" ? "primary" : "secondary"}`} type="button" onClick={() => { setMode("slice"); setPage(1); }} aria-pressed={mode === "slice"}>{t("common.dependencies")}</button>
              <button className={`button button-sm ${mode === "impact" ? "primary" : "secondary"}`} type="button" onClick={() => { setMode("impact"); setPage(1); }} aria-pressed={mode === "impact"}>{t("common.impact")}</button>
            </div>
          )}
          <span className="count-badge">{displayedDependencies.length} {t("common.relationships").toLowerCase()}</span>
          {canCreateCapsule && <button className="button primary button-sm" onClick={onCreateCapsule} disabled={busy}><CapsuleIcon size={15} />{t("components.createCapsule")}</button>}
        </div>}
      />

      {targetInfo && (
        <div className="target-context target-context-expanded">
          <div><span className="target-context-label">{t("common.target").toUpperCase()}</span><strong>{targetInfo.label}</strong></div>
          <span className="target-context-kind">{targetInfo.kind}</span>
          <span className="target-context-mode">{mode === "slice" ? t("dependencies.required") : t("dependencies.affected")}</span>
          <span className="target-context-stat">{displayedDependencies.length} {t("common.relationships").toLowerCase()}</span>
          <span className="target-context-stat">{t("dependencies.types", { count: relationKinds })}</span>
        </div>
      )}

      {analysisError && <div className="app-error-banner floating-alert floating-alert--local" role="alert"><span>{t("dependencies.analysisError", { error: analysisError })}</span><button type="button" aria-label={t("common.close")} onClick={() => setAnalysisError("")}>×</button></div>}

      <div className="dependency-controls-row">
        <FilterToolbar query={query} onQueryChange={(event) => { setQuery(event.target.value); setPage(1); }} placeholder={t("dependencies.search")} ariaLabel={t("dependencies.searchAria")} />
        <div className="display-toggle" role="group" aria-label={t("dependencies.representation")}>
          <button type="button" className={displayMode === "list" ? "active" : ""} aria-pressed={displayMode === "list"} onClick={() => setDisplayMode("list")}>{t("common.list")}</button>
          <button type="button" className={displayMode === "graph" ? "active" : ""} aria-pressed={displayMode === "graph"} onClick={() => setDisplayMode("graph")}>{t("common.graph")}</button>
        </div>
      </div>

      {filtered.length === 0 ? (
        <div className="empty-state dependency-empty">
          <div className="empty-state-icon"><DependenciesIcon size={28} /></div>
          <strong>{t("dependencies.noRelations")}</strong>
          <span>{loading ? t("dependencies.calculating", { mode: mode === "slice" ? t("common.dependencies").toLowerCase() : t("common.impact").toLowerCase() }) : targetInfo ? mode === "slice" ? t("dependencies.noOutgoing") : t("dependencies.noConsumers") : t("dependencies.noneDetected")}</span>
        </div>
      ) : displayMode === "graph" ? (
        <DependencyGraph dependencies={filtered} rootId={targetInfo?.rootId ?? ""} nodes={graphNodes} mode={mode} />
      ) : (
        <div className="dependency-table">
          <div className="dependency-table-head" aria-hidden="true"><span>{t("common.source").toUpperCase()}</span><span>{t("common.relationship").toUpperCase()}</span><span>{t("common.destination").toUpperCase()}</span></div>
          <div className="dependency-list">
            {pageItems.map((dependency, index) => (
              <div className={`dependency-row dependency-tone-${dependencyRelationTone(dependency.kind)}`} key={`${dependency.sourceId}-${dependency.targetId}-${dependency.kind}-${index}`}>
                <div className="dependency-node source"><span className="dependency-node-icon" aria-hidden="true"><DependencyNodeIcon size={24} /></span><span className="dependency-node-copy"><strong>{extractName(dependency.sourceId)}</strong><code title={dependency.sourceId}>{dependency.sourceId}</code></span></div>
                <div className="dependency-edge"><span className="edge-line" /><b title={dependency.evidence}>{dependency.kind}{dependency.confidence ? ` · ${dependency.confidence}%` : ""}</b><span className="edge-arrow">›</span></div>
                <div className="dependency-node target"><span className="dependency-node-icon" aria-hidden="true"><DependencyNodeIcon size={24} /></span><span className="dependency-node-copy"><strong>{extractName(dependency.targetId)}</strong><code title={dependency.targetId}>{dependency.targetId}</code></span></div>
              </div>
            ))}
          </div>
          <Pagination page={safePage} totalPages={totalPages} totalItems={filtered.length} pageSize={pageSize} previousLabel={t("projects.previous")} nextLabel={t("projects.next")} onPageChange={setPage} />
        </div>
      )}
    </section>
  );
}

function DependencyGraph({ dependencies, rootId, nodes, mode }: { dependencies: Dependency[]; rootId: string; nodes: string[]; mode: DependencyMode }) {
  const { t } = useI18n();
  const degree = new Map<string, number>();
  dependencies.forEach((dependency) => {
    degree.set(dependency.sourceId, (degree.get(dependency.sourceId) ?? 0) + 1);
    degree.set(dependency.targetId, (degree.get(dependency.targetId) ?? 0) + 1);
  });
  const visibleNodes = nodes.slice(0, 80);
  return (
    <div className="dependency-graph-canvas">
      <div className="dependency-graph-legend"><span>{mode === "slice" ? t("dependencies.targetToDeps") : t("dependencies.consumersToTarget")}</span><span>{t("dependencies.nodesVisible", { count: visibleNodes.length })}</span><span>{t("dependencies.edges", { count: dependencies.length })}</span></div>
      <div className="dependency-graph-root">
        <span>{t("common.target").toUpperCase()}</span><strong>{extractName(rootId) || t("dependencies.targetSelected")}</strong><code title={rootId}>{rootId}</code>
      </div>
      <div className="dependency-graph-node-grid">
        {visibleNodes.filter((node) => node !== rootId).map((node) => (
          <div className="dependency-graph-node" key={node}>
            <DependencyNodeIcon size={20} />
            <div><strong>{extractName(node)}</strong><code title={node}>{node}</code></div>
            <span>{degree.get(node) ?? 0}</span>
          </div>
        ))}
      </div>
      {nodes.length > visibleNodes.length && <div className="dependency-graph-limit">{t("dependencies.graphLimit")}</div>}
    </div>
  );
}

function uniqueGraphNodes(dependencies: Dependency[], rootId: string): string[] {
  const nodes = new Set<string>();
  if (rootId) nodes.add(rootId);
  dependencies.forEach((dependency) => { nodes.add(dependency.sourceId); nodes.add(dependency.targetId); });
  return Array.from(nodes);
}

function dependencyRelationTone(kind: string): string {
  const normalized = kind.trim().toLowerCase();
  if (normalized.includes("refer")) return "violet";
  if (normalized.includes("http") || normalized.includes("call")) return "blue";
  if (normalized.includes("extend") || normalized.includes("inherit")) return "amber";
  return "cyan";
}

function dependencyKey(dependency: Dependency): string {
  return `${dependency.sourceId}\u0000${dependency.targetId}\u0000${dependency.kind}`;
}

function deduplicateDependencies(dependencies: Dependency[]): Dependency[] {
  const keys = new Set<string>();
  return dependencies.filter((dependency) => {
    const key = dependencyKey(dependency);
    if (keys.has(key)) return false;
    keys.add(key);
    return true;
  });
}
