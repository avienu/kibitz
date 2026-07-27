/**
 * Repertoire marks in the moves panel (run-9): merge the backend's
 * per-ply match/deviation marks (`repertoire_marks`) into the glyph map
 * the panel renders — a tick on every trained move that was played, and
 * ONE deviation mark per color (the first: after it you're out of book,
 * and later re-entries via transposition keep their ticks anyway).
 *
 * Pure logic — no DOM, no Tauri — unit-testable in isolation.
 */

/** One mark from the `repertoire_marks` command (repertoire.rs). */
export interface RepertoireMark {
  /** 1-based mainline ply of the move the mark attaches to. */
  ply: number;
  /** Repertoire color whose card covers the position before `ply`. */
  color: "white" | "black";
  /** The move that repertoire trains from this position. */
  expectedSan: string;
  played: "matched" | "deviated";
}

/** What the moves panel renders on one ply. */
export interface RepGlyph {
  kind: "match" | "deviation";
  color: "white" | "black";
  /** For deviations: the move your repertoire plays here. */
  expectedSan: string;
  /** Ready-made tooltip text. */
  title: string;
}

function colorName(color: "white" | "black"): string {
  return color === "white" ? "White" : "Black";
}

/**
 * Row-model merge: ply → glyph. Every `matched` mark gets a tick;
 * `deviated` marks render only for the first deviation of each color.
 * The backend emits at most one mark per ply (one side moves per ply).
 */
export function repGlyphsByPly(marks: RepertoireMark[]): Map<number, RepGlyph> {
  const map = new Map<number, RepGlyph>();
  const deviated = new Set<"white" | "black">();
  for (const m of [...marks].sort((a, b) => a.ply - b.ply)) {
    if (m.played === "matched") {
      if (!map.has(m.ply)) {
        map.set(m.ply, {
          kind: "match",
          color: m.color,
          expectedSan: m.expectedSan,
          title: `in your ${colorName(m.color)} repertoire`,
        });
      }
    } else if (!deviated.has(m.color)) {
      deviated.add(m.color);
      map.set(m.ply, {
        kind: "deviation",
        color: m.color,
        expectedSan: m.expectedSan,
        title: `your ${colorName(m.color)} repertoire plays ${m.expectedSan} here`,
      });
    }
  }
  return map;
}
