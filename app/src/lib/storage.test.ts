import { describe, expect, it } from "vitest";
import { isCloudSyncedPath } from "./storage";

describe("isCloudSyncedPath", () => {
  it("flags the major cloud-sync locations", () => {
    // The maintainer's actual dev path — the case that saturated the machine.
    expect(
      isCloudSyncedPath(
        "/Users/x/Library/CloudStorage/Dropbox/prog/silman/testdata/corpus/scid.sqlite",
      ),
    ).toBe(true);
    expect(isCloudSyncedPath("/Users/x/Dropbox/chess/db.sqlite")).toBe(true);
    expect(isCloudSyncedPath("/Users/x/Google Drive/db.sqlite")).toBe(true);
    expect(isCloudSyncedPath("/Users/x/OneDrive/db.sqlite")).toBe(true);
    expect(
      isCloudSyncedPath("/Users/x/Library/Mobile Documents/com~apple~CloudDocs/db.sqlite"),
    ).toBe(true);
  });

  it("passes local locations", () => {
    expect(
      isCloudSyncedPath("/Users/x/Library/Application Support/org.kibitzchess.app/kibitz.sqlite"),
    ).toBe(false);
    expect(isCloudSyncedPath("/Users/x/chess/db.sqlite")).toBe(false);
  });
});
