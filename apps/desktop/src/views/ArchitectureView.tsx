import { useState, type ReactNode } from "react";
import type { WorkspaceModel } from "../types";
import SectionHeader from "../components/SectionHeader";
import ViewTabs from "../components/ViewTabs";
import { translateComponentKind, translateRole, useI18n } from "../i18n";
import {
  DependenciesIcon,
  FilesIcon,
  GlobeIcon,
  LinkIcon,
  MetricComponentsIcon,
  ServerIcon
} from "../components/Icons";

type ArchitectureViewProps = { workspace: WorkspaceModel | null };
type ArchitectureSection = "repositories" | "relations";
type ArchitectureTone = "cyan" | "violet" | "emerald" | "amber" | "blue" | "green" | "slate";

function normalizeRole(role?: string): string { return role?.trim().toLowerCase() ?? "unknown"; }
function roleTone(role?: string): ArchitectureTone {
  const normalized = normalizeRole(role);
  if (normalized.includes("backend")) return "violet";
  if (normalized.includes("frontend")) return "emerald";
  if (normalized.includes("fullstack")) return "amber";
  if (normalized.includes("library")) return "blue";
  if (normalized.includes("infra") || normalized.includes("platform")) return "green";
  if (normalized.includes("unknown")) return "cyan";
  return "slate";
}
function roleLabel(role?: string): string { const value = role?.trim(); return value && value.length > 0 ? value : "Unknown"; }
function repositoryIcon(role?: string): ReactNode {
  const normalized = normalizeRole(role);
  if (normalized.includes("backend")) return <MetricComponentsIcon size={40} />;
  if (normalized.includes("frontend")) return <GlobeIcon size={42} />;
  if (normalized.includes("fullstack")) return <ServerIcon size={40} />;
  if (normalized.includes("infra") || normalized.includes("platform")) return <ServerIcon size={40} />;
  if (normalized.includes("library")) return <MetricComponentsIcon size={40} />;
  return <FilesIcon size={40} />;
}
function technologyNames(technologies: Array<{ name: string }>): string[] {
  return technologies.map((technology) => technology.name.trim()).filter(Boolean).filter((name, index, items) => items.findIndex((item) => item.toLowerCase() === name.toLowerCase()) === index).slice(0, 4);
}

export default function ArchitectureView({ workspace }: ArchitectureViewProps) {
  const { language, t } = useI18n();
  const [section, setSection] = useState<ArchitectureSection>("repositories");

  if (!workspace) {
    return <div className="empty-state large architecture-empty"><div className="view-empty-icon architecture-empty__icon"><DependenciesIcon size={30} /></div><strong>{t("architecture.empty")}</strong></div>;
  }

  const unitCount = workspace.repositories.reduce((total, repository) => total + repository.projectUnits.length, 0);

  return (
    <section className="architecture-screen">
      <SectionHeader
        title={t("architecture.title")}
        icon={<DependenciesIcon size={26} />}
        className="view-section-header"
        right={<div className="architecture-header-status">
          <span className={`audit-check ${workspace.graphIntegrity.passed ? "is-pass" : "is-fail"}`} title={`${workspace.graphIntegrity.errorCount} ${t("common.errors")}`}>{t("common.graph")} {workspace.graphIntegrity.passed ? "PASS" : "FAIL"}</span>
          <div className="architecture-http-badge" title={t("architecture.crossProjectRelations", { count: workspace.crossProjectDependencies.length })}><LinkIcon size={18} /><span>{t("architecture.links")}</span>{workspace.crossProjectDependencies.length > 0 && <b>{workspace.crossProjectDependencies.length}</b>}</div>
        </div>}
      />

      <div className="architecture-summary-strip" aria-label={t("architecture.title")}>
        <div className="architecture-summary-stat"><span>{t("common.repositories").toUpperCase()}</span><strong>{workspace.repositories.length}</strong></div>
        <div className="architecture-summary-stat"><span>{t("common.projectUnits").toUpperCase()}</span><strong>{unitCount}</strong></div>
        <div className="architecture-summary-stat"><span>{t("common.crossProject").toUpperCase()}</span><strong>{workspace.crossProjectDependencies.length}</strong></div>
        <div className="architecture-summary-stat"><span>{t("common.graph").toUpperCase()}</span><strong className={workspace.graphIntegrity.passed ? "is-pass" : "is-fail"}>{workspace.graphIntegrity.passed ? "PASS" : "FAIL"}</strong></div>
      </div>

      <ViewTabs
        value={section}
        onChange={setSection}
        ariaLabel={t("architecture.title")}
        options={[
          { id: "repositories", label: t("common.repositories"), count: workspace.repositories.length },
          { id: "relations", label: t("architecture.relationsTitle"), count: workspace.crossProjectDependencies.length }
        ]}
      />

      <div className="view-tab-content architecture-tab-content">
        {section === "repositories" && (
          <div className="architecture-grid-v2">
            {workspace.repositories.map((repository) => {
              const primaryUnit = repository.projectUnits[0];
              const tone = roleTone(primaryUnit?.role);
              const role = roleLabel(primaryUnit?.role);
              const relationCount = workspace.crossProjectDependencies.filter((dependency) => dependency.sourceRepositoryId === repository.id || dependency.targetRepositoryId === repository.id).length;
              return (
                <article className={`architecture-repo-card architecture-repo-card--${tone}`} key={repository.id}>
                  <div className="architecture-repo-card__top">
                    <span className={`architecture-repo-card__hero architecture-repo-card__hero--${tone}`} aria-hidden="true">{repositoryIcon(primaryUnit?.role)}</span>
                    <span className={`architecture-repo-card__source architecture-repo-card__source--${tone}`}>{repository.origin === "clone" ? t("common.gitClone").toUpperCase() : repository.git ? "GIT" : t("common.folder").toUpperCase()}</span>
                  </div>
                  <div className="architecture-repo-card__identity"><strong title={repository.name}>{repository.name}</strong><span className={`architecture-role architecture-role--${tone}`}>{translateRole(role, t).toUpperCase()}</span></div>
                  <div className="architecture-repo-card__divider" />
                  <div className="architecture-unit-list-v2">
                    {repository.projectUnits.length > 0 ? repository.projectUnits.map((unit) => {
                      const unitTone = roleTone(unit.role);
                      const technologies = technologyNames(unit.model.technologies);
                      return (
                        <div className="architecture-unit-v2" key={unit.id}>
                          <div className="architecture-unit-v2__identity"><span className={`architecture-unit-v2__icon architecture-unit-v2__icon--${unitTone}`} aria-hidden="true"><MetricComponentsIcon size={20} /></span><strong title={unit.name}>{unit.name}</strong></div>
                          <div className="architecture-tech-list">{technologies.length > 0 ? technologies.map((technology) => <span className={`architecture-tech architecture-tech--${unitTone}`} key={`${unit.id}-${technology}`}>{technology}</span>) : <span className={`architecture-tech architecture-tech--${unitTone}`}>{t("architecture.filesystem")}</span>}</div>
                          <div className="architecture-layer-strip">
                            {topComponentKinds(unit.model.components).map(([kind, count]) => <span key={`${unit.id}-${kind}`}><b>{count}</b>{translateComponentKind(kind, language)}</span>)}
                            <span className="architecture-layer-total"><b>{unit.model.entrypoints.length}</b>{t("common.entrypoints").toLowerCase()}</span>
                            <span className="architecture-layer-total"><b>{unit.model.dependencies.length}</b>{t("common.dependencies").toLowerCase()}</span>
                          </div>
                        </div>
                      );
                    }) : <div className="architecture-unit-v2 architecture-unit-v2--empty"><div className="architecture-unit-v2__identity"><span className="architecture-unit-v2__icon architecture-unit-v2__icon--slate" aria-hidden="true"><FilesIcon size={19} /></span><strong>{t("architecture.noUnits")}</strong></div></div>}
                  </div>
                  <div className="architecture-repo-card__divider architecture-repo-card__divider--bottom" />
                  <div className="architecture-repo-card__relations"><DependenciesIcon size={21} /><span>{t("architecture.crossProjectRelations", { count: relationCount })}</span></div>
                </article>
              );
            })}
          </div>
        )}

        {section === "relations" && (
          workspace.crossProjectDependencies.length > 0 ? (
            <section className="architecture-relations-v2 architecture-relations-v2--standalone">
              <div className="architecture-relations-v2__header"><div><strong>{t("architecture.relationsTitle")}</strong><span>{t("architecture.links")}</span></div><span className="architecture-relations-v2__count">{workspace.crossProjectDependencies.length}</span></div>
              <div className="architecture-relations-v2__list">
                {workspace.crossProjectDependencies.map((dependency) => {
                  const sourceRepository = workspace.repositories.find((item) => item.id === dependency.sourceRepositoryId);
                  const targetRepository = workspace.repositories.find((item) => item.id === dependency.targetRepositoryId);
                  const sourceUnit = sourceRepository?.projectUnits.find((item) => item.id === dependency.sourceProjectUnitId);
                  const targetUnit = targetRepository?.projectUnits.find((item) => item.id === dependency.targetProjectUnitId);
                  return (
                    <div className="architecture-relation-v2" key={`${dependency.sourceRepositoryId}-${dependency.sourceProjectUnitId}-${dependency.sourceComponentId}-${dependency.targetRepositoryId}-${dependency.targetComponentId ?? dependency.targetEntrypointId ?? "target"}-${dependency.kind}`}>
                      <div className="architecture-relation-v2__node"><span>{t("architecture.source")}</span><strong>{sourceRepository?.name ?? dependency.sourceRepositoryId}</strong><small>{sourceUnit?.name ?? dependency.sourceProjectUnitId}</small></div>
                      <div className="architecture-relation-v2__edge"><span className="architecture-relation-v2__line" /><b>{dependency.kind} · {dependency.confidence}%</b><span className="architecture-relation-v2__arrow">›</span></div>
                      <div className="architecture-relation-v2__node architecture-relation-v2__node--target"><span>{t("architecture.target")}</span><strong>{targetRepository?.name ?? dependency.targetRepositoryId}</strong><small>{targetUnit?.name ?? dependency.targetProjectUnitId}</small></div>
                    </div>
                  );
                })}
              </div>
            </section>
          ) : <div className="empty-state architecture-relations-empty"><div className="view-empty-icon"><LinkIcon size={28} /></div><strong>{t("architecture.crossProjectRelations", { count: 0 })}</strong></div>
        )}
      </div>
    </section>
  );
}

function topComponentKinds(components: Array<{ kind: string }>): Array<[string, number]> {
  const counts = new Map<string, number>();
  for (const component of components) { const kind = component.kind || "unknown"; if (kind.toLowerCase() === "file") continue; counts.set(kind, (counts.get(kind) ?? 0) + 1); }
  return Array.from(counts.entries()).sort((left, right) => right[1] - left[1]).slice(0, 4);
}
