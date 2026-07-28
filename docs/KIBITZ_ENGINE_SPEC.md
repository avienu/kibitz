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
