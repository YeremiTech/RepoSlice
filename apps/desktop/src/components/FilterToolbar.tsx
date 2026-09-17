import type { ChangeEventHandler, ReactNode } from "react";
import { SearchIcon } from "./Icons";
import { useI18n } from "../i18n";

type FilterToolbarProps = {
  query: string;
  placeholder: string;
  onQueryChange: ChangeEventHandler<HTMLInputElement>;
  children?: ReactNode;
  ariaLabel?: string;
};

export default function FilterToolbar({
  query,
  placeholder,
  onQueryChange,
  children,
  ariaLabel = "Filtros"
}: FilterToolbarProps) {
  const { t } = useI18n();
  const label = ariaLabel === "Filtros" ? t("common.search") : ariaLabel;
  return (
    <div className={`filter-toolbar${children ? "" : " filter-toolbar--search-only"}`} role="search" aria-label={label}>
      <label className="search-control">
        <SearchIcon size={18} />
        <input value={query} onChange={onQueryChange} placeholder={placeholder} />
      </label>
      {children && <div className="filter-toolbar__controls">{children}</div>}
    </div>
  );
}
