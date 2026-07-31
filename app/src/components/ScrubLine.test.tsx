// @vitest-environment jsdom
import { cleanup, fireEvent, render } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import ScrubLine, { scrubTokens, type ScrubPreview } from "./ScrubLine";
import { gameFromSans } from "../lib/game";

const START_FEN = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

/** Ground truth the component must report: the replay's own fens. */
function fensOf(sans: string[], startFen?: string): string[] {
  const r = gameFromSans(sans, startFen ?? null);
  if (!r.ok) throw new Error("fixture line must replay");
  return r.game.fens;
}

afterEach(cleanup);

describe("scrubTokens", () => {
  it("numbers from the standard start like numberedLine", () => {
    expect(scrubTokens(["e4", "c5", "Nf3"], START_FEN).map((t) => t.text)).toEqual([
      "1. e4",
      "c5",
      "2. Nf3",
    ]);
  });

  it("numbers a mid-game black-to-move start with the leading ellipsis", () => {
    const fen = "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1";
    const toks = scrubTokens(["c5", "Nf3", "d6"], fen);
    expect(toks.map((t) => t.text)).toEqual(["1... c5", "2. Nf3", "d6"]);
    // Labels are always numbered — the board caption never shows a bare SAN.
    expect(toks.map((t) => t.label)).toEqual(["1... c5", "2. Nf3", "2... d6"]);
  });
});

describe("ScrubLine", () => {
  const SANS = ["e4", "c5", "Nf3"];

  function renderLine(over: Partial<Parameters<typeof ScrubLine>[0]> = {}) {
    const onPreview = vi.fn<(p: ScrubPreview | null) => void>();
    const utils = render(<ScrubLine sans={SANS} onPreview={onPreview} {...over} />);
    return { onPreview, ...utils };
  }

  it("renders the numbered line as per-move tokens", () => {
    const { container } = renderLine();
    const toks = [...container.querySelectorAll(".scrub-tok")].map((t) => t.textContent);
    expect(toks).toEqual(["1. e4", "c5", "2. Nf3"]);
    expect(container.textContent).toBe("1. e4 c5 2. Nf3");
  });

  it("hovering the first token previews the position after move 1", () => {
    const { container, onPreview } = renderLine();
    fireEvent.mouseOver(container.querySelectorAll(".scrub-tok")[0]);
    expect(onPreview).toHaveBeenLastCalledWith({
      fen: fensOf(SANS)[1],
      lastMove: ["e2", "e4"],
      ply: 1,
      label: "1. e4",
    });
  });

  it("hovering a middle token previews the position after that move", () => {
    const { container, onPreview } = renderLine();
    fireEvent.mouseOver(container.querySelectorAll(".scrub-tok")[1]);
    expect(onPreview).toHaveBeenLastCalledWith({
      fen: fensOf(SANS)[2],
      lastMove: ["c7", "c5"],
      ply: 2,
      label: "1... c5",
    });
  });

  it("hovering the last token previews the line's final position", () => {
    const { container, onPreview } = renderLine();
    fireEvent.mouseOver(container.querySelectorAll(".scrub-tok")[2]);
    expect(onPreview).toHaveBeenLastCalledWith({
      fen: fensOf(SANS)[3],
      lastMove: ["g1", "f3"],
      ply: 3,
      label: "2. Nf3",
    });
  });

  it("mouse leaving the line reports null", () => {
    const { container, onPreview } = renderLine();
    fireEvent.mouseOver(container.querySelectorAll(".scrub-tok")[1]);
    fireEvent.mouseOut(container.querySelector(".scrub-line")!);
    expect(onPreview).toHaveBeenLastCalledWith(null);
  });

  it("replays from a mid-game startFen (engine-extension lines)", () => {
    const fen = "rnbqkbnr/pp1ppppp/8/2p5/4P3/8/PPPP1PPP/RNBQKBNR w KQkq - 0 2";
    const { container, onPreview } = renderLine({ sans: ["Nf3", "d6"], startFen: fen });
    const toks = [...container.querySelectorAll(".scrub-tok")].map((t) => t.textContent);
    expect(toks).toEqual(["2. Nf3", "d6"]);
    fireEvent.mouseOver(container.querySelectorAll(".scrub-tok")[1]);
    expect(onPreview).toHaveBeenLastCalledWith({
      fen: fensOf(["Nf3", "d6"], fen)[2],
      lastMove: ["d7", "d6"],
      ply: 2,
      label: "2... d6",
    });
  });

  it("an unreplayable line renders as plain text and never previews", () => {
    const { container, onPreview } = renderLine({ sans: ["e4", "Qxf7"] });
    expect(container.querySelectorAll(".scrub-tok")).toHaveLength(0);
    expect(container.textContent).toBe("1. e4 Qxf7");
    fireEvent.mouseOver(container.firstChild as Element);
    fireEvent.mouseOut(container.firstChild as Element);
    expect(onPreview).not.toHaveBeenCalled();
  });

  it("arrow keys step the preview; Escape clears; blur clears", () => {
    const { container, onPreview } = renderLine();
    const line = container.querySelector(".scrub-line")!;
    fireEvent.keyDown(line, { key: "ArrowRight" });
    expect(onPreview).toHaveBeenLastCalledWith(
      expect.objectContaining({ ply: 1, label: "1. e4" }),
    );
    fireEvent.keyDown(line, { key: "ArrowRight" });
    expect(onPreview).toHaveBeenLastCalledWith(expect.objectContaining({ ply: 2 }));
    fireEvent.keyDown(line, { key: "ArrowLeft" });
    expect(onPreview).toHaveBeenLastCalledWith(expect.objectContaining({ ply: 1 }));
    fireEvent.keyDown(line, { key: "End" });
    expect(onPreview).toHaveBeenLastCalledWith(expect.objectContaining({ ply: 3 }));
    fireEvent.keyDown(line, { key: "Escape" });
    expect(onPreview).toHaveBeenLastCalledWith(null);
    fireEvent.keyDown(line, { key: "ArrowRight" });
    fireEvent.blur(line);
    expect(onPreview).toHaveBeenLastCalledWith(null);
  });

  it("unmounting while it owns the live preview clears it", () => {
    const { container, onPreview, unmount } = renderLine();
    fireEvent.mouseOver(container.querySelectorAll(".scrub-tok")[0]);
    onPreview.mockClear();
    unmount();
    expect(onPreview).toHaveBeenCalledWith(null);
  });

  it("unmounting when it does NOT own the preview stays silent", () => {
    const { onPreview, unmount } = renderLine();
    unmount();
    expect(onPreview).not.toHaveBeenCalled();
  });
});
