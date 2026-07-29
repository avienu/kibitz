# Change note — Explain panel is now bounded (supersedes round 1)

Give this to Claude Code alongside the round-1 README. It changes **only** the right pane
of the game view; nothing else in the round-1 spec moves.

## The problem
Explain grew to fill the right pane, squeezing the move list to a few rows and pushing its
own prose below the fold. It also had no way to get out of the way.

## The fix — four rules

1. **The pane has a definite height, not an intrinsic one.**
   Explain pane: `flex: 0 0 47%` when expanded, `flex: 0 0 auto` when collapsed, `min-height: 0`.
   Moves pane below it: `flex: 1 1 auto; min-height: 312px`.
   Do **not** implement this as `flex: 0 1 auto` + `max-height: 47%` — with an inner
   `flex: 1 1 0` body, the percentage never resolves and the prose scroller collapses to ~26px.

2. **The pane is three rows.**
   - header — `flex: 0 0 auto` (label, verdict pill, Coach/Neutral, collapse caret)
   - prose body — `flex: 1 1 0; min-height: 0; overflow: auto`
   - pinned foot — `flex: 0 0 auto`, top border
   The expander and the meta line live in the **pinned foot**. The way out of a bounded panel
   must never itself be inside the scroll region.

3. **Summary first.** Only the **first** finding renders. The rest sit behind a full-width
   ghost button in the pinned foot: `▾ N more findings — evidence is already on the board`
   (`background var(--panel2)`, `1px solid var(--line)`, radius 6, `500 11.5px Public Sans`,
   left-aligned). Expanded state is per-position and resets on every move step.
   The pinned foot must stay ~40px: the expander plus a **single non-wrapping** meta line
   (`white-space: nowrap`, ellipsis on the selection state). A two-line meta row starves the prose.

4. **Collapsing hides prose, never evidence.** The board overlay always shows the union of
   *all* findings for the position, expanded or not. A `▾`/`▸` ghost button at the right of the
   Explain header (`padding 5px 9px`, transparent, `1px solid var(--line)`, radius 5,
   `500 10px JetBrains Mono`) hides the body (`display: none`) and leaves header + verdict pill —
   the correct state for stepping quickly through a game.

## Acceptance check
At the default position, with Explain expanded, in the shipped app:
`body.scrollHeight <= body.clientHeight` — the headline and the first finding must be fully
readable without scrolling, and the expander must be visible without scrolling.
Collapsed: Explain ≈ 51px, Moves gets the remaining height.

## Also corrected
Board containers must be sized from the board's **total footprint**
(`size + 2×framePad + coordinate gutter`), not from `size` — sizing from `size` alone made the
walnut board overflow its card by ~12px.
