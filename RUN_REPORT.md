# Run 7 — 2026-07-26

## Headline

**All ten round-2 screens are built and merged** — the high-fidelity five
(Home A, Database, Profile, Opponent prep, Tactics) and the simplified
five (Opening tree, Position search, Openings SRS, Endgames, Settings +
Help & tour) — on exactly the five shared components the pattern budget
allows, with every maintainer ruling honored: ECO names resolve
everywhere the design shows one, Home ships Direction A only with its
degraded state snapshot-pinned, the commitment line is a real Settings
row that is simply absent when unset, and both batch operations have
homes with an estimate-confirm flow that quotes its own measurement
basis. 506 tests green (232 Rust incl. src-tauri + 274 frontend).
Screenshots: BLOCKED by a machine display/session state — see the
notes at the end; the harness is ready and the capture is a one-click
assist away.

## Backend contracts (committed first, everything typed and tested)

- **ECO names** (`eco_names`, `get_game.openingName`, prep fingerprint
  and weak-line names) from the bundled CC0 dataset — the run-6
  "codes-only" deviation is dead.
- **Profile evidence plies**: every example is now {game, ply} — the
  ply that produced the claim — so "click a number → open the game at
  the exact moment" is real end to end.
- **SRS previews**: due cards carry per-grade next intervals computed
  by the actual FSRS scheduler, proven equal (to 1e-12) to what grading
  then does.
- **Endgame verdict rows**: winning / slower (with stated DTZ cost) /
  throws / unverified, graded ONLY from silman-tb probes; defender
  replies are ENGINE rows. Tablebase-gated tests cover all three
  user verdicts.
- **Batch operations**: `batch_estimate` measures the static annotate
  rate live on a read-only sample (never mutating, never spawning an
  engine — spawn-count asserted); fresh-analysis estimates from a
  documented assumed rate and SAYS so (`estimateBasis`, surfaced
  verbatim in the confirm dialog). `batch_start` enqueues idempotently;
  pause is cooperative; the queue was already resumable.
- **Honest home data**: `home_summary` findings come only from a cached
  profile; `dueTactics` is always null because an endless
  weakness-weighted queue has no honest due count — the UI grays the
  numeral rather than inventing one. Commitment and prep-state are
  meta-backed settings with absent-by-default round-trip tests.

## The screens (selected verification highlights)

- **Home A**: commitment clause absent-when-unset tested four ways;
  "no prep started for X yet" appears only when a prep for X genuinely
  does not exist; the degraded state renders the short honest list and
  is snapshot-pinned. No Direction B anywhere; no home switch in state.
- **Profile**: every number is a control (motif cells, structure bars,
  phase/conversion tiles); the aside retargets; rows open the game at
  the claim ply; "Train this weakness" restricts the tactics weakness
  weights to the claim's motif (tested with a fixture profile —
  `nextPuzzle` receives only the seeded kind). Peer-baseline columns
  that have no data source show "—" with an explanation, not numbers.
- **Prep**: stepper persists selections and back-navigates freely;
  step-2 entry records prep-state so Home's greeting stays truthful;
  Lichess/chess.com fetch buttons are DISABLED with the reason (no
  sync IPC exists — CLI-only), stated in the serif footnote.
- **Tactics**: the puzzle board never receives evidence overlays
  (asserted in a test); the clock renders only in timed modes; the
  reasoning aside shares the app-level voice state with Explain.
- **SRS**: grade buttons show the real scheduler's intervals; 1–4/⏎
  keyboard with the focused-input exception, tested.
- **Endgames**: verdict rows styled per spec with DTZ costs; the
  verification label is honest per drill ("TABLEBASE TRUTH · N PIECES"
  only when the defender actually probes; "HEURISTIC DEFENDER ·
  TERMINAL GRADING" otherwise).
- **Settings**: Schedule row (commitment) with set/clear round-trip
  tests; spawn policy stated in words; batch rows share the Database
  confirm flow. **Help & tour**: reader from the bundled guide, six
  tour cards anchored beside their rail groups, replayable.
- **Deep links** now cover screen/player/opponent/claim/db — every
  screen is reachable by URL for demos and automated capture.

## Honest omissions (each stated in the UI or a tooltip, never faked)

Peer-baseline columns (no peer aggregate exists yet); prep fingerprint
avg-Elo (not in the aggregate); Database Event/Date/Source filter chips
(no backend fields); Tactics "due" numeral (endless queue); SRS
new-card cap claim (no cap implemented); analysis column shows "legacy"
without a year (not stored); prep step-1 Elo/span (names only);
master-games plies column is "at ply" (reach ply, which also matches
the stated ranking rule).

## Conditional scale test

Skipped by its own condition — testdata/private/ still has no ≥5M
corpus. Noted: Position search and the opening tree now DISPLAY their
measured timings, so when a megabase lands those numbers become
user-visible product claims (the harness already reports real ms).

## Screenshots (committed, real data — docs/screenshots/run7/)

Captured from the running app on scid.sqlite (7,786 games: your five
si4 bases + TWIC test data + personal games), dark theme, driven by
deep links:

01-home — the full Home A: your real commitment line ("Club night ·
Thursday — no prep started for Khachian, Melik yet."), Continue on the
Jacobs game at ply 25, 43 SRS due with the tactics numeral honestly
grayed, findings from your freshly cached profile (Undefended allowed
1,318 leading, backward-pawn 45.1%), real TWIC imports under "New
since Sunday", and the engine-cold Running panel.
02-home-degraded — the empty-database state: the short honest list,
nothing else. 03-database — chips, source tags, analysis column,
batch actions. 04-profile — the acceptance centerpiece: serif lede
naming your two dominant leaks, motif matrix with the evidence aside
live on the 1,318-allowed claim (three games with ply anchors), honest
"—" under VS PEERS, Train this weakness. 05-prep — stepper with your
club opponent prefilled from the deep link. 06-tactics — five modes,
weakness-targeted default, "why this puzzle" aside, no overlays on the
board. 07-srs — grade row with real FSRS intervals on the buttons.
08-endgames — tier browser + verdict aside. 09-tree / 10-search —
with their measured-timing pills. 11-settings — including the
Schedule row holding the real commitment. 12-help — TOC + reader.

One robustness fix fell out of the capture session: a corrupt or
old-format `last_game` meta value used to error the whole Home screen;
it now degrades to an absent Continue card (the write path was always
correct — the bad value came from a by-hand seed during staging).

# Run 6 — 2026-07-26

## Headline

**The design system is in, whole.** The old tab UI is gone; the app now
runs the handoff-1 game view end to end — nav rail with live badges,
Studio Walnut and Instrument board treatments on the real chessground,
the shared evidence-overlay language with the reference's exact
filled-polygon arrows, bidirectional prose⇄board linkage, the specced
keyboard map, and a status strip driven by the real job queue. All three
design-gap rulings (eval-bar states, annotation editing, resize) are
implemented, the run-5 residuals are fixed, and the committed
screenshots (docs/screenshots/run6/) show YOUR game — Jacobs–O'Connor,
Pauba Library 2012 — through the full pipeline in both themes and both
treatments. 349 tests (189 Rust + 160 vitest) green; scale test skipped
(no ≥5M corpus present).

## Item 1 — the design system

- **Tokens & typography**: full custom-property set, dark default and
  the derived light theme; Public Sans / Source Serif 4 / JetBrains Mono
  bundled locally as woff2 with their OFL license texts (LICENSES.md
  rows added). Every panel, label and prose block sits on the tokens.
- **Board treatments**: Studio Walnut (default) + Instrument as
  chessground container theming — exact square colors, frame gradient
  and shadows, per-square grain/seam, gutter coordinates (walnut
  uppercase on-frame, instrument lowercase outside), piece drop-shadows,
  geometry as f(size), sizes snapped to multiples of 8. Treatment 1c not
  built (rejected, per the handoff).
- **Evidence-overlay language**: ONE module (app/src/lib/evidence.ts)
  produces every ring/wedge/wash/arrow. Marks follow the spec paint
  order under the pieces; arrows are the module's own SVG layer with the
  reference's exact filled-polygon geometry — 33u offsets, 27u×17u head,
  5.2u shaft, contrast outline, opacity 0.42+0.44i — verified
  point-for-point against the reference math including black
  orientation. Semantic hues identical in both themes (tested
  byte-identical across treatment/theme).
- **Explanation data contract (schema v3)**: `Explanation{tag, eval
  (White-POV readout), headline{coach,neutral}, blocks[{kind,
  text{coach,neutral}, evidence{alerts,attackers,defenders,imbalance,
  key,arrows}}]}` built by silman-verbalize from the SAME renderers the
  narration uses — the voice toggle is instant and can never change the
  evidence. Documented in SILMAN_ENGINE_SPEC.md; the UI never
  synthesizes explanations.
- **Game view**: rail → header → board column (eval bar, 656 board,
  move controls, keyboard hint) → Explain-above-Moves → status strip,
  all to the README's paddings and type sizes. The Explain panel
  auto-runs the free static screen each ply (the ENGINE still never
  runs uninvited — the empty state and its copy survive on quiet
  positions until you press E). Sentence hover isolates that sentence's
  evidence at intensity 1.0; square click filters the prose; stepping
  clears both. Keyboard map as specced with the focused-input exception.
- **Every capability has a rail home** — including the CLI-only ones
  (TWIC ingest and Account syncs get placeholder panels that document
  their exact CLI invocations until real UI exists; honest, not fake).

## Item 2 — design-gap rulings

- **2a Eval bar**: per-ply evals prefer fresh analyses over legacy;
  NO-DATA renders an empty track with a muted "—" (never a fake 0.0);
  MATE pins the bar to the winner with a winner-colored #N readout;
  hover tooltip states the source ("Stockfish 18 · depth 24" / "legacy
  import · <engine>" / "no analysis"). State derivation unit-tested.
- **2b Annotation editing**, inside the Moves-panel patterns: click a
  comment row to edit in place (serif textarea; Enter commits, Esc
  cancels, empty deletes); click the current move again or right-click
  any move for the NAG/comment popover (!, !!, ?, ??, !?, ?!, clear);
  play a non-mainline legal move on the board and the app offers "add
  as a variation"; × deletes a variation. Save/Revert drive the
  encoding-v2 token IPC; nothing new invented in storage. Walkthrough
  above is the taste-review script.
- **2c Resize**: minimum window 1180×760; the board column absorbs
  extra space with the board snapping to the largest multiple-of-8 that
  fits (floor 496); right pane fixed at 472px; the rail collapses to
  56px icons below 1280px window width.

## Item 3 — run-5 residuals

- Wandering-king repetition: WeakKing narration keys by side, not
  square — a king hunt is one story.
- Eval phrasing bands: at ±5.00 pawns (DECISIVE_CP) prose says "simply
  winning" / "completely winning for White" and the number stays in the
  eval readout; boundary-tested at 499/500; mate wording still outranks
  every band.
- Underpromotion: a promotion picker (Q/R/B/N, keys 1–4, Esc) now
  guards every board that accepts moves; the tactics caveat is gone.

## Screenshots (committed, real data)

docs/screenshots/run6/: 01 dark+walnut (the approved target), 02
light+walnut, 03 dark+instrument, 04 light+instrument — all the same
position (ply 25 of your Jacobs game, database #3680, 35 confirmed / 42
refuted / 16 unclear verdicts folded) so the treatments can be compared
on identical evidence: d7/e5/g5 alert rings, attacker arrows into e5
and d7, coach-voice blocks, +1.9 eval bar, legacy variation rows in the
moves list. Taken from the running app via deep link
(#game=3680&ply=25&theme=…&treatment=…), which is now a real feature.

## Honest deviations & judgment calls (review welcome)

- Variation provenance (ENGINE d24 vs LEGACY 2011 rows) is classified
  by a tested heuristic over the variation's comments (engine names /
  years) because provenance is not stored in the token stream; storing
  it structurally is the clean fix and a run-7 candidate.
- Fresh mate analyses store a ±10000 sentinel without a distance, so
  the eval BAR shows "#" pinned (the Explain readout shows #N when the
  explanation supplies it); storing mate distance in analyses is the
  companion run-7 fix.
- The old live-analysis panel has no home in the new shell — engine use
  flows through Re-analyze + Jobs, which matches the engine-off
  principle; flag if you want a live-analysis surface back.
- Header meta shows site/year · ECO · plies · provenance; no opening
  NAME lookup exists yet (ECO-to-name table is a small run-7 item).
- The nav-rail "Explain on" badge, profile findings count, and SRS/
  tactics badges are real data; TWIC "wk" and syncs badges are omitted
  (no data source) rather than faked.

# Run 5 — 2026-07-26

## Headline

All four of your feedback items that fell to the core pipeline are fixed
and regression-gated: **mate scores can no longer render as material**
(full score-matrix tests), **annotations now narrate the delta** (new
narrations architecture with a full-game similarity gate), **plan
synthesis ships** (convergent hints become one ranked composite plan),
and the **WSUI firing-rule study is published** in VALIDATION.md — the
incumbent solo rule won on the data, so the default is unchanged and the
alternatives are config knobs. **The Fathom FFI (the run's riskiest item)
landed early and fully validated** — crates/silman-tb probes 3-man
Syzygy files with WDL and DTZ answers matching the Lichess tablebase API
exactly. Voice, discoverability, and trainer work: agent addenda below.

## Item 1 — mate scores (bug)

Every path from engine score to prose was audited. `EngineCheck` and
`EngineEval` gained `mate_in` (schema v2); the job runner converts
engine mate lines to the beneficiary's POV (`mate_for_beneficiary` in
results, mate-aware confirmed/refuted grading); fold-back nulls
`score_delta_cp` whenever a mate is present so no downstream path can
see a 100-pawn sentinel. Templates: "The engine confirms it: {pv} —
forced mate in {mate}." / "…mates in {mate} even after the defense's
best try." / checkmate-on-board / eval-side variants. The required
matrix is tested end-to-end: positive cp ("wins about two pawns"),
negative cp, mate for ("forced mate in 3"), mate against, mate-in-0
(checkmate), plus a belt-and-suspenders case where a cp sentinel sneaks
in NEXT TO a mate and the mate wording still wins. A fold-back test
plants a mate verdict in a done job and asserts the exported PGN says
"forced mate in 3" and never "pawns".

## Item 2 — repetition (bug): the delta-narration redesign

The root cause was architectural: generated comments lived inline in the
movetext, so both the annotator and fold-back did incremental token
surgery with only local memory. Redesign:

- **Migration 0007**: generated prose moved to a `narrations(game_id,
  ply)` side table, merged into exports after the mainline move. Human
  comments are never touched; regeneration is wholesale, deterministic,
  and idempotent (asserted).
- **One shared narrator** (`narrate_game`) serves both batch annotation
  and verdict fold-back — the two paths cannot drift apart again.
- **Delta filtering**: an annotation narrates what the MOVE changed —
  new alerts, themes that appear or change magnitude, plans as they
  form. Standing themes restate only at phase boundaries. An alert that
  vanishes and re-arises within 8 plies (attacker captured and instantly
  replaced) is not retold. A verdict on a persisting alert narrates once,
  not at every ply the screen kept firing.
- **Blunder-class moves** (`?`/`??` NAGs) lead tactically with all
  positional boilerplate suppressed.
- **Terminal positions** get no chatter (no more "the knight on b8
  hangs" under the checkmating move).
- **Phrasing variety**: same-kind alerts on different squares select
  square-seeded `.alt` template variants; clauses shared by two alerts
  in one comment render once. (Also fixed while in there: blockade plan
  hints now attribute to the DEFENDING side — "Another idea for Black:
  blockade White's passed pawn".)

The required gate: the Opera game (Morphy 1858, with `?`/`??` NAGs on
the two famous mistakes) is annotated end-to-end and snapshot-tested,
and the test FAILS if any two consecutive narrations exceed 0.6 Jaccard
word-set similarity. The snapshot reads as a story now — see
`app/silman-db/tests/snapshots/narration__opera_game_annotated.snap`.

## Item 4 — plan synthesis

`silman_core::plans::synthesize()` clusters PlanHints by (side, file of
the hint's destination square); destination squares get double vote
weight for naming the target; composites rank by (count of distinct
supporting imbalances, magnitude-weighted score). `FeatureRecord` v2
carries `composite_plans`. The verbalizer narrates the top composite as
ONE unified sentence — "Everything points to d5: reroute the knight
there and pile up on the backward pawn in front of it." — the runner-up
briefly, the rest dropped; member hints are consumed so they never
repeat as singles. Golden test: the Sveshnikov bind position (FEN
r1bqkb1r/pp3ppp/2np1n2/1N2p3/4P3/2N5/PPP2PPP/R1BQKB1R w KQkq - 0 7)
converges on d5 with outpost + knight-route + backward-pawn support.

## Item 5 — WSUI firing-rule study

`WsuiConfig` gained `rule: FiringRule` (solo / pair /
high-solo-or-two-distinct / weighted score, unit-tested); the validation
harness sweeps every family × threshold × SEE band on the train half and
publishes each family's best point on the holdout. Full table in
docs/VALIDATION.md. Verdict: **the incumbent solo rule wins outright**
(81.3% recall / 39.2% FP, balanced objective 42.1 vs 38.5 for the best
challenger). Every stricter rule pays for FP reduction with
disproportionate recall loss — pair rules halve FP but drop ~28 recall
points, because most real tactics present as one dominant alert. Default
unchanged; `PairAtOrAbove` (15.6% FP / 93.1% precision) is documented as
the knob for latency-sensitive batch profiling. Personal-corpus motif
counts re-verified byte-identical (Undefended allowed 1,318; WeakKing
57/713 conversion — same as run 4).

## Phase 5 — Fathom FFI (crates/silman-tb) — DONE, validated

Vendored Fathom @ c9c6fef (LICENSE verified MIT verbatim before
vendoring; per-file sha256s in the crate README), compiled via `cc`, no
bindgen. Safe API: `Tablebase::init` (global-mutex guarded — Fathom has
process-global state), `probe_wdl` (thread-safe), **`probe_root`** (DTZ
best-move — shipped, not parked), cozy-chess `Board` adapters, bare-kings
short-circuit, structured errors (castling rights, nonzero rule50,
too-many-pieces). Validation: 10/10 tests against real 3-man Syzygy
files (`scripts/fetch_syzygy_test_files.sh`, ~26 KB) with every probe
cross-checked against the Lichess tablebase API — KQvK Win dtz 13 best
Qa7 (exact match), KRvK Win dtz 23, KPvK wrong-corner Win vs rook-pawn
Draw, promotion-position root move a7a8=Q dtz 1. Tests skip cleanly when
the files are absent (CI-safe). License gate extended and green for all
five BSD crates.

## Agent addenda (verified)

- **Item 3 — Silman voice (agent, verified):** `Voice { Coach, Neutral }`
  in silman-verbalize as a pure template overlay — 69 `coach.*` keys in
  `templates/coach.tmpl`; lookups resolve `coach.<key>` then fall back to
  the base key, so both voices state identical facts and every lint,
  grounding test, and snapshot runs over BOTH voices. Coach is the
  default per your spec; the setting persists in the existing `meta`
  table (no migration), with Tauri get/set commands, a Coach/Neutral
  select in the Explain panel, CLI honoring it, and the LLM prompt
  voice-aware with a voice-respecting template fallback. Engine-verdict
  clauses are deliberately NOT voiced — mate/eval text stays literal, so
  the item-1 guarantees hold in both voices.
- **Item 6 — discoverability (agent, verified):** docs/USER_GUIDE.md
  (every tab/button/workflow + a CLI-only section with exact
  invocations), in-app Help modal rendering the bundled guide, first-run
  overlay, and four clarity renames: Analyze→**Load PGN**,
  Prep→**Opponent Prep**, Profile→**Player Profile** (your complaint),
  Run jobs→**Run engine jobs** — plus tooltips on every tab and the
  engine Analyze button ("the engine only runs on demand"). Honest
  findings it documented rather than hid: import/sync is CLI-only, the
  eco/result filters exist in the backend but have no UI controls yet,
  and export is clipboard-only.
- **Phase 5 — Repertoire Trainer (agent, verified):** crates/silman-srs
  (BSD) implements FSRS-4.5 DIRECTLY — the `fsrs` crate was rejected
  because its tree pulls `priority-queue` (LGPL-3.0-or-later OR MPL-2.0),
  which fails our license gate; evidence in docs/LICENSES.md's new
  "evaluated and rejected" table. Scheduler tested against the published
  reference numbers (Good-chain stability 3.71→14.09→46.92→139.63→377.30,
  post-lapse ≈2.9188). Migration 0008: per-color repertoires, cards keyed
  by the ep-normalized position hash, full review history. Train tab
  drives the main board (flips for Black), "add line to repertoire"
  from any loaded game, `import-repertoire` CLI. Not named MoveTrainer.
- **Phase 5 — tactics trainer (agent, verified):** migration 0009 +
  streaming import (the real 5,876,919-row Lichess dump: 127 s, ~9 MB
  peak RSS, 0 malformed). Five modes: rated drill (Elo ledger, K=40→20),
  motif-filtered (73 tags), **weakness-weighted** — theme→motif mapping
  table onto your profile's AlertKind axes; with your actual profile
  shape (Undefended allowed 1,318) the selection shifts loose-piece
  puzzles from 33.3% to 52.7% of picks, and every pick shows its reason
  ("picked because your games allow many loose-piece tactics…") —
  Woodpecker cycles (fixed sets, per-cycle time/accuracy comparison),
  and Heisman-style speed drill. No engine anywhere: verification is
  exact-match + cozy-chess checkmate detection.
- **Phase 5 — endgame trainer (agent, verified):** landed within the
  run — no spillover. 27 drills in three tiers (essentials → technique →
  rook endings), defined in a data file with ZERO copyrighted text:
  original one-line instructions over public-domain theory (opposition,
  square of the pawn, Lucena's bridge, Philidor's third rank, Vancura,
  wrong-bishop corner, Q vs 7th-rank pawn win/draw cases). Every FEN's
  theoretical result was verified against Syzygy before inclusion.
  Opponent play: DTZ-optimal tablebase moves via silman-tb wherever the
  loaded files cover the position — with per-move policing that fails
  the drill the moment your move flips the theoretical result — else a
  documented deterministic heuristic; Stockfish never spawns (asserted).
  With the 3-man test set 16/27 drills get the tablebase opponent; a
  3-4-5 set covers all 27 (fetch script provided). Migration 0011
  records attempts with opponent/verification provenance and mastery
  streaks. It also fixed the migration runner to per-version bookkeeping
  so the reserved-but-unused 0010 slot applies safely whenever it lands.

## Acceptance sample — your game, full pipeline, coach voice

Jacobs–O'Connor, MCC Swiss 2012 (C63 Schliemann, 0-1), imported fresh
from mygames.pgn and run through the whole pipeline: 86 positions, 64
screens fired, 64 bounded confirms (18 confirmed / 32 refuted / 14
unclear), delta narration in coach voice with your 2011 legacy analysis
preserved as variations. Excerpts (verbatim):

> **13. Nxe5** *{Black's rook on d7 is calling for reinforcements — the
> defense around it is stretched thin. It is attacked by White's knight
> on e5 but defended only by Black's knight on f6, the queen on e7 and
> the king on c8. A capture sequence here wins about two pawns. The
> engine confirms it: Qxe5 Bxf6, winning about 1.9 pawns. Meanwhile,
> White's knight on e5 hangs — it stands under attack with no friend in
> sight…}*
>
> **23. Rae1** *{…White's heavy pieces have the open lines to
> themselves. The d- and e-files are open. … Here is what White should
> be dreaming about: double the heavy pieces on the open file and make
> it their private highway.}* ← composite plan, coach voice
>
> **38. cxd5** *{In this endgame White's better pawn structure counts
> for a great deal. White has passed pawns on e4 and d5. … Here is what
> Black should be dreaming about: put a piece in front of White's passer
> and put its dreams on hold (key squares: e5).}* ← blockade attributed
> to the DEFENDER (fixed this run)
>
> **42. Kf6** *{…The engine confirms it: c3 b5 c2 — **forced mate in
> 9**.}* ← bug 1, fixed, in production

Honest residuals a reviewer should know about (run-6 candidates, listed
in DECISIONS_NEEDED judgment items): (a) a wandering king in a lost
endgame re-narrates its "draft" at each new square — the alert key
includes the square, so each is technically news, but a coach would say
"the king hunt is on" once; keying WeakKing deltas by side rather than
square would compress moves 38–43 here. (b) Very large winning evals
still read "winning about 9.1 pawns" — accurate but a "completely
winning" phrasing band above ~5 pawns would read better. (c) The guide
agent finished before the trainer tabs landed; USER_GUIDE.md needs a
Train/Tactics(/Endgames) section — queued behind the endgame agent so
one documentation pass covers all three.


# RUN_REPORT.md

# Run 4 — 2026-07-26

## Headline

All four maintainer verdicts fixed with regression tests; **Phase 4
profile shipped and run on the full personal corpus**; LLM verbalizer and
UI wiring per agent addenda below; the five acceptance games are
re-annotated with the full pipeline (static screen → bounded engine →
verdict fold-back) — samples at the end of this section for judgment.
Scale track skipped (no ≥5M corpus in testdata/private/).

## The maintainer verdicts

1. **Prose leaks — fixed.** The verbalizer got a per-key grammatical
   evidence dispatcher; the dump path no longer exists in the code, and a
   lint (no `_ [ ] { } "`, no digit-after-colon) runs inside every
   snapshot, over all records, and over every template with slots
   stripped. Your exact complaint case now renders: "White has the
   superior minor pieces. Black's f8-bishop is a problem piece, hemmed in
   by its own pawns on c5 and d6."
2. **NAG rendering — fixed in the UI** (glyph map with tooltip-only
   unknowns; $201 renders as nothing visible + "diagram marker (imported)"
   tooltip). Provenance answer: $201/$18/$14/$10 are genuine imported
   Fritz/SCID annotator NAGs from your 2011 auto-analysis — real data,
   not our markers, so they stay stored; only their RENDERING changes.
3. **Legacy analysis provenance — the full requirement set:**
   (a) fresh runs capture the engine's real `id name` and stamp it (with
   nodes and timestamp) on every stored analysis row and job result;
   (b) import now parses engine comments into structured `analyses` rows
   tagged legacy-import: your corpus yielded **7,420 rows across 387
   games — Stockfish 2.1.1 64bit (5,408), Stockfish 2.0.1 (1,994), Toga
   II 1.2.1a (18)** — with mixed comments keeping their human text
   ("Move out of book Nf6 82%..."), stacked double annotations peeled,
   and SCID blunder markers (****Dn) excluded from names — each shape
   regression-tested against your actual comment bytes;
   (c) legacy evals render muted with an engine-vintage tooltip, fresh
   normal, both White-POV-normalized (POV conversion unit-tested);
   (d) re-analyze game action (UI button + `silman-cli reanalyze-game`)
   enqueues fresh per-position evals; fresh rows are preferred for
   display; legacy rows are never deleted or overwritten.
4. **Cosmetics:** window title = app + open db; annotation display
   toggle full/hover/hidden.

## Confirm-verdict fold-back (goal 3)

wsui-confirm verdicts now merge into stored annotations: a confirmed
alert leads its comment with the engine's SAN PV and score; a refuted
alert is dropped from the prose (a refuted screen with no surviving alert
inserts nothing); persisting weaknesses narrate once, not once per ply.
Idempotent via jobs.folded_at; `run-jobs` folds automatically. Tests
cover confirmed, refuted, idempotency, and the two anti-repetition rules
(both were found by reading real acceptance output, then locked in).

Across the five acceptance games: 264 bounded jobs, 0 failures —
**69 confirmed, 179 refuted, 16 unclear**. The 68% refute rate is the
architecture doing its job: the static screen over-fires cheaply
(by design, per docs/VALIDATION.md), the bounded engine cleans it up, and
fold-back keeps refuted claims out of the user-visible prose.

## Phase 4 — PlayerProfile (goal 4)

silman-profile::player is pure aggregation (BSD, no I/O), hand-validated
on a fixture where every number is computable by inspection. The app
pipeline extracts per-ply alert sets (persistence-aware: a weak king is
ONE opportunity, counted when it appears), merges evals from `analyses`
(fresh preferred, POV-normalized), samples mid-game structure flags, and
profiles the **full 1,026-game personal corpus in ~2 s**.

Three representative findings from your own profile (spot-check games in
brackets; every claim is drill-down-able in the Profile UI):

1. **Loose pieces are the biggest leak — in both directions.** You cash
   80% of newly-loose enemy pieces (1,179 of 1,479 opportunities taken),
   but you ALLOWED 1,318 newly-loose pieces of your own — roughly 1.3 per
   game [games 3749, 3740, 3732]. Nunn's LPDO, both sides of the coin.
2. **Backward pawns are your worst structure.** In 113 games where you
   carried a backward pawn you scored **45.1%**, against a 55.9% overall
   baseline — the only structure flag below 49% [3686, 3667, 3661]. For
   contrast: your isolated-pawn games score 57.2% (340 games) — isolation
   doesn't hurt you; backwardness does.
3. **Attacking conversion against weak kings is under-realized:** of 713
   fresh enemy weak-king situations you punished 57 and let 656 pass
   [3749, 3740, 3729]. Combined with the legacy-eval ACPL slope (opening
   82 → middlegame 182 → endgame 201, over the 387 games your 2011 self
   analyzed), the profile's claim is: tactics execution late in the game
   is the highest-value training target.

(Eval coverage is 3.7% — the legacy 2011 analyses. A full fresh ACPL
pass is one command: `reanalyze-game` per game or a batch loop; ~2-3 h of
engine time for the whole corpus at 200k nodes.)

## CLI surface audit (goal 2)

UI paths now exist for: browse/search/tree, prep view, profile report
(with drill-down), annotate game, re-analyze game, run jobs (+status
strip), export PGN, explain position (+LLM mode), annotation editing.
Intentionally CLI-only, with exact commands:

- Imports: `silman-cli --db <db> init | import-pgn <f> | import-si4 <base>`
- Network syncs: `twic-sync --from N | lichess-sync <u> | chesscom-sync <u> | fics-sync <u> <year>`
- Validation harness: `cargo run -p silman-db --bin wsui-validate -- ...`
- Fingerprint (superseded by Profile UI but kept): `fingerprint <player>`
- `explain <fen>` / `explain-llm <fen>` (LLM needs ANTHROPIC_API_KEY)

Rationale: imports and network syncs are provenance-sensitive batch
operations that want explicit flags; they are the next natural UI
candidates if wanted.

## Agent addenda (verified)

- **LLM verbalizer** (Phase 4, goal 5): LlmTransport trait keeps the BSD
  crate network-free (confirmed absent from its cargo tree); strict
  grounding validation (SAN tokens verbatim-in-record or legal-in-FEN,
  squares must exist in the record) with TOTAL template fallback on any
  violation, transport error, or empty output; 8 offline hallucination-
  injection tests all fire fallback; app-layer AnthropicTransport +
  `explain-llm` CLI; live test env-gated (SILMAN_LLM_TESTS=1).
- **UI wiring**: NAG glyphs w/ tooltip-only unknowns and invisible $201;
  legacy evals muted w/ engine-vintage tooltips, POV-normalized (tested);
  Annotate/Re-analyze/Run-jobs buttons with a polling status strip and a
  background worker on its own connection; Export PGN via clipboard+modal
  (no dialog plugin in capabilities — deliberate); Profile tab with
  clickable drill-down into every example game; prep-view profile-weakness
  strip; window title from app+db; annotation display toggle
  full/hover/hidden. 21 src-tauri + 58 vitest tests. Everything remains
  VISUALLY unverified (no screen access) — same standing caveat.

## Final verification

131 workspace + 21 src-tauri + 58 vitest tests green; fmt/clippy clean
everywhere including `--features llm`; license gate green ×4 (ureq
confirmed absent from silman-verbalize's tree with the llm feature on).

## Acceptance samples — five games, three positions each


#### Game 1 — sounix–christoforo 2011, C41, 1-0, 89 plies (your hands-on reference game)

**[confirmed] after 34. Bf3:**
> White's king is seriously exposed around g2. The pressure comes from Black's pawn on f4. The pawn cover is gone on the f-file, the shield pawn on the g-file has pushed too far forward and the h-file in front of the king is wide open. The engine confirms it: Kxe6 Bg4+ Kd5, winning about 4.6 pawns. Meanwhile, White's rook on e6 hangs — nobody defends it. It is under attack from Black's king on d5. White has the superior minor pieces. White owns the bishop pair in an open position. Meanwhile, White has the initiative and is dictating play. A good plan for White: open the position so the bishops can breathe.

**[confirmed-late] after 41. Bxg6:**
> White's bishop on g6 hangs — nobody defends it. It is under attack from Black's king on f6. The engine confirms it: Kxg6 Bd2 Rb8, winning about 2.9 pawns. White's minor pieces are a touch better placed. White owns the bishop pair in an open position. Meanwhile, White's pawn structure is a shade healthier. Black has a passed pawn on f4. Black's pawns on c4, f4 and a7 are isolated. White has a queenside pawn majority. Black has a kingside pawn majority. At the same time, White is slightly ahead on material. Black is up the exchange. Beyond that, Black has a little more room to work with. A good plan for White: open the position so the bishops can breathe. Another idea for White: advance the queenside pawn majority. Another idea for White: blockade Black's passed pawn (key squares: f3). Another idea for Black: use the extra space and sidestep piece trades.

**[endgame] after 39. Kf3:**
> White's king is not entirely safe around f3. The pressure comes from Black's pawn on f4, the pawn on e5, the king on f5 and the rook on g6. Enemy pieces are massing around it. The engine confirms it: Rh8 Be4+ Kf6, winning about 5.5 pawns. In this endgame Black's better pawn structure counts for a great deal. Black has passed pawns on f4 and e5. Black's pawns on c4 and a7 are isolated. White has a queenside pawn majority. Black has a kingside pawn majority. Meanwhile, Black is ahead on material. Black is up the exchange. At the same time, Black enjoys a clear space advantage. A good plan for Black: advance the queenside pawn majority. Another idea for Black: blockade Black's passed pawn (key squares: f3). Another idea for Black: use the extra space and sidestep piece trades.


#### Game 3749 — sounix–Dennis70x7 2012, B32, 1-0, 49 plies (decisive tactical)

**[confirmed] after 10. Be4:**
> Black's rook on a8 hangs — nobody defends it. It is under attack from White's bishop on e4. The engine confirms it: Rb8 Nc3 Ne7, winning about 1.6 pawns. White's pawn structure is a shade healthier. Black's d7-pawn is backward. White has a queenside pawn majority. Meanwhile, White has a little more room to work with. At the same time, White is slightly ahead in development. A good plan for White: advance the queenside pawn majority. Another idea for White: use the extra space and sidestep piece trades. Another idea for White: open the position before the opponent finishes development.

**[confirmed-late] after 25. Rf7+:**
> Black's queen on e5 is trapped — it has no safe square to run to. It is attacked by White's bishop on d4, and nothing defends it. A capture sequence here wins roughly a queen. The engine confirms it: Kh8 Qh7#, winning about 100 pawns. Meanwhile, Black's king is seriously exposed around g7. The pressure comes from White's queen on d3, the pawn on g5, the bishop on h5, the pawn on e6 and the rook on f7. The pawn cover is gone on the g-file and the f-file in front of the king is wide open. On top of that, White's king is seriously exposed around g1. The pressure comes from Black's queen on e5. The shield pawn on the g-file has pushed too far forward and the f-file in front of the king is wide open. Also, Black's queen on e5 hangs — nobody defends it. It is under attack from White's bishop on d4. Black's knight on e7 hangs — nobody defends it. It is under attack from White's rook on f7. Meanwhile, Black's knight on e7 is trapped — it has no safe square to run to. It is attacked by White's rook on f7, and nothing defends it. A capture sequence here wins roughly a minor piece. On top of that, White's queen on d3 hangs — nobody defends it. It is under attack from Black's pawn on c4. White's minor pieces are a touch better placed. White owns the bishop pair in an open position. Meanwhile, White's pawn structure is a shade healthier. Black's pawns on h6 and a7 are isolated. White's e6-pawn is isolated. White has a kingside pawn majority. At the same time, White is slightly ahead on material. White is a pawn up. Beyond that, White has slightly the better of the open lines. The f-file is open. The b-, e- and g-files are half-open for Black. The c- and d-files are half-open for White. White has a rook on the seventh rank. White has a little more room to work with. A good plan for White: open the position so the bishops can breathe. Another idea for White: double the heavy pieces on the open file. Another idea for White: use the extra space and sidestep piece trades.

**[quiet] after 11. Nc3:**
> No immediate tactics jump out; the position will be decided by its long-term features. White is clearly ahead in development. A good plan for White: open the position before the opponent finishes development.


#### Game 3506 — sounix–forense 2012, C62, 0-1, 174 plies (endgame marathon)

**[confirmed] after 13. Nd4:**
> Black's queen on e6 cannot be adequately defended. It is attacked by White's knight on d4 but defended only by Black's pawn on f7. A capture sequence here wins roughly a rook. The engine confirms it: Qd7 Nxc6 Qxc6, winning about 2.1 pawns. Black's pawn structure is a shade healthier. Black's pawns on f7 and h7 are isolated. White's pawns on a2, c2 and c3 are isolated. White's pawns on c2 and c3 are doubled. White has a kingside pawn majority. Meanwhile, White is slightly ahead on material. White is a pawn up. At the same time, White has a small edge in the fight for the key squares. Black's position has holes on f5 and f6. White's position has a hole on c4. A good plan for White: reroute the knight toward the waiting outpost (key squares: f5).

**[confirmed-late] after 76. Ke3:**
> Black's king is not entirely safe around b4. The pressure comes from White's queen on b2. Enemy pieces are massing around it. The engine confirms it: Kc5 Qa1 Kd5, winning about 5.1 pawns. In this endgame Black's pawn structure should decide the game. Black has passed pawns on a2, b3 and c4. Black has a queenside pawn majority. Meanwhile, in this endgame White's extra material should decide matters. White is up a decisive amount of material. At the same time, Black enjoys a clear space advantage. A good plan for Black: blockade Black's passed pawn (key squares: a1). Another idea for Black: use the extra space and sidestep piece trades.

**[quiet] after 6. Bxc6:**
> No immediate tactics jump out; the position will be decided by its long-term features. White is ahead on material. White is three pawns up.


#### Game 3721 — Fezzik–sounix 2012, A64, draw, 90 plies (quiet positional)

**[confirmed] after 26. Ne3:**
> Black's queen on g4 hangs — nobody defends it. It is under attack from White's knight on e3. The engine confirms it: Qh4 Ne4 Rf8, winning about 1.7 pawns. Meanwhile, White's knight on d6 hangs — nobody defends it. It is under attack from Black's rook on d8. On top of that, White's bishop on g2 is trapped — it has no safe square to run to. It is attacked by Black's queen on g4 but defended only by White's king on g1 and the knight on e3. Also, White's king is not entirely safe around g1. The pressure comes from Black's queen on g4. The pawn cover is gone on the g-file and the shield pawn on the h-file has pushed too far forward. White has the superior minor pieces. White owns the bishop pair in an open position. Meanwhile, Black's pawn structure is a shade healthier. White has a passed pawn on d5. White's pawns on b2, f2, d5 and h6 are isolated. Black has a queenside pawn majority. A good plan for White: open the position so the bishops can breathe. Another idea for Black: blockade White's passed pawn (key squares: d6).

**[quiet] after 19. gxh5:**
> No immediate tactics jump out; the position will be decided by its long-term features. White has a winning material advantage. White is up a decisive amount of material. Meanwhile, Black has the healthier pawn structure. White's pawns on f2, d5 and h5 are isolated. Black has a queenside pawn majority. At the same time, Black has the initiative and is dictating play.

**[other] after 18. hxg4:**
> Black's knight on h5 cannot be adequately defended. It is attacked by White's pawn on g4 but defended only by Black's pawn on g6. A capture sequence here wins roughly a minor piece. The engine could not confirm this at the given budget. Meanwhile, White's king is not entirely safe around g1. The pawn cover is gone on the h-file and the shield pawn on the g-file has pushed too far forward. White's minor pieces are a touch better placed. White owns the bishop pair in an open position. Meanwhile, Black's pawn structure is a shade healthier. White's d5-pawn is isolated. Black has a queenside pawn majority. At the same time, White is slightly ahead on material. White is two pawns up. Beyond that, Black has slightly the better of the open lines. The e-file is open. The f-file is half-open for Black. The c- and h-files are half-open for White. White has a little more room to work with. A good plan for White: open the position so the bishops can breathe. Another idea for Black: double the heavy pieces on the open file. Another idea for White: use the extra space and sidestep piece trades.


#### Game 3727 — elmaestro–sounix 2012, B92, 0-1, 82 plies (run-3 comparison game)

**[confirmed] after 35... Bxd4:**
> White's king is seriously exposed around h1. The pressure comes from Black's rook on e2, the bishop on d4 and the rook on g5. Enemy pieces are massing around it and the pawn cover is gone on the g-file. The engine confirms it: Rc7+ Kf8 Rc8+, winning about 3.7 pawns. Meanwhile, Black's king is seriously exposed around f7. The pressure comes from White's rook on f3 and the pawn on f6. The pawn cover is gone on the f-file and the e-file in front of the king is wide open. White enjoys a clear space advantage. A good plan for White: use the extra space and sidestep piece trades.

**[confirmed-late] after 41... Kf8:**
> White's king is seriously exposed around h1. The pressure comes from Black's rook on e2, the rook on g2 and the bishop on d4. Enemy pieces are massing around it and the pawn cover is gone on the g-file. The engine confirms it: Rg8+ Kxg8 f7+, winning about 100 pawns. Meanwhile, Black's king is seriously exposed around f8. The pressure comes from White's pawn on f6 and the rook on g7. Enemy pieces are massing around it, the pawn cover is gone on the f-file, the e-file in front of the king is wide open and the back rank is airless. White enjoys a clear space advantage. A good plan for White: use the extra space and sidestep piece trades.

**[quiet] after 27. c5:**
> No immediate tactics jump out; the position will be decided by its long-term features. White enjoys a clear space advantage. A good plan for White: use the extra space and sidestep piece trades.


---


# Run 3 — 2026-07-25/26

## Headline

**Phase 2 complete** (prep view shipped end-to-end). **Annotation-editing
UI shipped** (the last run-2 Phase 1 leftover — Phase 1 is now fully done
except the ≥5M scale acceptance, which was conditional and skipped: no
corpus in testdata/private/). **Phase 3 reached its checkpoint**: full WSUI
+ 8 imbalance detectors with cited golden tests, FeatureRecord v1,
SQLite job queue with the engine-off principle asserted in tests,
verbalize template mode, and a published validation number:
**holdout recall 81.3% / FP 39.2% / precision 89.2%** (docs/VALIDATION.md).

## Step 0 + standing rulings

Everything reproduced green (110 workspace + 18 src-tauri + 40 vitest at
run end). Added the missing explicit in-check-null truncation test;
fully-truncated variations no longer persist as empty `()`. Item 4 CLOSED
empirically: the si4 Elo top nibble is zero in all 95,066 rating fields
across the ten real databases (SI4_FORMAT_NOTES.md §6.8). Scale track
skipped (condition not met).

## Built

1. **silman-core**: attack/defense maps with x-ray pins; SEE; WSUI screen
   (W/S/U/I per spec: zone pressure, flank-king shield defects, back-rank,
   trapped-with-attackable-in-one, loose pieces, SEE-graded inadequate
   defense with pin-aware attacker AND defender discounting, overloaded
   sole defenders); all eight imbalance detectors with structured evidence
   and plan hints (incl. the spec's BFS knight-route, piece-safety-aware);
   phase classifier; `analyze()` → FeatureRecord v1 (spec-exact JSON,
   snapshot-tested). 33 tests on cited positions (Sveshnikov d5 tabiya,
   French bad bishop, Noah's Ark trap, LPDO, back-rank pattern, Giuoco
   quiet control, CPW mutual-hang position asserted as NOT quiet).
2. **silman-verbalize** (subagent): data-file templates with fallback
   chains, spec composition order, FEN-derived piece attribution, 4 prose
   snapshots + a no-invention property test (every square in the output
   provably present in the record).
3. **Job queue** (migration 0005): purposes wsui-confirm/user-analysis/
   batch-annotate, resumable (running→pending reset), serial worker with
   ONE lazily-spawned engine per batch; blocking UCI client with a spawn
   counter. **Engine-off asserted**: a quiet game annotates with 0 jobs
   and 0 spawns even when a worker runs afterward; a trap-position game
   enqueues without spawning; a gated live test proves one process grades
   a fired alert.
4. **Batch annotator**: static analysis per mainline position, comments
   only when the tactical/positional story changes, one bounded confirm
   job per fired screen. CLI: annotate-game / run-jobs / explain.
5. **Prep view**: backend ranking (frequency-weighted underperformance,
   depth-discounted, deviation-boosted) + master-games-at-hash; UI cards
   with click-through to the game at the right ply. Hand-computable
   fixture test (the Villain Scandinavian corpus).
6. **Annotation editing**: silman-db edit API (token update + index
   rebuild) and UI (inline comments/NAGs/variations, board-input variation
   capture); round-trip tests UI-shape → db → exported PGN.
7. **Validation harness** + docs/VALIDATION.md (numbers above, train-only
   tuning, reproduction commands, honest caveats). CC0 500-puzzle fixture
   committed; TWIC-derived quiet set deliberately NOT committed.
8. **Explain panel** (stretch): prose + evidence-square overlays.

## Detector iteration (documented, per the brief)

Real-game prose exposed noise the golden tests could not: three iterations
on the same B92 game took fired screens 51 → 44 → 34 (home-rank loose/
trapped suppression, boxed-by-own-army suppression, space floor + side
attribution, story-change comment gating, open-file plan square fix). The
symmetric-perft-position finding from run 2 ("engine-quiet ≠ statically
quiet") became a documented validation caveat: the 39% quiet-side fire
rate is largely real defended tension that the bounded engine step is
designed to refute.

## Numbers

| Metric | Value |
|---|---|
| WSUI holdout recall / FP / precision | 81.3% / 39.2% / 89.2% |
| Screen speed | microseconds/position (static, zero search) |
| Demo game (82 plies) | 34 fired screens → 34 bounded jobs, 0 failed |
| Test totals | 110 workspace + 18 src-tauri + 40 vitest |

## What a human must do or decide next

1. Judge the demo annotations (below in the final session message; also:
   `silman-cli --db <db> annotate-game <id> && run-jobs && export-pgn <id>`)
   against the Phase 3 acceptance bar ("useful, not wrong" on 5 games).
2. Visual pass over the three new UI surfaces (never seen on a screen):
   Prep tab, annotation editing, explain overlays. `npm run tauri dev`.
3. Verbalizer polish candidates: raw-ish evidence fallback phrasing
   ("blocking_pawns [\"e5\",\"d6\"]"), space-plan phrasing in pure
   endgames, subject-verb agreement on multi-defender sentences.
4. Decide whether wsui-confirm verdicts should flow back INTO stored
   annotations automatically (currently they land in jobs.result).
5. Megabase for the scale track (still pending real data).

## Deviations

- WsuiConfig defaults keep SEE bands 100/300 (validation chose 150/400 by
  +0.6 objective — within noise; revisit with per-detector thresholds).
- Committed fixture is puzzles-only; quiet negatives are reproducible-not-
  committed (TWIC redistribution rule).


# Run 2 addendum — 2026-07-25 (maintainer rulings implemented)

The maintainer decided items 1–3 and 7 (and accepted 6). All implemented,
tested, CI-verified:

- **Encoding v2** exactly per the specified design: inline escape tokens
  (COMMENT varint+UTF-8, NAG, nestable VAR_START/VAR_END, NULL_MOVE, END,
  reserved ESCAPE) above move indices 0–217. Both importers now store full
  annotation trees — the PGN reader captures comments/NAGs (including
  `!?`-suffix normalization)/variations, and the sg4 decoder's token
  stream flows straight into storage: re-importing mypages stored 16,653
  comments, 29,576 NAGs, 8,680 variations inline. Export renders all of it
  back; the round-trip test now asserts **full token-level semantic
  equality** on an annotated Latin-1 fixture. Real-data proof: benoni2's
  `{LC}` annotator comment survives si4 → db → PGN.
- **Null moves**: dedicated token; in-check nulls (PGN-legal in analysis
  lines, unrepresentable as a legal position) truncate the affected line
  gracefully rather than failing the game.
- **One-shot v1→v2 migration** on db open, as ruled (no dual encodings):
  the 121k-game corpus upgraded transparently in ~13 s; a 7.8k-game copy
  in 0.6 s. Note: v1 databases never stored annotations, so upgrading
  cannot recover them — re-import from source where annotations matter
  (done for the SCID bases).
- **Duplicates**: sources carry a kind (personal > twic > online > other);
  on collision the kept game is upgraded to the highest-priority source's
  headers+movetext and the losing copy is recorded in a `duplicates` link
  table — nothing deleted. Tested both orders (personal-then-TWIC,
  TWIC-then-personal-upgrades).
- **Etiquette**: User-Agent contact filled in (config-supplied); FICS
  sync prints a personal-use/bandwidth notice, same posture as TWIC.
- **This phase 1 blocker is now closed.** Phase 1 outstanding items are
  only: annotation *editing* UI (storage now exists; UI is run-3 work with
  the prep view) and the ≥5M-game scale acceptance.

# Run 2 — 2026-07-25 (later the same day)

## ⚠ Headline: Phase 1 is complete EXCEPT annotation storage/editing

Per this run's instructions, parked decisions that block acceptance were
NOT decided. DECISIONS_NEEDED.md items 1–2 (annotation storage / encoding
v2, null moves) gate exactly two Phase 1 deliverables: storing si4/PGN
annotations and the annotation-EDITING half of the game view. Everything
else in Phase 1 is built and verified; the game browser is read-only. The
sg4 decoder already extracts all annotations (16,653 comments / 29,576
NAGs / 8,680 variations counted in your two big SCID bases alone), so once
encoding v2 is blessed, storage is a contained change.

**Phase 2 checkpoint: reached** (network clients + repertoire fingerprint).

## Step 0 — verification of run 1's claims

All of run 1's claims reproduced: fmt/clippy/license-gate green, 28+9
tests green, perft green, position search 30 ms warm / 64 µs miss on the
121k corpus. One nuance the run-1 report under-stated: its find-fen
timings were warm-cache; a cold first query on the 11k-hit Sicilian is
~315 ms. No other discrepancies.

## Built this run

1. **Cleanroom .sg4 move decoding** (crates/si4-read): all documented
   format gaps resolved empirically against your own databases — the
   community doc's rook table is transposed; pawn double-push is code 15;
   piece numbers are swap-remove list indices; null move is byte 0x00;
   variation markers branch from before the previous move; tags ≥0xF1 are
   coded. Findings recorded in docs/SI4_FORMAT_NOTES.md §6 with the
   validation protocol. **7,905/7,905 real games decode** with every move
   legal, ply counts matching the index, and final material matching the
   index signature. No SCID source consulted at any point.
2. **Full .si4 import** (`import-si4`): mypages + twictest = 7,786 games
   in 3.9 s, zero failures; shared insert path with the PGN importer so
   duplicate detection is cross-source; dropped annotations counted and
   reported to the user at import time.
3. **ECO tagging** via the bundled CC0 lichess openings dataset
   (data/openings/, in LICENSES.md), deepest-book-position match by
   position hash (transposition-aware); source tag as fallback.
4. **Opening tree** (`opening-tree`): per-move games/W/D/L/avg-elo/perf
   from the position index (ply-0 rows + next-move byte, migration 0002/3).
   Hand-validated on a fixture; sane on real data (master vs blitz
   segments separate cleanly). 22 ms typical, 956 ms worst-case (startpos).
5. **TWIC ingester**: incremental, provenance-tagged, donation notice,
   explicit --from on first run, 404 stop, no TWIC data in repo. Live
   verification: issue 1650 = 11,027/11,027 games after the Latin-1 fix.
6. **PGN encoding fix**: the reader now decodes UTF-8-else-Latin-1 per
   line. Before the fix TWIC 1650 lost 420 games AND imported ~420
   header-less fragments (run-1's reader was silently wrong on the PGN
   spec's own canonical encoding — worth remembering as a class of bug).
7. **Lichess + chess.com clients**: serial, resumable (since / month
   cursors in meta), 429 backoff, descriptive UA with PLACEHOLDER_EMAIL
   for you to fill in. Offline fixture tests (29); live tests env-gated
   behind SILMAN_NET_TESTS=1 and run exactly once each during the build.
8. **ICC & FICS** (user-requested): ICC has no scriptable HTTP surface
   (SPA + websocket protocol; documented stub with manual PGN-export
   path). FICS implemented via ficsgames.org download CGI with
   documented usage caveats (DECISIONS_NEEDED #7).
9. **PGN export** (`export-pgn`) + verification-bar tests: import→export→
   reparse semantic equality on an annotated Latin-1 fixture (mainline +
   headers; annotations excluded per parked decisions), export-reimport
   dedup, and TWIC-vs-personal duplicate collision tests including a
   same-players-same-day negative case.
10. **Repertoire fingerprint** (Phase 2 checkpoint): pure aggregation in
    silman-profile (insta-snapshot + property tests), db adapter + CLI.
    Real output for `sounix` (1,031 games): 58.4% as White on 1.e4 with
    Caro-Kann B12 at 73.7% and Modern B06 at 29.4%; Sicilian repertoire
    as Black; deviation points with example games. Transposition-aware by
    position hash, split by color, per-ECO scores, first-book-exit
    deviations — exactly the shape the Phase 2 prep view needs.
11. **Game browser UI** (read-only, per the parked-decision ruling): a
    Database tab beside the existing Analyze tab — open db, paged/filtered
    game list, click-to-replay on the board with the existing stepper, an
    opening-tree panel that follows the displayed position (W/D/L bars,
    avg elo, perf; clicking a tree row advances along the mainline), and a
    "games reaching this position" list. Verified: clippy/fmt clean, 12
    Rust tests (list SQL, filters, full Opera-game SAN decode), 23 vitest
    tests, tsc + vite green, macOS debug bundle builds. Not verified:
    visual click-through (no screen access) — same caveat as run 1's demo,
    same remedy: `cd app && npm run tauri dev`.

## Numbers (details in docs/BENCHMARKS.md, run-2 section)

| Metric | Value |
|---|---|
| sg4 decode validation | 7,905/7,905 real games |
| si4 import (7,786 games) | 3.9 s |
| TWIC 1650 live ingest (11,027 games) | ~8 s, 0 failures |
| 121k-corpus reimport with ECO | 111 s (1,093 games/s) |
| Opening tree typical / worst | 22 ms / 956 ms (startpos) |
| Fingerprint (1,031 games) | ~0.5 s |

## Deviations / notes

- ROADMAP says si4 import covers "annotations where representable" —
  nothing is representable in encoding v1, so mainline-only import is
  technically compliant, but I am not claiming the criterion in spirit;
  it's the headline blocker above.
- Duplicate-detection rule evolution: header signatures now prefer
  UTCDate; 8 games of the Lichess corpus shifted from duplicate to unique
  vs run 1 (dup rule is parked decision #3 — unchanged in substance).
- The fingerprint's "example game" for a deviation shows any database game
  reaching that position (context, not the player's own game) — labeled
  "e.g." in the CLI.
- New non-blocking decisions filed: opening-tree root-latency escalation
  (#6), ficsgames.org usage posture (#7).

## What a human must do or decide next

1. **Decide encoding v2** (DECISIONS_NEEDED 1–2) — the only Phase 1
   blocker. A concrete best-in-world design was discussed and is
   referenced in the file.
2. Fill in `PLACEHOLDER_EMAIL` in app/silman-db/src/net/mod.rs (User-Agent
   contact) before real Lichess/chess.com use.
3. Bless or tune the dup-detection definition (#3) and the ficsgames.org
   posture (#7).
4. Optional: run `silman-cli twic-sync --from <current-issue>` weekly (or
   ask for a scheduled routine); TWIC scheduling inside the app is still
   open.
5. Scale test toward ≥5M games (megabase) — includes the opening-tree
   root-cache decision (#6).

---

# Run 1 — 2026-07-25

Hardware: Apple M1 Max, 10 cores, 64 GB, macOS (Darwin 25.5.0); rustc 1.94.1,
node 25.9.0. History: 6 conventional commits from empty repo to checkpoint.

## Outcome summary

- **Phase 0: complete**, with two acceptance caveats that require a human or
  CI (see "Remaining risk"): Linux execution of the demo, and real .si4 data.
- **cozy-chess benchmark: GO** — faster than shakmaty on every axis measured
  (movegen 3.6x, attack queries 1.09x, perft(3) 1.8x). docs/BENCHMARKS.md.
- **Phase 1: checkpoint reached** — schema + migrations, streaming PGN
  importer with duplicate detection, ep-normalized Zobrist position index,
  and `silman-cli find-fen` answering "which games reached this FEN" with
  timing printed (63 µs miss / 139 ms worst 72k-hit query on a 121k-game,
  8.15M-position corpus).
- Verification: `cargo fmt --check`, `cargo clippy --workspace --all-targets
  -- -D warnings`, 28 workspace tests, license gate — all green locally; the
  same plus vitest (16) and src-tauri tests (9, incl. two against a real
  Stockfish 18) in app/. Perft suite green at documented depths.

## What was built (by commit)

1. `docs:` project charter/architecture/spec/roadmap (pre-existing content).
2. `build:` cargo workspace (4 BSD crates + GPL silman-db + GPL bench),
   per-crate licenses, CI (fmt/clippy/test/license-gate, Linux+macOS),
   `scripts/license_gate.sh`, perft suite in silman-core.
3. `docs:` benchmark results (GO) + cleanroom si4 format notes.
4. `feat(db):` silman-db — migrations (0001), SAN parser/formatter,
   versioned 1-byte move encoding, malformed-tolerant streaming PGN reader,
   importer with dup detection + provenance, position index, query, CLI.
5. `feat(app):` Tauri v2 demo — chessground board, PGN stepping, tokio UCI
   manager (engine off by default; Analyze is explicit), engine-path
   resolution chain, CI app job. Built by a subagent; reviewed, licenses
   merged into docs/LICENSES.md.
6. `feat(si4):` cleanroom .si4/.sn4 spike + synthetic fixture + dump example.

## Key numbers

| Metric | Value |
|---|---|
| Benchmark GO/NO-GO bar (≤2x shakmaty) | cozy-chess **faster** on all 3 axes |
| Corpus import | 121,220 games / 86 s = 1,409 games/s |
| Positions indexed | 8,154,858 (index ≈ 640 MB db total) |
| find-fen: common/typical/miss | 139 ms / 5–31 ms / 63 µs |
| Duplicates detected in corpus | 112 (+1 planted dup in tests) |

## Deviations from the docs, with rationale

- **Benchmark method:** ROADMAP says "vs published shakmaty numbers on same
  hardware" — contradictory; published numbers are from other hardware. I
  benchmarked both libraries live in one harness (`bench/movegen-bench`,
  GPL-3.0, outside `crates/*` so shakmaty never enters the BSD graph, not
  even as a dev-dependency). Strictly stronger comparison.
- **CPW "position 6" perft figures didn't match its FEN** as commonly
  transcribed; expected counts were re-derived and triple-verified
  (cozy-chess = shakmaty = Stockfish 18 `go perft`). Test renamed to say so.
- **Position hash is ep-normalized** (app/silman-db/src/hash.rs):
  cozy-chess hashes a phantom en-passant file after every double push; raw
  hashes would miss FEN queries and split transpositions (empirically:
  Sicilian-after-1.e4-c5 query returned 0 games before, 11,465 after).
  ARCHITECTURE.md says "Zobrist hash" without this detail; the version is
  recorded in the db (`position_hash_version` = 1) and enforced at open.
- **silman-db is a workspace crate under app/** (ARCHITECTURE places the db
  layer in the app; a separate lib crate keeps the CLI and tests fast and
  keeps heavy Tauri deps out of the root workspace). `app/src-tauri` is a
  standalone cargo package (own lockfile) for the same reason.
- **License gate uses `cargo tree`'s license output** rather than installing
  cargo-license in CI — same check, zero extra tooling; still per-BSD-crate,
  still fails on GPL/LGPL/AGPL anywhere in their trees.
- **Move encoding ordering is self-defined** (sort by from/to/promotion),
  not cozy-chess's internal generation order, so the format is specified by
  the rules of chess rather than a library version (ARCHITECTURE requires
  "deterministic legal-move ordering"; this is the strongest form of it).
- **rust-version bumped 1.80 → 1.82** (for `Option::is_none_or`).

## Cleanroom compliance (si4)

si4-read was written exclusively from community documentation, gathered by a
research agent instructed to never open SCID source; the sources and every
known documentation gap are recorded in docs/SI4_FORMAT_NOTES.md. The
si4spec README links to SCID source as "reference decoder" pointers — those
links were not followed. Gaps that can only be resolved empirically are
listed in DECISIONS_NEEDED.md (§4–5), not resolved by guessing.

## Real-data verification (addendum, same day)

The user pointed at `~/Dropbox/chess/` (read-only). Results:

- **All ten real SCID .si4 databases parse with zero unresolvable entries**
  (47,533 game headers total): benoni2/3, brians, fics, friends_games,
  guy_reams, temp_ma, mypages (3,810 personal FICS/CwF games), twictest
  (4,046 TWIC/World Cup 2011 games), league4545 (39,628 ICC league games;
  its .sg4 is missing but header dumping doesn't need it). Spot-checks match
  known reality (Karjakin 2788 at Khanty-Mansiysk 2011, etc.), and a
  byte-level cross-check of benoni2 against its sibling PGN matched exactly —
  including reproducing odd `Site "3"`-style tags present in the source data.
  Rows shown as `? - ?` are genuine empty comment-only games in the file
  (flags say not-deleted, ply 0, comment count 1), not parse errors.
  → Phase 0's "real .si4 headers dump correctly" criterion **passes**.
- **`mygames` is a PGN, not an .si4** (128 KB, 60 OTB games). Imported with
  0 failures into `testdata/corpus/mygames.sqlite` (10.4 ms, 4,279 positions);
  position search over it works (e.g. 24 games reach the Sicilian after
  1.e4 c5, ~100 µs/query). `bigdatabase` has only an .sn4 (no .si4/.sg4) —
  incomplete remnant, unusable.
- `~/Dropbox/chess/` was not modified; all derived artifacts live in the
  repo's git-ignored `testdata/corpus/`.

Still open from real data: full .sg4 movetext decoding (Phase 1) now has
real files to validate against — the DECISIONS_NEEDED.md §5 ambiguities can
be settled empirically using mypages/twictest.

## What a human must do or decide next (prioritized)

1. ~~Provide real data~~ **Done — see addendum.** Remaining real-data gap:
   ChessBase-exported PGN corpora (the run found none) and the full megabase
   (`bigdatabase` is header-less); point silman at them when available.
2. ~~Run the demo app visually~~ **Done** — user confirmed `cd app && npm
   install && npm run tauri dev` works on macOS.
3. ~~Watch CI~~ **Done** — repo at github.com/avienu/silman; first run
   green on all four jobs (rust + app, each on ubuntu-latest and
   macos-latest), so "builds on Linux" is now verified by an actual run.
   Minor: actions/checkout@v4 and setup-node@v4 emit Node-20 deprecation
   notices; bump to @v5 whenever convenient.
4. **Decide the five parked items in DECISIONS_NEEDED.md** — most urgent are
   the PGN annotation-import policy and null-move encoding (both gate the
   full Phase 1 importer).
5. **Scale test**: full Phase 1 acceptance needs sub-second position search
   on ≥5M games; the largest corpus available this run was 121k games. A
   bigger Lichess month (or your megabase) will answer whether SQLite holds
   or RocksDB escalation is needed.
6. Minor: `tools/` holds Stockfish 18 (git-ignored, user-local); the engine
   path is configurable in-app and via SILMAN_STOCKFISH.

## Test/CI status at end of run

- Root workspace: 28 tests green (perft 6, bench kernel 1, silman-db 16,
  si4-read 5); fmt/clippy/license gate green.
- app/: vitest 16 green; src-tauri fmt/clippy green, 9 tests green including
  live-Stockfish integration; debug bundle builds on macOS.
- CI workflow present for both, unexecuted (no remote configured).
