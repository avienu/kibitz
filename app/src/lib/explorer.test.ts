import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  createExplorerLoader,
  EXPLORER_DEBOUNCE_MS,
  explorerTotal,
  getExplorerEnabled,
  saveExplorerEnabled,
  type ExplorerReply,
  type ExplorerState,
} from "./explorer";

function reply(white: number): ExplorerReply {
  return { white, draws: 0, black: 0, moves: [] };
}

describe("explorer loader", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it("debounces: rapid position changes cost ONE network request", async () => {
    const fetchFn = vi.fn((fen: string) => Promise.resolve(reply(fen.length)));
    const loader = createExplorerLoader(fetchFn);
    const states: ExplorerState[] = [];
    loader.request("fen-a", (s) => states.push(s));
    vi.advanceTimersByTime(200);
    loader.request("fen-b", (s) => states.push(s));
    vi.advanceTimersByTime(200);
    loader.request("fen-c", (s) => states.push(s));
    expect(fetchFn).not.toHaveBeenCalled();

    vi.advanceTimersByTime(EXPLORER_DEBOUNCE_MS);
    await vi.runAllTimersAsync();
    expect(fetchFn).toHaveBeenCalledTimes(1);
    expect(fetchFn).toHaveBeenCalledWith("fen-c");
    expect(states.filter((s) => s.kind === "data")).toHaveLength(1);
  });

  it("caches per FEN: a revisit answers synchronously with no request", async () => {
    const fetchFn = vi.fn((fen: string) => Promise.resolve(reply(fen.length)));
    const loader = createExplorerLoader(fetchFn);
    loader.request("fen-a", () => {});
    await vi.runAllTimersAsync();
    expect(fetchFn).toHaveBeenCalledTimes(1);
    expect(loader.cacheSize()).toBe(1);

    let sync: ExplorerState | null = null;
    loader.request("fen-a", (s) => (sync = s));
    expect(sync).toEqual({ kind: "data", reply: reply(5) });
    expect(fetchFn, "cache hit must not refetch").toHaveBeenCalledTimes(1);
  });

  it("drops stale responses when a newer request superseded them", async () => {
    let resolveA: ((r: ExplorerReply) => void) | null = null;
    const fetchFn = vi.fn((fen: string) =>
      fen === "slow"
        ? new Promise<ExplorerReply>((res) => (resolveA = res))
        : Promise.resolve(reply(99)),
    );
    const loader = createExplorerLoader(fetchFn);
    const slowStates: ExplorerState[] = [];
    const fastStates: ExplorerState[] = [];
    loader.request("slow", (s) => slowStates.push(s));
    vi.advanceTimersByTime(EXPLORER_DEBOUNCE_MS);
    // The slow fetch is in flight; the user moves on.
    loader.request("fast", (s) => fastStates.push(s));
    vi.advanceTimersByTime(EXPLORER_DEBOUNCE_MS);
    await vi.runAllTimersAsync();
    resolveA!(reply(1));
    await Promise.resolve();

    expect(slowStates.map((s) => s.kind)).toEqual(["loading"]); // data dropped
    expect(fastStates.map((s) => s.kind)).toEqual(["loading", "data"]);
    // ...but the slow reply still landed in the cache for a revisit.
    expect(loader.cacheSize()).toBe(2);
  });

  it("surfaces failures as an error state, never a throw", async () => {
    const loader = createExplorerLoader(() => Promise.reject(new Error("offline")));
    const states: ExplorerState[] = [];
    loader.request("fen-x", (s) => states.push(s));
    await vi.runAllTimersAsync();
    expect(states[states.length - 1]).toEqual({
      kind: "error",
      message: "Error: offline",
    });
  });

  it("cancel() stops the pending request and silences late callbacks", async () => {
    const fetchFn = vi.fn(() => Promise.resolve(reply(1)));
    const loader = createExplorerLoader(fetchFn);
    const states: ExplorerState[] = [];
    loader.request("fen-a", (s) => states.push(s));
    loader.cancel();
    await vi.runAllTimersAsync();
    expect(fetchFn).not.toHaveBeenCalled();
    expect(states.map((s) => s.kind)).toEqual(["loading"]);
  });
});

describe("toggle persistence", () => {
  it("defaults OFF (network-quiet) and round-trips", () => {
    localStorage.removeItem("kibitz.onlineExplorer");
    expect(getExplorerEnabled()).toBe(false);
    saveExplorerEnabled(true);
    expect(getExplorerEnabled()).toBe(true);
    saveExplorerEnabled(false);
    expect(getExplorerEnabled()).toBe(false);
  });
});

describe("explorerTotal", () => {
  it("sums W/D/L", () => {
    expect(explorerTotal({ white: 3, draws: 2, black: 1 })).toBe(6);
  });
});
