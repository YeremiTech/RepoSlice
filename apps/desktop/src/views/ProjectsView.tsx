import { useMemo, useState } from "react";
import type { RepositoryRecord, WorkspaceModel } from "../types";
import { compactPath } from "../lib/format";
import { translateRole, useI18n } from "../i18n";
import SectionHeader from "../components/SectionHeader";
import Modal from "../components/Modal";
import { ChevronIcon, FolderIcon, PlusIcon, ProjectsIcon } from "../components/Icons";

type Props = {
  repositories: RepositoryRecord[];
  workspace: WorkspaceModel | null;
  currentId: string;
  onSelect: (id: string) => void;
  onAdd: () => void;
  onClone: () => void;
  onUpdate: (id: string) => void;
  onRemove: (id: string) => void;
  onCreateWorkspace: (name: string) => void;
  onResetWorkspace: () => void;
  onResetAll: () => void;
  busy: boolean;
};

const PAGE_SIZE = 4;

type ProjectTone = "cyan" | "violet" | "emerald" | "blue" | "amber" | "slate";

function roleTone(role?: string): ProjectTone {
  const normalized = role?.trim().toLowerCase() ?? "";

  if (normalized.includes("backend")) return "violet";
  if (normalized.includes("frontend")) return "emerald";
  if (normalized.includes("fullstack")) return "blue";
  if (normalized.includes("library")) return "amber";
  if (normalized.includes("infra") || normalized.includes("platform")) return "amber";

  return "cyan";
}

export default function ProjectsView({
  repositories,
  workspace,
  currentId,
  onSelect,
  onAdd,
  onClone,
  onUpdate,
  onRemove,
  onCreateWorkspace,
  onResetWorkspace,
  onResetAll,
  busy
}: Props) {
  const { language, t, formatDate } = useI18n();
  const confirmationWord = language === "es" ? "CONFIRMAR" : "CONFIRM";
  const [page, setPage] = useState(1);
  const [newWorkspaceOpen, setNewWorkspaceOpen] = useState(false);
  const [newWorkspaceName, setNewWorkspaceName] = useState("");
  const [resetOpen, setResetOpen] = useState(false);
  const [removeOpen, setRemoveOpen] = useState(false);
  const [confirmText, setConfirmText] = useState("");
  const [resetLevel, setResetLevel] = useState<"workspace" | "all">("workspace");
  const totalPages = Math.max(1, Math.ceil(repositories.length / PAGE_SIZE));
  const safePage = Math.min(page, totalPages);
  const currentRepository = repositories.find((repository) => repository.id === currentId) ?? null;

  const visibleRepositories = useMemo(() => {
    const start = (safePage - 1) * PAGE_SIZE;
    return repositories.slice(start, start + PAGE_SIZE);
  }, [repositories, safePage]);

  return (
    <>
    <section className="projects-neon-screen">
      <SectionHeader
        title={t("projects.title")}
        icon={<ProjectsIcon size={26} />}
        className="view-section-header"
        right={<div className="projects-neon-actions">
          <button className="button secondary" onClick={onClone} disabled={busy} type="button">{t("projects.clone")}</button>
          <button className="button primary" onClick={onAdd} disabled={busy} type="button"><PlusIcon size={18} /><span>{t("projects.addFolder")}</span></button>
          <button className="button secondary" onClick={() => setNewWorkspaceOpen(true)} disabled={busy} type="button"><PlusIcon size={16} />{t("projects.newWorkspace")}</button>
          <button className="button secondary button-icon-only" onClick={() => setResetOpen(true)} disabled={busy} type="button" title={t("projects.restore")} aria-label={t("projects.restore")}>↻</button>
        </div>}
      />

      {currentRepository && <div className="project-selection-bar">
        <div className="project-selection-context">
          <span className="project-selection-eyebrow">{t("common.repository")}</span>
          <strong className="project-selection-name">{currentRepository.name}</strong>
          <code className="project-selection-path" title={currentRepository.path}>{compactPath(currentRepository.path, 72)}</code>
        </div>
        <div className="project-selection-actions">
          <button className="button secondary button-sm" onClick={() => onUpdate(currentRepository.id)} disabled={busy || currentRepository.origin !== "clone"} title={currentRepository.origin === "clone" ? t("projects.updateHint") : t("projects.updateManagedOnly")} type="button">{t("projects.updateRepository")}</button>
          <button className="button danger button-sm" onClick={() => setRemoveOpen(true)} disabled={busy} type="button">{t("projects.removeRepository")}</button>
        </div>
      </div>}

      {repositories.length === 0 ? (
        <div className="projects-neon-empty">
          <div className="projects-neon-empty__icon" aria-hidden="true">
            <FolderIcon size={30} />
          </div>
          <strong>{t("projects.empty")}</strong>
          <div className="projects-neon-empty__actions">
            <button className="button primary" onClick={onAdd} disabled={busy} type="button"><PlusIcon size={18} /><span>{t("projects.addFolder")}</span></button>
            <button className="button secondary" onClick={onClone} disabled={busy} type="button">{t("projects.clone")}</button>
          </div>
        </div>
      ) : (
        <>
          <div className="projects-neon-grid">
            {visibleRepositories.map((repository) => {
              const scanned = workspace?.repositories.find((item) => item.id === repository.id);
              const units = scanned?.projectUnits ?? [];
              const analysis = scanned?.audit.analysis ?? repository.analysis;
              const tone = roleTone(units[0]?.role);
              const isActive = currentId === repository.id;
              const branch = scanned?.git?.branch || analysis?.branch || "detached";
              const commit = scanned?.git?.commit?.slice(0, 7) || analysis?.commit?.slice(0, 7) || "—";

              return (
                <button
                  className={`project-neon-card project-neon-card--${tone}${analysis ? ` project-neon-card--state-${analysis.status.toLowerCase()}` : ""}${isActive ? " project-neon-card--active" : ""}`}
                  key={repository.id}
                  onClick={() => onSelect(repository.id)}
                  type="button"
                >
                  <div className="project-neon-card__top">
                    <div className="project-neon-card__icon" aria-hidden="true">
                      <FolderIcon size={32} />
                    </div>

                    <span className={`project-neon-card__source${isActive ? " is-active" : ""}`}>
                      {repository.origin === "clone" ? t("common.gitClone").toUpperCase() : t("common.folder").toUpperCase()}
                    </span>
                  </div>

                  <div className="project-neon-card__identity">
                    <strong>{repository.name}</strong>
                    <span title={repository.path}>{compactPath(repository.path, 76)}</span>
                  </div>

                  <div className="project-neon-card__divider" />

                  <div className="project-neon-card__facts">
                    {analysis ? (
                      <>
                        <div className="project-neon-fact">
                          <span>{t("common.branch")}</span>
                          <code title={branch}>{branch}</code>
                        </div>
                        <div className="project-neon-fact">
                          <span>{t("common.commit")}</span>
                          <code>{commit}</code>
                        </div>
                      </>
                    ) : (
                      <div className="project-neon-fact">
                        <span>{t("common.origin")}</span>
                        <code>{repository.origin === "clone" ? t("common.gitClone") : t("common.local")}</code>
                      </div>
                    )}
                  </div>

                  <div className="project-neon-card__units">
                    {units.length > 0 ? (
                      units.map((unit) => (
                        <span
                          className={`project-neon-unit project-neon-unit--${roleTone(unit.role)}`}
                          key={unit.id}
                          title={`${unit.name} · ${translateRole(unit.role, t)}`}
                        >
                          <span>{unit.name}</span>
                          <i>·</i>
                          <span>{translateRole(unit.role, t)}</span>
                        </span>
                      ))
                    ) : analysis ? (
                      <span className={`project-neon-unit project-neon-unit--${analysisStatusTone(analysis.status)}`}>
                        <span>{analysisStatusLabel(analysis.status, t)}</span>
                        <i>·</i>
                        <span>{t("projects.unitShort", { units: analysis.projectUnits, components: analysis.components })}</span>
                      </span>
                    ) : (
                      <span className="project-neon-unit project-neon-unit--slate">
                        <span>{t("status.notAnalyzed")}</span>
                      </span>
                    )}
                  </div>

                  {analysis && (
                    <div className="project-neon-card__analysis">
                      <span className={`project-analysis-state state-${analysis.status.toLowerCase()}`}>{analysisStatusLabel(analysis.status, t)}</span>
                      <span className="project-analysis-date"><small>{t("common.lastAnalysis")}</small>{formatDate(analysis.completedAtMs)}</span>
                      <span className={`project-analysis-graph ${analysis.graphPassed ? "is-pass" : "is-fail"}`}>{t("common.graph")} {analysis.graphPassed ? "PASS" : "FAIL"}</span>
                    </div>
                  )}
                </button>
              );
            })}
          </div>

          {totalPages > 1 && (
            <footer className="projects-neon-footer">
              <span>
                {t("projects.showing", { visible: visibleRepositories.length, total: repositories.length })}
              </span>

              <div className="projects-neon-pagination" aria-label={t("projects.pagination")}>
                <button
                  aria-label={t("projects.previous")}
                  className="projects-neon-page projects-neon-page--previous"
                  disabled={safePage <= 1}
                  onClick={() => setPage((current) => Math.max(1, current - 1))}
                  type="button"
                >
                  <ChevronIcon size={15} />
                </button>

                <span className="projects-neon-page projects-neon-page--current">{safePage}</span>

                <button
                  aria-label={t("projects.next")}
                  className="projects-neon-page projects-neon-page--next"
                  disabled={safePage >= totalPages}
                  onClick={() => setPage((current) => Math.min(totalPages, current + 1))}
                  type="button"
                >
                  <ChevronIcon size={15} />
                </button>
              </div>
            </footer>
          )}
        </>
      )}
    </section>

    {newWorkspaceOpen && (
      <Modal
        title={t("projects.newWorkspace")}
        className="project-modal project-modal--workspace"
        onClose={() => setNewWorkspaceOpen(false)}
        footer={<>
          <button className="button secondary" onClick={() => { setNewWorkspaceOpen(false); setNewWorkspaceName(""); }}>
            {t("common.cancel")}
          </button>
          <button
            className="button primary"
            disabled={!newWorkspaceName.trim()}
            onClick={() => {
              onCreateWorkspace(newWorkspaceName.trim());
              setNewWorkspaceOpen(false);
              setNewWorkspaceName("");
            }}
          >
            {t("common.create")}
          </button>
        </>}
      >
          <label>
            {t("projects.workspaceName")}
            <input
              autoFocus
              value={newWorkspaceName}
              onChange={(e) => setNewWorkspaceName(e.target.value)}
              placeholder={t("projects.workspacePlaceholder")}
              onKeyDown={(e) => {
                if (e.key === "Enter" && newWorkspaceName.trim()) {
                  onCreateWorkspace(newWorkspaceName.trim());
                  setNewWorkspaceOpen(false);
                  setNewWorkspaceName("");
                }
              }}
            />
          </label>
      </Modal>
    )}

    {removeOpen && currentRepository && (
      <Modal
        title={t("projects.removeRepository")}
        description={currentRepository.origin === "clone" ? t("projects.removeCloneDescription") : t("projects.removeLocalDescription")}
        className="project-modal project-modal--remove"
        onClose={() => setRemoveOpen(false)}
        footer={<>
          <button className="button secondary" onClick={() => setRemoveOpen(false)}>{t("common.cancel")}</button>
          <button
            className="button danger"
            disabled={busy}
            onClick={() => {
              onRemove(currentRepository.id);
              setRemoveOpen(false);
            }}
          >
            {t("projects.removeRepository")}
          </button>
        </>}
      >
        <div className="projects-remove-summary">
          <strong>{currentRepository.name}</strong>
          <span>{currentRepository.path}</span>
        </div>
      </Modal>
    )}

    {resetOpen && (
      <Modal
        title={t("projects.restore")}
        description={t("projects.restoreDescription")}
        className="project-modal project-modal--reset reset-modal"
        onClose={() => setResetOpen(false)}
        footer={<>
          <button className="button secondary" onClick={() => { setResetOpen(false); setConfirmText(""); }}>
            {t("common.cancel")}
          </button>
          <button
            className="button danger"
            onClick={() => {
              if (resetLevel === "all" && confirmText !== confirmationWord) return;
              if (resetLevel === "workspace") onResetWorkspace();
              else onResetAll();
              setResetOpen(false);
              setConfirmText("");
            }}
            disabled={resetLevel === "all" && confirmText !== confirmationWord}
          >
            {resetLevel === "workspace" ? t("projects.deleteWorkspace") : t("projects.restoreAll")}
          </button>
        </>}
      >
          <div className="reset-options">
            <label className={`reset-option ${resetLevel === "workspace" ? "selected" : ""}`}>
              <input
                type="radio"
                name="resetLevel"
                value="workspace"
                checked={resetLevel === "workspace"}
                onChange={() => { setResetLevel("workspace"); setConfirmText(""); }}
              />
              <div>
                <strong>{t("projects.deleteCurrent")}</strong>
                <span>{t("projects.deleteCurrentDescription")}</span>
              </div>
            </label>

            <label className={`reset-option ${resetLevel === "all" ? "selected" : ""}`}>
              <input
                type="radio"
                name="resetLevel"
                value="all"
                checked={resetLevel === "all"}
                onChange={() => setResetLevel("all")}
              />
              <div>
                <strong>{t("projects.restoreAll")}</strong>
                <span>{t("projects.restoreAllDescription")}</span>
              </div>
            </label>
          </div>

          {resetLevel === "all" && (
            <label className="reset-confirmation">
              {language === "es" ? <>Escribe <strong>{confirmationWord}</strong> para continuar:</> : <>Type <strong>{confirmationWord}</strong> to continue:</>}
              <input
                type="text"
                value={confirmText}
                onChange={(e) => setConfirmText(e.target.value)}
                placeholder={confirmationWord}
                autoFocus
              />
            </label>
          )}
      </Modal>
    )}
    </>
  );
}

function analysisStatusLabel(status: string, t: (key: string) => string): string {
  if (status === "COMPLETED") return t("status.completed");
  if (status === "COMPLETED_WITH_WARNINGS") return t("status.completedWarnings");
  if (status === "PARTIAL") return t("status.partial");
  if (status === "FAILED") return t("status.failed");
  return status;
}

function analysisStatusTone(status: string): ProjectTone {
  if (status === "COMPLETED") return "emerald";
  if (status === "COMPLETED_WITH_WARNINGS") return "amber";
  if (status === "PARTIAL" || status === "FAILED") return "slate";
  return "cyan";
}
