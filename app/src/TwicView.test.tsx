// @vitest-environment jsdom
import { cleanup, fireEvent, render, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import TwicView from "./TwicView";
import {
  twicAckNotice,
  twicCatalog,
  twicDownload,
  twicRefreshCatalog,
  twicSetAutoSync,
  type TwicCatalog,
} from "./lib/net";

/* IPC is mocked at the lib/net boundary; the pure helpers are the real
 * implementations so selection logic is exercised end to end. */
vi.mock("./lib/net", async () => {
  const actual = await vi.importActual<typeof import("./lib/net")>("./lib/net");
  return {
    ...actual,
    twicCatalog: vi.fn(),
    twicRefreshCatalog: vi.fn(),
    twicDownload: vi.fn(),
    twicSetAutoSync: vi.fn(() => Promise.resolve()),
    twicAckNotice: vi.fn(() => Promise.resolve()),
    netCancel: vi.fn(() => Promise.resolve(true)),
  };
});

const NOTICE = "This looks like your first TWIC sync. … personal use only …";

function catalogFixture(overrides: Partial<TwicCatalog> = {}): TwicCatalog {
  return {
    firstAvailable: 1648,
    latestImported: 1650,
    latestKnown: 1652,
    rows: [
      { issue: 1652, imported: false, games: null, approxDate: "2026-07-06" },
      { issue: 1651, imported: false, games: null, approxDate: "2026-06-29" },
      { issue: 1650, imported: true, games: 4210, approxDate: "2026-06-22" },
      { issue: 1649, imported: false, games: null, approxDate: "2026-06-15" },
      { issue: 1648, imported: true, games: 3999, approxDate: "2026-06-08" },
    ],
    autoSync: false,
    noticeAcknowledged: true,
    firstRunNotice: NOTICE,
    ...overrides,
  };
}

beforeEach(() => {
  vi.mocked(twicCatalog).mockResolvedValue(catalogFixture());
  vi.mocked(twicDownload).mockResolvedValue(3);
});

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe("TWIC catalog table", () => {
  it("lists every known issue with status, games and approx week", async () => {
    const { container, getByText } = render(<TwicView progress={null} />);
    await waitFor(() => expect(twicCatalog).toHaveBeenCalled());
    expect(getByText("1652")).toBeTruthy();
    expect(getByText("1648")).toBeTruthy();
    expect(container.querySelectorAll(".twic-imported").length).toBe(2);
    expect(getByText("4,210")).toBeTruthy();
    // Dates are labelled approximate, in the cell and the footnote.
    expect(getByText("≈ 2026-06-22")).toBeTruthy();
    expect(container.textContent).toContain("approximate");
    // Header subtitle: real counts.
    expect(container.textContent).toContain("2 of 5 issues imported");
  });

  it("keeps the personal-use posture in the footer", async () => {
    const { container } = render(<TwicView progress={null} />);
    await waitFor(() => expect(twicCatalog).toHaveBeenCalled());
    expect(container.textContent).toContain("personal use only");
    expect(container.textContent).toContain("never bundles or redistributes");
    expect(container.textContent).toContain("theweekinchess.com");
  });

  it("shows the refresh call-to-action instead of fake rows when nothing is known", async () => {
    vi.mocked(twicCatalog).mockResolvedValue(
      catalogFixture({ latestImported: null, latestKnown: null, rows: [], noticeAcknowledged: false }),
    );
    const { container } = render(<TwicView progress={null} />);
    await waitFor(() => expect(twicCatalog).toHaveBeenCalled());
    expect(container.querySelector(".dtable")).toBeNull();
    expect(container.textContent).toContain("Refresh catalog");
    expect(container.textContent).toContain("HEAD requests");
  });
});

describe("downloads", () => {
  it("Download all missing sends exactly the missing issues", async () => {
    const { getByText } = render(<TwicView progress={null} />);
    await waitFor(() => expect(twicCatalog).toHaveBeenCalled());
    fireEvent.click(getByText("Download all missing (3)"));
    await waitFor(() =>
      expect(twicDownload).toHaveBeenCalledWith([1652, 1651, 1649]),
    );
  });

  it("checkbox selection downloads only the selected issues", async () => {
    const { container, getByText } = render(<TwicView progress={null} />);
    await waitFor(() => expect(twicCatalog).toHaveBeenCalled());
    fireEvent.click(container.querySelector('input[aria-label="Select TWIC 1651"]')!);
    fireEvent.click(getByText("Download selected (1)"));
    await waitFor(() => expect(twicDownload).toHaveBeenCalledWith([1651]));
  });

  it("imported rows have no checkbox — an issue is never fetched twice", async () => {
    const { container } = render(<TwicView progress={null} />);
    await waitFor(() => expect(twicCatalog).toHaveBeenCalled());
    expect(container.querySelector('input[aria-label="Select TWIC 1650"]')).toBeNull();
    expect(container.querySelector('input[aria-label="Select TWIC 1649"]')).toBeTruthy();
  });
});

describe("first-run notice", () => {
  it("gates the very first download behind an in-UI acknowledgement", async () => {
    vi.mocked(twicCatalog).mockResolvedValue(
      catalogFixture({ latestImported: null, noticeAcknowledged: false }),
    );
    const { getByText } = render(<TwicView progress={null} />);
    await waitFor(() => expect(twicCatalog).toHaveBeenCalled());
    fireEvent.click(getByText("Download all missing (3)"));
    // No download yet — the kibitz-db FIRST_RUN_NOTICE text is shown.
    expect(twicDownload).not.toHaveBeenCalled();
    expect(getByText(NOTICE)).toBeTruthy();
    fireEvent.click(getByText("I understand — personal use only"));
    await waitFor(() => expect(twicAckNotice).toHaveBeenCalled());
    await waitFor(() => expect(twicDownload).toHaveBeenCalledWith([1652, 1651, 1649]));
  });

  it("skips the dialog once issues exist or it was acknowledged", async () => {
    const { getByText } = render(<TwicView progress={null} />);
    await waitFor(() => expect(twicCatalog).toHaveBeenCalled());
    fireEvent.click(getByText("Download all missing (3)"));
    await waitFor(() => expect(twicDownload).toHaveBeenCalled());
    expect(twicAckNotice).not.toHaveBeenCalled();
  });
});

describe("refresh + auto-download + progress", () => {
  it("Refresh catalog reports the honest HEAD request count", async () => {
    vi.mocked(twicRefreshCatalog).mockResolvedValue({ latestKnown: 1653, requests: 2 });
    const { getByText, container } = render(<TwicView progress={null} />);
    await waitFor(() => expect(twicCatalog).toHaveBeenCalled());
    fireEvent.click(getByText("Refresh catalog"));
    await waitFor(() => expect(twicRefreshCatalog).toHaveBeenCalled());
    expect(container.textContent).toContain("TWIC 1653");
    expect(container.textContent).toContain("2 HEAD requests");
    expect(twicCatalog).toHaveBeenCalledTimes(2); // reloaded after refresh
  });

  it("auto-download toggle persists via twic_set_auto_sync", async () => {
    const { container } = render(<TwicView progress={null} />);
    await waitFor(() => expect(twicCatalog).toHaveBeenCalled());
    const box = container.querySelector<HTMLInputElement>(".twic-auto input")!;
    expect(box.checked).toBe(false);
    fireEvent.click(box);
    await waitFor(() => expect(twicSetAutoSync).toHaveBeenCalledWith(true));
  });

  it("shows the inline job row with per-issue progress and cancel", async () => {
    const { container, getByText } = render(
      <TwicView
        progress={{
          kind: "twic",
          label: "TWIC download",
          done: 1,
          total: 3,
          detail: "downloading TWIC 1651…",
          active: true,
          error: null,
        queued: [],
        }}
      />,
    );
    await waitFor(() => expect(twicCatalog).toHaveBeenCalled());
    expect(getByText("DOWNLOADING TWIC")).toBeTruthy();
    expect(container.textContent).toContain("1 / 3");
    expect(container.textContent).toContain("downloading TWIC 1651…");
    expect(getByText("Cancel")).toBeTruthy();
  });
});
