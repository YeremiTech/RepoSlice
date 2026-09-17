import { ChevronIcon } from "./Icons";

type Props = {
  page: number;
  totalPages: number;
  totalItems: number;
  pageSize: number;
  previousLabel: string;
  nextLabel: string;
  onPageChange: (page: number) => void;
};

export default function Pagination({ page, totalPages, totalItems, pageSize, previousLabel, nextLabel, onPageChange }: Props) {
  if (totalPages <= 1) return null;
  const from = (page - 1) * pageSize + 1;
  const to = Math.min(page * pageSize, totalItems);
  return (
    <div className="data-pagination" aria-label={`${page}/${totalPages}`}>
      <span>{from.toLocaleString()}–{to.toLocaleString()} / {totalItems.toLocaleString()}</span>
      <div>
        <button type="button" aria-label={previousLabel} title={previousLabel} disabled={page <= 1} onClick={() => onPageChange(Math.max(1, page - 1))}><ChevronIcon size={14} /></button>
        <b>{page}</b><span>/</span><span>{totalPages}</span>
        <button type="button" className="is-next" aria-label={nextLabel} title={nextLabel} disabled={page >= totalPages} onClick={() => onPageChange(Math.min(totalPages, page + 1))}><ChevronIcon size={14} /></button>
      </div>
    </div>
  );
}
