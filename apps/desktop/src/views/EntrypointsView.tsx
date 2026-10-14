import { useEffect, useMemo, useState } from "react";
import type { Entrypoint, EntrypointMetadata, ProjectModel } from "../types";
import { httpMethod, methodTone, relativeProjectPath } from "../lib/format";
import { useI18n } from "../i18n";
import ExplorerCommandBar from "../components/ExplorerCommandBar";
import Pagination from "../components/Pagination";
import { CapsuleIcon, EntrypointsIcon } from "../components/Icons";

type EntrypointsViewProps = {
  model: ProjectModel | null;
  selectedTarget: string;
  onSelectTarget: (target: string) => void;
  onCreateCapsule: () => void;
  canCreateCapsule: boolean;
  busy: boolean;
};

export default function EntrypointsView({
  model,
  selectedTarget,
  onSelectTarget,
  onCreateCapsule,
  canCreateCapsule,
  busy
}: EntrypointsViewProps) {
  const { t } = useI18n();
  const [query, setQuery] = useState("");
  const [method, setMethod] = useState("ALL");
  const [detailId, setDetailId] = useState("");
  const [page, setPage] = useState(1);

  const methods = useMemo(() => {
    if (!model) return [];
    return Array.from(new Set(model.entrypoints.map((entrypoint) => httpMethod(entrypoint.name)))).sort();
  }, [model]);

  const filtered = useMemo(() => {
    if (!model) return [];
    const normalized = query.trim().toLowerCase();
    return model.entrypoints.filter((entrypoint) => {
      const currentMethod = httpMethod(entrypoint.name);
      const matchesMethod = method === "ALL" || currentMethod === method;
      const matchesQuery = !normalized || entrypoint.name.toLowerCase().includes(normalized) || entrypoint.file.toLowerCase().includes(normalized);
      return matchesMethod && matchesQuery;
    });
  }, [model, query, method]);

  const pageSize = 36;
  const totalPages = Math.max(1, Math.ceil(filtered.length / pageSize));
  const safePage = Math.min(page, totalPages);
  const pageItems = filtered.slice((safePage - 1) * pageSize, safePage * pageSize);
  const activeEntrypoint = model?.entrypoints.find((item) => item.id === detailId) ?? null;
  const metadata = activeEntrypoint ? model?.analysis.entrypoints.find((item) => item.entrypointId === activeEntrypoint.id) ?? null : null;

  useEffect(() => {
    if (!model || !selectedTarget) return;
    const index = model.entrypoints.findIndex((item) => item.id === selectedTarget);
    if (index < 0) return;
    setMethod("ALL");
    setQuery("");
    setDetailId(selectedTarget);
    setPage(Math.floor(index / 36) + 1);
  }, [model, selectedTarget]);

  if (!model) {
    return (
      <div className="empty-state large entrypoints-empty">
        <div className="view-empty-icon entrypoints-empty__icon"><EntrypointsIcon size={30} /></div>
        <strong>{t("entrypoints.empty")}</strong>
      </div>
    );
  }

  function selectEntrypoint(entrypoint: Entrypoint) {
    setDetailId(entrypoint.id);
    onSelectTarget(entrypoint.id);
  }

  return (
    <section className="entrypoints-panel">
      <ExplorerCommandBar
        title={t("entrypoints.title")}
        icon={<EntrypointsIcon size={22} />}
        query={query}
        onQueryChange={(event) => { setQuery(event.target.value); setPage(1); }}
        placeholder={t("entrypoints.search")}
        searchAriaLabel={t("entrypoints.searchAria")}
        controls={<select className="select-control" value={method} onChange={(event) => { setMethod(event.target.value); setPage(1); }} aria-label={t("entrypoints.filterMethod")}>
          <option value="ALL">{t("entrypoints.allMethods")}</option>
          {methods.map((value) => <option value={value} key={value}>{value}</option>)}
        </select>}
        actions={<>
          <span className="count-badge">{filtered.length} {t("common.visible")}</span>
          {canCreateCapsule && <button className="button primary button-sm" onClick={onCreateCapsule} disabled={busy}><CapsuleIcon size={15} />{t("components.createCapsule")}</button>}
        </>}
      />

      <div className={`explorer-layout entrypoint-explorer ${activeEntrypoint ? "has-detail" : ""}`}>
        {filtered.length === 0 ? (
          <div className="empty-state">
            <div className="empty-state-icon"><EntrypointsIcon size={28} /></div>
            <strong>{t("entrypoints.notFound")}</strong>
            <span>{t("entrypoints.notFound")}</span>
          </div>
        ) : (
          <div className="entrypoints-grid">
            {pageItems.map((entrypoint) => {
              const methodName = httpMethod(entrypoint.name);
              const route = entrypoint.name.replace(methodName, "").trim();
              const active = selectedTarget === entrypoint.id;
              const entryMetadata = model.analysis.entrypoints.find((item) => item.entrypointId === entrypoint.id);
              return (
                <button
                  className={`entrypoints-card tone-${methodTone(methodName)} ${active ? "is-selected" : ""}`}
                  key={entrypoint.id}
                  onClick={() => selectEntrypoint(entrypoint)}
                  type="button"
                  aria-pressed={active}
                >
                  <div className="entrypoints-card__top">
                    <span className={`method-badge tone-${methodTone(methodName)}`}>{methodName}</span>
                    <span className="entrypoints-kind">{entrypoint.kind}</span>
                  </div>
                  <strong className="entrypoints-route" title={route || entrypoint.name}>{route || entrypoint.name}</strong>
                  <span className="entrypoints-card-meta">
                    <span title={entryMetadata?.controller ?? undefined}>{entryMetadata?.controller ?? t("entrypoints.noController")}</span>
                    {entryMetadata && <b>{entryMetadata.confidence}%</b>}
                  </span>
                  <span className="entrypoints-file" title={entrypoint.file}>{relativeProjectPath(entrypoint.file, model.root)}</span>
                  {entrypoint.repositoryName && <span className="entrypoints-scope" title={`${entrypoint.repositoryName} / ${entrypoint.projectUnitName ?? ""}`}>{entrypoint.repositoryName} / {entrypoint.projectUnitName}</span>}
                </button>
              );
            })}
          </div>
        )}

        {activeEntrypoint && (
          <EntrypointDetail entrypoint={activeEntrypoint} metadata={metadata} root={model.root} onClose={() => setDetailId("")} />
        )}
      </div>
      <Pagination page={safePage} totalPages={totalPages} totalItems={filtered.length} pageSize={pageSize} previousLabel={t("projects.previous")} nextLabel={t("projects.next")} onPageChange={setPage} />
    </section>
  );
}

function EntrypointDetail({ entrypoint, metadata, root, onClose }: { entrypoint: Entrypoint; metadata: EntrypointMetadata | null; root: string; onClose: () => void }) {
  const { t } = useI18n();
  const method = metadata?.method ?? httpMethod(entrypoint.name);
  const route = metadata?.path ?? (entrypoint.name.replace(httpMethod(entrypoint.name), "").trim() || entrypoint.name);
  return (
    <aside className="explorer-detail-panel" aria-label={t("entrypoints.detail")}>
      <div className="explorer-detail-head">
        <div><span>{t("entrypoints.detail").toUpperCase()}</span><strong>{route}</strong></div>
        <button type="button" className="detail-close" aria-label={t("common.close")} onClick={onClose}>×</button>
      </div>
      <div className="detail-badges">
        <span className={`method-badge tone-${methodTone(method)}`}>{method}</span>
        <span>{entrypoint.kind}</span>
        {metadata && <span>{metadata.confidence}% {t("common.confidence").toLowerCase()}</span>}
      </div>
      <dl className="detail-kv-list">
        <div><dt>{t("common.controller")}</dt><dd>{metadata?.controller ?? "—"}</dd></div>
        <div><dt>{t("common.actionHandler")}</dt><dd>{metadata?.action ?? "—"}</dd></div>
        <div><dt>{t("common.routeName")}</dt><dd>{metadata?.routeName ?? "—"}</dd></div>
        <div><dt>{t("common.domain")}</dt><dd>{metadata?.domain ?? "—"}</dd></div>
        <div><dt>{t("common.component")}</dt><dd>{entrypoint.componentId}</dd></div>
        <div><dt>{t("common.file")}</dt><dd title={entrypoint.file}>{relativeProjectPath(entrypoint.file, root)}</dd></div>
      </dl>
      {metadata?.middleware?.length ? (
        <><div className="detail-section-title">{t("common.middleware").toUpperCase()}</div><div className="detail-chip-list">{metadata.middleware.map((item) => <span key={item}>{item}</span>)}</div></>
      ) : null}
      <div className="detail-section-title">{t("common.evidence").toUpperCase()}</div>
      {metadata?.evidence?.length ? (
        <div className="detail-evidence-list">
          {metadata.evidence.slice(0, 12).map((item, index) => (
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
