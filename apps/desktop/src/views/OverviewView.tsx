import { useState } from "react";
import type { AnalysisRecord, ProjectModel, RuntimeCapabilities, WorkspaceModel } from "../types";
import { componentTone, formatDuration } from "../lib/format";
import { translateComponentKind, translateRole, translateTechnologyCategory, useI18n } from "../i18n";
import MetricCard from "../components/MetricCard";
import SectionHeader from "../components/SectionHeader";
import ViewTabs from "../components/ViewTabs";
import {
  DependenciesIcon,
  FilesIcon,
  MetricComponentsIcon,
  MetricEntrypointsIcon,
  OverviewIcon,
  RuntimeBrandIcon,
  TechnologyBrandIcon
} from "../components/Icons";

type OverviewViewProps = {
  model: ProjectModel | null;
  runtime: RuntimeCapabilities | null;
  workspace?: WorkspaceModel | null;
  analysis?: AnalysisRecord | null;
  onOpenAudit?: () => void;
};

type OverviewSection = "summary" | "technologies" | "runtime";

export default function OverviewView({ model, runtime, workspace, analysis, onOpenAudit }: OverviewViewProps) {
  const { language, t, formatDateTime } = useI18n();
  const [section, setSection] = useState<OverviewSection>("summary");

  if (workspace) {
    const units = workspace.repositories.flatMap((repository) => repository.projectUnits);
    const roles = ["Frontend", "Backend", "Fullstack", "Library", "Infrastructure", "Unknown"];
    const components = units.reduce((total, unit) => total + unit.model.components.length, 0);
    const entrypoints = units.reduce((total, unit) => total + unit.model.entrypoints.length, 0);
    const dependencies = units.reduce((total, unit) => total + unit.model.dependencies.length, 0);

    return (
      <div className="view-stack overview-view overview-workspace-view">
        <section className="metrics-grid overview-metrics">
          <MetricCard label={t("common.repositories")} value={workspace.repositories.length} tone="cyan" icon={<FilesIcon size={32} />} />
          <MetricCard label={t("common.projectUnits")} value={units.length} tone="violet" icon={<MetricComponentsIcon size={32} />} />
          <MetricCard label={t("common.components")} value={components} tone="green" icon={<MetricEntrypointsIcon size={32} />} />
          <MetricCard label={t("common.entrypoints")} value={entrypoints} tone="amber" icon={<DependenciesIcon size={32} />} />
        </section>

        <section className="overview-columns overview-workspace-panels">
          <article className="panel compact-panel">
            <SectionHeader title={t("common.dependencies")} />
            <div className="metric-pair">
              <div className="metric-pair-item"><strong>{dependencies}</strong><span>{t("overview.internal")}</span></div>
              <div className="metric-pair-item"><strong>{workspace.crossProjectDependencies.length}</strong><span>{t("overview.crossProject")}</span></div>
            </div>
          </article>
          <article className="panel compact-panel">
            <SectionHeader title={t("overview.roleDistribution")} />
            <div className="role-distribution role-distribution--compact">
              {roles.map((role) => (
                <div key={role}><span>{translateRole(role, t)}</span><strong>{units.filter((unit) => unit.role === role).length}</strong></div>
              ))}
            </div>
          </article>
        </section>
      </div>
    );
  }

  if (!model) {
    return (
      <div className="empty-state large overview-empty">
        <div className="view-empty-icon overview-empty__icon"><OverviewIcon size={30} /></div>
        <strong>{t("overview.noData")}</strong>
      </div>
    );
  }

  const componentCounts = new Map<string, number>();
  model.components.forEach((component) => componentCounts.set(component.kind, (componentCounts.get(component.kind) ?? 0) + 1));
  const counts = Array.from(componentCounts.entries()).sort((a, b) => b[1] - a[1]);

  const runtimeEntries = runtime
    ? model.analysis.runtimeRequirements
        .filter((requirement, index, all) => all.findIndex((item) => item.executable === requirement.executable && item.name === requirement.name) === index)
        .map((requirement) => {
          const tool = runtime.tools.find((item) => item.executable.toLowerCase() === requirement.executable.toLowerCase());
          return { ...requirement, available: tool?.available ?? false, version: tool?.version };
        })
    : [];

  const availableProjectRuntimes = runtimeEntries.filter((entry) => entry.available).length;

  return (
    <div className="view-stack overview-view">
      {analysis && (
        <section className={`analysis-status-strip status-${analysis.status.toLowerCase()}`}>
          <div className="analysis-status-main">
            <span className="analysis-status-dot" />
            <div><strong>{analysisStatusLabel(analysis.status, t)}</strong><span>{t("common.lastAnalysis")} {formatDateTime(analysis.completedAtMs)}</span></div>
          </div>
          <div className="analysis-status-facts">
            <span><small>{t("common.graph").toUpperCase()}</small><b className={analysis.graphPassed ? "is-pass" : "is-fail"}>{analysis.graphPassed ? "PASS" : "FAIL"}</b></span>
            <span><small>{t("common.duration").toUpperCase()}</small><b>{formatDuration(analysis.durationMs)}</b></span>
            <span><small>COMMIT</small><b>{analysis.commit?.slice(0, 8) ?? "—"}</b></span>
            <span><small>{t("common.diagnostics").toUpperCase()}</small><b>{analysis.diagnosticsError} E · {analysis.diagnosticsWarning} W</b></span>
          </div>
          {onOpenAudit && <button className="button secondary button-sm" type="button" onClick={onOpenAudit}>{t("overview.viewAudit")}</button>}
        </section>
      )}

      <section className="metrics-grid overview-metrics">
        <MetricCard label={t("common.files")} value={model.files} tone="cyan" icon={<FilesIcon size={32} />} />
        <MetricCard label={t("common.components")} value={model.components.length} tone="violet" icon={<MetricComponentsIcon size={32} />} />
        <MetricCard label={t("common.entrypoints")} value={model.entrypoints.length} tone="green" icon={<MetricEntrypointsIcon size={32} />} />
        <MetricCard label={t("common.dependencies")} value={model.dependencies.length} tone="amber" icon={<DependenciesIcon size={32} />} />
      </section>

      <ViewTabs
        value={section}
        onChange={setSection}
        ariaLabel={t("nav.overview")}
        className="overview-view-tabs"
        options={[
          { id: "summary", label: t("nav.overview") },
          { id: "technologies", label: t("overview.technologies"), count: model.technologies.length },
          { id: "runtime", label: t("overview.runtime"), count: runtimeEntries.length }
        ]}
      />

      <div className="view-tab-content overview-tab-content">
        {section === "summary" && (
          <section className="overview-columns overview-summary-columns">
            <article className="panel compatibility-panel">
              <SectionHeader title={t("overview.compatibility")} />
              <div className="compatibility-content">
                <div className="compatibility-ring"><span>{model.compatibility}</span></div>
                <div className="compatibility-copy"><strong>{compatibilityText(model.compatibility, t)}</strong></div>
              </div>
              <div className="compatibility-scale">
                {["L0", "L1", "L2", "L3"].map((level) => (
                  <div className={`scale-node ${Number(level.slice(1)) <= Number(model.compatibility.slice(1)) ? "active" : ""}`} key={level}>
                    <span>{level}</span><small>{compatibilityScaleText(level, t)}</small>
                  </div>
                ))}
              </div>
            </article>

            <article className="panel distribution-panel">
              <SectionHeader title={t("overview.componentDistribution")} right={<span className="count-badge">{model.components.length} {t("common.total")}</span>} />
              <div className="component-distribution compact-distribution">
                {counts.slice(0, 12).map(([kind, count]) => {
                  const percentage = model.components.length === 0 ? 0 : Math.round((count / model.components.length) * 100);
                  return (
                    <div className={`distribution-card tone-${componentTone(kind)}`} key={kind}>
                      <div className="distribution-head"><strong>{count}</strong><span>{percentage}%</span></div>
                      <span className="distribution-label">{translateComponentKind(kind, language)}</span>
                      <div className="distribution-track"><span style={{ width: `${percentage}%` }} /></div>
                    </div>
                  );
                })}
              </div>
            </article>
          </section>
        )}

        {section === "technologies" && (
          <article className="panel technologies-panel overview-full-panel">
            <SectionHeader title={t("overview.technologies")} right={<span className="count-badge">{model.technologies.length}</span>} />
            <div className="technology-catalog technology-catalog--wide" role="list" aria-label={t("overview.technologies")}>
              {model.technologies.map((technology) => (
                <article className="technology-tile" role="listitem" key={`${technology.category}-${technology.name}-${technology.projectUnitId ?? "local"}`} title={technology.evidence.map((item) => `${item.source}: ${item.detail}`).join("\n") || undefined}>
                  <div className="technology-name-cell">
                    <div className="technology-icon" aria-hidden="true"><TechnologyBrandIcon name={technology.name} size={28} /></div>
                    <div className="technology-name-copy"><strong>{technology.name}</strong><span>{translateTechnologyCategory(technology.classification || technology.category, language)}</span></div>
                  </div>
                  <div className="confidence technology-confidence"><strong>{technology.confidence}%</strong><div className="confidence-track"><span style={{ width: `${technology.confidence}%` }} /></div></div>
                </article>
              ))}
            </div>
          </article>
        )}

        {section === "runtime" && (
          <article className="panel runtime-panel overview-full-panel">
            <SectionHeader title={t("overview.runtime")} right={runtimeEntries.length > 0 ? <span className="count-badge">{availableProjectRuntimes}/{runtimeEntries.length} {t("common.available").toLowerCase()}</span> : undefined} />
            <div className="runtime-grid runtime-grid--wide">
              {runtimeEntries.length > 0 ? runtimeEntries.map(({ name, executable, available, version }) => (
                <div className={`runtime-card runtime-${runtimeTone(name)} ${available ? "is-available" : "is-missing"}`} key={`${name}-${executable}`}>
                  <div className="runtime-brand-shell" aria-hidden="true"><RuntimeBrandIcon name={name} size={32} /></div>
                  <div className="runtime-card-copy"><strong>{name}</strong><span title={version ?? undefined}>{available ? t("common.available") : t("common.notDetected")}</span>{version && <small>{version}</small>}</div>
                </div>
              )) : <div className="runtime-empty">{runtime ? t("overview.noRuntime") : t("overview.runtimeUnavailable")}</div>}
            </div>
          </article>
        )}
      </div>
    </div>
  );
}

function compatibilityScaleText(level: string, t: (key: string) => string): string {
  if (level === "L0") return t("overview.detected");
  if (level === "L1") return t("overview.syntax");
  if (level === "L2") return t("overview.structure");
  return t("overview.framework");
}

function compatibilityText(level: string, t: (key: string) => string): string {
  if (level === "L0") return t("architecture.filesystem");
  if (level === "L1") return t("overview.syntax");
  if (level === "L2") return t("overview.structure");
  if (level === "L3") return t("overview.framework");
  if (level === "L4") return "Runtime";
  if (level === "L5") return "Capsule";
  return t("common.unknown");
}

function runtimeTone(name: string): string {
  const normalized = name.toLowerCase();
  if (normalized === "node.js") return "node";
  if (normalized === ".net") return "dotnet";
  if (normalized === "java") return "java";
  if (normalized === "docker") return "docker";
  if (normalized === "python") return "python";
  if (normalized === "php") return "php";
  if (normalized === "composer") return "composer";
  if (normalized === "go") return "go";
  if (normalized === "rust") return "rust";
  if (normalized === "ruby") return "ruby";
  if (normalized === "elixir") return "elixir";
  if (normalized === "mix") return "mix";
  return "generic";
}

function analysisStatusLabel(status: string, t: (key: string) => string): string {
  if (status === "COMPLETED") return t("overview.analysisCompleted");
  if (status === "COMPLETED_WITH_WARNINGS") return t("overview.analysisWarnings");
  if (status === "PARTIAL") return t("overview.analysisPartial");
  if (status === "FAILED") return t("overview.analysisFailed");
  return status;
}
