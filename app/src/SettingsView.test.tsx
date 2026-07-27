// @vitest-environment jsdom
import { cleanup, fireEvent, render, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import SettingsView from "./SettingsView";
import { commitmentGet, commitmentSet } from "./lib/db";

/* IPC is mocked at the lib/db boundary; pure helpers are re-implemented
 * minimally so the component renders without a Tauri runtime. */
vi.mock("./lib/db", () => ({
  batchEstimate: vi.fn(() => Promise.reject(new Error("no db"))),
  batchStart: vi.fn(),
  commitmentGet: vi.fn(),
  commitmentSet: vi.fn(),
  getSavedDbPath: () => "testdata/corpus/scid.sqlite",
  jobsStatus: vi.fn(() => Promise.reject(new Error("no db"))),
  runJobs: vi.fn(),
}));
vi.mock("./lib/endgame", () => ({
  endgameOverview: vi.fn(() => Promise.reject(new Error("no db"))),
}));
vi.mock("./lib/net", () => ({
  railNetBadges: vi.fn(() => Promise.reject(new Error("no db"))),
  twicCatalog: vi.fn(() => Promise.reject(new Error("no db"))),
  twicSetAutoSync: vi.fn(),
}));
vi.mock("./lib/engine", () => ({
  getSavedEnginePath: () => "",
  getSavedNodes: () => 2_000_000,
  resolveEnginePath: vi.fn(() => Promise.resolve("/usr/local/bin/stockfish")),
  saveEnginePath: vi.fn(),
  saveNodes: vi.fn(),
}));

function renderSettings() {
  return render(
    <SettingsView
      voice="coach"
      onVoice={vi.fn()}
      annotationMode="full"
      onAnnotationMode={vi.fn()}
      treatment="walnut"
      onTreatment={vi.fn()}
      theme="dark"
      onTheme={vi.fn()}
    />,
  );
}

const labelField = (c: HTMLElement) =>
  c.querySelector<HTMLInputElement>('input[aria-label="Commitment label"]')!;
const opponentField = (c: HTMLElement) =>
  c.querySelector<HTMLInputElement>('input[aria-label="Commitment opponent"]')!;

beforeEach(() => {
  vi.mocked(commitmentGet).mockResolvedValue({ label: null, opponent: null });
  vi.mocked(commitmentSet).mockImplementation((label, opponent) =>
    Promise.resolve({ label, opponent }),
  );
});

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe("Settings — recurring commitment row (commitment_get/set)", () => {
  it("is absent by default: both fields empty when the store holds nulls", async () => {
    const { container } = renderSettings();
    await waitFor(() => expect(commitmentGet).toHaveBeenCalled());
    expect(labelField(container).value).toBe("");
    expect(opponentField(container).value).toBe("");
  });

  it("round-trips set: Save persists the typed label and opponent", async () => {
    const { container, getByText } = renderSettings();
    await waitFor(() => expect(commitmentGet).toHaveBeenCalled());
    fireEvent.change(labelField(container), { target: { value: "Club night · Thursday" } });
    fireEvent.change(opponentField(container), { target: { value: "R. Halvorsen" } });
    fireEvent.click(getByText("Save"));
    await waitFor(() =>
      expect(commitmentSet).toHaveBeenCalledWith("Club night · Thursday", "R. Halvorsen"),
    );
    // The row reflects the STORED state returned by the backend.
    expect(labelField(container).value).toBe("Club night · Thursday");
  });

  it("saves empty fields as nulls (a blank label clears that field)", async () => {
    const { container, getByText } = renderSettings();
    await waitFor(() => expect(commitmentGet).toHaveBeenCalled());
    fireEvent.change(labelField(container), { target: { value: "Club night" } });
    fireEvent.click(getByText("Save"));
    await waitFor(() => expect(commitmentSet).toHaveBeenCalledWith("Club night", null));
  });

  it("Clear nulls both fields in the store and empties the inputs", async () => {
    vi.mocked(commitmentGet).mockResolvedValue({
      label: "Club night · Thursday",
      opponent: "R. Halvorsen",
    });
    const { container, getByText } = renderSettings();
    await waitFor(() => expect(labelField(container).value).toBe("Club night · Thursday"));
    fireEvent.click(getByText("Clear"));
    await waitFor(() => expect(commitmentSet).toHaveBeenCalledWith(null, null));
    expect(labelField(container).value).toBe("");
    expect(opponentField(container).value).toBe("");
  });
});

describe("Settings — honesty rows", () => {
  it("states the engine-off default in words", async () => {
    const { container } = renderSettings();
    await waitFor(() => expect(commitmentGet).toHaveBeenCalled());
    expect(container.textContent).toContain("The engine is off by default");
    expect(container.textContent).toContain("On explicit request only");
  });

  it("shows the fixed cburnett piece set as a read-only row", async () => {
    const { container } = renderSettings();
    await waitFor(() => expect(commitmentGet).toHaveBeenCalled());
    expect(container.textContent).toContain("cburnett");
  });
});
