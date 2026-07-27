# Handoff: Silman — board treatment (“Studio Walnut”) + game view

> **Note (2026-07):** “Silman” in this document is the working-era name of what is now **Kibitz**. The handoff is preserved verbatim as the design source of truth.


## Overview
This bundle specifies two things for the Silman desktop app (Tauri v2 + React, macOS/Linux):

1. **The board.** A chessground-compatible board *skin* — squares, frame, coordinates, piece shadows — plus a single systematized **evidence-overlay language** (alert targets, attacker/defender arrows, imbalance squares, key squares, last move) reused by Explain, Opponent prep, and Profile drill-down.
2. **The game view.** The densest screen in the app: nav rail → board (dominant) → Explain panel + Moves/annotations → persistent engine/jobs status strip.

Approved direction: **Studio Walnut** board (`1a`) as the default, **Instrument** (`1b`) as the neutral alternate for screenshots/lessons/light theme. **Graphite Depth** (`1c`) was rejected — do not build it.

## About the Design Files
The files in `reference/` are **design references written in HTML** — prototypes that show intended look and behavior. They are **not production code to copy**. `*.dc.html` files use a streaming-preview authoring runtime that does not exist in your app; ignore the `<x-dc>` / `sc-for` / `dc-import` machinery entirely.

Your task: **recreate these designs inside the Silman codebase’s existing environment** (React + TypeScript in the Tauri front end), using its established patterns — and, critically, **rendering the position with the real chessground instance already in the app**. The reference implements its own board renderer only because the prototype had no chessground available. Piece SVGs must not be redesigned.

## Fidelity
**High fidelity.** Colors, typography, spacing, radii, shadows and interaction states below are final and exact. Match them pixel-for-pixel. The only intentionally loose parts are: window chrome, real data, and anything explicitly marked “decide in code”.

---

## Screen 1 — Board component

### Purpose
Render any FEN with optional evidence overlays. Used at several sizes: 656px (game view), 442px (cards/lessons), smaller in trainers.

### Structure
```
shell (inline-block, line-height 0)
└─ frame        position:relative, padding = framePad, plus left/bottom coordinate gutter
   ├─ grid      position:relative, display:grid, 8×8 equal tracks, size×size, overflow:hidden
   │  ├─ 64 × square   position:relative, 100%×100%, flex centered
   │  │   ├─ 0..n overlay marks  (absolutely positioned, pointer-events:none)
   │  │   ├─ piece <img>          width/height = pieceScale (87%), centered
   │  │   └─ optional inner coordinate label
   │  └─ overlay <svg>  position:absolute, inset:0, 100%×100%, pointer-events:none,
   │                    overflow:visible — arrows drawn in board-pixel coordinates
   ├─ file labels (a–h) absolutely positioned along the bottom gutter, translateX(-50%)
   └─ rank labels (1–8) absolutely positioned along the left gutter, translateY(-50%)
```

### Geometry (as a function of `size`, the grid edge in px)
| Value | Studio Walnut (default) | Instrument (alternate) |
| --- | --- | --- |
| `framePad` | `round(size × 0.028)` (18px @ 656) | `0` |
| coordinate gutter | `round(size × 0.052)` (34px @ 656) | `round(size × 0.038)` |
| coordinate placement | on the frame, uppercase `A–H` | outside the grid, lowercase `a–h` |
| frame border-radius | `5px` | `2px` |
| grid border-radius | `2px` | `1px` |
| cell | `size / 8` | same |
| piece scale | `0.87` of cell (tweakable 0.72–1.0) | same |

### Studio Walnut — exact values (dark theme / light theme)
- Light square: `#e6cda2` / `#f3e2c0`
- Dark square: `#a5703e` / `#b98552`
- Frame: `linear-gradient(155deg,#4b3421 0%,#31210f 55%,#241708 100%)` / `linear-gradient(155deg,#8a6039 0%,#6d4726 100%)`
- Frame shadow (dark): `inset 0 0 0 1px rgba(255,232,190,0.16), inset 0 2px 0 rgba(255,236,198,0.14), 0 26px 60px -24px rgba(0,0,0,0.85)`
- Frame shadow (light): `inset 0 0 0 1px rgba(255,246,226,0.34), 0 20px 44px -22px rgba(78,50,20,0.5)`
- Grid ring (dark): `inset 0 0 0 1px rgba(38,22,8,0.55), 0 0 22px -6px rgba(0,0,0,0.6)`; (light): `inset 0 0 0 1px rgba(96,62,32,0.4)`
- Grain, applied per square as `background-image` **over** the base colour:
  `repeating-linear-gradient(<92deg on dark squares | 88deg on light squares>, rgba(66,38,12,0.055) 0 1px, rgba(255,255,255,0) 1px 4px), radial-gradient(140% 100% at 30% 0%, rgba(255,255,255,0.10), rgba(0,0,0,0) 70%)` — grain alpha `0.055` dark theme, `0.045` light
- Piece shadow: `filter: drop-shadow(0 3px 3px rgba(28,16,6,0.42))` / `drop-shadow(0 3px 3px rgba(90,60,26,0.32))`
- Coordinate colour: `#d8b989` / `#f6e9cd`, opacity `0.92`, font `600 <max(9, size×0.0225)>px "JetBrains Mono"`, letter-spacing `0.1em` on files

### Instrument — exact values (dark / light)
- Light square `#c4ccd1` / `#eaeef0`; dark square `#59666f` / `#8d9ba4`
- No frame, no grain. Per-square seam: `box-shadow: inset -1px -1px 0 rgba(0,0,0,0.06)`
- Grid ring (dark): `inset 0 0 0 1px rgba(226,236,242,0.14), 0 0 0 1px rgba(9,12,14,0.9)`; (light): `inset 0 0 0 1px rgba(52,66,76,0.22), 0 0 0 1px rgba(52,66,76,0.12)`
- Piece shadow: `drop-shadow(0 1px 1px rgba(0,0,0,0.34))` / `drop-shadow(0 1px 1px rgba(60,72,80,0.22))`
- Coordinate colour `#77848d` / `#6a767e`, opacity `0.75`

### Evidence-overlay language (the reusable system)
One meaning per colour, one shape per role. **Never** introduce a new colour or shape for a new surface.

| Role | Colour (line / fill) | Shape |
| --- | --- | --- |
| Alert target | `#e05c4b` / `rgba(224,92,75,0.20)` | Ring: `inset 9%`, `border: max(2, cell×0.045)px solid line`, `border-radius:50%`, `box-shadow: 0 0 <cell×0.18>px fill` |
| Attacker | `#e8a13c` / `rgba(232,161,60,0.18)` | Corner wedge `linear-gradient(135deg, fill 0 28%, transparent 28%)` + arrow |
| Defender | `#4f9ad8` / `rgba(79,154,216,0.16)` | Corner wedge (same geometry, no arrow) |
| Imbalance | `#5fb08a` / `rgba(95,176,138,0.20)` | Full-square wash |
| Key square (plan target) | `#a98bd4` / `rgba(169,139,212,0.20)` | Corner wedge |
| Last move | — / `rgba(238,206,102,0.24)` dark, `rgba(214,178,58,0.32)` light | Full-square wash + `inset 0 0 0 1px` of the same hue at 0.3/0.4 alpha |
| Selected square | — | `inset 0 0 0 max(2, cell×0.035)px rgba(255,255,255,0.8)` dark / `rgba(30,38,44,0.7)` light |

**Paint order per square (bottom → top):** base square → last-move wash → imbalance → key → defender → attacker → alert ring → selected ring → piece. Arrows draw above squares, below nothing (single SVG layer over the grid).

**Arrows** always point attacker → target, never the reverse. Drawn as filled polygons (no markers) in board-pixel space, with `u = cell / 100`:
- start offset from source centre `33u`, tip stops `33u` short of target centre
- head length `27u`, head half-width `17u`, shaft half-width `5.2u`
- fill = the role’s line colour; stroke = `rgba(10,13,15,0.5)` dark / `rgba(255,255,255,0.5)` light at `max(0.75, cell×0.016)`
- opacity = `0.42 + 0.44 × intensity`
- de-duplicate arrows by `from|to` — first role wins (alert/attacker before key)

**Intensity** is the single knob for “how loud”: overlays render at `intensity 0.44` by default and `1.0` for the currently hovered Explain sentence. Ring opacity = `0.42 + 0.5 × intensity`; wash opacity = `0.5 + 0.5 × intensity`; wedge opacity = `0.55 + 0.45 × intensity`.

### chessground mapping (do this instead of a custom renderer)
- Squares/frame/coords/grain: CSS on the chessground container and `cg-board` (`--cg-*` custom props or a `.silman-walnut` theme class); keep the piece sprites as-is.
- Alert rings, wedges, washes: chessground `customSvg` / `highlight` classes per square (`autoShapes`), or absolutely positioned square children — whichever your chessground version supports more cleanly.
- Arrows: chessground `drawable.autoShapes` with custom brushes named `alert` `attacker` `defender` `imbalance` `key`, using the hex values above.
- Board size must stay a multiple of 8 px to avoid seam rounding.

---

## Screen 2 — Game view (`1d`)

Shell: `1540 × 952`, `background: var(--bg)`, `border: 1px solid var(--line2)`, `radius 10px`, `overflow hidden`, `box-shadow 0 30px 70px -40px rgba(0,0,0,0.7)`. Horizontal flex: rail → main column.

### A. Nav rail — `flex: 0 0 204px`
`background var(--panel)`, `border-right 1px solid var(--line)`.
- Header block, padding `16px 16px 14px`, bottom border: wordmark `SILMAN` (`700 13px "JetBrains Mono"`, letter-spacing `0.16em`) and db line `400 10.5px "Public Sans"` `var(--faint)` — “scid.sqlite · 121,438 games”.
- Scrollable body, padding `12px 8px`. Group headings: `700 9.5px "JetBrains Mono"`, letter-spacing `0.16em`, `var(--faint)`, padding `8px 8px 6px` (`14px` top for subsequent groups).
- Items: flex row, space-between, padding `7px 9px`, radius `5px`, `500 12.5px "Public Sans"` `var(--dim)`; badge `500 9.5px "JetBrains Mono"` `var(--faint)`. Active item: `600` weight, `var(--tx)`, `background var(--panel3)`, `box-shadow inset 2px 0 0 var(--accent)`. Hover: `background var(--panel2)`.
- Groups and items (this list *is* the discoverability fix — every capability, including CLI-only ones, gets a home):
  - **STUDY** — Database `121k` · **Game** (active) · Opening tree · Position search
  - **COACH** — Explain `on` · Profile `9 findings` · Opponent prep
  - **TRAIN** — Openings SRS `24 due` · Tactics `12 due` · Endgames
  - **DATA IN / OUT** — Import PGN / SCID · TWIC ingest `wk 1601` · Account syncs `3` · Jobs `264`
  - footer group (separated by top border, padding `10px 8px 12px`) — Settings · Help & tour

### B. Header bar
Padding `13px 20px`, `background var(--panel)`, bottom border `var(--line)`, flex row, gap `18px`, centered.
- Title `600 15px "Public Sans"`; result `700 12px "JetBrains Mono"` `var(--dim)`.
- Meta line `400 11.5px "Public Sans"` `var(--faint)`, margin-top `4px`: “Paris, 1858 · Philidor Defence, C41 · 33 plies · personal > TWIC provenance”.
- Right: segmented board-treatment control (`walnut | instrument`), a `1px × 26px var(--line)` divider, then `Annotate` / `Re-analyze` / `Export PGN` buttons — padding `7px 12px`, `background var(--panel2)`, `1px solid var(--line2)`, radius `5px`, `500 11.5px`; hover `background var(--panel3)`.
- Segmented control: wrapper `background var(--panel2)`, `1px solid var(--line)`, radius `5px`, `overflow hidden`; segment padding `6px 11px`, `500 11px`; selected segment `background var(--accent)`, colour `#14181b`, weight `600`.

### C. Board column — `flex: 1`
`background: var(--desk)` = `radial-gradient(120% 90% at 50% -20%, #1b2024 0%, #0c0f11 70%)` dark / `radial-gradient(120% 90% at 50% -20%, #ffffff 0%, #eae7e2 70%)` light. Column flex, centered, gap `18px`, padding `22px 26px`.
- **Eval bar**, left of the board, gap `16px`: label `EVAL` (`700 9px "JetBrains Mono"` `var(--faint)`), track `width 12px`, full height, `background #2c343a` (dark in *both* themes — the track is Black’s share), `inset 0 0 0 1px var(--line2)`, radius `3px`; fill anchored bottom = White’s share, `height = clamp(6%, 50 + eval×9, 94%)`, `background #e8ecee` dark / `#fbfaf8` light, `box-shadow 0 -1px 0 rgba(0,0,0,0.35)`; numeric readout below, `700 10px "JetBrains Mono"` `var(--dim)`.
- **Board** at `size 656`, walnut, `pieceScale 0.87`.
- **Move controls** row, gap `10px`: a single grouped button bar (`background var(--panel2)`, `1px solid var(--line2)`, radius `7px`, `overflow hidden`, `1px var(--line)` dividers between buttons) with `|◀` `◀ Prev` `Next ▶` `▶|` (padding `9px 15/17px`, `500 12px`); then a ply readout pill `ply 24 / 33` (`500 11.5px "JetBrains Mono"` `var(--dim)`, same pill styling); then `Flip`; then a keyboard hint `400 11px` `var(--faint)`, max-width 200px: “← → step · ↑ ↓ jump 5 · f flip · e explain”.

### D. Right pane — `flex: 0 0 472px`
`background var(--panel)`, `border-left 1px solid var(--line)`. Two stacked panels; **Explain sits above Moves** so prose and notation share one column and one hover model.

**Explain panel** — `max-height 58%`, bottom border `var(--line)`.
- Header: padding `12px 16px`, bottom border; `EXPLAIN` (`700 10.5px "JetBrains Mono"`, letter-spacing `0.18em`); verdict pill (`700 9.5px "JetBrains Mono"`, letter-spacing `0.1em`, padding `4px 8px`, radius `4px`, `background var(--panel2)`, `inset 0 0 0 1px var(--line)`, colour `var(--accent)`, or `var(--bad)` when the tag is `FORCED MATE`) showing e.g. `TACTICAL SCREEN FIRED  +2.6`; right-aligned `Coach | Neutral` segmented control.
- Body: scrollable, padding `16px 18px 18px`.
  - Headline: `600 17px/1.42 "Source Serif 4"`, letter-spacing `-0.005em`, `text-wrap: pretty`, margin-bottom `14px`.
  - Sentence blocks: flex row, gap `11px`, padding `9px 11px`, negative `margin: 0 -11px`, radius `7px`, `transition: background 120ms ease, opacity 120ms ease`. Hovered: `background var(--panel2)`. Plan blocks additionally carry `inset 0 0 0 1px var(--line)`.
    - Role dot `7px` circle, margin-top `7px`: alert `var(--bad)`, imbalance `var(--good)`, plan `var(--violet)`; on hover add `0 0 0 4px <same hue at 0.18 alpha>`.
    - Kind label `700 9px "JetBrains Mono"`, letter-spacing `0.16em`, role colour, margin-bottom `5px`: `TACTICAL ALERT` / `IMBALANCE` / `PLAN`.
    - Prose `400 14px/1.6 "Source Serif 4"`, `var(--dim)`; plan prose `var(--tx)`.
  - Footer meta row: top border, `400 10.5px` `var(--faint)`, gap 14px, `·` separators — “Static screen · no engine spawned”, voice/source (“Coach voice · templates”), and selection state (“hover a line to isolate its evidence” / “filtered to d7”).
- **Empty state** (no screen fired on this ply): serif paragraph `400 14px/1.6` `var(--dim)` — “No screen has fired on this position. Silman keeps the engine cold until you ask, or until a tactical screen actually trips.” — then a primary button (`background var(--accent)`, colour `#14181b`, padding `9px 15px`, radius `6px`, `600 12px`) reading `Explain position E` with the shortcut glyph in mono at 0.65 opacity; below it, `400 11.5px` `var(--faint)` listing which plies already have explanations.

**Moves & annotations panel** — `flex: 1`, min-height 0.
- Header: `MOVES` label, right side segmented `full | hover | hidden` (the annotation display toggle) + a `Save` button (disabled look: transparent, `var(--faint)`, `1px solid var(--line)`).
- Body: scrollable, padding `8px 10px 16px`. One row per move pair: `display:grid; grid-template-columns: 34px 1fr 1fr; column-gap: 4px; padding: 1px 0`.
  - Move number: `500 12px "JetBrains Mono"` `var(--faint)`, right-aligned, padding `5px 4px 0 6px`.
  - Move cell (button): flex baseline row, gap `5px`, padding `5px 7px`, radius `5px`, `500 13px/1.3 "JetBrains Mono"` `var(--tx)`. Current ply: `700`, colour `#14181b`, `background var(--accent)`. Contains SAN, then NAG glyph (`700 12px` mono — `var(--bad)` for `? ?? ?!`, `var(--accent)` for `! !!`), then eval (`500 10.5px` mono `var(--faint)`).
  - Comment row: `grid-column: 2 / span 2`, margin `2px 0 8px`, padding `0 7px`, `400 12.5px/1.6 "Source Serif 4"` `var(--dim)`, `text-wrap: pretty`. In `hover` mode render at `opacity 0.42` and only bring to 1 on row hover; in `hidden` mode omit.
  - Variation row: `grid-column: 2 / span 2`, margin-bottom `9px`, padding `7px 9px`, `background var(--panel2)`, radius `6px`, tag (`500 10px` mono `var(--faint)`, letter-spacing `0.08em`) + line (`400 11.5px/1.55` mono `var(--dim)`).
    - **Fresh** analysis: `box-shadow inset 2px 0 0 var(--info)`, upright, full opacity, tag like `ENGINE d24`.
    - **Legacy (2011)** analysis: `box-shadow inset 2px 0 0 var(--faint)`, `font-style italic`, `opacity 0.72`, tag `LEGACY 2011`. Fresh always supersedes legacy in display order; legacy is retained, never deleted.

### E. Status strip (footer)
`background var(--panel2)`, top border `var(--line)`, `500 11px "JetBrains Mono"` `var(--dim)`; cells padded `9px 16px` and separated by `1px var(--line)` right borders.
1. Engine: `7px` status dot (`var(--faint)` idle, `var(--good)` running) + `ENGINE IDLE` + `var(--faint)` detail “Stockfish 18 · nodes 2,000,000”.
2. `JOBS` + “0 pending · 0 running · 264 done · 0 failed”.
3. Long-running job with progress: label `TWIC 1601`, a `5px` track (`var(--panel3)`, radius 3px) with `var(--good)` fill, and a `%` readout — this is the pattern for *every* batch operation (db-wide annotate, fresh-ACPL pass, syncs).
4. Right-aligned nudge: “Openings SRS · 24 due today”.

---

## Interactions & Behavior
- **Move stepping.** `→` / `←` = ±1 ply, `↓` / `↑` = ±5, `|◀` / `▶|` = start/end, clicking any move jumps to it. Keyboard is global to the game view (don’t swallow keys while a text input is focused). Ply readout `ply N / 33`.
- **Prose ⇄ board linkage (bidirectional).**
  - Hovering an Explain sentence raises *only that sentence’s* evidence to `intensity 1.0` and highlights the row; everything else drops to the `0.44` baseline.
  - Clicking a square selects it (click again to clear), draws the selected ring, and **filters the prose**: sentences that don’t reference that square drop to `opacity 0.34`. The footer meta updates to “filtered to <square>”.
  - Stepping a move clears both hover and selection.
- **Voice toggle** swaps every headline and sentence between `coach` (anthropomorphised: “the knight is doing three jobs at once”) and `neutral` (“Nd7 is attacked twice and defended twice”) strings. Same evidence either way — the overlay set must not change with voice.
- **Annotation display toggle** — `full` (comments + variations shown), `hover` (dimmed to 0.42, full on row hover), `hidden` (omitted).
- **Engine-off principle** is visible: positions without a fired screen show the empty state and an explicit `Explain position` (`E`) action; the status dot stays `ENGINE IDLE` until something actually runs.
- **Flip** (`f` or the button) mirrors squares, pieces, coordinates and arrow geometry together.
- Transitions: only the 120ms background/opacity fades on Explain rows and standard button hovers. No entrance animation on the board.

## State Management
```ts
type GameViewState = {
  ply: number;                  // 0..plyCount, index into the position list
  hoverSentence: number | null; // index into current explanation's blocks
  selectedSquare: string | null;// e.g. "d7"
  voice: 'coach' | 'neutral';
  annotationMode: 'full' | 'hover' | 'hidden';
  boardTreatment: 'walnut' | 'instrument';
  theme: 'dark' | 'light';
  flipped: boolean;
};
```
Derived per render: `position = positions[ply]` (FEN + last move), `explanation = explanations[ply] ?? null`, `evidence = hoverSentence != null ? blocks[hoverSentence].evidence : union(allBlocks.evidence)` (plus `lastMove` and `selectedSquare` merged in), `intensity = hoverSentence != null ? 1 : 0.44`.

Data the screen needs, per game: ordered plies with SAN/NAG/eval/comment/variations and a `legacy: boolean` per stored analysis; per-ply explanation objects `{ tag, eval, headline: {coach, neutral}, blocks: [{ kind, text: {coach, neutral}, evidence }] }` where `evidence = { alerts, attackers, defenders, imbalance, key, arrows: [{from, to, kind}] }`. Explanations are produced by the existing static screen — the UI must not synthesize them. Job/engine state feeds the status strip; keep it subscribed app-wide, not per screen.

## Design Tokens
```css
/* dark (default) */
--bg:#0e1113; --panel:#15191c; --panel2:#1a1f23; --panel3:#202629;
--line:rgba(233,241,246,0.09); --line2:rgba(233,241,246,0.16);
--tx:#e6ebee; --dim:#9ba5ac; --faint:#6c767d;
--accent:#d6a04a; --good:#6fb894; --bad:#de6f5e; --info:#6aa8dd; --violet:#a98bd4;
--desk:radial-gradient(120% 90% at 50% -20%,#1b2024 0%,#0c0f11 70%);

/* light — derived, same roles */
--bg:#f2f0ec; --panel:#fbfaf8; --panel2:#f1efeb; --panel3:#e9e6e1;
--line:rgba(24,30,34,0.10); --line2:rgba(24,30,34,0.20);
--tx:#1a1e21; --dim:#5e666b; --faint:#8b9297;
--accent:#a9762a; --good:#3f8a66; --bad:#c1503f; --info:#35709f; --violet:#7b5cad;
--desk:radial-gradient(120% 90% at 50% -20%,#ffffff 0%,#eae7e2 70%);
```
Evidence hues are **not** themed — they are semantic and identical in both themes (see the overlay table).

**Typography.** `Public Sans` — UI, labels, buttons (400/500/600/700). `Source Serif 4` — all prose: Explain headlines and sentences, PGN comments, marketing copy. `JetBrains Mono` — moves, evals, coordinates, counters, status strip, group headings. Sizes used: 9, 9.5, 10, 10.5, 11, 11.5, 12, 12.5, 13, 14, 15, 17px. Line-height 1 for labels/pills, 1.3–1.42 for tight UI, 1.55–1.6 for prose. Uppercase-with-tracking (`0.1–0.22em`) is reserved for mono labels.

**Spacing scale** (px): 1, 2, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 16, 18, 20, 22, 26, 34.
**Radii**: 1, 2, 3, 4, 5, 6, 7, 10px. **Shadows**: see per-component values above; the app shell uses `0 30px 70px -40px rgba(0,0,0,0.7)`.
**Prose rule**: every prose block gets `text-wrap: pretty`.

## Assets
- `reference/pieces/*.svg` — the **cburnett** piece set, copied from `lichess-org/lila@master:public/piece/cburnett/`. Already present in the app via chessground; use the app’s copy, do not re-import or restyle. Board treatments must work with these pieces unmodified.
- Fonts: Public Sans, Source Serif 4, JetBrains Mono (Google Fonts / SIL OFL). Bundle them locally — the desktop app must render offline.
- No other imagery.

## Screenshots
`screenshots/01-game-view-dark.png` — the `1d` game view, dark theme (the approved target).
`screenshots/02-game-view-light.png` — same screen, light theme derived from the tokens.
`screenshots/03-board-treatments-1a-1b.png` — Studio Walnut (`1a`, approved default) beside Instrument (`1b`, alternate), both showing the evidence overlays.
Screenshots are scaled captures of the reference HTML — treat the numbers in this README as authoritative over pixel-measuring the images.

## Files
- `reference/Silman Board Study.dc.html` — the full design: board treatments `1a` (approved), `1b` (alternate), `1c` (rejected), the evidence legend, and the `1d` game view. Contains a working SAN move engine and the sample annotated game (Morphy–Duke of Brunswick, Paris 1858) used for the mock; both are demo scaffolding, not app code.
- `reference/Board.dc.html` — the board renderer prototype. Read it for exact overlay geometry and palette maths; replace with chessground in the app.
- `reference/pieces/` — piece SVGs, for reference only.

## Out of scope in this bundle
Home-screen directions (coach-first vs library-first), Profile, Tactics trainer, Opponent prep, Settings, Help/first-run tour. The nav rail above reserves their entry points; those screens are still to be designed.
