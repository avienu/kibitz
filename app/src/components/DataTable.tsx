/**
 * DataTable — round-2 shared component 1 of 5 (design/handoff-2 §pattern
 * budget). Header row + rows on a shared `grid-template-columns`, 9px/14px
 * cell padding, `var(--line)` row separators, hover `var(--panel2)`.
 * Used by Database, Position search, Opening tree, Prep fingerprint and
 * Master games — build once, no per-screen forks.
 *
 * Props contract (stable — other screens depend on it):
 * - `columns`: column templates as render props. `header` renders in the
 *   mono header row; `render(row)` renders each cell. `align: "right"`
 *   right-aligns both. `sort` (a comparator) opts a column into sorting —
 *   clicking its header cycles asc → desc → source order.
 * - `gridTemplate`: the shared grid-template-columns string, e.g.
 *   "26px 1.6fr 1.6fr 58px 1.2fr 92px 64px 96px 84px".
 * - `rowKey(row)`: stable React key.
 * - `onRowClick(row)`: optional; rows get the pointer/hover affordance.
 * - `rowClassName(row)`: optional extra class per row.
 * - `footer`: optional row under the table (pagination, footnotes).
 * - `empty`: rendered instead of rows when `rows` is empty.
 */
import { useMemo, useState } from "react";
import type { ReactNode } from "react";

export interface DataTableColumn<T> {
  /** Stable column id (also the sort-state key). */
  key: string;
  header: ReactNode;
  render: (row: T) => ReactNode;
  /** Comparator enabling sorting on this column (opt-in). */
  sort?: (a: T, b: T) => number;
  align?: "left" | "right";
}

export interface DataTableProps<T> {
  columns: readonly DataTableColumn<T>[];
  rows: readonly T[];
  /** Shared grid-template-columns for header and every row. */
  gridTemplate: string;
  rowKey: (row: T) => string | number;
  onRowClick?: (row: T) => void;
  rowClassName?: (row: T) => string | undefined;
  footer?: ReactNode;
  empty?: ReactNode;
}

interface SortState {
  key: string;
  dir: 1 | -1;
}

export default function DataTable<T>({
  columns,
  rows,
  gridTemplate,
  rowKey,
  onRowClick,
  rowClassName,
  footer,
  empty,
}: DataTableProps<T>) {
  const [sort, setSort] = useState<SortState | null>(null);

  const sorted = useMemo(() => {
    if (!sort) return rows;
    const col = columns.find((c) => c.key === sort.key);
    if (!col?.sort) return rows;
    const cmp = col.sort;
    return [...rows].sort((a, b) => sort.dir * cmp(a, b));
  }, [rows, columns, sort]);

  const onHeaderClick = (col: DataTableColumn<T>) => {
    if (!col.sort) return;
    setSort((s) => {
      if (s?.key !== col.key) return { key: col.key, dir: 1 };
      if (s.dir === 1) return { key: col.key, dir: -1 };
      return null; // third click: back to source order
    });
  };

  const grid = { gridTemplateColumns: gridTemplate };

  return (
    <div className="dtable">
      <div className="dtable-head" style={grid} role="row">
        {columns.map((c) => (
          <span
            key={c.key}
            role="columnheader"
            className={
              `dtable-hcell${c.align === "right" ? " right" : ""}` +
              (c.sort ? " sortable" : "") +
              (sort?.key === c.key ? " sorted" : "")
            }
            onClick={() => onHeaderClick(c)}
          >
            {c.header}
            {c.sort && sort?.key === c.key && (
              <span className="dtable-sort-glyph">{sort.dir === 1 ? "▲" : "▼"}</span>
            )}
          </span>
        ))}
      </div>
      {sorted.length === 0
        ? empty && <div className="dtable-empty">{empty}</div>
        : sorted.map((row) => (
            <div
              key={rowKey(row)}
              role="row"
              className={
                `dtable-row${onRowClick ? " clickable" : ""}` +
                (rowClassName?.(row) ? ` ${rowClassName(row)}` : "")
              }
              style={grid}
              onClick={onRowClick ? () => onRowClick(row) : undefined}
            >
              {columns.map((c) => (
                <span key={c.key} className={`dtable-cell${c.align === "right" ? " right" : ""}`}>
                  {c.render(row)}
                </span>
              ))}
            </div>
          ))}
      {footer && <div className="dtable-foot">{footer}</div>}
    </div>
  );
}
