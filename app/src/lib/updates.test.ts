/**
 * Updater dry-run test (run-8 item 3): validates the version-compare and
 * platform-key selection logic against a MOCK latest.json fixture parsed
 * directly from disk (__fixtures__/latest.json) — no network, no updater
 * plugin. The fixture has the exact shape the release pipeline publishes
 * (scripts/release/generate_latest_json.mjs).
 */
import { describe, expect, it } from "vitest";
import fixture from "./__fixtures__/latest.json";
import {
  compareVersions,
  feedOffer,
  getSavedUpdateCheck,
  platformKey,
  saveUpdateCheck,
  selectPlatform,
  type LatestFeed,
} from "./updates";

const feed = fixture as LatestFeed;

describe("compareVersions", () => {
  it("orders numeric cores numerically, not lexically", () => {
    expect(compareVersions("0.1.0", "0.2.0")).toBe(-1);
    expect(compareVersions("0.10.0", "0.9.0")).toBe(1); // lexical would say 0.10 < 0.9
    expect(compareVersions("1.0.0", "1.0.0")).toBe(0);
    expect(compareVersions("2.0.0", "10.0.0")).toBe(-1);
  });

  it("tolerates a leading v and short versions", () => {
    expect(compareVersions("v0.2.0", "0.1.0")).toBe(1);
    expect(compareVersions("1.0", "1.0.0")).toBe(0);
  });

  it("sorts a prerelease before its release", () => {
    expect(compareVersions("1.0.0-beta.1", "1.0.0")).toBe(-1);
    expect(compareVersions("1.0.0", "1.0.0-rc.1")).toBe(1);
    expect(compareVersions("1.0.0-beta.1", "1.0.0-beta.2")).toBe(-1);
  });
});

describe("platformKey", () => {
  it("builds the {os}-{arch} keys the Tauri updater looks up", () => {
    expect(platformKey("darwin", "aarch64")).toBe("darwin-aarch64");
    expect(platformKey("linux", "x86_64")).toBe("linux-x86_64");
  });
});

describe("selectPlatform against the mock feed", () => {
  it("finds the exact platform entries we ship (darwin-aarch64, linux-x86_64)", () => {
    expect(selectPlatform(feed, "darwin-aarch64")?.url).toContain("aarch64.app.tar.gz");
    expect(selectPlatform(feed, "linux-x86_64")?.url).toContain("amd64.AppImage");
  });

  it("returns null for platforms the feed does not cover", () => {
    expect(selectPlatform(feed, "windows-x86_64")).toBeNull();
    expect(selectPlatform(feed, "linux-aarch64")).toBeNull();
  });

  it("falls back to darwin-universal for mac archs when present", () => {
    const universal: LatestFeed = {
      version: "0.3.0",
      platforms: { "darwin-universal": { signature: "sig", url: "u.tar.gz" } },
    };
    expect(selectPlatform(universal, "darwin-aarch64")?.url).toBe("u.tar.gz");
    expect(selectPlatform(universal, "darwin-x86_64")?.url).toBe("u.tar.gz");
    expect(selectPlatform(universal, "linux-x86_64")).toBeNull();
  });
});

describe("feedOffer (full decision)", () => {
  it("offers the 0.2.0 fixture to a 0.1.0 install on covered platforms", () => {
    const offer = feedOffer(feed, "0.1.0", "darwin-aarch64");
    expect(offer).not.toBeNull();
    expect(offer?.version).toBe("0.2.0");
    expect(offer?.signature.length).toBeGreaterThan(0);
    expect(offer?.url).toMatch(/^https:\/\/github\.com\/avienu\/kibitz\/releases\/download\//);
  });

  it("offers nothing when already current or ahead", () => {
    expect(feedOffer(feed, "0.2.0", "darwin-aarch64")).toBeNull();
    expect(feedOffer(feed, "0.3.0", "darwin-aarch64")).toBeNull();
  });

  it("offers nothing on an uncovered platform even when newer", () => {
    expect(feedOffer(feed, "0.1.0", "windows-x86_64")).toBeNull();
  });

  it("fixture platform entries all carry signature + https url (feed contract)", () => {
    for (const [key, p] of Object.entries(feed.platforms)) {
      expect(key).toMatch(/^(darwin|linux|windows)-[a-z0-9_]+$/);
      expect(p.signature.length).toBeGreaterThan(0);
      expect(p.url.startsWith("https://")).toBe(true);
    }
  });
});

describe("check-for-updates setting", () => {
  it("defaults ON, round-trips OFF and back", () => {
    localStorage.removeItem("kibitz.updates.checkOnLaunch");
    expect(getSavedUpdateCheck()).toBe(true); // default ON
    saveUpdateCheck(false);
    expect(getSavedUpdateCheck()).toBe(false);
    saveUpdateCheck(true);
    expect(getSavedUpdateCheck()).toBe(true);
  });
});
