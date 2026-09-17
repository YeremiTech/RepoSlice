import { useState } from "react";
import type { AnalysisRecord, Repository, RepositoryRecord, WorkspaceModel } from "../types";
import SectionHeader from "../components/SectionHeader";
import ViewTabs from "../components/ViewTabs";
import { translateAnalysisMessage, translateDiagnosticMessage, useI18n } from "../i18n";
import { AuditIcon, ScanIcon } from "../components/Icons";

type Props = {
  repositories: RepositoryRecord[];
  workspace: WorkspaceModel | null;
  repositoryId: string;
  busy: boolean;
  onScan: () => void;
};

export default function AuditView({ repositories, workspace, repositoryId, busy, onScan }: Props) {
  const { t } = useI18n();
  const [section, setSection] = useState<"summary" | "integrity" | "frameworks" | "diagnostics">("summary");
  if (!repositoryId) {
    return <WorkspaceAudit repositories={repositories} workspace={workspace} busy={busy} onScan={onScan} />;
  }

  const record = repositories.find((item) => item.id === repositoryId);
  const loaded = workspace?.repositories.find((item) => item.id === repositoryId) ?? null;
  const analysis = loaded?.audit.analysis ?? record?.analysis ?? null;

  return (
    <div className="view-stack audit-view">
      <SectionHeader
        title={t("audit.title")}
        icon={<AuditIcon size={26} />}
        className="view-section-header"
        right={
          <div className="audit-header-actions">
            {analysis && <StatusBadge status={analysis.status} />}
            <button className="button secondary" type="button" disabled={busy} onClick={onScan}>
              <ScanIcon size={16} /> {busy ? t("common.scanning") : t("common.rescan")}
            </button>
          </div>
        }
      />

      {!analysis ? (
        <section className="audit-empty panel">
          <AuditIcon size={36} />
          <strong>{t("audit.noAudit")}</strong>
          <span>{t("audit.runToRegister")}</span>
          <button className="button primary" type="button" disabled={busy} onClick={onScan}>
            <ScanIcon size={17} /> {t("audit.analyzeRepository")}
          </button>
        </section>
      ) : (
        <>
          <AuditSummary analysis={analysis} />
          <ViewTabs
            value={section}
            onChange={setSection}
            ariaLabel={t("audit.title")}
            options={[
              { id: "summary", label: t("common.analysis") },
              { id: "integrity", label: t("common.integrity") },
              { id: "frameworks", label: t("common.frameworks"), count: analysis.frameworkDetections },
              { id: "diagnostics", label: t("common.diagnostics"), count: analysis.diagnosticsError + analysis.diagnosticsWarning + analysis.diagnosticsInfo }
            ]}
          />
          <div className="view-tab-content audit-tab-content">
            {section === "summary" && <div className="audit-summary-layout"><AnalysisIdentity record={record} loaded={loaded} analysis={analysis} /><AuditCoverage analysis={analysis} /></div>}
            {section === "integrity" && <GraphAudit loaded={loaded} analysis={analysis} />}
            {section === "frameworks" && <FrameworkAudit loaded={loaded} analysis={analysis} />}
            {section === "diagnostics" && <DiagnosticsAudit loaded={loaded} analysis={analysis} />}
          </div>
        </>
      )}
    </div>
  );
}

function WorkspaceAudit({ repositories, workspace, busy, onScan }: Omit<Props, "repositoryId">) {
  const { t, formatDate } = useI18n();
  const [section, setSection] = useState<"repositories" | "integrity">("repositories");
  const analyzed = repositories.filter((item) => item.analysis);
  const completed = analyzed.filter((item) => item.analysis?.status === "COMPLETED").length;
  const graph = workspace?.graphIntegrity;

  return (
    <div className="view-stack audit-view">
      <SectionHeader
        title={t("audit.workspaceTitle")}
        icon={<AuditIcon size={26} />}
        className="view-section-header"
        right={
          <button className="button secondary" type="button" disabled={busy || repositories.length === 0} onClick={onScan}>
            <ScanIcon size={16} /> {busy ? t("common.scanning") : t("audit.analyzeAll")}
          </button>
        }
      />

      <section className="audit-summary-grid audit-summary-grid--workspace">
        <AuditMetric label={t("common.repositories")} value={repositories.length.toString()} tone="cyan" />
        <AuditMetric label={t("audit.completed")} value={completed.toString()} tone="green" />
        <AuditMetric label={t("audit.crossGraph")} value={graph ? (graph.passed ? "PASS" : "FAIL") : "N/D"} tone={graph?.passed ? "green" : graph ? "amber" : "violet"} />
        <AuditMetric label={t("common.relationships")} value={(workspace?.crossProjectDependencies.length ?? 0).toString()} tone="violet" />
      </section>

      <ViewTabs
        value={section}
        onChange={setSection}
        ariaLabel={t("audit.workspaceTitle")}
        options={[
          { id: "repositories", label: t("common.repositories"), count: repositories.length },
          { id: "integrity", label: t("common.integrity"), count: graph?.errorCount ?? 0 }
        ]}
      />

      <div className="view-tab-content audit-tab-content">
        {section === "repositories" && <section className="panel audit-repositories-panel">
          <SectionHeader title={t("audit.repositoryStatus")} right={<span className="count-badge">{repositories.length}</span>} />
          <div className="audit-repository-table-scroll">
            <div className="audit-repository-table" role="table" aria-label={t("audit.repositoryStatus")}>
              <div className="audit-repository-row audit-repository-head" role="row">
                <span role="columnheader">{t("common.repository")}</span><span role="columnheader">{t("common.origin")}</span><span role="columnheader">{t("common.status")}</span><span role="columnheader">{t("common.commit")}</span><span role="columnheader">{t("audit.lastAnalysis")}</span><span role="columnheader">{t("audit.graph")}</span><span role="columnheader">{t("common.diagnostics")}</span>
              </div>
              {repositories.map((repository) => {
                const analysis = repository.analysis;
                return <div className="audit-repository-row" role="row" key={repository.id}>
                  <strong role="cell" title={repository.name}>{repository.name}</strong>
                  <span role="cell" className="audit-origin">{repository.origin === "clone" ? t("common.gitClone") : t("common.local")}</span>
                  <span role="cell">{analysis ? <StatusBadge status={analysis.status} compact /> : <span className="audit-muted">{t("status.notAnalyzed")}</span>}</span>
                  <code role="cell" title={analysis?.commit ?? undefined}>{analysis?.commit?.slice(0, 8) ?? "—"}</code>
                  <span role="cell" className="audit-analysis-date">{analysis ? formatDate(analysis.completedAtMs) : "—"}</span>
                  <span role="cell" className={`audit-graph-status ${analysis?.graphPassed ? "audit-pass" : analysis ? "audit-fail" : "audit-muted"}`}>{analysis ? (analysis.graphPassed ? "PASS" : "FAIL") : "—"}</span>
                  <span role="cell" className="audit-diagnostics-count">{analysis ? `${analysis.diagnosticsError}E · ${analysis.diagnosticsWarning}W · ${analysis.diagnosticsInfo}I` : "—"}</span>
                </div>;
              })}
            </div>
          </div>
        </section>}

        {section === "integrity" && (graph ? <section className="panel audit-graph-panel">
          <SectionHeader title={t("audit.crossIntegrity")} right={<span className={`audit-check ${graph.passed ? "is-pass" : "is-fail"}`}>{graph.passed ? "PASS" : "FAIL"}</span>} />
          <div className="audit-integrity-grid">
            <IntegrityItem label={t("audit.sourceUnitsMissing")} value={graph.missingSourceUnits.length} />
            <IntegrityItem label={t("audit.targetUnitsMissing")} value={graph.missingTargetUnits.length} />
            <IntegrityItem label={t("audit.sourceComponentsMissing")} value={graph.missingSourceComponents.length} />
            <IntegrityItem label={t("audit.targetComponentsMissing")} value={graph.missingTargetComponents.length} />
            <IntegrityItem label={t("audit.targetEntrypointsMissing")} value={graph.missingTargetEntrypoints.length} />
          </div>
        </section> : <div className="empty-state"><strong>{t("audit.analyzeAllToValidate")}</strong></div>)}
      </div>

    </div>
  );
}

function AuditSummary({ analysis }: { analysis: AnalysisRecord }) {
  const { t } = useI18n();
  const diagnostics = analysis.diagnosticsError + analysis.diagnosticsWarning + analysis.diagnosticsInfo;
  return (
    <section className="audit-summary-grid">
      <AuditMetric label={t("common.status")} value={statusLabel(analysis.status, t)} detail={translateAnalysisMessage(analysis.message, t)} tone={statusTone(analysis.status)} />
      <AuditMetric label={t("common.integrity")} value={analysis.graphPassed ? "PASS" : "FAIL"} detail={analysis.graphClean ? "Grafo limpio" : `${analysis.cycles} ciclos · ${analysis.selfDependencies} autorrelaciones`} tone={analysis.graphPassed ? (analysis.graphClean ? "green" : "amber") : "amber"} />
      <AuditMetric label={t("common.diagnostics")} value={diagnostics.toString()} detail={`${analysis.diagnosticsError} errores · ${analysis.diagnosticsWarning} avisos`} tone={analysis.diagnosticsError ? "amber" : analysis.diagnosticsWarning ? "violet" : "green"} />
      <AuditMetric label={t("common.duration")} value={formatDuration(analysis.durationMs)} detail={`Engine ${analysis.engineVersion}`} tone="cyan" />
    </section>
  );
}

function AnalysisIdentity({ record, loaded, analysis }: { record?: RepositoryRecord; loaded: Repository | null; analysis: AnalysisRecord }) {
  const { t, formatDate } = useI18n();
  const filesPerSecond = analysis.durationMs > 0
    ? Math.round(analysis.files / (analysis.durationMs / 1000))
    : 0;
  return (
    <section className="panel audit-identity-panel">
      <SectionHeader title={t("audit.identity")} />
      <div className="audit-kv-grid">
        <KeyValue label={t("audit.analysisId")} value={analysis.analysisId} mono />
        <KeyValue label={t("common.origin")} value={(record?.origin ?? loaded?.origin) === "clone" ? t("common.gitClone") : t("common.folder")} />
        <KeyValue label={t("common.branch")} value={analysis.branch ?? loaded?.git?.branch ?? "—"} mono />
        <KeyValue label={t("common.commit")} value={analysis.commit ?? loaded?.git?.commit ?? "—"} mono />
        <KeyValue label={t("audit.lastAnalysis")} value={formatDate(analysis.completedAtMs)} />
        <KeyValue label={t("audit.fingerprint")} value={analysis.modelFingerprint || "—"} mono />
        <KeyValue label={t("audit.throughput")} value={filesPerSecond > 0 ? t("audit.filesPerSecond", { count: filesPerSecond }) : "—"} />
        <KeyValue label="Engine" value={`RepoSlice ${analysis.engineVersion}`} />
        <KeyValue label="Registry" value={analysis.registrySignature || "—"} mono />
        <KeyValue label={t("common.path")} value={record?.path ?? loaded?.root ?? "—"} mono />
      </div>
    </section>
  );
}

function AuditCoverage({ analysis }: { analysis: AnalysisRecord }) {
  const { t } = useI18n();
  return (
    <section className="panel audit-coverage-panel">
      <SectionHeader title={t("audit.coverage")} />
      <div className="audit-integrity-grid audit-coverage-grid">
        <IntegrityItem label={t("common.files")} value={analysis.files} />
        <IntegrityItem label={t("common.projectUnits")} value={analysis.projectUnits} />
        <IntegrityItem label={t("common.components")} value={analysis.components} />
        <IntegrityItem label="Entrypoints" value={analysis.entrypoints} />
        <IntegrityItem label={t("common.dependencies")} value={analysis.dependencies} />
        <IntegrityItem label="Cross-project" value={analysis.crossProjectDependencies} />
        <IntegrityItem label={t("common.frameworks")} value={analysis.frameworkDetections} />
        <IntegrityItem label={t("common.actionable")} value={analysis.actionableFrameworks} />
      </div>
    </section>
  );
}

function GraphAudit({ loaded, analysis }: { loaded: Repository | null; analysis: AnalysisRecord }) {
  const { t } = useI18n();
  return (
    <section className="panel audit-graph-panel">
      <SectionHeader title={`${t("common.integrity")} · ${t("common.graph")}`} right={<span className={`audit-check ${analysis.graphPassed ? "is-pass" : "is-fail"}`}>{analysis.graphPassed ? "PASS" : "FAIL"}</span>} />
      <div className="audit-integrity-grid">
        <IntegrityItem label={t("audit.missingSources")} value={analysis.missingSources} />
        <IntegrityItem label={t("audit.missingTargets")} value={analysis.missingTargets} />
        <IntegrityItem label={t("audit.selfDependencies")} value={analysis.selfDependencies} />
        <IntegrityItem label={t("common.cycles")} value={analysis.cycles} />
      </div>
      {loaded ? (
        <div className="audit-unit-list">
          {loaded.audit.units.map((unit) => (
            <details className="audit-detail" key={unit.projectUnitId}>
              <summary>
                <span><strong>{unit.projectUnitName}</strong><small>{unit.projectUnitId}</small></span>
                <span className={`audit-check ${unit.passed ? (unit.clean ? "is-pass" : "is-warning") : "is-fail"}`}>{unit.passed ? (unit.clean ? t("status.clean").toUpperCase() : t("status.warning").toUpperCase()) : "FAIL"}</span>
              </summary>
              <div className="audit-detail-body">
                {unit.missingSources.length > 0 && <AuditStringList title={t("audit.missingSources")} values={unit.missingSources} />}
                {unit.missingTargets.length > 0 && <AuditStringList title={t("audit.missingTargets")} values={unit.missingTargets} />}
                {unit.cycles.length > 0 && <AuditStringList title={t("common.cycles")} values={unit.cycles.map((cycle) => cycle.join(" → "))} />}
                {unit.missingSources.length === 0 && unit.missingTargets.length === 0 && unit.cycles.length === 0 && unit.selfDependencies === 0 && <span className="audit-pass">{t("audit.noIssues")}</span>}
              </div>
            </details>
          ))}
        </div>
      ) : (
        <AuditLoadHint />
      )}
    </section>
  );
}

function FrameworkAudit({ loaded, analysis }: { loaded: Repository | null; analysis: AnalysisRecord }) {
  const { t } = useI18n();
  const frameworks = loaded?.projectUnits.flatMap((unit) => unit.model.analysis.frameworkDetections.map((framework) => ({ ...framework, unitName: unit.name }))) ?? [];
  return (
    <section className="panel audit-framework-panel">
      <SectionHeader title={t("audit.frameworkDetection")} right={<span className="count-badge">{analysis.actionableFrameworks}/{analysis.frameworkDetections} {t("common.actionable").toLowerCase()}</span>} />
      {frameworks.length > 0 ? (
        <div className="audit-framework-list">
          {frameworks.map((framework, index) => (
            <details className="audit-detail" key={`${framework.unitName}-${framework.name}-${index}`}>
              <summary>
                <span><strong>{framework.name}</strong><small>{framework.unitName} · {framework.confidence}% {t("common.confidence").toLowerCase()}</small></span>
                <span className={`audit-check ${framework.actionable ? "is-pass" : "is-warning"}`}>{framework.actionable ? t("common.actionable").toUpperCase() : t("status.review").toUpperCase()}</span>
              </summary>
              <div className="audit-detail-body">
                <div className="audit-framework-flags">
                  <span>{t("audit.directEvidence")}: <strong>{framework.directEvidence ? t("audit.yes") : t("audit.no")}</strong></span>
                  <span>{t("audit.evidences")}: <strong>{framework.evidence.length}</strong></span>
                </div>
                {framework.evidence.map((evidence, evidenceIndex) => (
                  <div className="audit-evidence" key={`${evidence.source}-${evidenceIndex}`}>
                    <span>{evidence.kind}</span>
                    <strong>{evidence.source}</strong>
                    <p>{evidence.detail}</p>
                    <code>{evidence.confidence}%</code>
                  </div>
                ))}
              </div>
            </details>
          ))}
        </div>
      ) : (
        <AuditLoadHint />
      )}
    </section>
  );
}

function DiagnosticsAudit({ loaded, analysis }: { loaded: Repository | null; analysis: AnalysisRecord }) {
  const { t } = useI18n();
  const diagnostics = loaded?.projectUnits.flatMap((unit) => unit.model.analysis.diagnostics.map((diagnostic) => ({ ...diagnostic, unitName: unit.name }))) ?? [];
  return (
    <section className="panel audit-diagnostics-panel">
      <SectionHeader title={t("common.diagnostics")} right={<span className="count-badge">{analysis.diagnosticsError}E · {analysis.diagnosticsWarning}W · {analysis.diagnosticsInfo}I</span>} />
      {loaded ? (
        diagnostics.length > 0 ? (
          <div className="audit-diagnostic-list">
            {diagnostics.map((diagnostic, index) => (
              <article className={`audit-diagnostic audit-diagnostic--${diagnostic.level}`} key={`${diagnostic.code}-${index}`}>
                <span className="audit-diagnostic-level">{diagnosticLevelLabel(diagnostic.level, t)}</span>
                <div><strong>{diagnostic.code}</strong><p>{translateDiagnosticMessage(diagnostic.code, diagnostic.message, t)}</p><small>{diagnostic.unitName}{diagnostic.file ? ` · ${diagnostic.file}` : ""}</small></div>
              </article>
            ))}
          </div>
        ) : <div className="audit-clean-message">{t("audit.noDiagnostics")}</div>
      ) : <AuditLoadHint />}
    </section>
  );
}

function AuditMetric({ label, value, detail, tone }: { label: string; value: string; detail?: string; tone: "cyan" | "green" | "violet" | "amber" }) {
  return <article className={`audit-metric audit-metric--${tone}`}><span>{label}</span><strong>{value}</strong>{detail && <small>{detail}</small>}</article>;
}

function IntegrityItem({ label, value }: { label: string; value: number }) {
  return <div className={`audit-integrity-item ${value === 0 ? "is-zero" : "has-value"}`}><strong>{value}</strong><span>{label}</span></div>;
}

function KeyValue({ label, value, mono = false }: { label: string; value: string; mono?: boolean }) {
  return <div className="audit-kv"><span>{label}</span><strong className={mono ? "is-mono" : ""} title={value}>{value}</strong></div>;
}

function StatusBadge({ status, compact = false }: { status: string; compact?: boolean }) {
  const { t } = useI18n();
  return <span className={`audit-status audit-status--${status.toLowerCase()} ${compact ? "is-compact" : ""}`}>{statusLabel(status, t)}</span>;
}

function AuditStringList({ title, values }: { title: string; values: string[] }) {
  return <div className="audit-string-list"><strong>{title}</strong>{values.map((value, index) => <code key={`${value}-${index}`}>{value}</code>)}</div>;
}

function AuditLoadHint() {
  const { t } = useI18n();
  return <div className="audit-load-hint">{t("audit.loadHint")}</div>;
}

function diagnosticLevelLabel(level: string, t: (key: string) => string): string {
  if (level === "error") return t("diagnostic.level.error");
  if (level === "warning") return t("diagnostic.level.warning");
  return t("diagnostic.level.info");
}

function statusLabel(status: string, t: (key: string) => string): string {
  if (status === "COMPLETED") return t("status.completed");
  if (status === "COMPLETED_WITH_WARNINGS") return t("status.completedWarnings");
  if (status === "PARTIAL") return t("status.partial");
  if (status === "FAILED") return t("status.failed");
  return status;
}

function statusTone(status: string): "cyan" | "green" | "violet" | "amber" {
  if (status === "COMPLETED") return "green";
  if (status === "COMPLETED_WITH_WARNINGS") return "amber";
  if (status === "PARTIAL" || status === "FAILED") return "amber";
  return "cyan";
}

function formatDuration(milliseconds: number): string {
  if (milliseconds < 1000) return `${milliseconds} ms`;
  if (milliseconds < 60_000) return `${(milliseconds / 1000).toFixed(milliseconds < 10_000 ? 1 : 0)} s`;
  const minutes = Math.floor(milliseconds / 60_000);
  const seconds = Math.round((milliseconds % 60_000) / 1000);
  return `${minutes}m ${seconds}s`;
}
