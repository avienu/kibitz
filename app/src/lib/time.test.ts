import { describe, expect, it } from "vitest";
import { parseUtc, utcDateLocal, utcDateTimeLocal, utcWeekdayLocal } from "./time";

// The audit #10 shape: a timestamp shortly after UTC midnight is still
// "yesterday" west of Greenwich and already "tomorrow" far east.
const JUST_PAST_UTC_MIDNIGHT = "2026-07-28 01:30:00";

describe("parseUtc", () => {
  it("parses stored SQLite timestamps and rejects junk", () => {
    expect(parseUtc("2026-07-27 12:00:00")?.toISOString()).toBe("2026-07-27T12:00:00.000Z");
    expect(parseUtc("not a time")).toBeNull();
    expect(parseUtc("2026-07-27")).toBeNull();
  });
});

describe("utcDateLocal (audit #10)", () => {
  it("renders the date in the viewer's zone, not the UTC digits", () => {
    // Los Angeles is UTC-7 in July: 2026-07-28 01:30 UTC is still Jul 27.
    expect(utcDateLocal(JUST_PAST_UTC_MIDNIGHT, "America/Los_Angeles")).toBe("2026-07-27");
    // Tokyo (UTC+9) is already well into Jul 28.
    expect(utcDateLocal(JUST_PAST_UTC_MIDNIGHT, "Asia/Tokyo")).toBe("2026-07-28");
    // At UTC itself the digits agree.
    expect(utcDateLocal(JUST_PAST_UTC_MIDNIGHT, "UTC")).toBe("2026-07-28");
  });

  it("falls back to the raw date part for malformed input", () => {
    expect(utcDateLocal("2026-07-27 garbage")).toBe("2026-07-27");
  });
});

describe("utcDateTimeLocal", () => {
  it("renders date and clock time in the given zone", () => {
    expect(utcDateTimeLocal("2026-07-27 12:00:00", "UTC")).toBe("2026-07-27 12:00");
    expect(utcDateTimeLocal("2026-07-27 12:00:00", "America/Los_Angeles")).toBe(
      "2026-07-27 05:00",
    );
  });

  it("passes malformed input through unchanged", () => {
    expect(utcDateTimeLocal("t")).toBe("t");
  });
});

describe("utcWeekdayLocal", () => {
  it("names the LOCAL weekday — the 'New since Tuesday' label class", () => {
    // 2026-07-28 is a Tuesday in UTC; 01:30 UTC is still Monday in LA.
    expect(utcWeekdayLocal(JUST_PAST_UTC_MIDNIGHT, "UTC")).toBe("Tuesday");
    expect(utcWeekdayLocal(JUST_PAST_UTC_MIDNIGHT, "America/Los_Angeles")).toBe("Monday");
    expect(utcWeekdayLocal("junk")).toBeNull();
  });
});
