/**
 * Moves & annotations panel (design/handoff-1 §D + deliverable 2b): the
 * move-pair grid with NAG glyphs, per-move evals, serif comment rows and
 * FRESH/LEGACY variation blocks — plus in-place annotation editing for
 * database games (comment edit/delete, NAG picker, variation delete).
 * All edits round-trip through the encoding-v2 tokens (lib/tokens.ts →
 * update_game_tokens); the panel never invents storage.
 */
import {useState, useEffect, useRef} from "react";
import { formatWhiteCp, legacyEvalTitle, type PlyEval } from "./lib/analyses";
import type { AnnotationMode } from "./lib/gameView";
import { nagTone, type MovesRow, type PairCell } from "./lib/movesView";
import { nagView } from "./lib/nags";
import type { RepGlyph } from "./lib/repMarks";
import {
  deleteComment,
  deleteVariation,
  setComment,
  setNag,
  type JsonToken,
} from "./lib/tokens";

/** NAG picker choices (design order): ! !! ? ?? !? ?! + clear. */
const NAG_CHOICES: { value: number; glyph: string }[] = [
  { value: 1, glyph: "!" },
  { value: 3, glyph: "!!" },
  { value: 2, glyph: "?" },
  { value: 4, glyph: "??" },
  { value: 5, glyph: "!?" },
  { value: 6, glyph: "?!" },
];

export interface MovesEditing {
  tokens: JsonToken[];
  /** Generated coach narrations by mainline ply (display-only). */
  narrations?: ReadonlyMap<number, string>;
  onChange: (tokens: JsonToken[]) => void;
  dirty: boolean;
  saving: boolean;
  onSave: () => void;
  onRevert: () => void;
}

interface MovesPanelProps {
  rows: MovesRow[];
  currentPly: number;
  evals: Map<number, PlyEval> | null;
  annotationMode: AnnotationMode;
  onAnnotationMode: (m: AnnotationMode) => void;
  onSelectPly: (ply: number) => void;
  /** Null when the loaded game has no editable annotation stream. */
  editing: MovesEditing | null;
  /** Repertoire marks per mainline ply (run-9); null/empty renders
   * nothing — marks only exist when a repertoire has cards. */
  repGlyphs?: Map<number, RepGlyph> | null;
}

interface CommentDraft {
  /** Token index of the comment being edited, or the move token index
   * when adding a new comment (`isNew`). */
  index: number;
  isNew: boolean;
  text: string;
}

export default function MovesPanel({
  rows,
  currentPly,
  evals,
  annotationMode,
  onAnnotationMode,
  onSelectPly,
  editing,
  repGlyphs,
}: MovesPanelProps) {
  // Follow the game: keep the current move visible while stepping
  // (run-8 user report — the panel lost you mid-game).
  const bodyRef = useRef<HTMLDivElement | null>(null);
  useEffect(() => {
    const body = bodyRef.current;
    if (!body) return;
    const cur = body.querySelector<HTMLElement>(".mv-cell.cur");
    if (!cur) return;
    const b = body.getBoundingClientRect();
    const c = cur.getBoundingClientRect();
    if (c.top < b.top + 8 || c.bottom > b.bottom - 8) {
      cur.scrollIntoView({ block: "nearest" });
    }
  }, [currentPly]);

  const [draft, setDraft] = useState<CommentDraft | null>(null);
  /** Move token index the NAG popover is open for. */
  const [nagFor, setNagFor] = useState<number | null>(null);

  const change = (tokens: JsonToken[]) => {
    editing?.onChange(tokens);
    setDraft(null);
    setNagFor(null);
  };

  const commitDraft = () => {
    if (!draft || !editing) return;
    if (draft.isNew) {
      change(setComment(editing.tokens, draft.index, draft.text));
    } else {
      const t = draft.text.trim();
      change(
        t
          ? editing.tokens.map((tok, i): JsonToken =>
              i === draft.index ? { t: "comment", text: t } : tok,
            )
          : deleteComment(editing.tokens, draft.index),
      );
    }
  };

  const moveCell = (cell: PairCell | null, ellipsis: boolean) => {
    if (!cell) return <span className="mv-cell mv-empty">{ellipsis ? "…" : ""}</span>;
    const cur = cell.ply === currentPly;
    const ev = evals?.get(cell.ply);
    const nag = cell.nag !== null ? nagView(cell.nag) : null;
    const rep = repGlyphs?.get(cell.ply);
    return (
      <span className="mv-cell-wrap">
        <button
          className={`mv-cell${cur ? " cur" : ""}`}
          onClick={() => {
            onSelectPly(cell.ply);
            // Second activation on the current move opens the NAG picker.
            if (cur && editing && cell.tokenIndex !== null) {
              setNagFor((n) => (n === cell.tokenIndex ? null : cell.tokenIndex));
            }
          }}
          onContextMenu={(e) => {
            if (editing && cell.tokenIndex !== null) {
              e.preventDefault();
              setNagFor((n) => (n === cell.tokenIndex ? null : cell.tokenIndex));
            }
          }}
          title={editing ? "click: go to move · click again / right-click: annotate" : undefined}
        >
          {cell.san}
          {nag && !nag.hidden && (
            <span
              className={`mv-nag mv-nag-${cell.nag !== null ? nagTone(cell.nag) : "plain"}${nag.unknown ? " unknown" : ""}`}
              title={nag.title}
            >
              {nag.glyph}
            </span>
          )}
          {ev && (
            <span
              className={`mv-eval${ev.kind === "legacy" ? " legacy" : ""}`}
              title={ev.kind === "legacy" ? legacyEvalTitle(ev.engine) : ev.engine}
            >
              {formatWhiteCp(ev.whiteCp)}
            </span>
          )}
          {rep && (
            <span className={`mv-rep mv-rep-${rep.kind}`} title={rep.title}>
              {rep.kind === "match" ? "✓" : "≠"}
            </span>
          )}
        </button>
        {editing && nagFor !== null && nagFor === cell.tokenIndex && (
          <span className="nag-pop" role="menu">
            {NAG_CHOICES.map((c) => (
              <button
                key={c.value}
                className={`mv-nag-${nagTone(c.value)}`}
                onClick={() => change(setNag(editing.tokens, cell.tokenIndex!, c.value))}
              >
                {c.glyph}
              </button>
            ))}
            <button onClick={() => change(setNag(editing.tokens, cell.tokenIndex!, null))}>
              clear
            </button>
            <button
              onClick={() => {
                setNagFor(null);
                setDraft({ index: cell.tokenIndex!, isNew: true, text: "" });
              }}
            >
              comment
            </button>
          </span>
        )}
      </span>
    );
  };

  const commentEditor = (
    <div className="mv-comment-edit">
      <textarea
        autoFocus
        value={draft?.text ?? ""}
        placeholder="Comment (empty deletes) — Enter commits, Esc cancels"
        spellCheck={false}
        onChange={(e) => draft && setDraft({ ...draft, text: e.target.value })}
        onKeyDown={(e) => {
          if (e.key === "Enter" && !e.shiftKey) {
            e.preventDefault();
            commitDraft();
          } else if (e.key === "Escape") {
            e.preventDefault();
            setDraft(null);
          }
        }}
      />
    </div>
  );

  const body = rows.map((row, i) => {
    switch (row.kind) {
      case "pair": {
        // A fresh "add comment" draft renders right under its move's row.
        const draftHere =
          draft?.isNew &&
          (row.white?.tokenIndex === draft.index || row.black?.tokenIndex === draft.index);
        return (
          <div key={i}>
            <div className="mv-row">
              <span className="mv-num">{row.num}.</span>
              {moveCell(row.white, row.whiteEllipsis)}
              {moveCell(row.black, false)}
            </div>
            {draftHere && (
              <div className="mv-row">
                <span className="mv-num" />
                <div className="mv-comment-span">{commentEditor}</div>
              </div>
            )}
          </div>
        );
      }
      case "comment": {
        if (annotationMode === "hidden") return null;
        const isEditing = draft && !draft.isNew && draft.index === row.tokenIndex;
        return (
          <div key={i} className={`mv-row mv-has-comment mode-${annotationMode}`}>
            <span className="mv-num" />
            <div className="mv-comment-span">
              {isEditing ? (
                commentEditor
              ) : (
                <div
                  className={`mv-comment${editing ? " editable" : ""}`}
                  title={editing ? "click to edit" : undefined}
                  onClick={() =>
                    editing && setDraft({ index: row.tokenIndex, isNew: false, text: row.text })
                  }
                >
                  {row.text}
                </div>
              )}
            </div>
          </div>
        );
      }
      case "narration": {
        if (annotationMode === "hidden") return null;
        return (
          <div key={i} className={`mv-row mv-has-comment mode-${annotationMode}`}>
            <span className="mv-num" />
            <div className="mv-comment-span">
              <div
                className="mv-narration"
                title="Generated by Annotate — regenerates on re-annotation; not hand-editable"
              >
                <span className="mv-narration-tag">COACH</span>
                {row.text}
              </div>
            </div>
          </div>
        );
      }
      case "variation": {
        if (annotationMode === "hidden") return null;
        return (
          <div key={i} className={`mv-row mode-${annotationMode}`}>
            <span className="mv-num" />
            <div className={`mv-var mv-var-${row.style}`}>
              <span className="mv-var-tag">{row.tag}</span>
              <span className="mv-var-line">{row.line}</span>
              {editing && (
                <button
                  className="mv-x"
                  title="Delete variation"
                  onClick={() => change(deleteVariation(editing.tokens, row.varStartIndex))}
                >
                  ×
                </button>
              )}
            </div>
          </div>
        );
      }
    }
  });

  return (
    <section className="moves-panel">
      <header className="panel-header">
        <span className="panel-label">MOVES</span>
        <span className="seg" role="group" aria-label="Annotation display">
          {(["full", "hover", "hidden"] as const).map((m) => (
            <button key={m} className={annotationMode === m ? "cur" : ""} onClick={() => onAnnotationMode(m)}>
              {m}
            </button>
          ))}
        </span>
        {editing && editing.dirty && (
          <button className="btn-ghost" onClick={editing.onRevert} disabled={editing.saving}>
            Revert
          </button>
        )}
        <button
          className={`btn-save${editing?.dirty ? " dirty" : ""}`}
          onClick={() => editing?.onSave()}
          disabled={!editing?.dirty || editing.saving}
        >
          {editing?.saving ? "Saving…" : "Save"}
        </button>
      </header>
      <div className="moves-body" ref={bodyRef}>
        {body}
        {rows.length === 0 && <div className="mv-empty-note">No game loaded.</div>}
      </div>
    </section>
  );
}
