// @vitest-environment jsdom
/**
 * Profile build-out tests (round-2 spec): claim-param preselection, aside
 * retargeting on number click, evidence-row → game-at-ply wiring (mocked
 * navigation), and the lede naming the top finding from fixture data.
 */
import { cleanup, fireEvent, render, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import ProfileView from "./ProfileView";
import { PROFILE_FIXTURE as FIXTURE } from "./lib/profileFixture";

vi.mock("./lib/db", () => ({
  buildProfile: vi.fn().mockResolvedValue(null),
  cacheProfile: vi.fn().mockResolvedValue({ player: "sounix", builtAt: "2026-07-26 12:00:00" }),
  matchingPlayers: vi.fn().mockResolvedValue([]),
  getGame: vi.fn().mockImplementation((id: number) =>
    Promise.resolve({
      id,
      white: "sounix",
      black: `opp-${id}`,
      event: "Bergens SK",
      date: "2026.07.17",
      site: "?",
      round: null,
      result: "0-1",
      eco: null,
      openingName: null,
      whiteElo: null,
      blackElo: null,
      plyCount: 60,
      startFen: null,
      sans: [],
    }),
  ),
}));

function renderProfile(props: { claim?: string | null } = {}) {
  const onLoadGameAt = vi.fn();
  const onNavigate = vi.fn();
  const utils = render(
    <ProfileView
      profile={FIXTURE}
      onProfileBuilt={vi.fn()}
      onLoadGameAt={onLoadGameAt}
      claim={props.claim ?? null}
      opponent={null}
      onNavigate={onNavigate}
    />,
  );
  return { ...utils, onLoadGameAt, onNavigate };
}

afterEach(cleanup);

describe("Profile — lede", () => {
  it("names the top finding from the fixture in prose", () => {
    const { container } = renderProfile();
    const lede = container.querySelector(".pf2-lede");
    expect(lede).toBeTruthy();
    expect(lede!.textContent).toContain("exposed kings");
    expect(lede!.textContent).toContain("38%");
  });
});

describe("Profile — claim preselection (the `claim` navigation param)", () => {
  it("pre-targets the aside at the claimed cell", () => {
    const { getByText } = renderProfile({ claim: "motif:WeakKing:allowed" });
    expect(getByText("11 ALLOWED")).toBeTruthy(); // the count pill
  });

  it("falls back to the loudest motif when no claim is passed", () => {
    // WeakKing allowed (11) > missed (6): the default cell is allowed.
    const { getByText } = renderProfile();
    expect(getByText("11 ALLOWED")).toBeTruthy();
  });
});

describe("Profile — every number is a control", () => {
  it("clicking a structure bar retargets the aside", () => {
    const { getByText } = renderProfile({ claim: "motif:WeakKing:missed" });
    expect(getByText("6 MISSED")).toBeTruthy();
    fireEvent.click(getByText("own isolated pawn"));
    expect(getByText("22 GAMES")).toBeTruthy();
  });

  it("clicking a phase tile retargets the aside (honest empty example list)", () => {
    const { getByText } = renderProfile();
    fireEvent.click(getByText("MIDDLEGAME"));
    expect(getByText("200 MOVES")).toBeTruthy();
    expect(getByText(/no per-game example list/i)).toBeTruthy();
  });

  it("clicking a motif cell selects that exact cell's claim", () => {
    const { getByText } = renderProfile({ claim: "motif:WeakKing:allowed" });
    // The Undefended row's missed cell shows "3".
    fireEvent.click(getByText("3"));
    expect(getByText("3 MISSED")).toBeTruthy();
  });
});

describe("Profile — evidence rows open the game AT THE PLY", () => {
  it("row click calls onLoadGameAt(game, ply) from the claim's example", async () => {
    const { getByText, onLoadGameAt } = renderProfile({ claim: "motif:WeakKing:missed" });
    await waitFor(() => expect(getByText("ply 43")).toBeTruthy());
    fireEvent.click(getByText("ply 43"));
    expect(onLoadGameAt).toHaveBeenCalledWith(7, 43);
  });

  it("the Open game action opens the first supporting game at its ply", () => {
    const { getByText, onLoadGameAt } = renderProfile({ claim: "motif:WeakKing:allowed" });
    fireEvent.click(getByText("Open game"));
    expect(onLoadGameAt).toHaveBeenCalledWith(8, 29);
  });
});

describe("Profile — Train this weakness seeds the tactics queue", () => {
  it("navigates to tactics with the selected motif claim", () => {
    const { getByText, onNavigate } = renderProfile({ claim: "motif:WeakKing:missed" });
    fireEvent.click(getByText("Train this weakness"));
    expect(onNavigate).toHaveBeenCalledWith("tactics", { claim: "motif:WeakKing:missed" });
  });

  it("is disabled for non-motif claims (only motifs can seed the queue)", () => {
    const { getByText } = renderProfile();
    fireEvent.click(getByText("own isolated pawn"));
    expect((getByText("Train this weakness") as HTMLButtonElement).disabled).toBe(true);
  });
});
