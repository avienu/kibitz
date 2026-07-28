// @vitest-environment jsdom
import { cleanup, render } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { FeedbackRows } from "./EndgameView";
import type { VerdictRow } from "./lib/endgame";

/** Fixture rows in the accumulated StepReport wire shape. */
const ROWS: VerdictRow[] = [
  { index: 1, san: "Rc4", verdict: "winning", note: "" },
  // Reply rows carry their TRUE source (audit #9): tablebase here…
  { index: 2, san: "Ra1", verdict: "tablebase", note: "" },
  {
    index: 3,
    san: "Kc7",
    verdict: "slower",
    dtzCost: 8,
    note: "Still winning, but the tablebase path is 8 plies longer.",
  },
  { index: 4, san: "Kb8??", verdict: "throws", note: "The position is now a draw." },
  { index: 5, san: "Rd4", verdict: "unverified", note: "No tablebase coverage for this position." },
  // …and heuristic when no tables covered the reply. Never "ENGINE".
  { index: 6, san: "Ke2", verdict: "heuristic", note: "" },
];

afterEach(cleanup);

describe("FeedbackRows (design/handoff-2 §Endgames feedback aside)", () => {
  it("renders no | SAN | verdict | note per accumulated row", () => {
    const { container } = render(<FeedbackRows rows={ROWS} />);
    const rows = [...container.querySelectorAll(".eg-row")];
    expect(rows).toHaveLength(6);
    expect(rows[0].querySelector(".eg-no")?.textContent).toBe("1.");
    expect(rows[0].querySelector(".eg-san")?.textContent).toBe("Rc4");
    expect(rows[2].querySelector(".eg-note")?.textContent).toContain("8 plies longer");
  });

  it("styles each verdict with its semantic class and honest label (audit #9)", () => {
    const { container } = render(<FeedbackRows rows={ROWS} />);
    const verdicts = [...container.querySelectorAll(".eg-verdict")];
    expect(verdicts.map((v) => v.className)).toEqual([
      "eg-verdict v-winning",
      "eg-verdict v-tablebase",
      "eg-verdict v-slower",
      "eg-verdict v-throws",
      "eg-verdict v-unverified",
      "eg-verdict v-heuristic",
    ]);
    expect(verdicts.map((v) => v.textContent)).toEqual([
      "WINNING",
      "TABLEBASE",
      "SLOWER",
      "THROWS",
      "UNVERIFIED",
      "HEURISTIC",
    ]);
    // The word ENGINE must never appear: nothing here is the engine.
    expect(container.textContent).not.toContain("ENGINE");
  });

  it("states the DTZ cost for slower even when the backend note is empty", () => {
    const { container } = render(
      <FeedbackRows rows={[{ index: 1, san: "Kc7", verdict: "slower", dtzCost: 4, note: "" }]} />,
    );
    expect(container.querySelector(".eg-note")?.textContent).toBe("DTZ +4 plies.");
  });
});
