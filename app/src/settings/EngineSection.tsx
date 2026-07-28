/**
 * Engine manager (run 10): the Settings group that owns everything about
 * the Stockfish binary and the Syzygy tablebases — resolved path, the
 * `id name` version string (verified by an explicit UCI handshake, never
 * a background spawn), the node budget, and the tablebase directory with
 * the same "not configured" honesty the endgame screen has.
 *
 * Self-contained by design: SettingsView only mounts it. Persistence
 * follows the existing localStorage pattern (lib/engine.ts): the engine
 * path and node budget ride the keys the analyze commands already read;
 * the tablebase dir is pushed to the backend on save and at app launch.
 */
import { useCallback, useEffect, useState } from "react";
import type { ReactNode } from "react";
import type { TbInfo } from "../lib/endgame";
import {
  engineIdentify,
  getSavedEnginePath,
  getSavedNodes,
  getSavedTbDir,
  resolveEnginePath,
  saveEnginePath,
  saveNodes,
  saveTbDir,
  setTablebaseDir,
  tablebaseStatus,
} from "../lib/engine";
import {
  parseNodesInput,
  tbStatusLine,
  verifiedLine,
  verifyFailedLine,
} from "../lib/engineSettings";

/** Same row grid as SettingsView's Row (230px label | 1fr | 200px). */
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

export default function EngineSection() {
  const [enginePath, setEnginePath] = useState(getSavedEnginePath);
  const [resolved, setResolved] = useState<string | null>(null);
  const [resolveError, setResolveError] = useState<string | null>(null);
  const [verifying, setVerifying] = useState(false);
  const [verifyLine, setVerifyLine] = useState<string | null>(null);

  const [nodesText, setNodesText] = useState(() => String(getSavedNodes()));
  const [tbDir, setTbDir] = useState(getSavedTbDir);
  const [tb, setTb] = useState<TbInfo | null>(null);
  const [tbNote, setTbNote] = useState<string | null>(null);

  // Resolve the effective binary whenever the override changes; the
  // verification line belongs to a specific binary, so it resets too.
  useEffect(() => {
    let cancelled = false;
    setVerifyLine(null);
    resolveEnginePath(enginePath)
      .then((p) => {
        if (cancelled) return;
        setResolved(p);
        setResolveError(null);
      })
      .catch((e) => {
        if (cancelled) return;
        setResolved(null);
        setResolveError(String(e));
      });
    return () => {
      cancelled = true;
    };
  }, [enginePath]);

  useEffect(() => {
    tablebaseStatus()
      .then(setTb)
      .catch(() => setTb(null));
  }, []);

  const doVerify = useCallback(async () => {
    setVerifying(true);
    setVerifyLine(null);
    try {
      const id = await engineIdentify(enginePath);
      setVerifyLine(verifiedLine(id));
      // A binary that passed the handshake is safe to persist.
      saveEnginePath(enginePath);
    } catch (e) {
      setVerifyLine(verifyFailedLine(e));
    } finally {
      setVerifying(false);
    }
  }, [enginePath]);

  const onEnginePathChange = useCallback((v: string) => {
    setEnginePath(v);
    // Persist immediately (the pre-run-10 behavior); Verify additionally
    // proves the binary speaks UCI.
    saveEnginePath(v);
  }, []);

  const nodesParsed = parseNodesInput(nodesText);
  const onNodesCommit = useCallback(() => {
    const n = parseNodesInput(nodesText);
    if (n !== null) {
      saveNodes(n);
      setNodesText(String(n));
    } else {
      setNodesText(String(getSavedNodes()));
    }
  }, [nodesText]);

  const applyTbDir = useCallback(async () => {
    try {
      const info = await setTablebaseDir(tbDir);
      setTb(info);
      saveTbDir(tbDir);
      setTbNote(
        info.available ? "Saved — tables loaded." : "Saved, but no usable tables at that path.",
      );
    } catch (e) {
      setTbNote(`Tablebase dir: ${e}`);
    }
  }, [tbDir]);

  return (
    <div className="set-group">
      <div className="set-group-head">ENGINE MANAGER</div>
      <Row
        label="Engine binary"
        help="Resolution order: this override, then KIBITZ_STOCKFISH, the repo binary, and stockfish on PATH. Verify runs the UCI handshake only — no search, no background spawn."
        value={
          <div className="set-stack">
            <input
              className="set-value mono editing"
              type="text"
              value={enginePath}
              placeholder="resolved automatically if empty"
              spellCheck={false}
              onChange={(e) => onEnginePathChange(e.target.value)}
            />
            <div className="set-subline mono" title={resolved ?? resolveError ?? undefined}>
              {resolved !== null
                ? `resolves to ${resolved}`
                : (resolveError ?? "resolving…")}
            </div>
            {verifyLine && (
              <div
                className={`set-subline${verifyLine.startsWith("Not usable") ? " bad" : " good"}`}
              >
                {verifyLine}
              </div>
            )}
          </div>
        }
        action={
          <button
            className="btn-ghost"
            onClick={() => void doVerify()}
            disabled={verifying || resolved === null}
            title="Spawn the binary, run the uci handshake, read its id name, quit"
          >
            {verifying ? "Verifying…" : "Verify & get version"}
          </button>
        }
      />
      <Row
        label="Node budget"
        help="Per analysis request; every engine job is bounded by it. Separators are fine (2,000,000)."
        value={
          <input
            className={`set-value mono editing${nodesParsed === null ? " invalid" : ""}`}
            type="text"
            inputMode="numeric"
            value={nodesText}
            spellCheck={false}
            onChange={(e) => setNodesText(e.target.value)}
            onBlur={onNodesCommit}
            onKeyDown={(e) => e.key === "Enter" && onNodesCommit()}
            aria-label="Node budget"
          />
        }
      />
      <Row
        label="Syzygy tablebases"
        help="Directory of .rtbw/.rtbz files for the endgame trainer. Empty = automatic (KIBITZ_SYZYGY, else the repo's testdata/syzygy). An explicit path is never silently substituted."
        value={
          <div className="set-stack">
            <input
              className="set-value mono editing"
              type="text"
              value={tbDir}
              placeholder="resolved automatically if empty"
              spellCheck={false}
              onChange={(e) => setTbDir(e.target.value)}
              onKeyDown={(e) => e.key === "Enter" && void applyTbDir()}
            />
            <div className="set-subline mono" title={tb?.note ?? undefined}>
              {tbStatusLine(tb, getSavedTbDir())}
            </div>
            {tbNote && <div className="set-subline">{tbNote}</div>}
          </div>
        }
        action={
          <button className="btn-ghost" onClick={() => void applyTbDir()}>
            Apply
          </button>
        }
      />
    </div>
  );
}
