// @vitest-environment jsdom
import { cleanup, fireEvent, render } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import DataTable, { type DataTableColumn } from "./DataTable";

interface Row {
  id: number;
  name: string;
  elo: number;
}

const ROWS: Row[] = [
  { id: 1, name: "Morphy", elo: 2690 },
  { id: 2, name: "Anderssen", elo: 2600 },
  { id: 3, name: "Steinitz", elo: 2650 },
];

const COLUMNS: DataTableColumn<Row>[] = [
  { key: "name", header: "NAME", render: (r) => r.name },
  {
    key: "elo",
    header: "ELO",
    align: "right",
    render: (r) => <span data-testid={`elo-${r.id}`}>{r.elo}</span>,
    sort: (a, b) => a.elo - b.elo,
  },
];

const GRID = "1.6fr 84px";

afterEach(cleanup);

function rowNames(container: HTMLElement): string[] {
  return [...container.querySelectorAll(".dtable-row")].map(
    (el) => el.querySelector(".dtable-cell")?.textContent ?? "",
  );
}

describe("DataTable", () => {
  it("renders header and rows on the shared grid template", () => {
    const { container } = render(
      <DataTable columns={COLUMNS} rows={ROWS} gridTemplate={GRID} rowKey={(r) => r.id} />,
    );
    const head = container.querySelector<HTMLElement>(".dtable-head")!;
    expect(head.style.gridTemplateColumns).toBe(GRID);
    const rows = container.querySelectorAll<HTMLElement>(".dtable-row");
    expect(rows).toHaveLength(3);
    for (const row of rows) {
      expect(row.style.gridTemplateColumns).toBe(GRID);
    }
    expect(rowNames(container)).toEqual(["Morphy", "Anderssen", "Steinitz"]);
  });

  it("column templates are render props (custom cell markup)", () => {
    const { getByTestId } = render(
      <DataTable columns={COLUMNS} rows={ROWS} gridTemplate={GRID} rowKey={(r) => r.id} />,
    );
    expect(getByTestId("elo-2").textContent).toBe("2600");
  });

  it("sorts on an opted-in column: asc → desc → source order", () => {
    const { container, getByText } = render(
      <DataTable columns={COLUMNS} rows={ROWS} gridTemplate={GRID} rowKey={(r) => r.id} />,
    );
    fireEvent.click(getByText("ELO"));
    expect(rowNames(container)).toEqual(["Anderssen", "Steinitz", "Morphy"]);
    fireEvent.click(getByText("ELO"));
    expect(rowNames(container)).toEqual(["Morphy", "Steinitz", "Anderssen"]);
    fireEvent.click(getByText("ELO"));
    expect(rowNames(container)).toEqual(["Morphy", "Anderssen", "Steinitz"]);
  });

  it("ignores clicks on columns without a comparator", () => {
    const { container, getByText } = render(
      <DataTable columns={COLUMNS} rows={ROWS} gridTemplate={GRID} rowKey={(r) => r.id} />,
    );
    fireEvent.click(getByText("NAME"));
    expect(rowNames(container)).toEqual(["Morphy", "Anderssen", "Steinitz"]);
  });

  it("row click fires with the row; clickable rows carry the hover class", () => {
    const onRowClick = vi.fn();
    const { container } = render(
      <DataTable
        columns={COLUMNS}
        rows={ROWS}
        gridTemplate={GRID}
        rowKey={(r) => r.id}
        onRowClick={onRowClick}
      />,
    );
    const rows = container.querySelectorAll<HTMLElement>(".dtable-row");
    // The hover affordance (background var(--panel2)) hangs off .clickable.
    expect(rows[1].classList.contains("clickable")).toBe(true);
    fireEvent.click(rows[1]);
    expect(onRowClick).toHaveBeenCalledWith(ROWS[1]);
  });

  it("non-clickable tables omit the hover affordance", () => {
    const { container } = render(
      <DataTable columns={COLUMNS} rows={ROWS} gridTemplate={GRID} rowKey={(r) => r.id} />,
    );
    const row = container.querySelector<HTMLElement>(".dtable-row")!;
    expect(row.classList.contains("clickable")).toBe(false);
  });

  it("renders empty state and footer slots", () => {
    const { container, getByText } = render(
      <DataTable
        columns={COLUMNS}
        rows={[]}
        gridTemplate={GRID}
        rowKey={(r: Row) => r.id}
        empty="No games match."
        footer={<div>footer note</div>}
      />,
    );
    expect(getByText("No games match.")).toBeTruthy();
    expect(getByText("footer note")).toBeTruthy();
    expect(container.querySelectorAll(".dtable-row")).toHaveLength(0);
  });
});
