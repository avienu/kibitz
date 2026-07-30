import { describe, expect, it } from "vitest";
import {
  candidateCoverage,
  cohortCaption,
  coverage,
  fitLabel,
  formatUserCp,
  fullyUnanalyzed,
  moveNo,
  statChips,
  unanalyzedNotice,
  verdictText,
  type CohortRow,
  type LabMove,
  type LabNode,
  type LabReport,
  type LineFit,
} from "./openingLab";

function report(over: Partial<LabReport> = {}): LabReport {
  return {
    player: "Lab, Tester",
    color: "black",
    ecos: ["E20", "E32"],
    games: 40,
    scorePct: 41.3,
    unanalyzedGames: 0,
    exit: { leftBook: 36, stillInBook: 4, medianExitPly: 17 },
    atExit: { evaluated: 30, equal: 20, better: 4, worse: 6 },
    errors: {
      analyzedGames: 32,
      gamesWithError: 24,
      bookPhase: 6,
      middlegame: 18,
      noErrorFound: 8,
      medianErrorPly: 39,
      middlegameP25Ply: 35,
      middlegameP75Ply: 51,
    },
    structures: [
      { flag: "own-isolated-pawn", games: 12, scorePct: 29.2, damage: 2.5 },
      { flag: "own-passed-pawn", games: 8, scorePct: 62.5, damage: 0 },
    ],
    nodes: [],
    homework: [],
    ...over,
  };
}

function move(over: Partial<LabMove> = {}): LabMove {
  return {
    san: "Bb4",
    games: 10,
    scorePct: 40,
    avgEvalCp: -35,
    evalGames: 6,
    inBook: true,
    inRep: false,
    damage: 1,
    replies: [],
    ...over,
  };
}

describe("verdictText — the product paragraph", () => {
  it("names the middlegame when games leave book equal and die later", () => {
    expect(verdictText(report())).toBe(
      "Your games leave book around move 9 with 80% of evaluated games still equal or " +
        "better — the opening is not where they die. The damage happens around moves " +
        "18–26: 18 of 24 first errors come after book, most often in own-isolated-pawn " +
        "positions. That is a middlegame-understanding gap, not a memorization gap — " +
        "see the structure homework below.",
    );
  });

  it("omits the structure clause when no structure carries damage", () => {
    const r = report({
      structures: [{ flag: "own-passed-pawn", games: 8, scorePct: 62.5, damage: 0 }],
    });
    expect(verdictText(r)).toContain("come after book. That is a middlegame");
  });

  it("collapses the move range when the quartiles agree", () => {
    const r = report({
      errors: { ...report().errors, middlegameP25Ply: 41, middlegameP75Ply: 41 },
    });
    expect(verdictText(r)).toContain("around move 21:");
  });

  it("calls out an opening problem when games are already worse at exit", () => {
    const r = report({ atExit: { evaluated: 20, equal: 4, better: 2, worse: 14 } });
    expect(verdictText(r)).toBe(
      "You are often already worse when you leave book: in 14 of 20 evaluated games " +
        "you were down more than half a pawn by move 9. The opening itself is costing " +
        "you here — start with the highest-damage branches below and adopt a sounder " +
        "book move.",
    );
  });

  it("calls out book-phase errors when they dominate", () => {
    const r = report({
      errors: { ...report().errors, bookPhase: 15, middlegame: 9 },
    });
    expect(verdictText(r)).toBe(
      "Your first significant mistakes come in the book phase: 15 of 24 first errors " +
        "happen at or before the book exit (leave book around move 9). Tightening the " +
        "branches below should pay off directly.",
    );
  });

  it("is honest when nothing is analyzed", () => {
    const r = report({ unanalyzedGames: 40 });
    expect(verdictText(r)).toBe(
      "You have 40 games here and leave book around move 9. None of them have engine " +
        "evals, so where the damage happens is honestly unknown — run the re-analysis " +
        "below to find out.",
    );
    expect(fullyUnanalyzed(r)).toBe(true);
  });

  it("handles the no-exit, no-games and no-error edges", () => {
    expect(verdictText(report({ games: 0 }))).toContain("No decided games");
    const stayed = report({
      unanalyzedGames: 40,
      exit: { leftBook: 0, stillInBook: 40, medianExitPly: null },
    });
    expect(verdictText(stayed)).toContain("mostly stay in book through the opening window");
    const clean = report({
      errors: { ...report().errors, gamesWithError: 0, bookPhase: 0, middlegame: 0 },
      atExit: { evaluated: 30, equal: 25, better: 5, worse: 0 },
    });
    expect(verdictText(clean)).toBe(
      "No significant errors (≥ 1.2 pawns) found in the 32 analyzed games, and you " +
        "score 41.3% here. Whatever is going wrong is subtler than a tactical swing — " +
        "the branch table below still shows where results lag.",
    );
  });
});

describe("coverage math", () => {
  it("computes the in-book share of observed replies", () => {
    const m = move({
      replies: [
        { san: "a6", games: 6, inBook: true },
        { san: "d6", games: 2, inBook: true },
        { san: "Rg8", games: 4, inBook: false },
      ],
    });
    expect(coverage(m)).toEqual({ inBook: 8, total: 12, pct: 67 });
  });

  it("is null with no observed replies — never a fake 0% or 100%", () => {
    expect(coverage(move({ replies: [] }))).toBeNull();
  });

  it("candidate coverage only exists where the user actually played the move", () => {
    const node: LabNode = {
      fen: "x",
      ply: 5,
      line: "1. e4 e5 2. Nf3 Nc6",
      games: 3,
      eco: null,
      openingName: null,
      repSan: null,
      hasExtension: false,
      damage: 0.5,
      moves: [move({ san: "Bb5", replies: [{ san: "a6", games: 2, inBook: true }] })],
      examples: [],
    };
    expect(candidateCoverage(node, "Bb5")).toEqual({ inBook: 2, total: 2, pct: 100 });
    expect(candidateCoverage(node, "d4")).toBeNull();
  });
});

describe("fit column honesty", () => {
  it("returns null without a cached profile (the build-a-profile state)", () => {
    const fit: LineFit = {
      flags: [{ flag: "own-doubled-pawns", scorePct: null, games: 0 }],
      fitAvailable: false,
      profilePlayer: null,
      profileBuiltAt: null,
    };
    expect(fitLabel(fit)).toBeNull();
    expect(fitLabel(null)).toBeNull();
  });

  it("names structures with profile scores, and says so when the profile lacks them", () => {
    const fit: LineFit = {
      flags: [
        { flag: "own-doubled-pawns", scorePct: 28.6, games: 7 },
        { flag: "own-passed-pawn", scorePct: null, games: 0 },
      ],
      fitAvailable: true,
      profilePlayer: "Lab, Tester",
      profileBuiltAt: "2026-07-30 12:00:00",
    };
    expect(fitLabel(fit)).toBe(
      "own-doubled-pawns 28.6% (7 games) · own-passed-pawn (no games in profile)",
    );
  });

  it("says so when the line leads to no distinctive structure", () => {
    const fit: LineFit = {
      flags: [],
      fitAvailable: true,
      profilePlayer: "Lab, Tester",
      profileBuiltAt: "x",
    };
    expect(fitLabel(fit)).toBe("no distinctive structures");
  });
});

describe("formatting helpers", () => {
  it("moveNo converts plies to fullmove numbers", () => {
    expect(moveNo(1)).toBe(1);
    expect(moveNo(2)).toBe(1);
    expect(moveNo(9)).toBe(5);
    expect(moveNo(17)).toBe(9);
  });

  it("formatUserCp renders signed pawns", () => {
    expect(formatUserCp(20)).toBe("+0.2");
    expect(formatUserCp(-160)).toBe("−1.6");
    expect(formatUserCp(0)).toBe("+0.0");
  });

  it("cohortCaption folds a single-code range", () => {
    const c: CohortRow = {
      color: "black",
      family: "Nimzo-Indian Defense",
      ecoMin: "E20",
      ecoMax: "E59",
      ecos: ["E20", "E32", "E59"],
      games: 412,
    };
    expect(cohortCaption(c)).toBe("as Black · E20–E59 · 412 games");
    expect(cohortCaption({ ...c, color: "white", ecoMin: "B20", ecoMax: "B20", games: 1 })).toBe(
      "as White · B20 · 1 game",
    );
  });

  it("unanalyzedNotice and statChips are honest about coverage", () => {
    expect(unanalyzedNotice(report())).toBeNull();
    const r = report({ unanalyzedGames: 7 });
    expect(unanalyzedNotice(r)).toBe(
      "7 of 40 games have no engine evals — eval and first-error findings skip them.",
    );
    expect(statChips(r)).toEqual([
      "40 games",
      "score 41.3%",
      "36 left book · 4 stayed in",
      "first errors: 6 book phase · 18 middlegame",
      "7 unanalyzed",
    ]);
    const bare = report({
      games: 1,
      unanalyzedGames: 1,
      errors: { ...report().errors, analyzedGames: 0 },
    });
    expect(statChips(bare)[0]).toBe("1 game");
    expect(statChips(bare)).not.toContain("first errors: 6 book phase · 18 middlegame");
  });
});
