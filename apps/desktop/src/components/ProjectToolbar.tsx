import type { RepositoryRecord, WorkspaceModel, WorkspaceRecord } from "../types";
import { useI18n, translateRole } from "../i18n";
import { PlusIcon, ScanIcon } from "./Icons";

type TargetOption = { id: string; label: string; type: string };
type Props = {
  workspaces: WorkspaceRecord[];
  workspaceId: string;
  repositories: RepositoryRecord[];
  repositoryId: string;
  unitId: string;
  workspace: WorkspaceModel | null;
  selectedTarget: string;
  targets: TargetOption[];
  busy: boolean;
  showTarget?: boolean;
  onWorkspaceChange: (id: string) => void;
  onRepositoryChange: (id: string) => void;
  onUnitChange: (id: string) => void;
  onTargetChange: (target: string) => void;
  onScan: () => void;
  onCreateWorkspace: () => void;
};

export default function ProjectToolbar(props: Props) {
  const { t } = useI18n();
  const repository = props.workspace?.repositories.find((item) => item.id === props.repositoryId);
  const showTarget = props.showTarget === true;

  return (
    <section className={`project-toolbar scope-toolbar ${showTarget ? "scope-toolbar--target" : "scope-toolbar--compact"}`} aria-label={t("toolbar.scope")} aria-busy={props.busy}>
      <div className="toolbar-field toolbar-workspace-field">
        <span className="field-label">{t("common.workspace").toUpperCase()}</span>
        <div className="toolbar-field-inline">
          <select aria-label={t("common.workspace")} value={props.workspaceId} disabled={props.busy} onChange={(event) => props.onWorkspaceChange(event.target.value)}>
            {props.workspaces.map((workspace) => <option key={workspace.id} value={workspace.id}>{workspace.name}</option>)}
          </select>
          <button type="button" className="button-icon toolbar-square-button" onClick={props.onCreateWorkspace} disabled={props.busy} title={t("toolbar.newWorkspace")} aria-label={t("toolbar.newWorkspace")}>
            <PlusIcon size={14} />
          </button>
        </div>
      </div>

      <div className="toolbar-field">
        <span className="field-label">{t("common.repository").toUpperCase()}</span>
        <select aria-label={t("toolbar.selectRepository")} value={props.repositoryId} disabled={props.busy} onChange={(event) => props.onRepositoryChange(event.target.value)}>
          <option value="">{t("common.all")}</option>
          {props.repositories.map((item) => <option key={item.id} value={item.id}>{item.name}</option>)}
        </select>
      </div>

      <div className="toolbar-field">
        <span className="field-label">{t("common.projectUnit").toUpperCase()}</span>
        <select aria-label={t("toolbar.selectUnit")} value={props.unitId} disabled={props.busy || !props.repositoryId} onChange={(event) => props.onUnitChange(event.target.value)}>
          <option value="">{t("common.allFem")}</option>
          {repository?.projectUnits.map((unit) => <option key={unit.id} value={unit.id}>{unit.name} · {translateRole(unit.role, t)}</option>)}
        </select>
      </div>

      {showTarget && (
        <div className="toolbar-field target-select">
          <span className="field-label">{t("common.target").toUpperCase()}</span>
          <select aria-label={t("toolbar.targetAria")} value={props.selectedTarget} disabled={props.busy || !props.workspace} onChange={(event) => props.onTargetChange(event.target.value)}>
            <option value="">{t("toolbar.selectTarget")}</option>
            {props.targets.map((target) => <option key={target.id} value={target.id}>{target.type} · {target.label}</option>)}
          </select>
        </div>
      )}

      <button type="button" className="button scan-button toolbar-square-button" disabled={props.busy || props.repositories.length === 0} onClick={props.onScan} title={props.busy ? t("common.scanning") : t("common.scan")} aria-label={props.busy ? t("common.scanning") : t("common.scan")}>
        <ScanIcon size={17} />
      </button>
    </section>
  );
}
