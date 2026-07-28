/**
 * Settings (design/handoff-2 §Settings): single column, max-width 1080px,
 * grouped cards — mono uppercase group strip on --panel2, rows of
 * `230px label+help | 1fr value | 200px action`. Values are read-only
 * fields (mono for paths/keys/numbers); actions are bordered ghost
 * buttons.
 *
 * Every previously working setting stays wired (voice, annotation
 * display, treatment, theme, engine path, node budget). New: the two
 * database-wide batch rows (same confirm flow as the Database header) and
 * the recurring-commitment row (commitment_get/set; clear → nulls).
 *
 * Honesty rules: the spawn policy row states the engine-off default in
 * words (it is a product principle, not a setting); the LLM verbaliser
 * and piece-set rows are read-only because no backend setting exists;
 * batch estimates show the measured-or-assumed basis string verbatim.
 */
import { useCallback, useEffect, useState } from "react";
import type { ReactNode } from "react";
import ScreenHeader from "./shell/ScreenHeader";
import LichessSection from "./settings/LichessSection";
import UpdatesSettings from "./UpdatesSettings";
import {
  batchEstimate,
  batchStart,
  commitmentGet,
  commitmentSet,
  getSavedDbPath,
  jobsStatus,
  runJobs,
  type BatchEstimate,
  type BatchKind,
  type JobsStatus,
} from "./lib/db";
import { endgameOverview, type TbInfo } from "./lib/endgame";
import {
  getSavedEnginePath,
  getSavedNodes,
  resolveEnginePath,
  saveEnginePath,
  saveNodes,
} from "./lib/engine";
import { fmtDurationMs } from "./lib/home";
import { railNetBadges, twicCatalog, twicSetAutoSync } from "./lib/net";
import type { AnnotationMode, BoardTreatmentChoice, Theme, Voice } from "./lib/gameView";

interface SettingsViewProps {
  voice: Voice;
  onVoice: (v: Voice) => void;
  annotationMode: AnnotationMode;
  onAnnotationMode: (m: AnnotationMode) => void;
  treatment: BoardTreatmentChoice;
  onTreatment: (t: BoardTreatmentChoice) => void;
  theme: Theme;
  onTheme: (t: Theme) => void;
}

interface ConfirmState {
  kind: BatchKind;
  estimate: BatchEstimate;
}

/** One settings row: 230px label+help | 1fr value | 200px action. */
function Row({
  label,
  help,
  value,
  action,
}: {
  label: string;
  help: ReactNode;
  value: ReactNode;
  action?: ReactNode;
}) {
  return (
    <div className="set-row">
      <div>
        <div className="set-label">{label}</div>
        <div className="set-help">{help}</div>
      </div>
      <div className="set-value-cell">{value}</div>
      <div className="set-actions">{action}</div>
    </div>
  );
}

const ANNOTATION_CYCLE: Record<AnnotationMode, AnnotationMode> = {
  full: "hover",
  hover: "hidden",
  hidden: "full",
};

export default function SettingsView({
  voice,
  onVoice,
  annotationMode,
  onAnnotationMode,
  treatment,
  onTreatment,
  theme,
  onTheme,
}: SettingsViewProps) {
  const [enginePath, setEnginePath] = useState(getSavedEnginePath);
  const [nodes, setNodes] = useState(getSavedNodes);
  const [resolved, setResolved] = useState("");
  const [editEngine, setEditEngine] = useState(false);
  const [editNodes, setEditNodes] = useState(false);

  const [jobs, setJobs] = useState<JobsStatus | null>(null);
  const [tb, setTb] = useState<TbInfo | null>(null);
  const [confirm, setConfirm] = useState<ConfirmState | null>(null);
  const [estimating, setEstimating] = useState<BatchKind | null>(null);
  const [note, setNote] = useState<string | null>(null);

  // Recurring commitment (meta-backed; absent by default).
  const [commitLabel, setCommitLabel] = useState("");
  const [commitOpponent, setCommitOpponent] = useState("");
  const [commitNote, setCommitNote] = useState<string | null>(null);

  // Network-ingestion mirrors (run 9): configured sync accounts and the
  // TWIC auto-download toggle (same meta keys the TWIC screen writes).
  const [syncCount, setSyncCount] = useState<number | null>(null);
  const [twicAuto, setTwicAuto] = useState<boolean | null>(null);
  const [twicWeek, setTwicWeek] = useState<number | null>(null);

  useEffect(() => {
    let cancelled = false;
    resolveEnginePath(enginePath)
      .then((p) => !cancelled && setResolved(p))
      .catch((e) => !cancelled && setResolved(`unresolved: ${e}`));
    return () => {
      cancelled = true;
    };
  }, [enginePath]);

  useEffect(() => {
    jobsStatus()
      .then(setJobs)
      .catch(() => setJobs(null));
    endgameOverview()
      .then((o) => setTb(o.tablebase))
      .catch(() => setTb(null));
    commitmentGet()
      .then((c) => {
        setCommitLabel(c.label ?? "");
        setCommitOpponent(c.opponent ?? "");
      })
      .catch(() => {}); // no database open — the row stays empty
    railNetBadges()
      .then((b) => setSyncCount(b.accountsConfigured))
      .catch(() => setSyncCount(null));
    twicCatalog()
      .then((c) => {
        setTwicAuto(c.autoSync);
        setTwicWeek(c.latestImported);
      })
      .catch(() => setTwicAuto(null));
  }, []);

  const toggleTwicAuto = useCallback(async () => {
    if (twicAuto === null) return;
    try {
      await twicSetAutoSync(!twicAuto);
      setTwicAuto(!twicAuto);
    } catch (e) {
      setNote(`TWIC auto-download: ${e}`);
    }
  }, [twicAuto]);

  /* ---- batch operations (same confirm flow as the Database header) ---- */

  const askBatch = useCallback(async (kind: BatchKind) => {
    setEstimating(kind);
    setNote(null);
    try {
      setConfirm({ kind, estimate: await batchEstimate(kind) });
    } catch (e) {
      setNote(`Estimate failed: ${e}`);
    } finally {
      setEstimating(null);
    }
  }, []);

  const startBatch = useCallback(async () => {
    if (!confirm) return;
    const { kind } = confirm;
    setConfirm(null);
    try {
      const started = await batchStart(kind);
      setNote(
        `${kind === "annotate" ? "Annotate" : "Fresh analysis"}: ${started.gamesEnqueued} game(s), ` +
          `${started.jobsEnqueued} job(s) enqueued (already-covered games skipped).`,
      );
      // Enqueueing is passive; the worker is the user-initiated engine
      // entry point. Start it now — that is what the user just confirmed.
      await runJobs();
      jobsStatus()
        .then(setJobs)
        .catch(() => {});
    } catch (e) {
      setNote(`Batch start: ${e}`);
    }
  }, [confirm]);

  /* ---- commitment ---- */

  const saveCommitment = useCallback(async () => {
    try {
      const c = await commitmentSet(
        commitLabel.trim() === "" ? null : commitLabel.trim(),
        commitOpponent.trim() === "" ? null : commitOpponent.trim(),
      );
      setCommitLabel(c.label ?? "");
      setCommitOpponent(c.opponent ?? "");
      setCommitNote(c.label ? "Saved — Home names this commitment." : "Saved.");
    } catch (e) {
      setCommitNote(`Save failed: ${e}`);
    }
  }, [commitLabel, commitOpponent]);

  const clearCommitment = useCallback(async () => {
    try {
      await commitmentSet(null, null);
      setCommitLabel("");
      setCommitOpponent("");
      setCommitNote("Cleared.");
    } catch (e) {
      setCommitNote(`Clear failed: ${e}`);
    }
  }, []);

  const ghost = (label: string, onClick: () => void, disabled = false) => (
    <button className="btn-ghost" onClick={onClick} disabled={disabled}>
      {label}
    </button>
  );

  const jobsValue = jobs
    ? `${jobs.pending.toLocaleString("en-US")} pending · ${jobs.done.toLocaleString("en-US")} done`
    : "job queue unavailable";

  return (
    <>
      <ScreenHeader title="Settings" subtitle="Engine, coach, data and appearance" />
      <div className="page-scroll">
        <div className="settings-page">
          {/* ---- ENGINE & ANALYSIS ---- */}
          <div className="set-group">
            <div className="set-group-head">ENGINE &amp; ANALYSIS</div>
            <Row
              label="Engine path"
              help="Resolved automatically when the override is empty."
              value={
                editEngine ? (
                  <input
                    className="set-value mono editing"
                    type="text"
                    value={enginePath}
                    placeholder="resolved automatically if empty"
                    spellCheck={false}
                    autoFocus
                    onChange={(e) => {
                      setEnginePath(e.target.value);
                      saveEnginePath(e.target.value);
                    }}
                    onKeyDown={(e) => e.key === "Enter" && setEditEngine(false)}
                  />
                ) : (
                  <div className="set-value mono" title={resolved}>
                    {enginePath || resolved || "resolving…"}
                  </div>
                )
              }
              action={ghost(editEngine ? "Done" : "Edit", () => setEditEngine((v) => !v))}
            />
            <Row
              label="Node budget"
              help="Per analysis request; every engine job is bounded by it."
              value={
                editNodes ? (
                  <input
                    className="set-value mono editing"
                    type="number"
                    min={1}
                    value={nodes}
                    autoFocus
                    onChange={(e) => {
                      const n = parseInt(e.target.value, 10);
                      if (Number.isFinite(n) && n > 0) {
                        setNodes(n);
                        saveNodes(n);
                      }
                    }}
                    onKeyDown={(e) => e.key === "Enter" && setEditNodes(false)}
                  />
                ) : (
                  <div className="set-value mono">{nodes.toLocaleString("en-US")} nodes</div>
                )
              }
              action={ghost(editNodes ? "Done" : "Edit", () => setEditNodes((v) => !v))}
            />
            <Row
              label="Spawn policy"
              help="The engine is off by default and never runs behind your back: it starts only when a tactical screen fires, when you explicitly ask for analysis, or when you start a batch job."
              value={<div className="set-value">On explicit request only</div>}
            />
            <Row
              label="Annotate database"
              help="Static Kibitz pass over every game; fired alerts queue bounded engine confirmations. Resumable — pause anytime."
              value={<div className="set-value mono">{jobsValue}</div>}
              action={ghost(
                estimating === "annotate" ? "Estimating…" : "Estimate & run…",
                () => void askBatch("annotate"),
                estimating !== null,
              )}
            />
            <Row
              label="Fresh analysis pass"
              help="One bounded evaluation per mainline position of every game, through the job queue. Legacy analysis is kept, never deleted."
              value={<div className="set-value mono">{jobsValue}</div>}
              action={ghost(
                estimating === "fresh-analysis" ? "Estimating…" : "Estimate & run…",
                () => void askBatch("fresh-analysis"),
                estimating !== null,
              )}
            />
            {note && <div className="set-note">{note}</div>}
          </div>

          {/* ---- COACH ---- */}
          <div className="set-group">
            <div className="set-group-head">COACH</div>
            <Row
              label="Default voice"
              help="Applies to Explain, drill feedback and puzzle reasons. Also stored in the open database; annotations regenerate on the next annotate pass."
              value={<div className="set-value">{voice === "coach" ? "Coach" : "Neutral"}</div>}
              action={ghost(voice === "coach" ? "Switch to Neutral" : "Switch to Coach", () =>
                onVoice(voice === "coach" ? "neutral" : "coach"),
              )}
            />
            <Row
              label="LLM verbaliser"
              help="Optional and strictly grounded: it may only rewrite detector output, never add claims; on any failure Kibitz falls back to template prose silently. No key is stored by the app — set ANTHROPIC_API_KEY for the CLI explain-llm command."
              value={<div className="set-value mono">not set · template prose</div>}
            />
          </div>

          {/* ---- DATA ---- */}
          <div className="set-group">
            <div className="set-group-head">DATA</div>
            <Row
              label="Database"
              help="Opened automatically at launch; duplicates are linked, never deleted. Change it from the Database screen."
              value={<div className="set-value mono">{getSavedDbPath()}</div>}
            />
            <Row
              label="Account syncs"
              help="Lichess, chess.com and FICS sync live on the Account syncs screen (rail: DATA IN / OUT). CLI equivalents: lichess-sync · chesscom-sync · fics-sync."
              value={
                <div className="set-value mono">
                  {syncCount === null
                    ? "unknown (no database open)"
                    : syncCount === 0
                      ? "no accounts configured"
                      : `${syncCount} account${syncCount === 1 ? "" : "s"} configured`}
                </div>
              }
            />
            <Row
              label="TWIC auto-download"
              help="When on, the newest issues download quietly when the database opens (newest first, max 5 per app launch, strictly serial; older issues stay manual on the TWIC screen). Personal-use only — TWIC data is never redistributed. Mirrored on the TWIC ingest screen."
              value={
                <div className="set-value mono">
                  {twicAuto === null
                    ? "unknown (no database open)"
                    : `${twicAuto ? "on" : "off"}${twicWeek !== null ? ` · newest imported wk ${twicWeek}` : ""}`}
                </div>
              }
              action={
                twicAuto !== null &&
                ghost(twicAuto ? "Turn off" : "Turn on", () => void toggleTwicAuto())
              }
            />
            <Row
              label="Tablebase"
              help="Resolves from KIBITZ_SYZYGY, else a repo-local testdata/syzygy directory."
              value={
                <div className="set-value mono" title={tb?.note ?? undefined}>
                  {tb
                    ? tb.available
                      ? `Syzygy loaded${tb.largest != null ? ` · up to ${tb.largest} pieces` : ""}`
                      : "not found"
                    : "unknown (no database open)"}
                </div>
              }
            />
            <Row
              label="Schedule"
              help="A recurring commitment Home plans around, e.g. “Club night · Thursday”. The opponent name is optional."
              value={
                <div className="set-commit">
                  <input
                    className="set-value editing"
                    type="text"
                    value={commitLabel}
                    placeholder="commitment label"
                    spellCheck={false}
                    onChange={(e) => setCommitLabel(e.target.value)}
                    aria-label="Commitment label"
                  />
                  <input
                    className="set-value editing"
                    type="text"
                    value={commitOpponent}
                    placeholder="opponent (optional)"
                    spellCheck={false}
                    onChange={(e) => setCommitOpponent(e.target.value)}
                    aria-label="Commitment opponent"
                  />
                </div>
              }
              action={
                <>
                  {ghost("Save", () => void saveCommitment())}
                  {ghost("Clear", () => void clearCommitment())}
                </>
              }
            />
            {commitNote && <div className="set-note">{commitNote}</div>}
          </div>

          {/* ---- LICHESS PLAY (own file: settings/LichessSection.tsx, run 10) ---- */}
          <LichessSection />

          {/* ---- UPDATES (own file: UpdatesSettings.tsx, run-8 packaging) ---- */}
          <UpdatesSettings />

          {/* ---- APPEARANCE ---- */}
          <div className="set-group">
            <div className="set-group-head">APPEARANCE</div>
            <Row
              label="Theme"
              help="Dark is the default; light derives from the same token roles."
              value={<div className="set-value">{theme === "dark" ? "Dark" : "Light"}</div>}
              action={ghost(theme === "dark" ? "Switch to Light" : "Switch to Dark", () =>
                onTheme(theme === "dark" ? "light" : "dark"),
              )}
            />
            <Row
              label="Board treatment"
              help="Studio Walnut is the approved default; Instrument is the neutral alternate."
              value={
                <div className="set-value">
                  {treatment === "walnut" ? "Studio Walnut" : "Instrument"}
                </div>
              }
              action={ghost(
                treatment === "walnut" ? "Switch to Instrument" : "Switch to Walnut",
                () => onTreatment(treatment === "walnut" ? "instrument" : "walnut"),
              )}
            />
            <Row
              label="Annotation display"
              help="How PGN comments and variations render in the Moves panel: full, hover (dimmed until pointed at) or hidden."
              value={<div className="set-value">{annotationMode}</div>}
              action={ghost(`Switch to ${ANNOTATION_CYCLE[annotationMode]}`, () =>
                onAnnotationMode(ANNOTATION_CYCLE[annotationMode]),
              )}
            />
            <Row
              label="Piece set"
              help="The chessground cburnett set — fixed for now."
              value={<div className="set-value mono">cburnett</div>}
            />
          </div>
        </div>
      </div>

      {confirm && (
        <div className="modal-overlay" onClick={() => setConfirm(null)}>
          <div className="modal" onClick={(e) => e.stopPropagation()}>
            <div className="modal-title">
              {confirm.kind === "annotate" ? "Annotate database" : "Fresh analysis pass"}
            </div>
            <p className="modal-prose">
              {confirm.estimate.games.toLocaleString("en-US")} game
              {confirm.estimate.games === 1 ? "" : "s"} to cover
              {confirm.estimate.games > 0 &&
                ` · estimated ${fmtDurationMs(confirm.estimate.totalEstimateMs)}`}
              . Jobs are resumable — pause anytime and the run picks up exactly where it left off;
              games already covered are skipped.
            </p>
            <p className="modal-basis">Estimate basis: {confirm.estimate.estimateBasis}</p>
            <div className="modal-actions">
              <button className="btn-secondary" onClick={() => setConfirm(null)}>
                Cancel
              </button>
              <button
                className="btn-primary"
                onClick={() => void startBatch()}
                disabled={confirm.estimate.games === 0}
              >
                {confirm.estimate.games === 0 ? "Nothing to do" : "Start"}
              </button>
            </div>
          </div>
        </div>
      )}
    </>
  );
}
