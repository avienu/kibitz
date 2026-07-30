# KIBITZ_ENGINE_SPEC.md

The novel core: explain a position the way a strong human coach does. Two stages:
(1) a cheap static tactical screen decides whether calculation is even relevant;
(2) if not, a positional imbalance assessment produces the verbal evaluation and
plan. The full engine is a gated verifier, not the primary explainer.

## Stage 1 — WSUI tactical screen (kibitz-core::wsui)

Run for BOTH sides, side-to-move first. All detectors are static (attack/defense
maps from cozy-chess), no search. Output: `Vec<TacticAlert>`.

Detectors (initial set; each returns alerts with squares/pieces involved and a
coarse severity):

- **W — Weak king**: king-zone attacker vs defender count; pawn-shield defects
  (missing/advanced shield pawns, open/half-open files toward the king); back-rank
  vulnerability (confined king + insufficient back-rank defenders); exposed king
  on open lines/diagonals.
- **S — Stalemated (trapped) pieces**: piece with 0 safe squares (all moves lose
  material or are illegal); severity scales with piece value; includes trapped-
  piece-attackable check.
- **U — Undefended pieces**: attacked-or-attackable pieces with zero defenders
  (loose pieces). Distinguish currently-attacked from merely-loose.
- **I — Inadequately defended**: attackers > defenders on a piece with favorable
  capture sequence (static exchange evaluation, SEE, via cozy-chess move data);
  overloaded defenders (one piece is the sole defender of ≥2 targets); pinned
  defenders don't count as full defenders.

Gate rule: if any alert ≥ threshold severity fires → enqueue a bounded engine
job (suggested initial budget: nodes-limited Stockfish, MultiPV 3) to verify or
refute a concrete tactic. The engine result annotates the alert
(confirmed: PV + score delta | refuted | unclear-at-budget). Thresholds and
budget are tunable config, benchmarked in Phase 3 against a tactical test set
(use Lichess puzzle positions as ground truth: screen should fire on ≥X% of
puzzle positions and on ≤Y% of quiet positions — measure, then set X, Y).

## Stage 2 — Imbalance assessment (kibitz-core::imbalance)

Runs when no tactic dominates (and also alongside, for context). Each detector
returns `Imbalance { kind, favors: Color|Balanced, magnitude: Minor|Clear|Winning,
evidence: squares/pieces/pawns, indicated_plans: Vec<PlanHint> }`.

Detectors:

1. **Minor-piece imbalance**: B vs N count per side; bishop pair; good/bad bishop
   (own pawns on bishop's color complex, mobility); knight quality (available
   outposts, proximity to action); open vs closed character of the position
   (locked central pawn chains) → which minor piece the structure favors.
2. **Pawn structure**: isolated, doubled, backward, hanging, passed (+ protected/
   connected/outside), chains and their bases, majorities per wing, pawn breaks
   available. PlanHints: which wing to play on (chain direction / majority),
   which break to prepare, blockade targets.
3. **Material**: raw count + standard imbalance patterns (R vs B+N, Q vs pieces,
   exchange, compensation flags deferred to engine confirmation).
4. **Files & diagonals**: open/half-open files, who controls them (doubled majors),
   7th-rank occupation potential; long-diagonal control.
5. **Squares & outposts**: holes (squares indefensible by pawns) in each camp;
   established vs available outposts; PlanHint: maneuver route for the knight
   (simple BFS over safe squares).
6. **Space**: pawn-defined space count per wing; cramped side → PlanHint: trade
   pieces / prepare a break.
7. **Development**: developed-minor count, castling status, uncoordinated pieces;
   only meaningful before a move threshold or with closed center caveat.
   PlanHint (if leading): open the position, act before opponent completes.
   Since run 11 this detector's who-is-ahead story is complemented by the
   history-fed development PRIOR (see "Development tracker" below), which
   reports each side's development TO-DO as separate Development
   imbalances.
8. **Initiative**: threat count per tempo (moves creating forcing replies),
   who is dictating; interacts with development lead.

## FeatureRecord (kibitz-core::record) — the universal contract

Versioned serde struct, JSON-stable. Sketch (finalize in code, keep this doc
in sync):

```json
{
  "schema_version": 1,
  "fen": "...",
  "side_to_move": "white",
  "phase": "middlegame",            // opening|middlegame|endgame (material+move based)
  "wsui": {
    "alerts": [ { "kind": "InadequatelyDefended", "side": "black",
                  "target": "c6", "attackers": ["e5","d4"], "defenders": ["b7"],
                  "see": 200, "severity": "high",
                  "engine_check": { "status": "confirmed", "pv": ["Nxc6","bxc6","Qd4"],
                                     "score_delta_cp": 180, "budget_nodes": 2000000 } } ],
    "screen_fired": true
  },
  "imbalances": [ { "kind": "PawnStructure", "favors": "white", "magnitude": "clear",
                    "evidence": { "isolated": ["d5"], "half_open_files": ["d"] },
                    "plans": [ { "hint": "BlockadeThenPressure", "squares": ["d4","d5"] } ] } ],
  "engine": null | { "eval_cp": 45, "best": "Nf3", "multipv": [...] },   // present only if run
  "provenance": { "generator": "kibitz-core", "version": "0.1.0" }
}
```

Version history: v2 added `mate_in` (EngineCheck beneficiary-POV /
EngineEval White-POV) and `composite_plans`; v3 added the Explanation
contract below (no change to the record fields themselves).

Consumers: kibitz-verbalize (single-position prose), kibitz-profile
(aggregation), app trainers (weakness targeting), app UI (highlight overlays
from `evidence` squares).

## Explanation (kibitz-core::record, built by kibitz-verbalize::explain) — the game-view contract

Schema v3 (run 6, design/handoff-1). One object per analyzed position; the
UI never synthesizes prose or evidence. Both voices arrive pre-rendered so
the voice toggle is instant and provably never changes the evidence.

```json
{
  "schema_version": 3,
  "tag": "TACTICAL SCREEN FIRED",    // | "FORCED MATE" | "QUIET POSITION"
  "eval": { "cp": 260, "mate": null, "display": "+2.6" },   // White POV; null when nothing ran
  "headline": { "coach": "...", "neutral": "..." },          // lead sentence, removed from block 0
  "blocks": [ {
      "kind": "alert",               // alert | imbalance | plan
      "text": { "coach": "...", "neutral": "..." },
      "evidence": {
        "alerts": ["c6"],            // red ring squares
        "attackers": ["e5","b5"],    // amber wedge squares
        "defenders": ["b7"],         // blue wedge squares (never arrows)
        "imbalance": [],             // green wash squares
        "key": [],                   // violet wedge squares (plan targets)
        "arrows": [ { "from": "e5", "to": "c6", "kind": "attacker" } ]  // kind: attacker|key
      } } ],
  "suggestions": [ {                 // run 10; omitted when empty
      "san": "Nd5", "uci": "c3d5", "score": 4,
      "serving": ["ManeuverKnightToOutpost", "PressureBackwardPawn"],
      "prophylactic": false,         // true = denies the opponent's plan
      "static_risk": 230,            // run 11; omitted when statically clean —
                                     // marked chips render only engine-cleared
      "evidence": { "key": ["d5"], "arrows": [ { "from": "c3", "to": "d5", "kind": "key" } ] }
  } ]
}
```

Block order mirrors narration: alerts (severity-desc, clause-deduped per
voice), dominance-selected imbalances, composite plans then leftover single
hints. Evidence rules: alert blocks ring the target and arrow every
attacker into it; imbalance blocks wash every square found in the
imbalance's structured evidence values; plan blocks mark key squares and
draw route arrows (first→last hint square, and composite route pairs ending
on the target). Prose bands: at ±5.00 pawns (`DECISIVE_CP`) engine prose
states a verdict ("simply winning") and the number stays in `eval`; mate
wording always outranks any band.

Suggestions (run 10, `kibitz-core::suggest`): up to three legal moves for
the side to move, synthesized statically from the record's plan hints —
each PlanHint token has a move-mapper (execute 3 / prepare 2 / enable 1);
a move serving several plans scores the sum of its weights plus one point
per extra plan served (convergence). When the opponent's leading plan
rivals ours (within one point), blocking candidates compete on equal terms
and are flagged `prophylactic`; their `serving` list leads with the denied
opponent tokens. A SEE safety gate (−60 cp) drops hanging candidates. The
field is empty whenever a confirmed tactic, known mate, or decisive engine
line gates positional talk, and the app additionally strips it on capture
plies (mid-exchange the only honest advice is to finish the exchange).
Adding this optional field did not change existing fields; the schema
stays v3.

### Suggestion verification (run 11): static veto + cursory engine review

The destination-only SEE gate misses candidates that abandon ANOTHER
piece (field report: French Winawer after 5.a3 — `f5??`/`f6??` shipped as
chips while the b4-bishop hung to axb4). Two layers fix this:

**Static whole-board veto (kibitz-core, no engine).** After each
surviving candidate, the opponent's best SEE capture anywhere on the
board is computed; when it nets `PIECE_LOSS_CP` (220 cp — the cheapest
piece-for-pawn loss) beyond what the move itself captured, the candidate
is MARKED, not dropped: `SuggestionOut` gains an optional `static_risk`
field (the net swing, cp; absent when clean — additive, schema stays v3).
When a piece is already en prise before the move, only candidates that
bring the swing back under the threshold stay clean; the rest are marked.
Statics one exchange deep cannot tell the Winawer theory move `...cxd4`
(axb4 is met by dxc3, regaining the piece) from the losers — it is marked
too, deliberately. Consumers with NO engine must drop marked candidates:
the narration closing does (`suggestion_closing_verified` with no cleared
list), the book-eval harness does, and the UI never renders a marked chip
unverified. Bad advice is worse than no advice.

**Cursory engine review (app layer, sanctioned trigger only).** The
maintainer's ruling: "at least a cursory engine review, at least if
tactics screen is present (WSUI)". When `wsui.screen_fired` — and only
then — the app runs one baseline bounded search of the position plus one
`go nodes` search per candidate (≤3 candidates + baseline = ≤4 searches
at 150k nodes each, `kibitz_db::verify::VERIFY_NODES`). The decision is
pure (`kibitz_db::verify::decide`): a candidate is REFUTED when its
mover-POV eval falls more than 150 cp (`REFUTE_MARGIN_CP`) below the
baseline (mate folds into a ±10000 sentinel); marked candidates need an
eval to CLEAR, clean candidates survive unless refuted. Two paths:

- Live explain: the `explain_position` IPC stays instant and static; the
  frontend then calls `verify_suggestions(fen)` (only when the screen
  fired and chips exist — the backend re-checks the gate and returns
  `{ ran: false }` for quiet positions without touching the engine).
  Response: `{ fen, ran, verdicts: [{ uci, san,
  verdict: "cleared"|"refuted" }], nodesPerSearch }` — FEN-stamped like
  `engine-info` events so ply-stepping drops stale results. Chip rules:
  clean chips render immediately (subtle pending state while the
  round-trip runs), refuted chips disappear, marked chips appear only
  when cleared; engine unavailable leaves marked chips hidden.
- Batch annotate: the wsui-confirm job's search doubles as the baseline
  and the same engine reviews the candidates; the cleared uci list is
  stored in the job result (`cleared_suggestions`) and the narration
  closing renders only cleared moves at reviewed plies.

**Annotate-time verification at closing-eligible plies (2026-07-29 field
report).** Confining the review to fired screens made the coach safe but
silent: in real middlegames the whole-board veto marks most candidates,
wsui-confirm jobs exist only where the screen fired, and quiet plan plies
therefore almost never produced a closing ("even though I'm in annotate
mode I don't see a recommended move"). Annotate — an explicit user engine
action under the run-9 ruling — now additionally enqueues one bounded
`suggest-verify` job per quiet closing-eligible ply: screen NOT fired
(fired plies already get the review via wsui-confirm), composite plans
present, static suggestions present, and not a capture ply (the
narrator's own closing gate). The job runs the identical review — one
baseline search plus one per candidate, ≤4 searches at `VERIFY_NODES`,
all folded to the side-to-move POV — and stores `cleared_suggestions` in
the same result shape, which the verdict loader merges into the per-ply
map (status-less: a suggestion review never grades an alert, and
fold-back's confirmed/refuted accounting stays wsui-confirm-only). After
fold-back, closings render engine-cleared moves at quiet plan plies;
refuted candidates still never appear.

A quiet position never triggers any engine work from this feature
outside an explicit user action (CLAUDE.md #6): live explain stays
static, and the suggest-verify jobs are enqueued only by Annotate and run
only when the job worker is started.

## Development tracker — the prior side of the dream system (run 11)

`kibitz-core::development` voices the classical opening principles as
dreams-under-uncertainty (never a rulebook): every other dream derives
from pawn structure, which is fog at move 5, so the opening needs a
PRIOR. It is a pure function over the MOVE SEQUENCE
(`track(start, &moves) -> DevelopmentReport`), because "this piece
already moved twice" needs history; with an empty move list it still
reports everything a bare position can show (wandering excepted).

Per side: minor pieces still on their home squares (listed), castled /
castling-available / king-in-center state, queen sortie (queen beyond
the third relative rank while two or more minors sleep — Jeremy Silman's CBOCS
p. 5 bound: second or third rank is fine), same-piece wandering (a
minor or rook moved twice-plus while two or more minors sleep; queens
excluded — repeated queen moves are the sortie rule's story; a piece
the enemy can profitably capture is mid-exchange, not wandering),
still-home center pawns and their unplayed two-square advances, and a
rough tempo balance. **Opening gate**: the tracker reports only before
fullmove 14 OR until both sides are castled and fully developed,
whichever comes first — and never in an endgame.

`imbalances(&report)` emits one `Development` imbalance per side with
dreams left, **favors = the side that OWNS the plans** (a to-do, not an
advantage — the verbalizer renders these with dedicated to-do headlines
and the book-eval harness keeps them out of the favors vote), carrying
five additive PlanHint tokens (schema stays v3):

| hint | evidence / squares |
|---|---|
| `CompleteDevelopment` | the sleeping minors' squares |
| `CastleIntoSafety` | king + preferred rook square |
| `ClaimTheCenter` | the unplayed center-pawn advance squares |
| `QueenAheadOfHerArmy` | the sortie queen's square (misplay observation) |
| `SamePieceWandering` | the wanderer's square + spelled-out move count (misplay observation) |

These flow through the SAME machinery as every other hint: plan
synthesis (the two misplay tokens and the location-shaped hints are
excluded from composite clustering — only `ClaimTheCenter` names a real
target), narration, explain blocks, and the suggestion mappers
(knight developments to natural central squares execute while bishops
prepare — the knight already knows where it wants to go; the castling
move executes with path-clearing enables; the hinted center pushes
execute). Prior tokens are never offered for prophylactic denial.
`analyze_with_history(start, moves)` is the one-call form; callers that
gate on external state (the openings book) call `track`/`augment`
directly.

**Book awareness (app layer)**: while the position is still in the
bundled CC0 openings book (`kibitz_db::fingerprint::theory_set`), the
narration walk and the live explain IPC withhold the development prior
and its suggestions and render a single quiet book line instead
(narrated once per game; the state latches at the first out-of-book
position). The explain IPC accepts additive optional `sans`/`start_fen`
params carrying the game so far; without them the tracker is silent and
position-only callers are unchanged. Book state never enters
kibitz-core — callers pass `in_book` into the assembly
(`kibitz_verbalize::explain_in_book`, `book_line`).

## kibitz-profile — corpus profiling

Input: iterator of (game, side-of-interest) + per-position FeatureRecords
(computed statically; engine numbers only where a batch job supplied them).
Output `PlayerProfile`:

- per-phase ACPL and blunder/mistake/inaccuracy counts (engine-dependent;
  computed only for corpora the user has batch-analyzed),
- motif matrix: for each tactical motif, opportunities-missed and
  tactics-allowed rates (WSUI + motif taggers make this computable WITHOUT
  full engine runs: screen-fired-and-ignored is a cheap proxy; engine-confirmed
  where available),
- structure/opening report: score and error rates by ECO family and by
  pawn-structure family,
- conversion: result distribution from first-reached ≥+2.0 / ≤−1.0 (engine-
  dependent),
- fingerprint: repertoire tree with frequencies, scores, and deviation points
  from mainline theory (vs bundled openings dataset).

Same code path profiles an opponent (prep) and the user (training targets).

## kibitz-verbalize

- Template mode (default, offline): deterministic prose from FeatureRecord.
  Templates in data files, one per (kind, magnitude, phase) with slot filling;
  composition rules order output: tactical alerts → dominant imbalances → plans.
- LLM mode (optional feature flag): trait `Verbalizer`; LLM impl receives the
  FeatureRecord JSON + strict system prompt (verbalize only supplied facts).
  Post-validation: extract all move/square mentions from output; every move must
  appear in record PVs/candidates and be legal in the position; on failure,
  return template-mode output instead. Never ship LLM output that fails
  validation.

## Validation plan (Phase 3 gate)

- WSUI screen: precision/recall against a sampled Lichess-puzzle set (positive
  class) and quiet positions sampled from master games with |eval| < 50cp and no
  puzzle at that ply (negative class). Publish the numbers in the repo.
- Imbalance detectors: golden-file tests on canonical instructional positions;
  every detector documents its positions and expected output.
- End-to-end: annotate a fixed set of games; snapshot-test the FeatureRecords.
