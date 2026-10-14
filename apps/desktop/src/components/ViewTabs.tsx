import type { ReactNode } from "react";

type TabOption<T extends string> = {
  id: T;
  label: string;
  count?: number;
  icon?: ReactNode;
  disabled?: boolean;
};

type Props<T extends string> = {
  value: T;
  onChange: (value: T) => void;
  options: TabOption<T>[];
  ariaLabel: string;
  compact?: boolean;
  className?: string;
};

export default function ViewTabs<T extends string>({ value, onChange, options, ariaLabel, compact = false, className = "" }: Props<T>) {
  return (
    <div className={`view-tabs${compact ? " view-tabs--compact" : ""}${className ? ` ${className}` : ""}`} role="tablist" aria-label={ariaLabel}>
      {options.map((option) => (
        <button
          key={option.id}
          type="button"
          role="tab"
          aria-selected={value === option.id}
          className={value === option.id ? "is-active" : ""}
          disabled={option.disabled}
          onClick={() => onChange(option.id)}
        >
          {option.icon && <span className="view-tabs__icon" aria-hidden="true">{option.icon}</span>}
          <span>{option.label}</span>
          {typeof option.count === "number" && <b>{option.count.toLocaleString()}</b>}
        </button>
      ))}
    </div>
  );
}
