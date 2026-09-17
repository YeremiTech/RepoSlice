import type { EngineState, View } from "../types";
import { useI18n } from "../i18n";
import {
  ArchitectureIcon,
  AuditIcon,
  CapsuleIcon,
  ComponentsIcon,
  DependenciesIcon,
  EntrypointsIcon,
  OverviewIcon,
  ProjectsIcon
} from "./Icons";

type SidebarProps = {
  view: View;
  engineState: EngineState;
  onChange: (view: View) => void;
};

export default function Sidebar({ view, engineState, onChange }: SidebarProps) {
  const { language, setLanguage, t } = useI18n();
  const stateLabel =
    engineState === "ready" ? t("engine.ready") : engineState === "unavailable" ? t("engine.unavailable") : t("engine.checking");

  const navigation = [
    {
      label: t("nav.workspace"),
      items: [
        { id: "overview" as View, label: t("nav.overview"), icon: OverviewIcon },
        { id: "projects" as View, label: t("nav.projects"), icon: ProjectsIcon },
        { id: "architecture" as View, label: t("nav.architecture"), icon: ArchitectureIcon },
        { id: "audit" as View, label: t("nav.audit"), icon: AuditIcon }
      ]
    },
    {
      label: t("nav.explore"),
      items: [
        { id: "components" as View, label: t("nav.components"), icon: ComponentsIcon },
        { id: "entrypoints" as View, label: t("nav.entrypoints"), icon: EntrypointsIcon },
        { id: "dependencies" as View, label: t("nav.dependencies"), icon: DependenciesIcon }
      ]
    },
    {
      label: t("nav.capsules"),
      items: [{ id: "capsules" as View, label: t("nav.myCapsules"), icon: CapsuleIcon }]
    }
  ];

  return (
    <aside className="sidebar">
      <div className="brand">
        <div className="brand-logo">
          <img src="/logo.png" alt="RepoSlice" />
        </div>
      </div>

      <nav className="sidebar-nav">
        {navigation.map((group) => (
          <div className="nav-group" key={group.label}>
            <span className="nav-label">{group.label}</span>
            {group.items.map((item) => {
              const Icon = item.icon;
              const active = view === item.id;

              return (
                <button
                  type="button"
                  className={`nav-button ${active ? "active" : ""}`}
                  key={item.id}
                  onClick={() => onChange(item.id)}
                  aria-current={active ? "page" : undefined}
                  title={item.label}
                >
                  <span className="nav-icon" aria-hidden="true"><Icon size={20} /></span>
                  <span className="nav-button__label">{item.label}</span>
                </button>
              );
            })}
          </div>
        ))}
      </nav>

      <div className="sidebar-footer">
        <div className="language-switcher" aria-label={t("language.selector")}>
          <span>{t("language.selector")}</span>
          <div role="group" aria-label={t("language.selector")}>
            <button type="button" className={language === "es" ? "active" : ""} onClick={() => setLanguage("es")} aria-pressed={language === "es"}>ES</button>
            <button type="button" className={language === "en" ? "active" : ""} onClick={() => setLanguage("en")} aria-pressed={language === "en"}>EN</button>
          </div>
        </div>

        <div className={`sidebar-status engine-${engineState}`} role="status" aria-live="polite" title={`${t("engine.local")}: ${stateLabel}`}>
          <span className="status-dot" aria-hidden="true" />
          <div>
            <strong>{t("engine.local")}</strong>
            <span>{stateLabel}</span>
          </div>
        </div>

        <div className="sidebar-version">RepoSlice v0.4.0</div>
      </div>
    </aside>
  );
}
