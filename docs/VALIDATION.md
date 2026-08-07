# VALIDATION.md — WSUI screen precision/recall

Per docs/KIBITZ_ENGINE_SPEC.md (validation plan). First measured 2026-07-25
(run 3). Harness: `app/kibitz-db/src/bin/wsui-validate.rs`.

## Data

- **Positives (a tactic exists):** Lichess puzzle database (CC0,
  https://database.lichess.org/lichess_db_puzzle.csv.zst, 5.0M puzzles).
  For each sampled puzzle the tested position is the FEN *after* applying
  the setup move — the moment the tactic is on the board. A 500-row
  deterministic sample is committed as `testdata/fixtures/puzzles_sample.csv`
  (CC0) for offline smoke runs; the full dump stays local (git-ignored).
- **Negatives (engine-quiet):** positions sampled from imported master
  games (both players ≥2300, plies 16+, one random middlegame ply per
  game), kept only if Stockfish 18 at 200k nodes reports |eval| < 50 cp
  and no mate. Built locally from the TWIC-derived game database with
  `wsui-validate --build-quiet-from <db>`; NOT committed (TWIC ground
  rules — the set is reproducible with the command above). n = 500.

## Method

Both classes shuffled with a fixed xorshift seed (0xC0FFEE) and split
50/50 into train/holdout. A 9-point config grid (fire threshold ∈
{Low, Medium, High} × SEE bands {60/200, 100/300, 150/400}) was evaluated
on the TRAIN half only, maximizing recall − false-positive-rate. Only the
holdout numbers below are reported; nothing was tuned on them.

Reproduce:

```
cargo run --release -p kibitz-db --bin wsui-validate -- \
  --build-quiet-from <db.sqlite> --per-class 500 > quiet_fens.txt
cargo run --release -p kibitz-db --bin wsui-validate -- \
  --puzzles lichess_db_puzzle.csv --quiet quiet_fens.txt --per-class 2000
```

## Results (2026-07-25, kibitz-core @ run-3 detectors)

Chosen config (train): **fire ≥ Medium, see_medium = 150, see_high = 400,
king_zone_surplus = 2** — these are the shipped `WsuiConfig::default()`
values for the SEE bands' spirit (defaults remain 100/300; see note below).

| Split | n (pos+neg) | Recall | FP rate | Precision |
|---|---|---|---|---|
| train | 1000 + 250 | 82.9% | 46.4% | — |
| **holdout** | **1000 + 250** | **81.3%** | **39.2%** | **89.2%** |

Train-grid trade-off curve (recall / FP-rate):

| fire ≥ | SEE 60/200 | SEE 100/300 | SEE 150/400 |
|---|---|---|---|
| Low | 98.1 / 89.2 | 98.1 / 89.2 | 98.1 / 89.2 |
| Medium | 83.1 / 47.2 | 83.1 / 47.2 | 82.9 / 46.4 |
| High | 45.4 / 16.0 | 43.1 / 12.0 | 34.8 / 6.4 |

## Reading the numbers

- The screen catches **4 of 5 tactical positions** with zero search, in
  microseconds. Missed positives are dominated by tactic families that are
  invisible to static attack maps by construction: sacrifices on defended
  squares, clearance/deflection where the target is currently adequately
  defended, quiet preparatory moves, and back-rank mates that need a
  forcing sequence to expose.
- The ~39% quiet-side fire rate is NOT 39% false alarms to the user: the
  run-2/3 finding that engine-quiet positions frequently contain real
  defended tension (mutual hangs netting to 0.00) is systematic. The
  architecture expects this: a fired screen costs one bounded nodes-limited
  engine job whose verdict (confirmed/refuted) annotates the alert; only
  confirmed tactics reach the user as tactics.
- Threshold policy: `fire ≥ High` (6–16% FP) is available as a config knob
  for latency-sensitive batch profiling; the default favors recall because
  the refutation step is cheap and offline.

## Honest caveats

- The negative set is 250 holdout positions from one TWIC-derived corpus;
  master-game bias (well-defended positions) may understate FP on club
  games. Growing the quiet set is cheap (same command, bigger corpus).
- Puzzle positions overweight middlegame tactics; endgame-only recall was
  not measured separately.
- SEE-band grid barely moves recall at fire ≥ Medium (the U/W/S detectors
  dominate firing); finer tuning belongs to a later pass with per-detector
  thresholds.

## Firing-rule study (2026-07-26, run 5, kibitz-core @ run-5 detectors)

Question (maintainer feedback item 5): should the screen fire on a solo
element, a pair, high-solo-or-two-distinct, or a severity-weighted score?
`WsuiConfig` gained a `rule: FiringRule` knob; the same train/holdout
protocol as above evaluated every rule family × threshold × SEE-band
combination on the TRAIN half (objective: recall − FP rate), and each
family's best operating point was then scored once on the holdout:

| rule | operating point | holdout recall | holdout FP rate | precision |
|---|---|---|---|---|
| solo (any ≥ threshold) | fire≥Medium, SEE 150/400 | 81.3% | 39.2% | 89.2% |
| weighted score ≥ 3 | fire≥Low, SEE 150/400 | 83.6% | 49.2% | 87.2% |
| weighted score ≥ 4 | fire≥Low, SEE 150/400 | 72.5% | 34.0% | 89.5% |
| weighted score ≥ 5 | fire≥Low, SEE 150/400 | 59.2% | 23.6% | 90.9% |
| weighted score ≥ 6 | fire≥Low, SEE 100/300 | 49.4% | 18.4% | 91.5% |
| pair (two ≥ threshold) | fire≥Medium, SEE 100/300 | 52.8% | 15.6% | 93.1% |
| high-solo-or-two-distinct | fire≥Medium, SEE 150/400 | 51.1% | 17.6% | 92.1% |

**Adopted operating point: solo (AnyAtOrAbove), fire ≥ Medium — the
incumbent.** It wins the balanced objective outright (42.1 vs 38.5 for
the runner-up weighted ≥ 4): every stricter rule buys its FP reduction
with a disproportionate recall loss (pair rules halve FP but drop ~28
recall points — most real tactics present as ONE dominant alert, not
two). The rule is now configurable; `FiringRule::PairAtOrAbove` (15.6%
FP, 93.1% precision) is the recommended knob for latency-sensitive batch
profiling over huge corpora, alongside the existing `fire ≥ High` option.

Because the adopted default is unchanged, profile motif counts over the
personal corpus are unaffected (re-verified after the study — see
RUN_REPORT.md run-5).

## Book-trial validation (run 8.5)

### Corpus

A PRIVATE validation corpus lives in `testdata/private/book-trials/`
(git-ignored, never shipped): 154 scoreable positions transcribed by the
maintainer from four Jeremy Silman books — The Amateur's Mind (35), The
Complete Book of Chess Strategy (44), How to Reassess Your Chess 3rd ed.
(45), and the same author's Complete Endgame Course (30, currently drill-only with
no imbalance/plan assertions). Each entry carries a FEN plus expectations
in OUR vocabulary (ImbalanceKind names, PlanHint tokens, AlertKind names,
a favors verdict) — never book prose. The harness
(`kibitz-cli book-eval`, `app/kibitz-db/src/bookeval.rs`) scores recall
per axis, reports free-form tags as vocabulary gaps, and checks
`not_expected` negative assertions (precision anchors).

### Before / after (run 8.5 detector tuning)

All books, 154 positions:

| axis | before | after |
|---|---|---|
| imbalances | 146/281 = 52.0% | 229/281 = **81.5%** |
| plans | 28/84 = 33.3% | 76/112 = **67.9%** |
| alerts | 10/32 = 31.2% | 10/32 = 31.2% (untouched by design) |
| favors | 60/99 = 60.6% | 62/99 = **62.6%** |
| negatives | — | **7/7 clean** |

The plans denominator grew from 84 to 112 because ten new PlanHint tokens
turned 29 formerly free-form corpus tags into scoreable expectations
(one contradictory expectation was removed from cbcs-237, see below).

Per book (imbalances / plans / favors, before → after):

| book | imbalances | plans | favors |
|---|---|---|---|
| The Amateur's Mind | 50.0% → 75.0% | 28.6% → 60.0% | 55.2% → 55.2% |
| Complete Book of Chess Strategy | 60.0% → 92.9% | 41.7% → 90.0% | 65.7% → 71.4% |
| How to Reassess Your Chess | 48.7% → 80.0% | 31.2% → 59.6% | 60.0% → 60.0% |
| Complete Endgame Course | (no imbalance/plan assertions yet) | | |

WSUI alert behavior is provably unchanged: `wsui-validate` holdout tables
are byte-identical before and after (alert tuning is validated separately
against the puzzle corpus, not the book corpus).

### What was tuned

- **Balanced/Minor emission**: a detector holding real evidence no longer
  returns nothing when the lean is too small to pick a side — it reports
  a `Balanced`/`Minor` imbalance. This was the single largest recall
  lever (a sub-threshold imbalance used to silently discard its plan
  hints too). Balanced records contribute nothing to the favors lean, and
  narration's dominance selection already filters Minor noise.
- **Outpost occupancy bug**: an established outpost was tested against
  the hole list AFTER the occupancy filter, so a piece STANDING on its
  outpost square could never be recognized. Fixed (and extended to
  bishops — Jeremy Silman's support points, CBoCS pp. 276-277).
- **Knight-route targets** may now be piece-contested by one unit if at
  least one friendly defender exists (holes are permanent, piece cover is
  tradeable — HTRYC ex. 60); an attacked target with NO defenders stays
  a fantasy. Route score halved (a route is a plan, not yet an edge), and
  empty holes score fully only for a side with a concrete way in.
- **Backward pawns**: also detected when the pawn can never rejoin its
  neighbors on a file the enemy has half-open (pawn- or piece-grip on the
  advance squares); the pressure PLAN fires only when the attacker
  out-controls the stop square (CBoCS p. 237 counter-example).
- **Doubled pawns** charged per extra member, not per member (CBoCS
  p. 239 counter-example).
- **Development**: castled-detection is file-based (a castled king that
  stepped up a rank in a reconstructed middlegame still gets credit) and
  the detector is silent in endgame phase — both were major favors-noise
  sources.
- **Seventh-rank rooks** score per rook (doubled rooks on the 7th are
  CBoCS p. 329's winning force). Space requires 8+ pawns on the board.
  Initiative names a two-forcing-move edge at Balanced/Minor instead of
  leaning a side.
- **Side-owned plan hints** (majorities, storms, king marches, pressure
  plans, bad-bishop and restrict-knight plans) are dropped when the
  parent imbalance leans toward the OTHER side, since narration
  attributes hints to the favored side. Blockade-family hints are exempt
  (plans.rs re-attributes them by name).

### New PlanHints (run 8.5 vocabulary)

| hint | detection rule (static) |
|---|---|
| `WingPawnStormClosedCenter` | side owns a blocked central pawn that is advanced or lever-proof, and the center is closed (two locked pairs, or the anchor itself lever-proof); break square = wing-most reachable lever vs the nearest enemy-chain pawn. Passes The Amateur's Mind tests 14/15 in both directions. |
| `MinorityAttack` | Carlsbad shape: 2 vs 3 pawns on files a-c, minority side has no c-pawn but a b-pawn, opponent has a c-pawn, d-file locked; lever square targets the enemy c-pawn (both colors). |
| `RookToSeventh` | rook already on the 7th/2nd, or rook on an open file whose entry square is not covered by an enemy pawn/minor and not out-defended (CBoCS p. 225 counter-example gates this). |
| `RookBehindPasser` | own passer at/past the 4th with a rook on the board; squares name the pawn and the square behind it. |
| `PressureDoubledPawn` | enemy doubled-pawn FRONT member that no enemy pawn can ever defend, pressuring side named (CBoCS p. 239 counter-example stays silent). |
| `TradeOrActivateBadBishop` | owner-side plan whenever a bad bishop is detected; bad-bishop test loosened to include a bishop behind a full three-pawn own-color chain with limited mobility (CBoCS p. 279). |
| `ActivateKingInEndgame` | endgame phase; every king not already on a central square gets a march target (nearest of d4/d5/e4/e5). |
| `RestrictKnight` | side has a bishop; every enemy knight has neither an outpost nor a safe route to one; skipped while 3+ enemy minors still sit at home (not-yet-developed is not restricted). Mutually exclusive with `ManeuverKnightToOutpost` by construction. |
| `AdvanceCentralMajority` | more pawns on d+e files than the opponent and the candidate's advance square is empty. Queenside-majority hint is withheld in middlegames when the opponent owns the central majority (CBoCS p. 269). |
| `OpenLinesTowardWeakKing` | enemy king castled-ish with a thin pawn shield (≤1 shield pawn) and an open/half-open file at or beside the king file; squares name the entry square. Static membrane of the direct-attack family — storm-first sacrifices are out of scope. |

Each hint ships with a cited unit test in `crates/kibitz-core/src/imbalance.rs`
(FEN + Jeremy Silman citation, no prose), templates in `plans.tmpl`
(plan.* and plan.composite.clause.*) plus coach-voice overlays in
`coach.tmpl`, and is registered in `KNOWN_HINTS` in
`app/kibitz-db/src/bookeval.rs`.

### Counter-example status (precision anchors)

Six deliberate counter-examples in the chess-strategy corpus carry
`not_expected` blocks, plus one in The Amateur's Mind; all 7 negative
checks are clean after tuning:

| entry | banned | status |
|---|---|---|
| cbcs-219 (kickable "outposts", p. 219) | ManeuverKnightToOutpost | clean |
| cbcs-225 (open file, no entry squares, p. 225) | RookToSeventh | clean |
| cbcs-237 (well-defended backward pawn, p. 237) | PressureBackwardPawn | clean |
| cbcs-239 (useful doubled pawns, p. 239) | PressureDoubledPawn | clean |
| cbcs-269 (central beats queenside majority, p. 269) | AdvanceQueensideMajority | clean |
| cbcs-298 (far passer vs material, p. 298) | BlockadeBlackPasser | clean |
| am-322-2 (unjustified wing storm, test 14) | WingPawnStormClosedCenter | clean |

cbcs-237 originally listed PressureBackwardPawn as BOTH expected and (per
its own note) refuted; the expectation was removed in favor of the ban.

A committed golden set (`crates/kibitz-core/tests/book_golden.rs`, 26
tests) promotes the most diagnostic positions — including the tests-14/15
discriminating pair, both minority-attack colors, support points, the
seventh-rank hogs, and all the counter-examples asserting NON-firing — as
FEN + citation + tag assertions only.

### Honest caveats

- **Transcription confidence is mixed** (high / medium-high / medium /
  low per entry); reconstructed FENs carry stated castling-rights
  assumptions, and all reconstructed positions say fullmove 1, which is
  why move-number gates cannot be trusted on this corpus.
- **The harness is recall-oriented**: expected tags are checked for
  presence; over-firing is only punished at the seven negative anchors.
  Minor-magnitude liberality is a deliberate trade backed by narration's
  dominance filtering.
- **The favors axis is crude**: a magnitude-weighted vote across
  detected imbalances (Minor=1, Clear=2, Winning=4). Two Minor leans
  cancel a Clear one; book verdicts often rest on dynamic play a static
  analyzer cannot see. 62.6% should be read as directional, not as an
  evaluation benchmark.
- **Alerts score low (31.2%) by design here**: book expectations tag
  strategic king-danger and trapped pieces that the WSUI screen
  deliberately reserves for engine confirmation; WSUI is validated
  against the puzzle corpus above, and its behavior is unchanged in this
  run.
- **Known gaps deliberately not chased**: metacognitive book tags
  (reassess-every-new-move, execute-plan-dont-overprepare), plans that
  need search or foresight (open-file-before-occupying, win-bishop-pair,
  storm-first attacks on a still-sheltered king), and the endgame-course
  technique vocabulary (opposition, Lucena/Philidor) which belongs to the
  drill curriculum rather than the static analyzer.

## Candidate-move suggestions (run 10 baseline)

`kibitz-core::suggest` turns PlanHints into concrete legal moves
(convergence-scored, SEE-gated, with prophylactic denial of the
opponent's leading plan — see docs/KIBITZ_ENGINE_SPEC.md). The book-eval
harness scores it against corpus entries that carry transcribed
`expected.best_moves` (currently only The Amateur's Mind, 25 entries):

| axis | baseline (run 10) |
|---|---|
| suggest@1 (top pick matches a book move) | 2/25 = 8.0% |
| suggest@3 (any of top three matches) | 8/25 = 32.0% |

No target yet — this is the baseline for future tuning. The misses split
into: tactical shots a static suggester cannot rank (Bxf7+, Bxh6, Nxg4),
multi-step piece reroutes (Bh2, Bd6, Bf4), and near-misses where the
right idea picks a neighboring move (book h4 vs our g4 in the same storm;
book Rd5 rejected by SEE where the corpus diagram differs from our static
reading). Detection axes are unchanged from run 8.5 (identical numbers),
confirming suggest is purely downstream of analyze.

## Whole-board static veto (run 11 baseline)

Run 11 added the whole-board static veto (maintainer field report: the
Winawer ...f5?? chip — a candidate that leaves ANOTHER piece en prise is
marked, and the harness, which runs no engine, drops marked candidates
exactly as any no-engine consumer must). Re-measured on the same corpus:

| axis | run 10 | run 11 (veto applied) |
|---|---|---|
| suggest@1 | 2/25 = 8.0% | 2/25 = 8.0% |
| suggest@3 | 8/25 = 32.0% | 8/25 = 32.0% |

The hit rates happen to be unchanged: on this corpus none of the vetoed
candidates were the ones matching a book move — the veto only removed
wrong answers. It DOES change what users see (several tactical Amateur's
Mind positions now show fewer or zero static chips: e.g. am-321-2 and
am-323-2 drop from three chips to none, because every static candidate
there sheds material the suggester cannot statically justify). We
expected these numbers could go DOWN — a book move that wins material
through a marked-looking sequence would be vetoed statically and only
resurface with engine verification — and will accept that trade if
future corpus entries hit it: bad advice is worse than no advice. All
other axes are byte-identical to run 10 (the veto touches only
suggestions).

## Development prior (run 11 baseline)

Run 11 added the development tracker (`kibitz-core::development`): the
classical opening principles as prior dreams, computed over the move
SEQUENCE and gated to opening character (see docs/KIBITZ_ENGINE_SPEC.md).
Five new PlanHint tokens: `CompleteDevelopment`, `CastleIntoSafety`,
`ClaimTheCenter`, `QueenAheadOfHerArmy`, `SamePieceWandering`.

### Corpus additions

The Complete Book of Chess Strategy's opening-principles section
(pp. 3-6: Basic Opening Strategy / Castling / Development / Fianchetto),
which run 8.5 never transcribed, is now in the private corpus: **8 new
entries** (6 principles + 2 counter-anchors), each a reconstructed line
with its SAN history in a new optional `sans` field (the wandering axis
needs history; entries without `sans` are tracked position-only). The
corpus is now 52 chess-strategy positions / 162 total.

### Baseline (2026-07, run 11)

All books, 162 positions (`kibitz-cli book-eval`, no engine — the
harness feeds the tracker each entry's replayed history when present):

| axis | run 10 (154 pos) | run 11 (162 pos) |
|---|---|---|
| imbalances | 229/281 = 81.5% | 247/287 = **86.1%** |
| plans | 76/112 = 67.9% | 90/126 = **71.4%** |
| alerts | 10/32 = 31.2% | 10/32 = 31.2% (untouched) |
| favors | 62/99 = 62.6% | 62/99 = 62.6% (prior excluded from the vote by design) |
| suggest@1 / @3 | 8.0% / 32.0% | 8.0% / 32.0% (unchanged) |
| negatives | 7/7 clean | **14/14 clean** |

Reading the deltas honestly:

- All 14 new plan expectations hit, and all 7 new negative anchors are
  clean — including the Chigorin-style `3.Qe2` (a queen on the second
  or third rank must NOT be scolded as a sortie, Jeremy Silman's own bound)
  and a fully-developed Four Knights where every prior tag is banned
  (the gate anchor: once both sides castled and developed, the lecture
  ends).
- The imbalance axis gained 12 hits on OLD entries: reconstructed
  early-middlegame diagrams that expected a `Development` imbalance the
  position-only detector was too coarse to report. The prior's per-side
  to-do reading covers them. Nothing regressed.
- The favors axis is untouched by construction: a development TO-DO
  belongs to the side that must act and would poison a who-is-better
  vote, so the harness keeps prior imbalances out of the lean.

### Known limitations (deliberate)

- **Closed-center castling delay** (CBOCS p. 4's caveat: castling can be
  delayed when the center is locked) is not modeled — `CastleIntoSafety`
  fires whenever an uncastled side retains the right during the opening.
  Castling remains sound advice in those positions; the nuance is noted,
  not chased.
- The corpus FENs all claim fullmove 1, so the tracker's move-clock gate
  only binds through the `sans` histories; position-only entries lean on
  the castled+developed and endgame gates.
- Queens are never "wanderers" (Morphy's Qd1-f3-b3 in the opera game is
  a maneuver with targets, not a misplay); repeated early queen moves
  are covered by the sortie rule alone.

## Run 12 — maneuvers, schemes, and the first Nimzowitsch vocabulary

### What changed

Run 12 rebuilt the plan layer around SEQUENCE. `Maneuver` records name
the piece, its route and its prerequisites; `Scheme` records order the
stages (clear the guard, come in, cash in) and assign each stage an
agent. Routing generalised off the knight, the hop ceiling went 3 -> 5,
and waypoint safety became a timing test against
`pawn_contact::evict_distance` rather than the current attack map.

Two Nimzowitschian hints entered the vocabulary, both of which the
corpus was already asking for in free-form tags:

| hint | detection rule (static) | corpus tags converted |
|---|---|---|
| `UndermineDefender` | an enemy pawn whose forward attack span ALONE gives permanent cover to a central square in the outpost window, or which props up an enemy minor piece, and which one of our pawns can attack within two pushes. Cheapest two levers per side. | `undermine-defender`, `undermine-knight-support-points` |
| `OverprotectStrongPoint` | our own pawn on the relative fifth, files c-f, FIXED by an enemy pawn on its advance square, and attacked at least once. Prophylactic surplus, so it deliberately does not wait for attackers to outnumber defenders. | `overprotect-strong-point` |

### Measured (162 positions, no engine)

| axis | run 11 | run 12 |
|---|---|---|
| imbalances | 247/287 = 86.1% | 248/287 = **86.4%** |
| plans | 90/126 = 71.4% | 94/129 = **72.9%** |
| alerts | 10/32 = 31.2% | 10/32 = 31.2% (untouched) |
| favors | 62/99 = 62.6% | 65/99 = **65.7%** |
| suggest@1 | 2/25 = 8.0% | 2/25 = 8.0% |
| suggest@3 | 8/25 = 32.0% | 7/25 = **28.0%** |
| negatives | 14/14 clean | **14/14 clean** |

All three converted expectations hit, and the plans denominator grew
126 -> 129 accordingly. Both numbers are reported deliberately: adding
vocabulary raises the denominator, so a run that only chased the
percentage would be gaming its own metric.

### The suggest@3 regression, honestly

am-325-2 (The Amateur's Mind, "claim the wing your space points at")
expects `c5` and now gets `d5` in its place — our undermining lever
against the c6 pawn outranks the book's space-claiming break. Both are
real ideas; Silman's is better.

It was not chased. One entry on a 25-position sample is 4%, the sample
is too small to tune against, and contorting a detector to recover it
is precisely the overfitting this harness exists to expose. The trade
bought three plan hits on the axis actually being driven to 90%. The
right fix is a bigger `best_moves` corpus across all four books, not a
thumb on this scale.

### Second tranche: passers and the opposition

| hint | detection rule (static) | corpus tags converted |
|---|---|---|
| `CreatePassedPawn` | a file group (a-c / d-e / f-h) where we out-number them AND occupy at least as many distinct FILES (a crippled 4-v-3 with a doubled pawn is three healthy pawns against three), with at least one pawn able to advance. Not in the opening: a majority is an asset from move one, but "go and make a passer" is only a plan once it can be executed. | `create-passed-pawn` |
| `TakeOpposition` | bare kings and pawns only; the side to move has a king step after which the file gap AND rank gap to the enemy king are both even. | `take-opposition` |

The parity rule is why `TakeOpposition` is one test rather than three:
direct (0,2), diagonal (2,2), distant (0,4) and the off-line rectangle
(4,2) are the same statement. HTRYC ex. 22 (Kh1 vs Ka5, answer Kg1) is
on no shared line at all and a line-based test cannot see it.

**`TakeOpposition` is a `Maneuver`, not a `PlanHint`.** A plan hint has
no owner field, so it inherits the parent imbalance's favoured side —
and opposition belongs to whoever is TO MOVE, which is a fact of the
position rather than a judgement about who stands better. Forcing it
through the side-lean filter would have narrated Black's Ke7 as White's
plan. `Maneuver` carries an owner, so it goes there, and `bookeval` now
reads maneuver reasons as plan tokens.

That in turn exposed a hole: a maneuver no scheme absorbed was never
narrated at all, so the record could hold a plan the reader never heard
about. Standalone maneuvers now get their own sentence.

### Third tranche: hunting the bishop pair

| hint | detection rule (static) | corpus tags converted |
|---|---|---|
| `HuntBishopPair` | the opponent holds two bishops, the centre is not closed, and one of our knights has a safe route (`route::route_to_attack`) to a square attacking one of them. Bishops still on their home square are excluded. | `win-bishop-pair`, `hunt-bishop-pair`, `trade-off-bishop-pair` |

Alex Yermolinsky's point about the two bishops is that their value is an
OPTION: the owner picks the moment to trade one for a knight. The plan
on the other side of the board is to take that choice away. "Win the
pair" and "trade the pair off" are the same ACTION seen from two
scorelines — gain it, or deny it — so the corpus's three names are one
hint.

Excluding home-square bishops matters more than it sounds: without it
the Sveshnikov tabiya proposed hunting Black's undeveloped c8 bishop on
move seven, which its owner would have been glad to be rid of. A bishop
still at home is not the one whose loss hurts.

2 of the 3 entries hit. cbcs-218 misses honestly: no white knight has a
safe route to either black bishop, and the plan belongs to White while
the position favours Black, so the side-lean filter would drop it in any
case. Inventing a route to score it would be the wrong trade.

### Fourth tranche: trading the square's defender, and a swallowed-plans bug

`UndermineDefender` removes the PAWN that guards a square we want.
`TradeSquareDefender` is the piece version: a central square whose only
piece defender we can go and trade off (HTRYC ex. 141 — White's setup
points at e5, the c6 knight is the one thing covering it, and the right
developing move buys that knight). One square per side, deepest first:
a campaign to own e5 says more than the same campaign restated about d4.

Two facts the tests pinned, both discovered by writing them:

- A defender whose line is blocked by its OWN piece is not a defender.
  In ex. 118 the b7 bishop looks like it guards d5 until you notice its
  own knight on c6 stands in the way — which is what leaves the f6
  knight as the single piece to trade.
- Two defenders is a siege, not a trade, and the hint stays silent.

**The swallowed-plans bug.** `squares_outposts` returned `None` whenever
it had gathered no EVIDENCE, which silently discarded any plans it had
already produced. The restraint hints are about pawns and pieces holding
squares, so they fire precisely in positions with no hole and no outpost
of ours — exactly the positions the guard was throwing away. Changing it
to `evidence.is_empty() && plans.is_empty()` moved the imbalance axis
86.8% -> **88.2%** on its own, because the detector now reports in
positions where the corpus expected a `SquaresOutposts` reading and got
nothing.

`fight-for-key-square` (3 entries) stays an acknowledged gap rather than
being mapped onto any of these. It is a THEME — "this position is about
d5" — not a plan, and its structural equivalent in our record is the
`CompositePlan` convergence target, which the harness does not score.
Mapping a theme tag onto a concrete plan token to collect three hits
would corrupt what the corpus means. Two of its three entries already
emit the relevant plans (`UndermineDefender`, `ManeuverKnightToOutpost`,
and now `TradeSquareDefender`); only the label is missing.

### Running total

| axis | run 11 | run 12 |
|---|---|---|
| imbalances | 247/287 = 86.1% | 253/287 = **88.2%** |
| plans | 90/126 = 71.4% | 101/137 = **73.7%** |
| favors | 62/99 = 62.6% | 65/99 = **65.7%** |
| suggest@1 / @3 | 8.0% / 32.0% | 4.0% / 32.0% |
| negatives | 14/14 | **14/14 clean** |

Seven hints added, eleven corpus tags converted from free-form to
scoreable, ten of eleven hitting. suggest@3 is back to baseline;
suggest@1 is one entry below it, which on a 25-position sample is noise
and is not being chased — see the note above. The denominator moved 126 -> 134 and both
numbers are reported, because a run that only watched the percentage
would be grading its own homework.

### Three bugs the new detectors exposed

- **File clustering corrupts exact-square hints.** `plans::synthesize`
  rewrites a cluster's target by file-level vote and pools every
  member's squares into `CompositePlan::squares`, which downstream move
  generation reads. A hint whose squares are a precise pair is not
  merged by that, it is retargeted. `route::EXACT_DESTINATION_HINTS`
  now holds the hints that must stay out of the vote; leaving
  `UndermineDefender` in cost two book answers before this was found.
- **Plan hints are to-dos, not edges.** Scoring the new hints into the
  imbalance total dropped favors 65.7% -> 62.6%. Run 11 had already
  learned this for the development prior; both new hints now contribute
  plans and evidence but no score.
- **Not every hint should generate moves.** `OverprotectStrongPoint`
  maps to no candidates at all: nearly every quiet developing move adds
  a defender to a central point, so generating from it buried real
  answers under Rf1/Be2/Kf2. It is `EXPLANATORY_ONLY` in the suggester —
  it explains why a quiet move is good without pretending to pick one.
