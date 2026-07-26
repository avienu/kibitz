/**
 * Settings (rail footer): narration voice, annotation display, board
 * treatment, theme, and the engine binary/nodes overrides. Persistence:
 * localStorage everywhere (+ the open database's meta table for voice via
 * set_narration_voice — the only settings IPC that exists).
 */
import { useEffect, useState } from "react";
import {
  getSavedEnginePath,
  getSavedNodes,
  resolveEnginePath,
  saveEnginePath,
  saveNodes,
} from "./lib/engine";
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

  useEffect(() => {
    let cancelled = false;
    resolveEnginePath(enginePath)
      .then((p) => !cancelled && setResolved(p))
      .catch((e) => !cancelled && setResolved(`unresolved: ${e}`));
    return () => {
      cancelled = true;
    };
  }, [enginePath]);

  const seg = <T extends string>(
    label: string,
    value: T,
    options: readonly { value: T; label: string }[],
    onPick: (v: T) => void,
    note?: string,
  ) => (
    <div className="settings-row">
      <div className="settings-label">{label}</div>
      <span className="seg" role="group" aria-label={label}>
        {options.map((o) => (
          <button key={o.value} className={value === o.value ? "cur" : ""} onClick={() => onPick(o.value)}>
            {o.label}
          </button>
        ))}
      </span>
      {note && <div className="settings-note">{note}</div>}
    </div>
  );

  return (
    <div className="page settings">
      <h2 className="page-title">Settings</h2>

      {seg(
        "Theme",
        theme,
        [
          { value: "dark", label: "Dark" },
          { value: "light", label: "Light" },
        ] as const,
        onTheme,
        "Dark is the default; light derives from the same token roles.",
      )}
      {seg(
        "Board treatment",
        treatment,
        [
          { value: "walnut", label: "Studio Walnut" },
          { value: "instrument", label: "Instrument" },
        ] as const,
        onTreatment,
        "Walnut is the approved default; Instrument is the neutral alternate.",
      )}
      {seg(
        "Narration voice",
        voice,
        [
          { value: "coach", label: "Coach" },
          { value: "neutral", label: "Neutral" },
        ] as const,
        onVoice,
        "Also stored in the open database; annotations regenerate on the next annotate pass.",
      )}
      {seg(
        "Annotation display",
        annotationMode,
        [
          { value: "full", label: "full" },
          { value: "hover", label: "hover" },
          { value: "hidden", label: "hidden" },
        ] as const,
        onAnnotationMode,
        "How PGN comments and variations render in the Moves panel.",
      )}

      <div className="settings-row">
        <div className="settings-label">Engine binary (optional override)</div>
        <input
          type="text"
          value={enginePath}
          placeholder="resolved automatically if empty"
          spellCheck={false}
          onChange={(e) => {
            setEnginePath(e.target.value);
            saveEnginePath(e.target.value);
          }}
        />
        <div className="settings-note">using: {resolved || "…"}</div>
      </div>
      <div className="settings-row">
        <div className="settings-label">Search nodes per analysis</div>
        <input
          type="number"
          min={1}
          value={nodes}
          onChange={(e) => {
            const n = parseInt(e.target.value, 10);
            if (Number.isFinite(n) && n > 0) {
              setNodes(n);
              saveNodes(n);
            }
          }}
        />
        <div className="settings-note">
          The engine stays off until a tactical screen fires or you explicitly ask.
        </div>
      </div>
    </div>
  );
}
