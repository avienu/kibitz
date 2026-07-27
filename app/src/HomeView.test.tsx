// @vitest-environment jsdom
import { cleanup, fireEvent, render } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { HomeContent, type HomeData } from "./HomeView";
import type { HomeSummary } from "./lib/db";

/** All-empty fixture: fresh database, nothing due, nothing cached. */
const EMPTY_SUMMARY: HomeSummary = {
  lastGame: null,
  newGames: [],
  newGamesTotal: 0,
  findingsAvailable: false,
  findings: [],
  profilePlayer: null,
  profileBuiltAt: null,
  dueSrs: 0,
  dueTactics: null,
  runningJobs: { pending: 0, running: 0, done: 0, failed: 0, workerActive: false },
};

const FIXED_NOW = new Date(2026, 6, 26); // Sunday, 26 July

function renderHome(data: HomeData) {
  return render(
    <HomeContent
      data={data}
      batchFraction={null}
      onNavigate={vi.fn()}
      onOpenGame={vi.fn()}
      now={FIXED_NOW}
    />,
  );
}

afterEach(cleanup);

describe("Home — degraded state (maintainer ruling: short honest list)", () => {
  it("renders exactly the honest list when nothing is due at all", () => {
    const { container } = renderHome({
      summary: EMPTY_SUMMARY,
      commitment: null,
      prepState: [],
    });
    // The exact degraded render is pinned: date + the three honest lines,
    // no action cards, no findings panel, no invented widgets.
    expect(container.firstChild).toMatchSnapshot();
    expect(container.querySelector(".home-degraded")).toBeTruthy();
    expect(container.textContent).toContain("Nothing due today.");
    expect(container.textContent).toContain("No new games this week.");
    expect(container.textContent).toContain("to surface findings.");
    expect(container.querySelectorAll(".home-card")).toHaveLength(0);
    expect(container.querySelector(".home-lower")).toBeNull();
  });

  it("keeps the Continue card when a last game exists (not part of the degraded test)", () => {
    const { container, getByText } = renderHome({
      summary: {
        ...EMPTY_SUMMARY,
        lastGame: { id: 7, white: "sounix", black: "christoforo", ply: 21, openedAt: "2026-07-25 20:11:00" , flipped: false },
      },
      commitment: null,
      prepState: [],
    });
    expect(getByText("Resume review")).toBeTruthy();
    expect(container.querySelector(".home-degraded")).toBeTruthy();
  });

  it("degraded Build-a-profile line navigates to Profile", () => {
    const onNavigate = vi.fn();
    const { getByText } = render(
      <HomeContent
        data={{ summary: EMPTY_SUMMARY, commitment: null, prepState: [] }}
        batchFraction={null}
        onNavigate={onNavigate}
        onOpenGame={vi.fn()}
        now={FIXED_NOW}
      />,
    );
    fireEvent.click(getByText("Build a profile"));
    expect(onNavigate).toHaveBeenCalledWith("profile");
  });
});

describe("Home — commitment clause honesty", () => {
  // A non-degraded summary so the full layout renders.
  const withDue: HomeSummary = { ...EMPTY_SUMMARY, dueSrs: 24 };

  it("the clause is simply ABSENT when no commitment is set", () => {
    const { container } = renderHome({ summary: withDue, commitment: null, prepState: [] });
    expect(container.querySelector(".home-clause")).toBeNull();
    // The greeting date still renders alone.
    expect(container.querySelector(".home-date")?.textContent).toBe("Sunday, 26 July");
  });

  it("clause absent when the commitment row exists but the label is null", () => {
    const { container } = renderHome({
      summary: withDue,
      commitment: { label: null, opponent: "R. Halvorsen" },
      prepState: [],
    });
    expect(container.querySelector(".home-clause")).toBeNull();
  });

  it("'no prep started' only with a committed opponent who has no prep entry", () => {
    const commitment = { label: "Club night Thursday", opponent: "R. Halvorsen" };
    const unprepped = renderHome({ summary: withDue, commitment, prepState: [] });
    expect(unprepped.container.querySelector(".home-clause")?.textContent).toBe(
      "Club night Thursday — no prep started for R. Halvorsen yet.",
    );
    cleanup();
    const prepped = renderHome({
      summary: withDue,
      commitment,
      prepState: [{ opponent: "R. Halvorsen", color: "black", startedAt: "2026-07-20 19:00:00" }],
    });
    expect(prepped.container.querySelector(".home-clause")?.textContent).toBe(
      "Club night Thursday.",
    );
  });
});

describe("Home — honest numerals and navigation", () => {
  const summary: HomeSummary = {
    ...EMPTY_SUMMARY,
    dueSrs: 24,
    findingsAvailable: true,
    profilePlayer: "sounix",
    profileBuiltAt: "2026-07-25 08:00:00",
    findings: [
      { label: "Fork — allowed against you", value: "31", evidenceCount: 31, claimId: "motif:Fork:allowed" },
      { label: "IQP games", value: "38%", evidenceCount: 22, claimId: "structure:IQP" },
    ],
    newGames: [
      {
        id: 3,
        white: "sounix",
        black: "kasparovfan88",
        result: "1-0",
        source: "lichess",
        sourceKind: "online",
        importedAt: "2026-07-24 06:00:00",
      },
    ],
    newGamesTotal: 8,
  };

  it("shows the SRS due numeral but NEVER a tactics number (endless queue)", () => {
    const { container } = renderHome({ summary, commitment: null, prepState: [] });
    const nums = [...container.querySelectorAll(".home-due-num")].map((n) => n.textContent);
    expect(nums).toEqual(["24", "–"]); // tactics numeral is a grayed dash, not a number
    expect(container.querySelector(".home-due-num.muted")?.textContent).toBe("–");
  });

  it("findings rows navigate to Profile with the claim pre-selected", () => {
    const onNavigate = vi.fn();
    const { getByText } = render(
      <HomeContent
        data={{ summary, commitment: null, prepState: [] }}
        batchFraction={null}
        onNavigate={onNavigate}
        onOpenGame={vi.fn()}
        now={FIXED_NOW}
      />,
    );
    fireEvent.click(getByText("Fork — allowed against you"));
    expect(onNavigate).toHaveBeenCalledWith("profile", { claim: "motif:Fork:allowed" });
  });

  it("new-game rows carry the source tag tone and the week label", () => {
    const { container, getByText } = renderHome({ summary, commitment: null, prepState: [] });
    expect(getByText("lichess").classList.contains("violet")).toBe(true);
    // 2026-07-24 was a Friday.
    expect(container.textContent).toContain("NEW SINCE FRIDAY");
    expect(container.textContent).toContain("8 games this week");
  });

  it("prep Go navigates to Prep with the typed opponent", () => {
    const onNavigate = vi.fn();
    const { getByPlaceholderText, getByText } = render(
      <HomeContent
        data={{ summary, commitment: null, prepState: [] }}
        batchFraction={null}
        onNavigate={onNavigate}
        onOpenGame={vi.fn()}
        now={FIXED_NOW}
      />,
    );
    fireEvent.change(getByPlaceholderText("Search a name…"), {
      target: { value: "R. Halvorsen" },
    });
    fireEvent.click(getByText("Go"));
    expect(onNavigate).toHaveBeenCalledWith("prep", { opponent: "R. Halvorsen" });
  });
});
