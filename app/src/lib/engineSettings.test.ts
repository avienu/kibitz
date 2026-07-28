import { describe, expect, it } from "vitest";
import {
  parseNodesInput,
  tbStatusLine,
  verifiedLine,
  verifyFailedLine,
} from "./engineSettings";

describe("verify lines", () => {
  it("names the engine on a full handshake", () => {
    expect(verifiedLine({ path: "/x/stockfish", name: "Stockfish 17.1" })).toBe(
      "Stockfish 17.1 — UCI handshake OK",
    );
  });

  it("stays honest when uciok arrived without an id name", () => {
    expect(verifiedLine({ path: "/x/mystery", name: null })).toMatch(/no id name/);
  });

  it("preserves the backend error on failure", () => {
    expect(verifyFailedLine("Timed out waiting for 'uciok' — not a UCI engine?")).toBe(
      "Not usable: Timed out waiting for 'uciok' — not a UCI engine?",
    );
  });
});

describe("tablebase status line", () => {
  const ready = { available: true, largest: 5, note: "loaded" };
  const missing = { available: false, largest: null, note: "no tablebase" };

  it("shows coverage and the resolution source", () => {
    expect(tbStatusLine(ready, "")).toBe(
      "up to 5 pieces · automatic (KIBITZ_SYZYGY, else testdata/syzygy)",
    );
    expect(tbStatusLine(ready, "/tb/syzygy")).toBe("up to 5 pieces · configured: /tb/syzygy");
  });

  it("says 'not configured' instead of faking coverage", () => {
    expect(tbStatusLine(missing, "")).toMatch(/^not configured/);
    expect(tbStatusLine(missing, "/gone")).toContain("configured: /gone");
  });

  it("admits an unknown status before the first probe returns", () => {
    expect(tbStatusLine(null, "")).toMatch(/status unknown/);
  });
});

describe("parseNodesInput", () => {
  it("accepts plain and separator-formatted positive integers", () => {
    expect(parseNodesInput("2000000")).toBe(2_000_000);
    expect(parseNodesInput("2,000,000")).toBe(2_000_000);
    expect(parseNodesInput("2_000_000")).toBe(2_000_000);
    expect(parseNodesInput(" 500 000 ")).toBe(500_000);
  });

  it("rejects zero, negatives and non-numbers", () => {
    for (const bad of ["0", "-5", "", "2m", "1e6", "nodes"]) {
      expect(parseNodesInput(bad), bad).toBeNull();
    }
  });
});
