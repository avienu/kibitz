import { describe, expect, it } from "vitest";
import { promoKeyRole, promotionPending, PROMO_ROLES } from "./promotion";

// White pawn on e7, ready to promote; Black pawn on a2 likewise.
const WHITE_PROMO = "8/4P3/8/8/8/1k6/p3K3/8 w - - 0 1";
const BLACK_PROMO = "8/4P3/8/8/8/1k6/p3K3/8 b - - 0 1";
const START = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

describe("promotionPending", () => {
  it("detects a white pawn reaching the 8th", () => {
    expect(promotionPending(WHITE_PROMO, "e7", "e8")).toBe("white");
  });

  it("detects a black pawn reaching the 1st", () => {
    expect(promotionPending(BLACK_PROMO, "a2", "a1")).toBe("black");
  });

  it("ignores non-pawn movers and non-final ranks", () => {
    expect(promotionPending(WHITE_PROMO, "e2", "e1")).toBeNull(); // king
    expect(promotionPending(START, "e2", "e4")).toBeNull(); // pawn mid-board
  });

  it("rejects illegal promotions", () => {
    // e7 pawn can't reach d8 (nothing to capture).
    expect(promotionPending(WHITE_PROMO, "e7", "d8")).toBeNull();
    expect(promotionPending("bad fen", "e7", "e8")).toBeNull();
  });
});

describe("promoKeyRole", () => {
  it("maps 1-4 to queen/rook/bishop/knight", () => {
    expect(["1", "2", "3", "4"].map(promoKeyRole)).toEqual([...PROMO_ROLES]);
    expect(promoKeyRole("5")).toBeNull();
    expect(promoKeyRole("Escape")).toBeNull();
  });
});
