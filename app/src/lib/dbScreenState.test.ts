import { beforeEach, describe, expect, it } from "vitest";
import {
  clearDbScreenState,
  dbScreenState,
  hasActiveFilters,
  updateDbScreenState,
} from "./dbScreenState";

describe("dbScreenState store", () => {
  beforeEach(() => {
    clearDbScreenState();
  });

  it("starts pristine", () => {
    expect(dbScreenState()).toEqual({
      player: "",
      eco: "",
      result: "",
      page: 0,
      scrollTop: 0,
      selectedGameId: null,
    });
    expect(hasActiveFilters(dbScreenState())).toBe(false);
  });

  it("survives a screen unmount/remount (module scope): the field-report fix", () => {
    // "I search for my name, pick a game, go back — search is gone."
    updateDbScreenState({ player: "Carlsen", page: 3, selectedGameId: 42, scrollTop: 810 });
    // A remount reads the store fresh — nothing was lost.
    expect(dbScreenState()).toMatchObject({
      player: "Carlsen",
      page: 3,
      selectedGameId: 42,
      scrollTop: 810,
    });
    expect(hasActiveFilters(dbScreenState())).toBe(true);
  });

  it("merges partial updates without clobbering the rest", () => {
    updateDbScreenState({ player: "Tal" });
    updateDbScreenState({ eco: "B10", result: "1-0" });
    updateDbScreenState({ page: 2 });
    expect(dbScreenState()).toMatchObject({ player: "Tal", eco: "B10", result: "1-0", page: 2 });
  });

  it("clear resets everything including selection and scroll", () => {
    updateDbScreenState({ player: "x", eco: "C41", page: 5, scrollTop: 99, selectedGameId: 7 });
    const s = clearDbScreenState();
    expect(s).toEqual(dbScreenState());
    expect(hasActiveFilters(s)).toBe(false);
    expect(s.selectedGameId).toBeNull();
    expect(s.scrollTop).toBe(0);
  });

  it("page alone counts as an active filter (Clear resets pagination too)", () => {
    updateDbScreenState({ page: 1 });
    expect(hasActiveFilters(dbScreenState())).toBe(true);
  });
});
