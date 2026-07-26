import { useMemo, useState } from "react";
import { formatWhiteCp, legacyEvalTitle, type PlyEval } from "./lib/analyses";
import {
  ANNOTATION_DISPLAY_MODES,
  commentView,
  getSavedAnnotationDisplay,
  saveAnnotationDisplay,
  type AnnotationDisplay,
} from "./lib/annotationDisplay";
import { nagView } from "./lib/nags";
import {
  buildAnnView,
  commentAfter,
  cycleNag,
  deleteComment,
  deleteVariation,
  setComment,
  type AnnItem,
  type JsonToken,
  type MoveItem,
} from "./lib/tokens";

interface AnnotatedMovesProps {
  startFen: string;
  tokens: JsonToken[];
  /** Current mainline ply shown on the board. */
  currentPly: number;
  /** Stored engine evals keyed by mainline ply (White-POV), if loaded. */
  evals?: Map<number, PlyEval> | null;
  dirty: boolean;
  saving: boolean;
  onSelectPly: (ply: number) => void;
  onChange: (tokens: JsonToken[]) => void;
  onSave: () => void;
  onRevert: () => void;
}

/** One NAG rendered as a glyph / marker / invisible tooltip (verdict 2). */
function NagGlyph({ nag }: { nag: number }) {
  const v = nagView(nag);
  if (v.hidden) return <span className="nag nag-diagram" title={v.title} />;
  if (v.unknown) {
    return (
      <span className="nag nag-unknown" title={v.title}>
        {v.glyph}
      </span>
    );
  }
  return <span className="nag">{v.glyph}</span>;
}

interface Block {
  indent: number;
  items: AnnItem[];
}

/**
 * Annotated move list for database games: mainline with inline comments
 * (muted), NAG suffixes, and variations in parentheses on indented lines.
 * Selecting a move reveals its edit controls (comment, NAG cycle);
 * comments and variations carry their own delete buttons.
 */
export default function AnnotatedMoves({
  startFen,
  tokens,
  currentPly,
  evals,
  dirty,
  saving,
  onSelectPly,
  onChange,
  onSave,
  onRevert,
}: AnnotatedMovesProps) {
  const [selected, setSelected] = useState<number | null>(null);
  const [editing, setEditing] = useState<{ moveIndex: number; text: string } | null>(null);
  const [display, setDisplay] = useState<AnnotationDisplay>(getSavedAnnotationDisplay);

  const setDisplayMode = (mode: AnnotationDisplay) => {
    setDisplay(mode);
    saveAnnotationDisplay(mode);
  };

  const view = useMemo(() => buildAnnView(startFen, tokens), [startFen, tokens]);

  // Group items into lines: mainline flows inline; each top-level variation
  // gets its own indented line (nested variations stay inline within it).
  const blocks = useMemo(() => {
    const out: Block[] = [];
    let cur: Block = { indent: 0, items: [] };
    for (const item of view.items) {
      if (item.kind === "varStart" && item.depth === 1) {
        if (cur.items.length > 0) out.push(cur);
        cur = { indent: 1, items: [item] };
      } else if (item.kind === "varEnd" && item.depth === 1) {
        cur.items.push(item);
        out.push(cur);
        cur = { indent: 0, items: [] };
      } else {
        cur.items.push(item);
      }
    }
    if (cur.items.length > 0) out.push(cur);
    return out;
  }, [view]);

  const change = (next: JsonToken[], clearSelection = false) => {
    onChange(next);
    if (clearSelection) setSelected(null);
    setEditing(null);
  };

  const startEditComment = (moveIndex: number) => {
    const existing = commentAfter(tokens, moveIndex);
    setEditing({ moveIndex, text: existing?.text ?? "" });
  };

  const editedMove = editing
    ? (view.items.find((it) => it.kind === "move" && it.index === editing.moveIndex) as
        | MoveItem
        | undefined)
    : undefined;

  const renderMove = (item: MoveItem) => {
    const isCur = item.mainlinePly !== null && item.mainlinePly === currentPly;
    const isSel = item.index === selected;
    const ev = item.mainlinePly !== null ? evals?.get(item.mainlinePly) : undefined;
    return (
      <span key={item.index} className="ann-move-wrap">
        <button
          className={
            "ann-move" +
            (isCur ? " cur" : "") +
            (item.depth > 0 ? " in-var" : "") +
            (isSel ? " sel" : "")
          }
          onClick={() => {
            setSelected(isSel ? null : item.index);
            if (item.mainlinePly !== null) onSelectPly(item.mainlinePly);
          }}
        >
          {item.num ? `${item.num} ` : ""}
          {item.san}
          {item.nag !== null && <NagGlyph nag={item.nag} />}
        </button>
        {ev &&
          (ev.kind === "legacy" ? (
            <span className="ply-eval legacy" title={legacyEvalTitle(ev.engine)}>
              {formatWhiteCp(ev.whiteCp)}
            </span>
          ) : (
            <span className="ply-eval" title={ev.engine}>
              {formatWhiteCp(ev.whiteCp)}
            </span>
          ))}
        {isSel && (
          <span className="ann-controls">
            <button
              title="Edit or add a comment"
              onClick={() => startEditComment(item.index)}
            >
              ✎
            </button>
            <button
              title="Cycle NAG: none / ! / ? / !! / ?? / !? / ?!"
              onClick={() => change(cycleNag(tokens, item.index))}
            >
              !?
            </button>
          </span>
        )}
      </span>
    );
  };

  const renderItem = (item: AnnItem) => {
    switch (item.kind) {
      case "move":
        return renderMove(item);
      case "comment": {
        const cv = commentView(display, item.text);
        if (!cv.visible) return null;
        if (cv.collapsed) {
          return (
            <span key={item.index} className="ann-comment ann-comment-collapsed" title={cv.title}>
              {cv.text}
            </span>
          );
        }
        return (
          <span key={item.index} className="ann-comment">
            {cv.text}
            <button
              className="ann-x"
              title="Delete comment"
              onClick={() => change(deleteComment(tokens, item.index), true)}
            >
              ×
            </button>
          </span>
        );
      }
      case "varStart":
        return (
          <span key={item.index} className="ann-paren">
            (
            <button
              className="ann-x"
              title="Delete variation"
              onClick={() => change(deleteVariation(tokens, item.index), true)}
            >
              ×
            </button>
          </span>
        );
      case "varEnd":
        return (
          <span key={item.index} className="ann-paren">
            )
          </span>
        );
    }
  };

  return (
    <div className="ann">
      <div className="ann-header">
        <h3>Moves &amp; annotations</h3>
        <span className="ann-view-toggle" title="Comment display: full / hover (° marker) / hidden">
          {ANNOTATION_DISPLAY_MODES.map((mode) => (
            <button
              key={mode}
              className={display === mode ? "cur" : ""}
              onClick={() => setDisplayMode(mode)}
            >
              {mode}
            </button>
          ))}
        </span>
        {dirty && <span className="ann-dirty">unsaved changes</span>}
        <button onClick={onSave} disabled={!dirty || saving}>
          {saving ? "Saving…" : "Save"}
        </button>
        <button
          onClick={() => {
            onRevert();
            setSelected(null);
            setEditing(null);
          }}
          disabled={!dirty || saving}
        >
          Revert
        </button>
      </div>
      {view.error && <div className="error">{view.error}</div>}
      <div className="ann-moves">
        {blocks.map((block, bi) => (
          <div key={bi} className={block.indent > 0 ? "ann-line ann-var" : "ann-line"}>
            {block.items.map(renderItem)}
          </div>
        ))}
      </div>
      {editing && (
        <div className="ann-comment-edit">
          <div className="ann-comment-label">
            Comment on {editedMove ? `${editedMove.num ? `${editedMove.num} ` : ""}${editedMove.san}` : "move"}
          </div>
          <textarea
            value={editing.text}
            onChange={(e) => setEditing({ ...editing, text: e.target.value })}
            placeholder="Comment text (empty deletes the comment)…"
            spellCheck={false}
          />
          <div className="ann-comment-buttons">
            <button onClick={() => change(setComment(tokens, editing.moveIndex, editing.text))}>
              Set comment
            </button>
            <button onClick={() => setEditing(null)}>Cancel</button>
          </div>
        </div>
      )}
    </div>
  );
}
