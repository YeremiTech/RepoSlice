import type { ChangeEventHandler, ReactNode } from "react";
import { SearchIcon } from "./Icons";

type ScopeOption = {
  id: string;
  label: string;
  count?: number;
  icon?: ReactNode;
};

type Props = {
  title: string;
  icon: ReactNode;
  query: string;
  placeholder: string;
  onQueryChange: ChangeEventHandler<HTMLInputElement>;
  searchAriaLabel: string;
  scopeValue?: string;
  scopeOptions?: ScopeOption[];
  onScopeChange?: (value: string) => void;
  controls?: ReactNode;
  actions?: ReactNode;
};

export default function ExplorerCommandBar({
  title,
  icon,
  query,
  placeholder,
  onQueryChange,
  searchAriaLabel,
  scopeValue,
  scopeOptions,
  onScopeChange,
  controls,
  actions
}: Props) {
  return (
    <header className="explorer-command-bar">
      <div className="explorer-command-bar__title">
        <span className="explorer-command-bar__icon" aria-hidden="true">{icon}</span>
        <h2>{title}</h2>
      </div>

      {scopeOptions && scopeOptions.length > 0 && onScopeChange && (
        <div className="explorer-command-bar__scope" role="tablist" aria-label={title}>
          {scopeOptions.map((option) => (
            <button
              key={option.id}
              type="button"
              role="tab"
              aria-selected={scopeValue === option.id}
              className={scopeValue === option.id ? "is-active" : ""}
              onClick={() => onScopeChange(option.id)}
            >
              {option.icon && <span aria-hidden="true">{option.icon}</span>}
              <span>{option.label}</span>
              {typeof option.count === "number" && <b>{option.count.toLocaleString()}</b>}
            </button>
          ))}
        </div>
      )}

      <label className="explorer-command-bar__search">
        <SearchIcon size={17} />
        <input value={query} onChange={onQueryChange} placeholder={placeholder} aria-label={searchAriaLabel} />
      </label>

      {controls && <div className="explorer-command-bar__controls">{controls}</div>}
      {actions && <div className="explorer-command-bar__actions">{actions}</div>}
    </header>
  );
}
