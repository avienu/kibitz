// @vitest-environment jsdom
import { cleanup, fireEvent, render } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import EvidencePane, { type EvidenceGame } from "./EvidencePane";

const FORK_GAMES: EvidenceGame[] = [
  { id: 11, title: "sounix — christoforo", ply: 34, date: "2026.07.24" },
  { id: 12, title: "R. Halvorsen — sounix", ply: 41, date: "2026.07.17" },
];

const IQP_GAMES: EvidenceGame[] = [
  { id: 21, title: "sounix — M. Sæther", ply: 58, date: "2026.07.10" },
];

afterEach(cleanup);

describe("EvidencePane", () => {
  it("renders the count pill, intro, rows and footer", () => {
    const { getByText } = render(
      <EvidencePane
        countLabel="31 GAMES"
        intro="Knight forks you allowed, most recent first."
        games={FORK_GAMES}
        footerNote="Every number opens its supporting games here."
      />,
    );
    expect(getByText("31 GAMES")).toBeTruthy();
    expect(getByText("Knight forks you allowed, most recent first.")).toBeTruthy();
    expect(getByText("sounix — christoforo")).toBeTruthy();
    expect(getByText("ply 34")).toBeTruthy();
    expect(getByText("2026.07.24")).toBeTruthy();
    expect(getByText("Every number opens its supporting games here.")).toBeTruthy();
  });

  it("re-targets when the claim subject changes (props swap the list)", () => {
    const { getByText, queryByText, rerender } = render(
      <EvidencePane countLabel="31 GAMES" intro="Fork claims." games={FORK_GAMES} />,
    );
    expect(getByText("sounix — christoforo")).toBeTruthy();

    rerender(<EvidencePane countLabel="22 GAMES" intro="IQP games." games={IQP_GAMES} />);
    // Old subject fully gone, new subject fully in — the aside follows the claim.
    expect(queryByText("sounix — christoforo")).toBeNull();
    expect(queryByText("31 GAMES")).toBeNull();
    expect(getByText("22 GAMES")).toBeTruthy();
    expect(getByText("IQP games.")).toBeTruthy();
    expect(getByText("sounix — M. Sæther")).toBeTruthy();
    expect(getByText("ply 58")).toBeTruthy();
  });

  it("opens a game at its claim row", () => {
    const onOpenGame = vi.fn();
    const { getByText } = render(
      <EvidencePane
        countLabel="31 GAMES"
        intro="Fork claims."
        games={FORK_GAMES}
        onOpenGame={onOpenGame}
      />,
    );
    fireEvent.click(getByText("R. Halvorsen — sounix"));
    expect(onOpenGame).toHaveBeenCalledWith(FORK_GAMES[1]);
  });

  it("renders the actions slot and empty state", () => {
    const { getByText } = render(
      <EvidencePane
        countLabel="0 GAMES"
        intro="Nothing selected."
        games={[]}
        empty="Select a claim to see its games."
        actions={<button>Train this weakness</button>}
      />,
    );
    expect(getByText("Select a claim to see its games.")).toBeTruthy();
    expect(getByText("Train this weakness")).toBeTruthy();
  });
});
