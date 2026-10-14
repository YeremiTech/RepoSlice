import { useEffect, useMemo, useState } from "react";
import type { CapsuleSummary, VerificationReport } from "../types";
import { compactPath } from "../lib/format";
import { useI18n } from "../i18n";
import SectionHeader from "../components/SectionHeader";
import { CapsuleIcon, ScanIcon } from "../components/Icons";

type CapsulesViewProps = {
  capsules: CapsuleSummary[];
  verificationByPath: Record<string, VerificationReport | undefined>;
  verifyingPath: string;
  onVerify: (capsule: CapsuleSummary) => void;
};

export default function CapsulesView({ capsules, verificationByPath, verifyingPath, onVerify }: CapsulesViewProps) {
  const { t } = useI18n();
  const [selectedPath, setSelectedPath] = useState(capsules[0]?.path ?? "");

  useEffect(() => {
    if (!capsules.some((item) => item.path === selectedPath)) {
      setSelectedPath(capsules[0]?.path ?? "");
    }
  }, [capsules, selectedPath]);

  const selected = useMemo(
    () => capsules.find((item) => item.path === selectedPath) ?? capsules[0] ?? null,
    [capsules, selectedPath]
  );

  if (capsules.length === 0) {
    return (
      <div className="empty-state large">
        <div className="view-empty-icon capsules-empty__icon"><CapsuleIcon size={30} /></div>
        <strong>{t("capsules.empty")}</strong>
      </div>
    );
  }

  return (
    <section className="panel capsules-panel capsules-master-detail">
      <SectionHeader
        title={t("capsules.title")}
        icon={<CapsuleIcon size={26} />}
        className="view-section-header"
        right={<span className="count-badge">{capsules.length} {t("common.found")}</span>}
      />

      <div className="capsules-workbench">
        <div className="capsule-list-pane" role="listbox" aria-label={t("capsules.listAria")}>
          {capsules.map((capsule) => {
            const report = verificationByPath[capsule.path];
            const state = report ? (report.passed ? "VERIFIED" : "INVALID") : "CREATED";
            const tone = state === "VERIFIED" ? "verified" : state.toLowerCase();
            const selectedItem = selected?.path === capsule.path;
            return (
              <button
                key={capsule.path}
                type="button"
                role="option"
                aria-selected={selectedItem}
                className={`capsule-list-item capsule-${tone} ${selectedItem ? "is-selected" : ""}`}
                onClick={() => setSelectedPath(capsule.path)}
              >
                <span className="capsule-list-icon"><CapsuleIcon size={19} /></span>
                <span className="capsule-list-copy">
                  <strong>{targetLabel(capsule.target)}</strong>
                  <small title={capsule.path}>{compactPath(capsule.path, 48)}</small>
                </span>
                <b className={`capsule-mini-status status-${tone}`}>{capsuleStateLabel(state, t)}</b>
              </button>
            );
          })}
        </div>

        {selected && (
          <CapsuleDetail
            capsule={selected}
            report={verificationByPath[selected.path]}
            verifying={verifyingPath === selected.path}
            onVerify={() => onVerify(selected)}
          />
        )}
      </div>
    </section>
  );
}

function CapsuleDetail({ capsule, report, verifying, onVerify }: { capsule: CapsuleSummary; report?: VerificationReport; verifying: boolean; onVerify: () => void }) {
  const { language, t } = useI18n();
  const state = report ? (report.passed ? "VERIFIED" : "INVALID") : "CREATED";
  const tone = state === "VERIFIED" ? "verified" : state.toLowerCase();
  const passedChecks = report?.checks.filter((item) => item.passed).length ?? 0;

  return (
    <article className={`capsule-detail-pane capsule-${tone}`}>
      <div className="capsule-detail-hero">
        <div className="capsule-symbol"><CapsuleIcon size={26} /></div>
        <div>
          <span>{t("capsules.detail").toUpperCase()}</span>
          <strong>{targetLabel(capsule.target)}</strong>
        </div>
        <span className={`capsule-status status-${tone}`}>{capsuleStateLabel(state, t)}</span>
      </div>

      <div className="capsule-detail-target">
        <span>{t("capsules.targetId")}</span>
        <code>{capsule.target}</code>
      </div>
      <div className="capsule-detail-target">
        <span>{t("capsules.location")}</span>
        <code title={capsule.path}>{capsule.path}</code>
      </div>

      <div className="capsule-verification-head">
        <div>
          <strong>{t("capsules.verification")}</strong>
          <span>{report ? t("capsules.checksPassed", { passed: passedChecks, total: report.checks.length }) : t("capsules.notVerified")}</span>
          {report && <span>{t("capsules.readiness", { score: report.readinessScore })}</span>}
        </div>
        {report && <b className={report.passed ? "check-pass" : "check-fail"}>{report.passed ? "PASS" : "FAIL"}</b>}
      </div>

      {report ? (
        <div className="verification-checks capsule-detail-checks">
          {report.checks.map((check) => (
            <div className="verification-row" key={check.name}>
              <div><span>{verificationCheckLabel(check.name, language)}</span><small title={check.detail}>{check.detail}</small></div>
              <b className={check.passed ? "check-pass" : "check-fail"}>{check.passed ? "PASS" : "FAIL"}</b>
            </div>
          ))}
        </div>
      ) : (
        <div className="capsule-unverified-note">{t("capsules.verifyNote")}</div>
      )}

      {report && report.sandboxValidationPlan.length > 0 && (
        <div className="capsule-sandbox-plan">
          <strong>{t("capsules.sandboxPlan")}</strong>
          <small>{t("capsules.sandboxPlanNote")}</small>
          {report.sandboxValidationPlan.map((plan) => (
            <div className="capsule-sandbox-row" key={`${plan.runtime}:${plan.manifest}`}>
              <div><b>{plan.runtime}</b><span>{plan.manifest}</span></div>
              <code>{plan.suggestedCommand}</code>
              {plan.networkRequired && <em>{t("capsules.networkRequired")}</em>}
            </div>
          ))}
        </div>
      )}

      <button className="button verify-button capsule-detail-verify" disabled={verifying} onClick={onVerify} type="button">
        <ScanIcon size={16} />{verifying ? t("common.verifying") : report ? t("capsules.verifyAgain") : t("capsules.verifyCapsule")}
      </button>
    </article>
  );
}



function verificationCheckLabel(name: string, language: "es" | "en"): string {
  if (language === "en") return name;
  const labels: Record<string, string> = {
    manifest: "Manifest",
    source: "Código fuente",
    "file-inventory": "Inventario de archivos",
    "content-integrity": "Integridad del contenido",
    "payload-integrity": "Integridad del payload",
    "symlink-safety": "Seguridad de symlinks",
    "portable-source-root": "Raíz portable",
    target: "Objetivo",
    environment: "Entorno"
  };
  return labels[name.toLowerCase()] ?? name;
}

function capsuleStateLabel(state: string, t: (key: string) => string): string {
  if (state === "VERIFIED") return t("status.verified").toUpperCase();
  if (state === "INVALID") return t("status.invalid").toUpperCase();
  return t("status.created").toUpperCase();
}

function targetLabel(target: string): string {
  const parts = target.split(":");
  const tail = parts[parts.length - 1] || target;
  return tail.length > 56 ? `${tail.slice(0, 53)}...` : tail;
}
