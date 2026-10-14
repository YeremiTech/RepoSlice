import { useEffect, useMemo, useState } from "react";
import type { Component, ProjectModel, SymbolMetadata } from "../types";
import { componentTone, relativeProjectPath } from "../lib/format";
import { translateComponentKind, useI18n } from "../i18n";
import ExplorerCommandBar from "../components/ExplorerCommandBar";
import Pagination from "../components/Pagination";
import { CapsuleIcon, ComponentsIcon, FilesIcon } from "../components/Icons";

type ComponentsViewProps = {
  model: ProjectModel | null;
  selectedTarget: string;
  onSelectTarget: (target: string) => void;
  onCreateCapsule: () => void;
  canCreateCapsule: boolean;
  busy: boolean;
};

type Scope = "semantic" | "files";

export default function ComponentsView({
  model,
  selectedTarget,
  onSelectTarget,
  onCreateCapsule,
  canCreateCapsule,
  busy
}: ComponentsViewProps) {
  const { language, t } = useI18n();
  const [query, setQuery] = useState("");
  const [kind, setKind] = useState("all");
  const [scope, setScope] = useState<Scope>("semantic");
  const [detailId, setDetailId] = useState("");
  const [page, setPage] = useState(1);

  const scopedComponents = useMemo(() => {
    if (!model) return [];
    return model.components.filter((component) =>
      scope === "files" ? component.kind.toLowerCase() === "file" : component.kind.toLowerCase() !== "file"
    );
  }, [model, scope]);

  const kinds = useMemo(
    () => Array.from(new Set(scopedComponents.map((component) => component.kind))).sort(),
    [scopedComponents]
  );

  const filtered = useMemo(() => {
    const normalized = query.trim().toLowerCase();
    return scopedComponents.filter((component) => {
      const matchesKind = kind === "all" || component.kind === kind;
      const matchesQuery =
        !normalized ||
        component.name.toLowerCase().includes(normalized) ||
        component.file.toLowerCase().includes(normalized) ||
        component.language.toLowerCase().includes(normalized);
      return matchesKind && matchesQuery;
    });
  }, [scopedComponents, query, kind]);

  const pageSize = 80;
  const totalPages = Math.max(1, Math.ceil(filtered.length / pageSize));
  const safePage = Math.min(page, totalPages);
  const pageItems = filtered.slice((safePage - 1) * pageSize, safePage * pageSize);
  const semanticCount = model?.components.filter((component) => component.kind.toLowerCase() !== "file").length ?? 0;
  const fileCount = model?.components.length ? model.components.length - semanticCount : 0;
  const activeComponent = model?.components.find((component) => component.id === detailId) ?? null;
  const metadata = model && activeComponent ? findSymbolMetadata(model, activeComponent) : null;

  useEffect(() => {
    if (!model || !selectedTarget.startsWith("component:")) return;
    const id = selectedTarget.slice("component:".length);
    const component = model.components.find((item) => item.id === id);
    if (!component) return;
    const nextScope: Scope = component.kind.toLowerCase() === "file" ? "files" : "semantic";
    const ordered = model.components.filter((item) => nextScope === "files" ? item.kind.toLowerCase() === "file" : item.kind.toLowerCase() !== "file");
    const index = ordered.findIndex((item) => item.id === id);
    setScope(nextScope);
    setKind("all");
    setQuery("");
    setDetailId(id);
    setPage(index >= 0 ? Math.floor(index / 80) + 1 : 1);
  }, [model, selectedTarget]);

  if (!model) {
    return (
      <div className="empty-state large components-empty">
        <div className="view-empty-icon components-empty__icon"><ComponentsIcon size={30} /></div>
        <strong>{t("components.empty")}</strong>
      </div>
    );
  }

  function changeScope(next: Scope) {
    setScope(next);
    setKind("all");
    setQuery("");
    setDetailId("");
    setPage(1);
  }

  function selectComponent(component: Component) {
    setDetailId(component.id);
    onSelectTarget(`component:${component.id}`);
  }

  return (
    <section className="components-neon-panel">
      <ExplorerCommandBar
        title={t("components.title")}
        icon={<ComponentsIcon size={22} />}
        query={query}
        onQueryChange={(event) => { setQuery(event.target.value); setPage(1); }}
        placeholder={scope === "semantic" ? t("components.searchSemantic") : t("components.searchFiles")}
        searchAriaLabel={t("components.searchAria")}
        scopeValue={scope}
        scopeOptions={[
          { id: "semantic", label: t("components.title"), count: semanticCount, icon: <ComponentsIcon size={14} /> },
          { id: "files", label: t("components.files"), count: fileCount, icon: <FilesIcon size={14} /> }
        ]}
        onScopeChange={(value) => changeScope(value as Scope)}
        controls={<select className="select-control" value={kind} onChange={(event) => { setKind(event.target.value); setPage(1); }} aria-label={t("components.filterType")}>
          <option value="all">{t("components.allTypes")}</option>
          {kinds.map((value) => <option value={value} key={value}>{value}</option>)}
        </select>}
        actions={<>
          <span className="count-badge">{filtered.length} {t("common.visible")}</span>
          {canCreateCapsule && <button className="button primary button-sm" onClick={onCreateCapsule} disabled={busy}><CapsuleIcon size={15} />{t("components.createCapsule")}</button>}
        </>}
      />

      <div className={`explorer-layout ${activeComponent ? "has-detail" : ""}`}>
        <div className="components-neon-table" role="table" aria-label={scope === "semantic" ? t("components.detected") : t("components.filesDetected")}>
          <div className="components-neon-row components-neon-row--head" role="row">
            <span role="columnheader">{t("common.type").toUpperCase()}</span>
            <span role="columnheader">{(scope === "semantic" ? t("common.component") : t("common.file")).toUpperCase()}</span>
            <span role="columnheader">{t("common.language").toUpperCase()}</span>
            <span role="columnheader">{t("common.relativePath").toUpperCase()}</span>
          </div>

          {filtered.length === 0 && (
            <div className="explorer-inline-empty">{t("components.noMatches")}</div>
          )}

          {pageItems.map((component) => {
            const target = `component:${component.id}`;
            const active = selectedTarget === target;
            const tone = componentTone(component.kind);
            const relative = relativeProjectPath(component.file, model.root);
            return (
              <button
                className={`components-neon-row components-neon-row--item tone-${tone} ${active ? "is-selected" : ""}`}
                key={component.id}
                onClick={() => selectComponent(component)}
                type="button"
                role="row"
                aria-pressed={active}
              >
                <span role="cell"><b className={`components-neon-type tone-${tone}`}>{translateComponentKind(component.kind, language)}</b></span>
                <strong className="components-neon-name" role="cell" title={component.name}>
                  {component.name}
                  {component.repositoryName && <small className="components-neon-scope">{component.repositoryName} / {component.projectUnitName}</small>}
                </strong>
                <span className={`components-neon-language ${component.language.toLowerCase() === "unknown" ? "is-unknown" : ""}`} role="cell">{component.language}</span>
                <code className="components-neon-file" role="cell" title={component.file}>{relative}</code>
              </button>
            );
          })}
        </div>

        {activeComponent && (
          <ComponentDetail component={activeComponent} metadata={metadata} root={model.root} onClose={() => setDetailId("")} />
        )}
      </div>
      <Pagination page={safePage} totalPages={totalPages} totalItems={filtered.length} pageSize={pageSize} previousLabel={t("projects.previous")} nextLabel={t("projects.next")} onPageChange={setPage} />
    </section>
  );
}

function findSymbolMetadata(model: ProjectModel, component: Component): SymbolMetadata | null {
  return model.analysis.symbols.find((item) => item.symbolId === component.id) ?? null;
}

function ComponentDetail({ component, metadata, root, onClose }: { component: Component; metadata: SymbolMetadata | null; root: string; onClose: () => void }) {
  const { language, t } = useI18n();
  const evidence = metadata?.evidence ?? [];
  return (
    <aside className="explorer-detail-panel" aria-label={t("components.detail")}>
      <div className="explorer-detail-head">
        <div>
          <span>{t("components.detail").toUpperCase()}</span>
          <strong>{component.name}</strong>
        </div>
        <button type="button" className="detail-close" aria-label={t("common.close")} onClick={onClose}>×</button>
      </div>
      <div className="detail-badges">
        <span className={`components-neon-type tone-${componentTone(component.kind)}`}>{translateComponentKind(component.kind, language)}</span>
        <span>{component.language || t("common.unknown")}</span>
        {metadata && <span>{metadata.confidence}% {t("common.confidence").toLowerCase()}</span>}
      </div>
      <dl className="detail-kv-list">
        <div><dt>{t("common.path")}</dt><dd title={component.file}>{relativeProjectPath(component.file, root)}</dd></div>
        <div><dt>{t("common.qualifiedName")}</dt><dd>{metadata?.qualifiedName ?? component.id}</dd></div>
        <div><dt>Namespace</dt><dd>{metadata?.namespace ?? "—"}</dd></div>
        <div><dt>{t("common.frameworkKind")}</dt><dd>{metadata?.frameworkKind ?? "—"}</dd></div>
      </dl>
      <div className="detail-section-title">{t("common.evidence").toUpperCase()}</div>
      {evidence.length > 0 ? (
        <div className="detail-evidence-list">
          {evidence.slice(0, 12).map((item, index) => (
            <div className="detail-evidence" key={`${item.source}-${index}`}>
              <div><strong>{item.kind}</strong><span>{item.confidence}%</span></div>
              <p>{item.detail}</p>
              <code title={item.source}>{relativeProjectPath(item.source, root)}</code>
            </div>
          ))}
        </div>
      ) : <div className="detail-empty">{t("components.noEvidence")}</div>}
    </aside>
  );
}
