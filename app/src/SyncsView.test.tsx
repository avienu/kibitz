// @vitest-environment jsdom
import { cleanup, fireEvent, render, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import SyncsView from "./SyncsView";
import { syncAccounts, syncRun, type SyncAccounts } from "./lib/net";

vi.mock("./lib/net", async () => {
  const actual = await vi.importActual<typeof import("./lib/net")>("./lib/net");
  return {
    ...actual,
    syncAccounts: vi.fn(),
    syncRun: vi.fn(() => Promise.resolve()),
    syncSetUsername: vi.fn(() => Promise.resolve()),
  };
});

function accountsFixture(overrides: Partial<SyncAccounts> = {}): SyncAccounts {
  return {
    lichess: {
      username: "SomeUser",
      lastReport: {
        at: "2026-07-26 21:00:00",
        gamesImported: 128,
        duplicatesSkipped: 40,
        gamesFailed: 0,
      },
    },
    chesscom: { username: null, lastReport: null },
    fics: { username: null, lastReport: null },
    ...overrides,
  };
}

beforeEach(() => {
  vi.mocked(syncAccounts).mockResolvedValue(accountsFixture());
});

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe("account cards", () => {
  it("seeds the persisted username and shows the stored last-sync report", async () => {
    const { container } = render(<SyncsView progress={null} />);
    const input = container.querySelector<HTMLInputElement>('input[aria-label="Lichess username"]')!;
    await waitFor(() => expect(input.value).toBe("SomeUser"));
    expect(container.textContent).toContain(
      "Last sync 2026-07-26 21:00:00 UTC: 128 imported · 40 duplicates · 0 failed",
    );
  });

  it("Sync now runs the existing client for the typed username", async () => {
    const { container } = render(<SyncsView progress={null} />);
    const input = container.querySelector<HTMLInputElement>('input[aria-label="Lichess username"]')!;
    await waitFor(() => expect(input.value).toBe("SomeUser"));
    fireEvent.click(input.closest(".sync-card")!.querySelector("button")!);
    await waitFor(() =>
      expect(syncRun).toHaveBeenCalledWith("lichess", "SomeUser", undefined, undefined),
    );
  });

  it("chess.com card syncs with its own username", async () => {
    const { container } = render(<SyncsView progress={null} />);
    await waitFor(() => expect(syncAccounts).toHaveBeenCalled());
    const input = container.querySelector<HTMLInputElement>('input[aria-label="chess.com username"]')!;
    fireEvent.change(input, { target: { value: "ccUser" } });
    fireEvent.click(input.closest(".sync-card")!.querySelector("button")!);
    await waitFor(() =>
      expect(syncRun).toHaveBeenCalledWith("chesscom", "ccUser", undefined, undefined),
    );
  });

  it("FICS card passes year and optional month, and requires a sane year", async () => {
    const { container } = render(<SyncsView progress={null} />);
    await waitFor(() => expect(syncAccounts).toHaveBeenCalled());
    const user = container.querySelector<HTMLInputElement>('input[aria-label="FICS username"]')!;
    const year = container.querySelector<HTMLInputElement>('input[aria-label="FICS year"]')!;
    const month = container.querySelector<HTMLInputElement>(
      'input[aria-label="FICS month (optional)"]',
    )!;
    const button = user.closest(".sync-card")!.querySelector("button")!;
    fireEvent.change(user, { target: { value: "FicsUser" } });
    fireEvent.change(year, { target: { value: "2025" } });
    fireEvent.change(month, { target: { value: "6" } });
    fireEvent.click(button);
    await waitFor(() => expect(syncRun).toHaveBeenCalledWith("fics", "FicsUser", 2025, 6));

    // An invalid year disables the run (ficsgames.org starts at 1999).
    fireEvent.change(year, { target: { value: "1990" } });
    expect(button.hasAttribute("disabled")).toBe(true);
  });
});

describe("honesty", () => {
  it("shows a running sync as indeterminate work, not a fake percentage", async () => {
    const { container } = render(
      <SyncsView
        progress={{
          kind: "lichess",
          label: "Lichess: SomeUser",
          done: 0,
          total: 0,
          detail: "downloading & importing — strictly serial; rate limits are respected",
          active: true,
          error: null,
        }}
      />,
    );
    await waitFor(() => expect(syncAccounts).toHaveBeenCalled());
    expect(container.textContent).toContain("rate limits are respected");
    expect(container.textContent).toContain("strictly serial");
    expect(container.textContent).not.toContain("%");
  });

  it("surfaces a persisted failure verbatim", async () => {
    vi.mocked(syncAccounts).mockResolvedValue(
      accountsFixture({
        lichess: {
          username: "SomeUser",
          lastReport: { at: "2026-07-25 10:00:00", error: "aborting after 4 rate-limit (429) responses" },
        },
      }),
    );
    const { container } = render(<SyncsView progress={null} />);
    await waitFor(() => expect(syncAccounts).toHaveBeenCalled());
    expect(container.textContent).toContain(
      "Failed (2026-07-25 10:00:00 UTC): aborting after 4 rate-limit (429) responses",
    );
  });

  it("keeps the FICS personal-use notice and the honest ICC note", async () => {
    const { container } = render(<SyncsView progress={null} />);
    await waitFor(() => expect(syncAccounts).toHaveBeenCalled());
    expect(container.textContent).toContain("volunteer-run archive");
    expect(container.textContent).toContain("Personal use only");
    expect(container.textContent).toContain("no scriptable export API");
    expect(container.textContent).toContain("Import PGN / SCID");
    expect(container.textContent).toContain("Provenance of every imported game");
  });

  it("names the bzip2 fallback when the last FICS run saved an archive", async () => {
    vi.mocked(syncAccounts).mockResolvedValue(
      accountsFixture({
        fics: {
          username: "FicsUser",
          lastReport: {
            at: "t",
            gamesImported: 0,
            duplicatesSkipped: 0,
            gamesFailed: 0,
            year: 2025,
            month: null,
            savedArchive: "/tmp/fics_FicsUser_2025_0.pgn.bz2",
          },
        },
      }),
    );
    const { container } = render(<SyncsView progress={null} />);
    await waitFor(() => expect(syncAccounts).toHaveBeenCalled());
    expect(container.textContent).toContain("bunzip2");
    expect(container.textContent).toContain("/tmp/fics_FicsUser_2025_0.pgn.bz2");
  });
});
