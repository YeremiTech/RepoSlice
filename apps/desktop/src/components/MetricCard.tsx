import type { ReactNode } from "react";

type Props = { label: string; value: number; tone: "violet" | "cyan" | "green" | "amber"; icon: ReactNode };
export default function MetricCard({ label, value, tone, icon }: Props) {
  return <article className={`metric-card tone-${tone}`}><div className="metric-glow"/><div className="metric-icon">{icon}</div><div><span>{label}</span><strong>{value.toLocaleString()}</strong></div></article>;
}
