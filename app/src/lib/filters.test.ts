import { describe, expect, it } from "vitest";
import {
  dateBoundParam,
  dateRangeHint,
  eloParam,
  isValidDateBound,
  SOURCE_KINDS,
} from "./filters";

describe("date bound validation", () => {
  it("accepts empty, YYYY, YYYY.MM and YYYY.MM.DD", () => {
    for (const ok of ["", "  ", "1992", "1992.05", "1992.05.10", "1858.11.02"]) {
      expect(isValidDateBound(ok), ok).toBe(true);
    }
  });

  it("rejects malformed and out-of-range bounds (backend mirror)", () => {
    for (const bad of ["92", "1992-05-10", "1992.13", "1992.05.32", "1992.5", "x", "1992."]) {
      expect(isValidDateBound(bad), bad).toBe(false);
    }
  });

  it("dateBoundParam sends only valid, non-empty bounds", () => {
    expect(dateBoundParam("")).toBeUndefined();
    expect(dateBoundParam(" 1992 ")).toBe("1992");
    expect(dateBoundParam("1992-05")).toBeUndefined();
  });

  it("dateRangeHint flags either bad bound, silent otherwise", () => {
    expect(dateRangeHint("1992", "1992.06")).toBeNull();
    expect(dateRangeHint("", "")).toBeNull();
    expect(dateRangeHint("92", "")).toMatch(/YYYY/);
    expect(dateRangeHint("", "1992.13")).toMatch(/YYYY/);
  });
});

describe("elo input", () => {
  it("parses plain integers and drops garbage", () => {
    expect(eloParam("2500")).toBe(2500);
    expect(eloParam(" 800 ")).toBe(800);
    expect(eloParam("")).toBeUndefined();
    expect(eloParam("-100")).toBeUndefined();
    expect(eloParam("9001")).toBeUndefined();
    expect(eloParam("2k5")).toBeUndefined();
  });
});

describe("source kinds", () => {
  it("matches the backend's accepted set in priority order", () => {
    expect([...SOURCE_KINDS]).toEqual(["personal", "twic", "online", "other"]);
  });
});
