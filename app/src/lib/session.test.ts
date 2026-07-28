import { describe, expect, it } from "vitest";
import { hasDeepLinkOverride, parseSession, serializeSession } from "./session";

const DB_SCREEN = {
  player: "O'Connor",
  eco: "B1",
  result: "1-0",
  page: 2,
  scrollTop: 340,
  selectedGameId: 3759,
};

describe("session blob", () => {
  it("round-trips view and database-screen state", () => {
    const parsed = parseSession(serializeSession("game", DB_SCREEN));
    expect(parsed).not.toBeNull();
    expect(parsed!.view).toBe("game");
    expect(parsed!.dbScreen).toEqual(DB_SCREEN);
  });

  it("rejects corrupt, foreign-version, and unknown-view blobs", () => {
    expect(parseSession(null)).toBeNull();
    expect(parseSession("")).toBeNull();
    expect(parseSession("not json")).toBeNull();
    expect(parseSession(JSON.stringify({ version: 99, view: "home" }))).toBeNull();
    expect(
      parseSession(JSON.stringify({ version: 1, view: "hacked", dbScreen: DB_SCREEN })),
    ).toBeNull();
  });

  it("sanitizes malformed dbScreen fields instead of failing", () => {
    const parsed = parseSession(
      JSON.stringify({
        version: 1,
        view: "database",
        dbScreen: { player: 7, page: -3, scrollTop: "x", selectedGameId: "y" },
      }),
    );
    expect(parsed!.dbScreen).toEqual({
      player: "",
      eco: "",
      result: "",
      page: 0,
      scrollTop: 0,
      selectedGameId: null,
    });
  });
});

describe("deep-link override", () => {
  it("detects db/game/screen params and ignores others", () => {
    expect(hasDeepLinkOverride("#db=/tmp/x.sqlite")).toBe(true);
    expect(hasDeepLinkOverride("#game=12&ply=4")).toBe(true);
    expect(hasDeepLinkOverride("#screen=profile")).toBe(true);
    expect(hasDeepLinkOverride("#theme=light")).toBe(false);
    expect(hasDeepLinkOverride("")).toBe(false);
  });
});
