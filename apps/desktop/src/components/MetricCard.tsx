import type { ReactNode } from "react";

type MetricCardProps = {
  label: string;
  value: number;
  tone: "violet" | "cyan" | "green" | "amber";
  icon?: ReactNode;
};

export default function MetricCard({ label, value, tone, icon }: MetricCardProps) {
  return (
    <article className={`metric-card tone-${tone}`}>
      <div className="metric-glow" aria-hidden="true" />
      <div className="metric-card-content">
        {icon && <div className="metric-icon-shell">{icon}</div>}
        <div className="metric-copy">
          <span className="metric-label">{label}</span>
          <strong>{value.toLocaleString()}</strong>
        </div>
      </div>
    </article>
  );
}
