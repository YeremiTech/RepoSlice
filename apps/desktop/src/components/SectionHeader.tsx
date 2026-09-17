import type { ReactNode } from "react";

type SectionHeaderProps = {
  title: string;
  right?: ReactNode;
  icon?: ReactNode;
  className?: string;
};

export default function SectionHeader({ title, right, icon, className = "" }: SectionHeaderProps) {
  const copy = <div className="section-header-copy"><h2>{title}</h2></div>;
  return (
    <div className={`section-header ${className}`.trim()}>
      {icon ? (
        <div className="section-header-heading">
          <span className="section-header-icon" aria-hidden="true">{icon}</span>
          {copy}
        </div>
      ) : copy}
      {right && <div className="section-header-right">{right}</div>}
    </div>
  );
}
