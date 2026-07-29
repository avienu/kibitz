// @vitest-environment jsdom
/**
 * Round-3 change note: the Explain panel is bounded. Summary first (only
 * the leading finding renders, the rest behind the pinned-foot
 * expander), the header caret collapses the body entirely, and neither
 * state may touch the board's evidence (asserted in lib/gameView.test —
 * deriveEvidence unions ALL blocks regardless).
 */
import { cleanup, fireEvent, render } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import ExplainPanel from "./ExplainPanel";
import type { ExplanationJson } from "./lib/gameView";

const EXPL: ExplanationJson = {
  schema_version: 3,
  tag: "TACTICAL SCREEN FIRED",
  eval: null,
  headline: { coach: "The knight is doing three jobs.", neutral: "Nd7 is overloaded." },
  blocks: [
    {
      kind: "alert",
      text: { coach: "first finding", neutral: "first finding" },
      evidence: { alerts: ["d7"] },
    },
    {
      kind: "imbalance",
      text: { coach: "second finding", neutral: "second finding" },
      evidence: { imbalance: ["e5"] },
    },
    {
      kind: "plan",
      text: { coach: "third finding", neutral: "third finding" },
      evidence: { key: ["d5"] },
    },
  ],
};

function renderPanel(overrides: Partial<Parameters<typeof ExplainPanel>[0]> = {}) {
  const props = {
    explanation: EXPL,
    explaining: false,
    verification: null,
    voice: "coach" as const,
    onVoice: vi.fn(),
    hoverSentence: null,
    onHoverSentence: vi.fn(),
    selectedSquare: null,
    onExplain: vi.fn(),
    explainedPlies: [],
    findingsExpanded: false,
    onToggleFindings: vi.fn(),
    collapsed: false,
    onToggleCollapsed: vi.fn(),
    ...overrides,
  };
  return { ...render(<ExplainPanel {...props} />), props };
}

afterEach(cleanup);

describe("summary first", () => {
  it("renders only the first finding; the rest sit behind the foot expander", () => {
    const { container, props } = renderPanel();
    expect(container.textContent).toContain("first finding");
    expect(container.textContent).not.toContain("second finding");
    expect(container.textContent).not.toContain("third finding");
    const expander = container.querySelector(".explain-expander")!;
    expect(expander.textContent).toBe("▾ 2 more findings — evidence is already on the board");
    fireEvent.click(expander);
    expect(props.onToggleFindings).toHaveBeenCalled();
  });

  it("expanded shows every finding and offers the way back", () => {
    const { container } = renderPanel({ findingsExpanded: true });
    expect(container.textContent).toContain("third finding");
    expect(container.querySelector(".explain-expander")!.textContent).toBe("▴ summary only");
  });

  it("a single finding gets no expander", () => {
    const { container } = renderPanel({
      explanation: { ...EXPL, blocks: EXPL.blocks.slice(0, 1) },
    });
    expect(container.querySelector(".explain-expander")).toBeNull();
  });

  it("the expander lives in the pinned foot, not the scroll region", () => {
    const { container } = renderPanel();
    expect(container.querySelector(".explain-foot .explain-expander")).not.toBeNull();
    expect(container.querySelector(".explain-body .explain-expander")).toBeNull();
  });
});

describe("collapse caret", () => {
  it("hides body and foot, keeps header + verdict pill", () => {
    const { container } = renderPanel({ collapsed: true });
    expect(container.querySelector(".explain-body")).toBeNull();
    expect(container.querySelector(".explain-foot")).toBeNull();
    expect(container.querySelector(".verdict-pill")).not.toBeNull();
    expect(container.querySelector(".explain-panel")!.classList.contains("collapsed")).toBe(true);
    expect(container.querySelector(".explain-collapse")!.textContent).toBe("▸");
  });

  it("caret toggles through the callback", () => {
    const { container, props } = renderPanel();
    fireEvent.click(container.querySelector(".explain-collapse")!);
    expect(props.onToggleCollapsed).toHaveBeenCalled();
  });
});

describe("pinned foot meta", () => {
  it("is a single line carrying source, voice and selection state", () => {
    const { container } = renderPanel({ selectedSquare: "d7" });
    const meta = container.querySelector(".explain-meta")!;
    expect(meta.textContent).toContain("Static screen · no engine spawned");
    expect(meta.textContent).toContain("Coach voice · templates");
    expect(meta.textContent).toContain("d7");
    // One element, no nested rows — the nowrap/ellipsis contract.
    expect(container.querySelectorAll(".explain-meta").length).toBe(1);
  });
});
