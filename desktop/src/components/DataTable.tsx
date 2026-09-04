import { ReactNode } from "react";

export interface Column<T> {
  key: string;
  label: string;
  render?: (item: T, index: number) => ReactNode;
  className?: string;
}

interface DataTableProps<T> {
  columns: Column<T>[];
  data: T[];
  filter?: string;
  filterFn?: (item: T, query: string) => boolean;
  keyFn: (item: T, index: number) => string;
  emptyMessage?: string;
  highlightFn?: (item: T) => boolean;
}

export function DataTable<T>({
  columns,
  data,
  filter = "",
  filterFn,
  keyFn,
  emptyMessage = "No data found.",
  highlightFn,
}: DataTableProps<T>) {
  const filtered = filter && filterFn
    ? data.filter((item) => filterFn(item, filter.toLowerCase()))
    : data;

  if (filtered.length === 0) {
    return (
      <div className="empty" style={{ padding: 40 }}>
        <p>{emptyMessage}</p>
      </div>
    );
  }

  return (
    <table className="direct-table">
      <thead>
        <tr>
          <th>#</th>
          {columns.map((col) => (
            <th key={col.key}>{col.label}</th>
          ))}
        </tr>
      </thead>
      <tbody>
        {filtered.map((item, i) => (
          <tr
            key={keyFn(item, i)}
            style={highlightFn?.(item) ? { background: "rgba(255, 165, 0, 0.1)" } : undefined}
          >
            <td className="mono small">{i + 1}</td>
            {columns.map((col) => (
              <td key={col.key} className={col.className}>
                {col.render ? col.render(item, i) : String((item as Record<string, unknown>)[col.key] ?? "—")}
              </td>
            ))}
          </tr>
        ))}
      </tbody>
    </table>
  );
}
