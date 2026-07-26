import { describe, expect, it } from "vitest";
import { NAG_GLYPHS, nagView } from "./nags";

describe("nagView", () => {
  it("maps the standard move-suffix NAGs", () => {
    expect(nagView(1).glyph).toBe("!");
    expect(nagView(2).glyph).toBe("?");
    expect(nagView(3).glyph).toBe("!!");
    expect(nagView(4).glyph).toBe("??");
    expect(nagView(5).glyph).toBe("!?");
    expect(nagView(6).glyph).toBe("?!");
  });

  it("maps positional assessment and idea NAGs", () => {
    expect(nagView(7).glyph).toBe("□");
    expect(nagView(10).glyph).toBe("=");
    expect(nagView(13).glyph).toBe("∞");
    expect(nagView(14).glyph).toBe("⩲");
    expect(nagView(15).glyph).toBe("⩱");
    expect(nagView(16).glyph).toBe("±");
    expect(nagView(17).glyph).toBe("∓");
    expect(nagView(18).glyph).toBe("+−");
    expect(nagView(19).glyph).toBe("−+");
    expect(nagView(22).glyph).toBe("⨀");
    expect(nagView(23).glyph).toBe("⨀");
    expect(nagView(32).glyph).toBe("⟳");
    expect(nagView(33).glyph).toBe("⟳");
    expect(nagView(36).glyph).toBe("↑");
    expect(nagView(40).glyph).toBe("↑");
    expect(nagView(44).glyph).toBe("∞=");
    expect(nagView(132).glyph).toBe("⇆");
    expect(nagView(133).glyph).toBe("⇆");
    expect(nagView(146).glyph).toBe("N");
  });

  it("known NAGs have no tooltip and are not markers", () => {
    for (const key of Object.keys(NAG_GLYPHS)) {
      const v = nagView(Number(key));
      expect(v.unknown).toBe(false);
      expect(v.hidden).toBe(false);
      expect(v.title).toBeUndefined();
    }
  });

  it("renders unknown NAGs as a dotted marker with a tooltip, never raw $N", () => {
    for (const n of [8, 21, 100, 139, 255]) {
      const v = nagView(n);
      expect(v.unknown).toBe(true);
      expect(v.hidden).toBe(false);
      expect(v.glyph).toBe("·");
      expect(v.glyph).not.toContain("$");
      expect(v.title).toBe(`annotation code $${n}`);
    }
  });

  it("renders NAG 201 as nothing visible except a tooltip", () => {
    const v = nagView(201);
    expect(v.hidden).toBe(true);
    expect(v.unknown).toBe(false);
    expect(v.glyph).toBe("");
    expect(v.title).toBe("diagram marker (imported)");
  });
});
