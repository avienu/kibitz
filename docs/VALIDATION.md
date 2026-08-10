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
- **Alerts score low (31.2%), and the claim that this was "by design"
  was wrong** — see the partition below. It stood unexamined from run 8.5
  until someone asked what the other 68.8% was made of.
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
real ideas; Jeremy Silman's is better.

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

### Fifth tranche: the best piece, and their best attacker

| hint | detection rule (static) | corpus tags converted |
|---|---|---|
| `KeepBestPiece` | our minor standing in the enemy half on a square its own pawn defends. | `keep-best-piece` |
| `TradeOffAttacker` | the enemy minor covering the most squares of our king's neighbourhood, provided we have a piece that can route to attack it. | `trade-attacker`, `trade-off-attackers` |

The two `TradeOffAttacker` entries look unrelated until you name the
piece: HTRYC ex. 130 trades White's light-squared bishop because it is
the only real attacking plan, and ex. 218 gives up a prized bishop to
remove a centralised knight. Both are "find their best attacker and take
it off", so both are one hint. All three entries hit.

The two hints are complements, and the Amateur's Mind position shows it:
White gets `KeepBestPiece e5` and Black gets `TradeOffAttacker e5` for
the same knight, which is exactly the argument going on in that
position.

### Narration: the scheme paragraph wins

Adding these made the Sveshnikov say the same thing four times across
two paragraphs — the scheme narrated "trade f6, then Nd5, then press
d6", and the plan paragraph separately offered "reroute the knight
there", "trade off the piece guarding it" and "walk the bishop round".

Plan-level talk about a square a scheme already covers is now
suppressed. The scheme states the whole campaign in order and states it
better; loose sentences about its parts are padding. The record keeps
everything (all of it still scores); only the prose is trimmed.

### Sixth tranche: the general weak pawn

| hint | detection rule (static) | corpus tags converted |
|---|---|---|
| `TargetWeakPawn` | an enemy pawn lying outside the forward attack span of every OTHER pawn its owner has, not still on its home rank, and already attacked by us. Deepest two. | `target-weak-pawns`, `pressure-weak-pawn`, `target-weak-pawn-on-half-open-file` |

One test catches isolated, backward and front-doubled pawns alike: a
pawn is permanently weak when no other pawn of its colour can ever come
to defend it. `PressureBackwardPawn` and `PressureDoubledPawn` remain as
the two special cases with their own extra conditions, and narration
keeps only the specific one when both name the same pawn — "pile up on
the backward d6 pawn" says strictly more than "pile up on a pawn nobody
can defend".

The home-rank bound is load-bearing. Without it every rook pawn
qualifies (g7 can never defend h7 either) and the hint calls an
untouched shelter a target — the first cut named h7 and f7 in positions
where nothing was wrong with them.

**2 of the 3 converted tags hit, and the headline percentage went DOWN**
(74.3% -> 74.1%) while the absolute count went up (104 -> 106). That is
the metric behaving correctly: three expectations we previously did not
measure at all are now measured, and we fail one of them. am-325-1 wants
a weak pawn on a file that is only half-open AFTER a capture Black has
not made yet, which a static reading of the position cannot see.
Converting only the tags that happened to pass would have shown a nicer
number and meant nothing.

### Seventh tranche: effective force (maintainer's insight)

Material is a board-wide sum, and that is a lie the moment the game has
a location. A rook on a8 that needs four moves to reach the kingside
contributes nothing to a fight happening at h2 — "almost like nothing",
in the maintainer's phrase. Being down material globally is a perfectly
good trade for owning the quarter of the board the game is decided in.

`crates/kibitz-core/src/force.rs` splits the board into three sectors by
file and weights every piece by how many moves it needs to arrive:
in-sector counts full, two moves two-thirds, three moves a third, and a
piece with no safe route counts zero. The routing search is the same one
the maneuver layer uses, so blockers and safety are already handled —
force that cannot arrive is not force, which is the whole point. Kings
are excluded: the king is what the fight is about, not a unit of it.

Feeding the local-force margin into the initiative lean moved
**imbalances 88.2% -> 90.9%**, the first axis to clear the 90% target.
`initiative` now reports in positions where it previously had nothing to
say, which is exactly where the corpus expected an `Initiative` reading.

It also fixed the Opera Game. At move 13 the annotation now reads
"White's initiative has become a stampede" while the material line still
says Black is three pawns up — which is what every human annotator says
about that game, and what the engine structurally could not say before.

Two honest notes. The `AttackWhereYouAreStronger` hint that came out of
the same module converts NO corpus tags: `activity-over-material` and
`seize-key-moment` turn out to be about TIMING (strike now, while their
pieces hang) rather than sector force, so they stay gaps. And the favors
axis did not move — the local-force term is small against a
magnitude-weighted vote across every imbalance, and that axis needs the
method change (outcomes plus evals, train/holdout), not another term.

### Running total

| axis | run 11 | run 12 |
|---|---|---|
| imbalances | 247/287 = 86.1% | 261/287 = **90.9%** |
| plans | 90/126 = 71.4% | 106/143 = **74.1%** |
| favors | 62/99 = 62.6% | 65/99 = **65.7%** |
| suggest@1 / @3 | 8.0% / 32.0% | 4.0% / 32.0% |
| negatives | 14/14 | **14/14 clean** |

Eleven hints added, seventeen corpus tags converted from free-form to
scoreable, fifteen of seventeen hitting. The plans percentage is up 2.7
points on a denominator that grew 126 -> 143; the absolute count is up
sixteen. suggest@3 is back to baseline;
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

## Favors refit (run 12) — fitted, not guessed

The who-stands-better vote used ONE weight for every imbalance kind. A
Minor lean in Development is not the same claim as a Minor lean in
Material, and treating them alike was most of why this axis sat at 62.6%.

The vote also lived inside the book-eval harness, which meant the product
had no verdict of its own — the only "who is better" answer in the
codebase existed to be scored. It now lives in
`kibitz_core::verdict`, and the harness calls it.

### Ground truth: game results, not engine evals

A centipawn score answers "who is winning with perfect play from here".
That is not the question Jeremy Silman's verdicts answer. "Who has the easier
game to play" is a practical claim, so the practical evidence is what
actually happened when two strong players played it out: **middlegame
positions sampled from decisive master games (both sides 2300+), labelled
by who won.** Noisy at any single position, honest in aggregate, and with
no engine anywhere in it.

`kibitz-cli favors-fit` samples, splits 50/50 on a fixed seed, fits by
coordinate ascent on TRAIN only, and scores the holdout once.

### Result (seed 0xC0FFEE, 4000 positions from the maintainer's corpus)

| kind | uniform | fitted |
|---|---|---|
| Material | 10 | **24** |
| MinorPieces | 10 | **18** |
| Space | 10 | 10 |
| Initiative | 10 | 10 |
| FilesDiagonals | 10 | **6** |
| Development | 10 | **6** |
| SquaresOutposts | 10 | **4** |
| PawnStructure | 10 | **2** |

| split | uniform | fitted |
|---|---|---|
| train | 57.9% | 67.2% |
| **holdout** | **56.7%** | **63.1%** |

Repeated on two further seeds: holdout gains of +7.9, +6.3 and +6.4
points. `Material` lands on 24 every time and `SquaresOutposts` on 4;
the middle of the table moves around, so only the ordering should be
read as settled, not the exact numbers.

### It transferred

Applied to the book corpus — 99 expectations that were never part of the
fit, judged by book prose rather than by results — **favors went
65.7% -> 68.7%**. Two independent ground truths agreeing is much
stronger evidence than either alone.

### The uncomfortable finding, and its retraction

`PawnStructure` fits at 2, the lowest of the eight. The first write-up of
this said "either our detector carries little outcome signal at master
level, or pawn structure genuinely predicts results weakly". **Both
halves were wrong**, and per-kind diagnostics (`favors-fit` now reports
them) say why:

| kind | leans% | correct% | Minor | Clear+ |
|---|---|---|---|---|
| Material | 49.1% | 70.1% | 70.5% | 69.0% |
| Initiative | 37.2% | 63.9% | 62.1% | **72.0%** |
| PawnStructure | **64.3%** | 59.5% | 57.0% | **67.3%** |
| FilesDiagonals | 44.3% | 57.8% | 56.3% | 64.6% |
| Space | 42.8% | 57.0% | 56.2% | 62.3% |
| MinorPieces | 30.2% | 56.5% | 56.4% | 57.8% |
| SquaresOutposts | 46.0% | 54.5% | 53.8% | 64.7% |
| Development | 4.5% | 54.4% | 53.6% | — |

`PawnStructure` is the THIRD most accurate detector we have. It fits low
because it leans **64.3% of the time** — more than anything else — at a
nearly uniform Minor magnitude. A detector that almost always picks a
side at modest accuracy is a persistent bias, not a discriminator, and
coordinate ascent suppressed it for being loud rather than for being
wrong. `SquaresOutposts` at 54.5% is the genuinely weak one.

### What the split actually shows

Every positional detector is **6-11 points more accurate when it commits
to Clear than when it shrugs at Minor** (Minor clusters at 53-57%, Clear+
at 62-72%). Material is the exception and is equally accurate at both —
a pawn up is a pawn up. So the shipped 1 / 2 / 4 magnitude ladder is a
guess, and the data says a committed reading is worth about 2.5 shrugs,
not 2.

### Two models, and why neither shipped

| model | params | outcome holdout | book favors |
|---|---|---|---|
| shipped (8 weights, ladder 1/2/4) | 8 | 63.1% | **68.7%** |
| free per-magnitude | 16 | 63.3% | — |
| shared ladder (8 weights + one multiplier) | 9 | **64.3%** | 65.7% |

The free 16-weight model gained nothing and produced `Development 30/30`
on a detector that leans 4.5% of the time — coordinate ascent fitting
noise. The 9-parameter model beats both on outcomes and loses on the
book corpus.

Neither difference is real. The book delta is **0.63 SE** on n=99; the
outcome delta is **1.37 SE** on n=3000. The models are statistically
indistinguishable and the numbers cannot arbitrate between them, so the
shipped weights stay — a coin flip is not a reason to change what the
product tells a user.

What IS on solid ground is the measurement: the Minor/Clear split is
computed over thousands of readings and is not close. Acting on it needs
either a corpus that can separate two models this near, or a detector
change that makes Minor readings rarer and sharper rather than a vote
change that discounts them after the fact. The second is the better
lead: `PawnStructure` leaning on two positions in three is the finding.

## Is the yardstick fair? (the maintainer's challenge)

Reading "PawnStructure fits lowest" as "pawn structure does not matter"
would be wrong, and the maintainer pushed back on the measurement itself:
*are we evaluating correctly?*

The challenge lands. Labelling a random middlegame ply by **who won the
game** is a practical signal and the honest answer to "who has the easier
game" — but it is settled thirty moves later. Material is immediate; a
backward pawn is a mortgage that may not be foreclosed before a blunder
decides the game. So outcome-labelling systematically under-credits
slow-acting imbalances, and pawn structure is the slowest on the board.

`favors-fit --engine` re-labels the same positions by what Stockfish
makes of the POSITION (120k nodes, |eval| >= 30cp, level positions
excluded rather than graded). That asks *was the assessment right*
instead of *did they win*.

| kind | by outcome | by engine | delta |
|---|---|---|---|
| Material | 70.1% | 76.8% | +6.7 |
| Initiative | 63.9% | 71.7% | +7.8 |
| **Space** | 57.0% | **66.7%** | **+9.7** |
| **Development** | 54.4% | **63.8%** | **+9.4** |
| PawnStructure | 59.5% | 64.7% | +5.2 |
| FilesDiagonals | 57.8% | 58.1% | +0.3 |
| MinorPieces | 56.5% | 59.5% | +3.0 |
| SquaresOutposts | 54.5% | 57.5% | +3.0 |

**The bias is real and it is not evenly spread.** Space and Development
gain nearly ten points apiece — they are slow, and outcomes were
punishing them for it. The shipped weights were fitted on outcome labels
and are therefore too low for those two.

**But it does not explain PawnStructure.** Refitting entirely on engine
labels still puts it lowest, at 2/5. Under both yardsticks it leans on
about two positions in three — more than any other detector — while
Material leans on half and is right three times in four. Over-leaning is
the diagnosis, and it survives the fairer measure. Its Minor band is
61.8% against 72.9% for Clear+, an eleven-point gap, the largest of the
eight.

`SquaresOutposts` is lowest-accuracy under **both** yardsticks (54.5%,
57.5%). That finding is robust; nothing about the labelling rescues it.

### Neither yardstick is ground truth

Outcomes under-credit slow factors. Engine evals are another engine's
opinion, and asking a positional analyzer to agree with Stockfish's
positional model is circular in a way that flatters agreement. The book
corpus is the only concept-aligned measure and is n=99 in aggregate, too
small to separate per-kind claims.

So the weights are not being refit on the engine labels either. What the
comparison establishes is narrower and more useful: the outcome-fitted
weights under-serve Space and Development specifically, and the
PawnStructure calibration finding is not a measurement artifact.

## What the alerts axis is actually made of

31.2% is uninterpretable on its own, and the standing explanation — that
the book tags strategic king-danger the WSUI screen deliberately reserves
for engine confirmation — was never checked. `kibitz-cli alerts-study`
partitions every missed alert into the three buckets that have different
answers:

| bucket | n | meaning |
|---|---|---|
| **screen defect** | **13** | the screen FIRED, so the engine would have been consulted, and we still produced no alert of the expected kind |
| static gap | 8 | screen quiet, but the expected alert is a structural feature (trapped piece, thin shelter) a detector could own with the engine off |
| engine-off cost | 1 | screen correctly quiet; seeing it genuinely needs a search |

**One of twenty-two.** The engine-off principle costs a single expectation
on this corpus. The other 21 are ours to fix with the engine off, and the
misses concentrate in two kinds: `WeakKing` (11) and `TrappedPiece` (7).
Both detectors exist and fire elsewhere — cbcs-192 "entombed rook" wants
TrappedPiece and gets WeakKing; cbcs-193 "entombed knights" the same.
They are not sensitive enough, which is a detector problem wearing a
principle's clothes.

So the axis is not a ceiling and should not be reported as one. It queues
behind favors as a detector gap, and it is the cheapest of the three
remaining axes to move.

## Is prophylaxis mis-ranked? (the corpus says the question is premature)

The engine gives a denying move a bonus that can outrank executing your
own plan, which surfaced when a book-cited golden regressed
(`rook_goes_behind_the_passer`: Jeremy Silman plays Ra1, we proposed Kf3). The
maintainer's proposal was to stop arguing about it and measure — the
corpus carries the author's own recommendation for 25 positions — with
the hypothesis that his prophylactic picks cluster where the opponent's
plan is faster, making the fix a tempo term.

`kibitz-cli prophylaxis-study` classifies each cited move by the role our
engine assigns it:

| role | n |
|---|---|
| constructive | 3 |
| denial | 4 |
| both | 3 |
| **neither** | **15** |

**The ranking question is largely moot.** For 15 of 25 of Jeremy Silman's own
recommendations the engine assigns no role at all — the move is not
generated by any plan hint we hold. suggest@1 sitting at 8% is therefore
not a ranking failure but a GENERATION failure, and no amount of
re-weighting denial against construction reaches a move we never propose.

The tempo hypothesis could not be tested: schemes exist in almost none of
these positions, so the horizon comparison never evaluates. Not refuted —
unmeasurable with what the corpus currently supports.

One thing the study did settle. `own_strength` and `opp_strength` come
out 1-versus-1 in nearly every entry, and the prophylaxis gate is
`opp_strength > 0 && opp_strength + 1 >= own_strength`. A gate that opens
on a tie opens always. The denial bonus is effectively unconditional,
which is the direct cause of the golden regression and is worth fixing
before anything is decided about how denial should rank.

## Retiring the sided-plan filter, and what it cost

The run-8.5 sided-plan filter DROPPED any plan whose parent imbalance
leaned the other way. With no owner on a hint, downstream would have
narrated it for the wrong player, so discarding it was the right trade
when the information did not exist. Schema v5 gives hints an owner and
every consumer now reads it, so the filter was pure loss.

**plans 74.1% -> 76.2%.** Narration now says "Here is what Black should
be dreaming about: the bad bishop wants a new life" in a Sveshnikov that
favours White — a true statement the filter was throwing away.

It was briefly kept on the grounds that more plans mean more engine jobs
during batch annotation, and CLAUDE.md #6 keeps the engine off by
default. **That misread the principle.** #6 permits three things: a fired
WSUI screen, an explicit user request, and user-initiated batch jobs.
Batch annotation is the third, named in the principle itself. #6 governs
the default path — browsing, stepping through a game — not a job the user
launched and is waiting on. A filter that buys engine time by silently
dropping correct plans pays in the wrong currency.

### One composite sentence disappeared, and that is a WIN

The Sveshnikov golden lost a composite plan. Do not restore it. The
cluster behind it only held together because two plans belonging to
OPPOSITE sides were being attributed to the same one; reading the owner
split them, and the sentence it produced was never true. Composite count
going down is not a regression here — it is a mis-clustering being
removed. The book-eval line moving is the symptom, not the disease.

### Which gate should enqueue a suggest-verify job

Three candidates over the 162 positions. The prediction, recorded before
running: jobs monotone decreasing, plans monotone non-increasing,
plans-per-job monotone increasing. All three held, so the instrument is
behaving.

| gate | jobs | plans | plans / job |
|---|---|---|---|
| any plan | 159 | 360 | 2.26 |
| **for side to move** (shipped) | **156** | **360** | **2.31** |
| converging (>=2 supports) | 19 | 49 | 2.58 |

**"For side to move" ships.** The deciding column is plans-per-job and it
barely separates: converging is 12% more efficient per job while
surfacing 86% fewer plans. Cutting engine work by 88% to gain a tenth of
a plan per job is not an efficiency win, it is a coverage cut wearing
one. "Any plan" is nearly identical in cost and strictly worse in
principle — it enqueues work for plies whose only plans belong to the
opponent, which is engine time bought with nothing to spend it on.

## Alerts: arbitration ruled out, and the PawnStructure threshold withdrawn

### Is TrappedPiece firing and losing, or not firing?

Opposite fixes, so it was instrumented rather than assumed.
`wsui::screen_trace` reports each detector's output before anything
downstream sees it. Two facts fell out immediately:

- **The screen does not arbitrate.** All three detectors append to one
  vec and it is only sorted by severity. Nothing is suppressed.
- `detect_trapped` has the screen's one silent exit: for the side NOT to
  move it needs a null-move board, which is unavailable when the mover is
  in check, and it then returns having examined nothing.

Positive control first, per §0.9 — the Noah's-Ark trapped bishop, where
TrappedPiece is known to fire and to survive to the output. The trace
reports `trapped: 1` and the output carries the alert, so the instrument
returns a positive and a zero elsewhere is a measurement.

Over the 22 misses: **the expected detector reports zero in every single
one**, and `trapped_skipped` never fires. Arbitration is not the cause,
the null-move exit is not the cause, and these are genuine non-firings.
The fix is sensitivity, and lowering a threshold will actually move
something.

One correction to the earlier bucket names, because it changes what to
file. "Screen defect" was the wrong label for the 13: in those the screen
FIRED, so the engine would have been consulted — the expected alert kind
simply never appeared. The under-firing concern belongs to the 8 QUIET
ones, and there it is real and worth naming: `decide()` fires on the
alerts it is given, so a silent detector KEEPS THE SCREEN QUIET. In those
eight positions the engine-off principle is being honoured by accident
rather than by design.

### PawnStructure lean threshold: withdrawn

Raising it 15 -> 45 was measured before and after retiring the
sided-plan filter, and the gains it appeared to offer were an artifact of
the interaction:

| | before (filter present) | after (filter retired) |
|---|---|---|
| plans | 74.1% -> 74.8% | 76.2% -> 76.2% |
| suggest@1 | 8.0% -> 12.0% | 8.0% -> 8.0% |
| favors | 68.7% -> 67.7% | 68.7% -> 67.7% |

With owners surviving a Balanced parent, the change buys **nothing
measurable** and still costs a point of favors, and it now breaks a
second citation-backed golden (`sveshnikov_d5_hole_and_backward_d6`)
rather than the first. Not shipped.

The detector finding stands unchanged — PawnStructure leans on two
positions in three at 60% and is right 68% when it commits. What is no
longer true is that raising this particular threshold is the way to act
on it.

## Naming the 13, and one WeakKing experiment that measured its own cost

### The 13 have a name now

"Screen defect" was withdrawn as wrong — the screen fired, so the engine
WAS consulted — and the bucket then had no label, which is worse than a
wrong one. Three readings were possible: no alert downstream of a
consultation, an alert of the wrong kind, or the right alert sorted out
of view. `screen_trace` settles it, and the screen neither truncates nor
suppresses, so nothing is sorted away.

**"Silent, screen fired", n=13.** The expected detector reported zero;
other detectors fired the screen anyway.

All 21 non-engine-off misses share that one cause. They differ only in
consequence — whether other evidence sufficed to fire the screen (13) or
not (8, filed separately, where the engine is never consulted and
engine-off is honoured by accident).

`detect_trapped` now returns `Scanned::Yes | Scanned::NoNullMove` rather
than an empty list for both. It is not the cause of anything today — the
exit fired in none of the 22 — but a detector that reports "nothing here"
when it means "could not evaluate" is an accessor you cannot audit.

### WeakKing: a structural exclusion, and why removing it did not ship

Shield defects were computed only for a king on the a-c or f-h files.
A king on d/e got no shield analysis at all, which blocked **7 of the 11
WeakKing misses by construction** — including one entry literally titled
"king trapped center".

Prediction before running: removing the restriction moves some of the 7
central-king entries and **none** of the 4 flank ones, whose zero must
come from elsewhere. Any flank movement is instrument failure.

**Held exactly.** Five moved — cbcs-138, cbcs-195, htryc-369-37,
htryc-386-169, htryc-391-200 — all central-king, no flank entry touched.
Alerts 31.2% -> **46.9%**, negatives 14/14 still clean.

**It is not shipped, because the corpus could not see its cost.** The
Sveshnikov tabiya then reports White's e1 king as weak on move 7, and
half a sample of quiet master middlegames began firing the screen. The
book corpus has no negative anchors for alerts, so it scored this as a
pure +5.

Calling that Sveshnikov reading "normal opening play" was too glib, and
the maintainer pushed back correctly: an uncastled king DOES grow more
dangerous as pieces come out. The mechanism is OPEN LINES, and this
position is the exception that names the missing condition — the e-file
is locked (e4 against e5) and the d-file is half-open FOR WHITE, blocked
by the very d6 pawn White is attacking. Black's queen on d8 is looking at
her own backward pawn. Nothing bears on e1.

The detector fired on "d-file shield pawn missing" without asking whether
the resulting file was open. A missing shield pawn in front of a flank
king exposes it; a missing central pawn does not, if the file is stopped
by an enemy pawn or locked. That is the defect, and it is the same
refinement flagged below — with the correction that it is not merely a
better gate for central kings, it is the condition the feature always
meant.

Gating central-king analysis on lost castling rights removes every false
positive (0 of 6 quiet positions fire) and every gain (back to 31.2%).
The reason is a property of the CORPUS, not the idea: reconstructed FENs
carry castling rights as transcription assumptions, so the flag does not
mean what it means in a real game.

What remains is position-based, and is now the primary hypothesis rather
than a fallback: count a missing shield pawn only where the file it
leaves behind is genuinely open — no pawn of either colour on it — and,
for a central king, only where an enemy major already bears down it.
Untested.
And the alerts axis needs negative anchors of its own before any
sensitivity change can be trusted — every gain here is currently measured
without a cost term.

## The alerts axis gets a cost term

The book corpus scores alerts on recall alone — its 14 negative anchors
cover imbalances and plans, and there are none for alerts. A sensitivity
change that recovers five expectations while starting to alert on healthy
positions therefore scores as a clean +5. That is how a gain and a
regression come to look identical, and it is why the WeakKing experiment
above was not shipped on its book number.

`kibitz-cli alerts-fp` supplies the denominator: the 500 engine-quiet
master positions built for the WSUI validation (both sides 2300+,
|eval| < 50cp at 200k nodes). Nothing in that set is a tactic, so every
alert is a cost and every screen firing buys an engine job for nothing.

### Baseline, shipped detector

| measure | value |
|---|---|
| screen fires | **43.2%** (216/500) |
| WeakKing alerts | 258 (0.52 per position) |
| TrappedPiece | 49 (0.10 per position) |
| Undefended | 713 |
| InadequatelyDefended | 102 |

43.2% is consistent with the 39.2% holdout FP rate recorded for the
screen in run 3 — a different sample of the same behaviour, and the
architecture expects it: a fired screen costs one bounded engine job
whose verdict annotates the alert.

### The WeakKing trade, both sides

| | book recall | screen fires on quiet | WeakKing alerts |
|---|---|---|---|
| shipped | 31.2% | 43.2% | 258 |
| central kings included | **46.9%** | **51.6%** | 386 |

+15.7 points of recall for +8.4 points of firing and 50% more WeakKing
alerts on positions where nothing is wrong. Neither number decides it
alone, which is the point of having both — and it is a real trade rather
than the free win the book corpus reported.

### The open-file condition, predicted and shipped

Prediction, recorded before running and falsifiable in both directions:
recall keeps at least 3 of the 5 (>= 40.6%) because "trapped in the
centre" positions have genuinely open lines, and quiet-set firing returns
to <= 47% because the Sveshnikov d-file is stopped by Black's own pawn.
If firing stayed near 51.6%, the mechanism was wrong.

Both held, narrowly: **recall 40.6%, firing 46.8%**.

| | book recall | quiet firing | WeakKing/pos |
|---|---|---|---|
| shipped (before) | 31.2% | 43.2% | 0.52 |
| central kings, naive | 46.9% | 51.6% | 0.77 |
| **central kings, open-file gated** | **40.6%** | **46.8%** | 0.61 |

+9.4 points of recall for +3.6 of firing — 2.6 recall points per point of
false positive, against 1.87 for the naive version. Negatives 14/14.

The mechanism was checked on both ends rather than inferred from the
totals. The Sveshnikov at move 7 now produces **no alerts at all** and
does not fire the screen; cbcs-138 "king trapped center" produces
WeakKing for Black. A missing pawn in front of a castled flank king is
exposure in itself; in the centre it is only exposure if the file it
leaves behind is genuinely open.

The Opera Game gained two king alerts and both are right: the d-file
really is open there (both d-pawns went on moves 3-5), Morphy castles
away from it shortly after, and Black's e8 king standing on that file is
the entire game.

## TrappedPiece: the corpus wants a different concept

Seven misses. Classified by which gate stops them, using SEE-safe
destination counts per piece:

- **Five** hold a piece with ZERO SEE-safe destinations that is simply
  not under attack (cbcs-192, am-322-1, am-323-2, am-328-1,
  htryc-381-151).
- **Two** hold no such piece at all — every minor has a safe square
  (cbcs-193 "entombed knights", am-10-1). Those pieces are mobile and
  useless, which is not what this detector measures.
- **None** hold a piece that is both immobile and attacked, so the
  detector's own core condition never passes with its attack requirement.

### Two predictions, one refuted

Predicted: relaxing the attack requirement recovers 3-5 of the five, and
TrappedPiece on quiet positions at least doubles.

**Wrong on both counts.** Recall did not move at all — 40.6% before and
after — while false positives rose anyway (0.10 -> 0.14 per position).
A second gate was also blocking: `!attacked && home_ranks.has(sq)`.

With both gates off the five do come back, and the price is the worst
measured today:

| change | recall | FP (quiet firing) | recall per FP point |
|---|---|---|---|
| WeakKing, open-file gated (shipped) | +9.4 | +3.6 | **2.6** |
| WeakKing, naive (rejected) | +15.7 | +8.4 | 1.87 |
| TrappedPiece, both gates off | +15.6 | +17.8 | **0.88** |

64.6% of engine-quiet master positions would fire the screen, and
TrappedPiece would fire five times as often, at 0.49 per position.
Below one recall point per false-positive point is not a trade worth
making.

### What that says

The gates are doing real work. A piece with no good square, unattacked,
on its home rank is an ordinary undeveloped piece — not an alert. The
five corpus entries want precisely what those gates exclude, and the
other two want mobility-with-no-purpose, which the detector does not
model at all.

So "entombed" is not a sensitivity setting of TrappedPiece. It is a
strategic property — a piece with no future — and its natural home is an
imbalance beside the existing bad-bishop detection, not the tactical
screen. Filed that way rather than tuned further; the alerts axis
ceiling for TrappedPiece is 7 misses that mostly should not be alerts.

## Entombment becomes an imbalance (#12), and what the cost term did to it

The previous section filed "entombed" as a concept error in the alerts
axis and predicted its natural home was an imbalance beside bad-bishop
detection. That is now built: `kibitz-core::entomb`, feeding
`minor_pieces` (a positional charge, next to the bad bishop) and
`material` (a ledger discount, because Jeremy Silman's p. 192 point is that the
side with the extra rook is WORSE).

### The definition, and why permanence is the whole of it

A piece is entombed when, within `route::MAX_HOPS`:

1. it cannot reach the enemy half of the board, nor take anything without
   losing by the trade;
2. the squares it can reach and hold number no more than
   `entomb::MAX_CELL` (3); and
3. **no sequence of two pawn moves by its owner changes either of those.**

Condition 3 is the entire distinction between an entombed piece and an
undeveloped one, and it is why this could never have been a TrappedPiece
threshold. The f1-bishop in the starting position satisfies (1) and (2) —
it has no moves at all — and fails (3), because e2-e4 hands it a
diagonal. The f1-bishop of The Amateur's Mind p. 10 satisfies all three,
because every white pawn on that board is frozen.

### Three cost-term measurements, and what each one deleted

`kibitz-cli entomb-fp` is the analogue of `alerts-fp` for a detector that
buys no engine time: the same 500 engine-quiet master positions, where
every firing is a false statement in the prose and a wrong discount in
the ledger. It was run BEFORE the detector was wired into anything, and
it rewrote the design three times.

| version | quiet-set firing | what it was calling entombed |
|---|---|---|
| condition (1) alone | **51.0%** (545 pieces) | 362 back-rank rooks |
| + `MAX_CELL` size test | **28.2%** (207) | 126 back-rank rooks |
| + own pieces are not walls | **1.2%** (6) | 5 bishops, 1 arrived rook |
| + pawn-only sealing, arrival | **0.4%** (2) | 2 hemmed bishops |
| + two pawn moves, not one | **0.0%** (0) | — |

Each row is a concept the first draft had wrong, not a threshold:

- **A cell has to be small.** Reaching nothing is the pure case; the
  p. 10 bishop shuffling between g2, h3 and h1 is the other end of it.
- **Your own men are not walls.** A rook on f8 with its king on g8 and
  its queen on d8 is not entombed; those pieces move. This one deletion
  took 27 points off the firing rate.
- **Only pawns seal a square.** Being watched by an enemy knight is a
  reason not to go somewhere this move, not a wall. This is what had
  condemned the c8-bishop of a Paulsen Sicilian, whose one route out
  (…Bb5) happened to be covered by a knight and a queen.
- **A piece already in enemy territory has arrived**, however little it
  can do next.
- **Two pawn moves, not one.** See below — this row is the one the book
  corpus caught rather than the quiet set.

### Fischer's bishop, and why the depth-1 test was not enough

At depth 1 the detector cost a favors point, and the entry it cost was
htryc-379-140 — Fischer-Gadia, Mar del Plata 1960, where White's
b3-bishop is boxed in by its own a2/c2 and Black's b5-pawn. No single
white pawn move frees it. `c2-c4` followed by `cxb5` does, and Jeremy Silman's
whole answer is that White is better because his structure is still
fluid. Depth 1 discounted that bishop 165cp and handed the position to
Black.

The candidate set that reaches the permanence test is one or two pieces
in a couple of positions per thousand, so the squared cost of depth 2 is
nothing measurable (`entomb-fp` over 500 positions: 0.46s, unchanged).

### One prediction, recorded and refuted

Predicted before running: the detector catches cbcs-193's h2-knight
(boxed by its own f3/g4 and Black's f4-pawn) and misses the b1-knight
(c3 is defended by the d2-pawn, so it can pay a bishop for itself and go
c3-e4-d6).

**Half right, and the wrong half is the interesting one.** b1 escapes
exactly as predicted. h2 escapes too, through **d2xe3** — a pawn capture
that removes the very pawn covering d2 and g3, and opens the box. The
book does not consider it. Captures stay in the permanence test: the
detector being stricter than the text is the right direction for
something that discounts material, and the committed test asserts the
non-detection with the reason rather than papering over it.

### The corpus re-transcription, stated plainly

Three entries had `TrappedPiece` in their `expected.alerts` because that
was the only vocabulary the transcription had for the book's own word.
One of them (am-10-1) said so in its own note: "TrappedPiece is our
closest alert for the entombed bishop". Those three moved to the plans
axis:

| entry | was | now |
|---|---|---|
| cbcs-192-entombed-rook | alert TrappedPiece | plan KeepPieceEntombed — **hit** |
| am-10-1 | alert TrappedPiece | plan KeepPieceEntombed — **hit** |
| cbcs-193-entombed-knights | alert TrappedPiece | plan KeepPieceEntombed — **miss** |

This is an edit to ground truth made by the implementer and it should be
read with that in mind. Two things bound it. The book itself names the
concept — both CBCS entries carry `concept: entombed-piece` from the
p. 192-193 entry title — so the correction is to the transcription, not
to the standard. And the third entry was re-transcribed knowing it would
FAIL, which is the test of whether a corpus edit is honest.

The other four TrappedPiece expectations were **left alone**, because
they are a different problem and not this one:

- am-322-1 — "the game's knight raid via c7 died on a8"
- am-323-2 — "the queen ultimately snared"
- am-328-1 — "the g6-bishop's near-entombment" (there is no piece on g6
  in the FEN; the diagram is before 15.Bg6?)
- htryc-381-151 — the note does not mention a trapped piece at all

The first three are COUNTERFACTUAL: the trapping happens several moves
into a line the book is warning against, not in the diagram. No static
detector can see them, and neither can an engine at this position
without first playing the bad move. Correcting them is a judgment for
the corpus author, not for the code.

### Results

| axis | before #12 | after #12 |
|---|---|---|
| imbalances | 90.9% (261/287) | **90.9%** (261/287) |
| plans | 76.2% (109/143) | **76.0%** (111/146) |
| alerts | 40.6% (13/32) | **44.8%** (13/29) |
| favors | 68.7% (68/99) | **68.7%** (68/99) |
| negatives | 14/14 | 30/31 (see #11 below) |

Read the plans row carefully: it went DOWN 0.2 points while gaining two
hits, because the axis gained three expectations and one of them is the
knight the detector deliberately does not claim. The rate fell and the
engine got better; that is what happens when you add an expectation you
know you fail.

**The alerts ceiling, restated.** 44.8%, up from 40.6%, and not one point
of it came from the screen getting better. Three expectations left the
denominator because they were never alerts. The Complete Book of Chess
Strategy's alerts axis is now 4/4. What remains is 16 misses over 29
expectations, and the partition below has not moved.

If the four counterfactual expectations were also corrected, the axis
would read **13/25 = 52.0%**. That number is offered as a bound, not
claimed: it is what the axis measures if every expectation in it is
something the position actually contains.

## #10 re-measured: still 6, and why #12 did not touch it

`kibitz-cli alerts-study` after #12:

| bucket | count |
|---|---|
| engine-off cost | 1 |
| static gap | **6** |
| silent, screen fired | 9 |

The static gap is unchanged at 6 — four WeakKing (am-324-2,
htryc-375-128, htryc-388-182, htryc-391-200) and two TrappedPiece
(am-323-2, am-328-1), exactly the six named before this run.

That is not an accident of counting. Re-running the study against the
pre-#12 transcription puts all three entombment entries in **silent,
screen fired** (12, now 9), never in the static gap: the screen was
already firing on those positions for other reasons, so the engine was
being consulted regardless. #12 removed three expectations the screen
was never going to satisfy and left the six structural gaps untouched.
The two TrappedPiece entries still in the gap are two of the four
counterfactuals above, so the honest reading of the static gap is
**four WeakKing misses and two expectations that are not about the
position.**

## The alerts axis gets corpus-side negatives too (#11, second source)

Until now the alerts axis had exactly one negative source: `alerts-fp`
over 500 engine-quiet master positions. That set is the honest one —
nobody chose it with these detectors in mind, so it can surprise you —
but it is anonymous. It moves a rate by fractions of a point and it
cannot say WHICH claim was wrong.

`not_expected` now carries an `alerts` list, and 17 alert bans were
written across the 10 transcribed counter-example entries. The selection
rule was fixed before running: on a counter-example whose text settles
the position as quiet, both judgment detectors must stay silent;
WeakKing is withheld from the ban only where king safety is genuinely an
open question (kings still in the centre at move 3, and a queen endgame
where every king is airy by nature).

Negative checks: **14 → 31. Thirty are clean and one is RED.**

| entry | banned | status |
|---|---|---|
| cbcs-239-doubled-pawns-useful (p. 239) | WeakKing | **FIRES** |

`r1bq1rk1/ppp2pp1/2np1n1p/4p3/2B1P3/2NPPN2/PPP3PP/R2Q1RK1 w`. White's
doubled e-pawns are the book's example of a GOOD doubled pawn — they
hold d5/f5/d4/f4 and hand White the half-open f-file, which is exactly
where his rook already is. The detector looks at the same structure and
reports `WeakKing white g1, f-file shield pawn missing`.

It was left red at first, deliberately. The alert is severity `low` and
**the screen does not fire**, so it costs no engine job — it is a claim
in the prose, not a tactical call — but the missing f-pawn is the whole
reason the position is good for White, so the one sentence the coach
adds contradicts the lesson. What was NOT allowed was tuning a detector
against one anchor, which is the mistake this document has spent two
runs cataloguing.

It is green now, and not because it was tuned to: see "cbcs-239 was the
first instance of a class" below, where the suspicion was written as a
falsifiable prediction, swept over 662 positions, found in 24 of them,
and cut back by a golden test before it shipped.

Worth stating flatly, because the caution was raised before the source
existed and it was right: a negative set chosen by someone who knows
what the detectors do is **weaker evidence, not useless evidence**. It
cannot surprise you the way the quiet holdout can, because the holdout
was assembled by people who had never heard of WeakKing. The failure
mode to guard against is therefore over-confidence in a clean sheet
here — "30/31 clean" is not the same claim as "30/31 clean on positions
nobody chose" — and not absence of signal. It found something on its
first run.

## Methodology: the cost term comes before the tuning, not after

**The worked example is the entombment table** (five design cuts,
51.0% → 28.2% → 1.2% → 0.4% → 0.0% quiet firing, identical book
numbers at every step — "Entombment becomes an imbalance", above).
Version one would have shipped, because it was reached first and
scored the same. Everything below is the rule; that table is the
argument.

Promoted here from three runs of evidence, because it decided the
outcome twice in this run alone.

The book corpus scores recall. A detector that fires more will always
look better on it, and there is no number anywhere in a recall-only
harness that distinguishes a gain from a regression. Every sensitivity
argument in the previous three sections was undecidable until a
denominator existed:

- **WeakKing** looked like a free +15.7 on the book corpus. With
  `alerts-fp` it was +15.7 recall for +8.4 points of screen firing —
  1.87 per point — and the gated version that shipped is 2.6 per point.
- **TrappedPiece with both gates off** looked like +15.6. With the cost
  term it is 0.88 recall points per false-positive point, the worst
  trade measured, and it was refused.
- **Entombment** would have shipped as a detector firing on **51% of
  quiet master positions**. Nothing in the book corpus would have said
  so; its numbers were unchanged in every version. The five deletions in
  the table above were all driven by a measurement taken before the
  detector was connected to anything.

The rule, then. A detector gets a denominator before it gets a
threshold. The denominator is a set nobody picked with that detector in
mind. It is measured before the detector is wired into the product, not
after the recall number looks good — because once the recall number
looks good, every subsequent measurement is an argument about whether to
give it up.

The corollary is the one that cost real work this run: **when the cost
term deletes a concept, that is the finding.** Four of the five entombment
revisions were not tuning. They were the detector being wrong about what
a wall is, and only a denominator could say so.

### And the corpus caught what the cost term could not

The rule above is not "the FP rate is the real metric." Entombment
shipped through two instruments and each one caught a failure the other
was structurally blind to.

The false-positive rate found **breadth errors**: a rule that condemns
half the quiet set is visibly wrong however few positions you inspect,
and the 51.0% row needed no judgment at all to read. What it cannot see
is a rule that fires rarely and is wrong when it does. At depth 1 the
detector fired on **two positions in five hundred** — a rate any
reviewer would have signed off — and one of the two it was quietly
right about while the corpus held the case that mattered.

The book corpus found the **depth error**: Fischer-Gadia, htryc-379-140,
where discounting White's b3-bishop 165cp flips a position Jeremy Silman
spends a page explaining is good for White. The bishop really is boxed
in. What the detector had wrong was how long for — `c2-c4` then `cxb5`
opens it, and the entire book answer rests on White's structure still
being fluid. No false-positive count over anonymous positions could ever
surface that, because the FP rate has no opinion about which side stands
better; it only counts firings. The corpus does nothing else.

So the two are not a primary metric and a sanity check. They are two
instruments with disjoint blind spots:

| | catches | blind to |
|---|---|---|
| quiet-set FP rate | firing too broadly, on positions nobody chose | firing rarely and wrongly |
| book corpus | being wrong about a position someone can explain | over-firing, by construction |

A recall metric is invariant to over-firing — version one and version
five of this detector score identically on it — which is why the FP rate
had to exist. And an FP rate is invariant to being wrong about the
positions you do fire on, which is why it could not have replaced the
corpus. Ship nothing on one of them.

## cbcs-239 was the first instance of a class, and the fix is smaller than it looks

The red anchor from the section above turned out not to be a choice
between "anchor too strict" and "WeakKing needs a proximity condition".
The maintainer proposed a third reading and it is the right one: the
f-pawn in Jeremy Silman's good-doubled-pawn position **did not leave the
board, it relocated by capture onto e3.** The number of pawns in front of
g1 is unchanged. "Shield file empty" was a proxy for "shield pawn traded
away", in exactly the way "central king" was a proxy for "open file" one
section earlier.

### Prediction, recorded before the sweep was written

1. Class size 8-30 across the 662 positions of the book corpus plus the
   quiet holdout.
2. WeakKing fires on more than 80% of them, because the condition is
   per-file and has no mechanism for seeing the relocated pawn.
3. Therefore cbcs-239 is the first instance of a class.

Refutation conditions stated at the same time: a class of 2 or fewer
means the anchor is too strict and the condition is withdrawn; firing
well under 80% means something already separates these positions and
cbcs-239 fires for a reason not yet found.

### Result: `kibitz-cli shield-study`

| | |
|---|---|
| positions scanned | 662 |
| shield file empty, own doubled pawn next door | **24** |
| WeakKing names that file as a missing shield pawn | **24 (100%)** |
| …and has no other reason to alert at all | **21 (88%)** |

All three predictions held. The study reports blame rather than firing
on purpose: a king under real pressure keeps its alert whatever the pawn
count says, and counting those would have inflated the case from 21 to
24.

### The condition, and the golden test that cut it down

An empty shield file is not a defect when a neighbouring file carries an
own doubled pawn — **unless the file the pawn left is genuinely open**,
in which case the pawn being alive one file over is no comfort at all.

The second clause was not in the first draft. It was forced by a
committed golden, `wrecked_shield_open_file_fires_w`: a shattered
kingside where Black has played …gxf6 and White's rook stands on the
open g-file. That position has the identical signature — g-file bare,
doubled f-pawns next door — and the king really is in trouble. Without
the open-file clause the condition silenced it.

Worth noticing which instrument caught which failure, because it is the
third distinct one in this run. The corpus anchor found the defect; the
sweep established it was a class and not one position; and the golden
set found that the fix was too broad. None of the three could have done
another's job.

The two now sit as a pair in `wsui_golden.rs` — same signature, opposite
verdicts, decided by whether the file is open.

### What it cost and what it bought, against the prediction

Predicted: book recall unchanged; cbcs-239 green; WeakKing 0.57-0.59 per
quiet position; **screen firing down 1-3 points.**

| measure | before | after |
|---|---|---|
| book alerts | 13/29 = 44.8% | **13/29 = 44.8%** |
| negative anchors | 30/31 | **31/31** |
| WeakKing per quiet position | 0.61 (306) | **0.58 (289)** |
| screen fires on quiet | 46.8% | **46.6%** |

The first three held. **The fourth is refuted, by the criterion written
down before the run.** Screen firing moved 0.2 points, not 1-3, which
the prediction had already named as the line: under half a point means
the shield note was almost never the thing tipping the screen.

So this is a **prose-accuracy fix and not a false-positive win**, and it
has to be described that way. It deletes seventeen wrong sentences per
five hundred positions and saves one engine job. The seventeen sentences
are worth deleting — each one told a user their king was exposed in a
position where the structure is an asset, and one of them contradicted
the page it was transcribed from. But the honest headline is that the
alerts axis did not move and neither did the cost term.

Nothing here changes the standing rule. The condition shipped because a
class of 24 supported it and three instruments agreed, not because one
anchor was red.

### One thing found and not fixed

`route::route_to` treats an enemy-occupied square as a passable waypoint
whether or not the capture is sound, so a rook walled in behind a
defended pawn "routes" straight through it. `entomb` needed its own BFS
because of it. Benign where `route_to` is used today, real all the same,
and fixing it would move every maneuver number in this document. Parked
as a decision rather than a surprise — see DECISIONS_NEEDED.md,
"route_to passes through unsound captures".

## The counterfactuals leave the alerts axis: 52.0%, with stated exclusions

The maintainer ruled on the four counterfactual TrappedPiece
expectations, and the bound from the entombment section is now the
figure.

**am-328-1 was a plain transcription error and is simply corrected.**
Its note claimed TrappedPiece for "the g6-bishop's near-entombment", but
the FEN is the diagram BEFORE 15.Bg6? — there is no piece on g6 to trap.
The expectation is removed, not relocated.

**The other three are real chess content about a derived position**, and
the corpus now has a category for that: `expected.line_conditional`, an
expectation that holds only after a recorded continuation is played from
the entry's FEN. am-322-1 (the knight raid that dies on a8), am-323-2
(the queen ultimately snared) and htryc-381-151 (the bishop trapped by
the recommended exchange sacrifice) moved there. They are excluded from
position-level scoring, counted and printed by the harness, and become
scorable when a suggest-then-verify harness can walk the line and screen
the resulting position — the category where that engine gets graded
eventually, created now rather than invented under pressure. The SAN
lines are not yet transcribed (they need the books at hand) and each
entry is flagged `[line not yet transcribed]` until they are.

| axis | with counterfactuals | after the ruling |
|---|---|---|
| alerts | 13/29 = 44.8% | **13/25 = 52.0%** |
| line-conditional (unscored) | — | 3 |

Every expectation still in the denominator is now something the diagram
actually contains. The #10 partition after the ruling: **1 engine-off,
4 static gap, 7 silent-screen-fired** — the static gap is purely the
four WeakKing misses, which the next section prices.

## Pricing the four WeakKing misses: two mechanisms, both real, neither ships bare

All four are silent today — no WeakKing at any severity. Read together
they are not one gap. am-324-2 and htryc-391-200 hold a king still on
its home d/e square, shield intact, nothing attacking the zone, while a
castled opponent prepares to open the centre — exposure that is
TEMPORAL, invisible to every current arm because every current arm reads
the present. htryc-375-128 and htryc-388-182 hold castled kings with
intact shields under a massed piece funnel — force that can ARRIVE,
where the zone-surplus arm counts force that already attacks.

### Prediction, recorded before the sweep was written

1. The split is 2+2: lagging-king covers exactly the first pair, the
   sector funnel exactly the second, and no single condition covers all
   four.
2. Lagging-king class size: book 6-14, quiet 10-40 (2-8%). Refutation
   line stated in advance: **quiet frequency above ~10% means the bare
   condition cannot ship.**
3. WeakKing silent on >90% of lagging-king candidates.
4. Sector-funnel (force_in margin >= 300cp at a flank king) quiet
   frequency 15-35% — predicted DEAD as a bare condition before running.

### Result: `kibitz-cli king-study`, 662 positions

| | book | quiet | WeakKing already fires |
|---|---|---|---|
| A — lagging king | 16 | **57 (11.4%)** | 31% / 26% of candidates |
| B — sector funnel | 61 | **176 (35.2%)** | 28% / 22% of candidates |

Prediction 1 **held exactly**: am-324-2 and htryc-391-200 are A and not
B on the miss side; htryc-375-128 and htryc-388-182 are B and not A. Two
mechanisms, no overlap, no third thing.

Prediction 4 held at the top of its band: B is dead as a bare condition,
as predicted.

Predictions 2 and 3 were **refuted**. A came in at 11.4% quiet — above
the 10% line written down in advance, so the bare lagging-king condition
does not ship either, by the criterion set before the run. And WeakKing
is not silent on the classes: it already fires on a quarter to a third
of the candidates through its existing arms, which means a naive
implementation would not only add its own false positives but double-
count a substantial overlap.

### The price, in the currency the shipped detector set

The shipped standard is 2.6 recall points per point of quiet firing
(WeakKing open-file gate). Each recovered miss is worth 4 points on the
25-expectation axis:

- **Bare A**: +8 recall for ~+8.4 points of new quiet firing — ~0.95 per
  point, worse than the 1.87 naive-WeakKing version that was already
  rejected.
- **Bare B**: +8 recall for ~+27.6 — 0.29 per point, the worst number
  yet measured on this axis.

For a GATED lagging-king detector to meet the shipped 2.6 standard, its
+8 recall may cost at most ~3.1 points of quiet firing: the gate must
cut 57 quiet candidates to about 15 while keeping both misses. The
obvious candidate term is the one the book names in both entries — the
opponent must actually be able to OPEN the centre (a central lever
available, or half-open central files) — but designing and pre-
registering that gate is its own piece of work, deliberately not begun
in the run that priced it.

**The floor, stated.** The alerts axis stands at 52.0% and will not move
past 60.0% (both A misses) without a gated lagging-king design, or past
68.0% (all four) without a funnel gate that cuts 35.2% by two orders of
severity. Nothing else remains in the static gap. That is what "priced"
means: the next point on this axis has a known cost and a named
mechanism, and both were established by predictions that were allowed to
fail.

## Run 12 close-out: the blockade B2 pair, and the corpus goes multi-author

Corpus composition, stated per the standing rule (every figure below
sits on this denominator): **165 positions — 162 Jeremy Silman across
four books, 3 Nimzowitsch (Chess Praxis batch 1)**; positive
expectations 293 imbalance / 149 plan / 25 alert / 102 favors; negative
anchors 37 (31 prior + 6 new); quiet holdout unchanged at 500.

### The prose remediation, first

Per the maintainer's ruling, before the corpus grew: of the 41 entries
the audit flagged, **35 were rewritten** — authorial sentences replaced
with factual claims in our own words, citations and reconstruction
caveats kept intact — and **6 were judged already clean**. The criterion:
a note is clean when its flagged content is only a titular caption used
as an identifier, bare moves or results (facts about the book's answer),
or our own reconstruction record; it is rewritten when it reproduces the
author's sentences or rule phrasings. The four clean titular captions
were normalized to quoted-title form so the criterion is visible in the
file. Axes unchanged before/after (the notes field is unscored), which
was the point: the positions and citations were never the problem.

### The three missing lines, closed

The maintainer authorized reading the books directly. All three
line_conditional entries now carry their SAN, replay-verified from each
entry's FEN with an independent library. Reading the actual answers
corrected a placeholder error: in htryc-381-151 the trapped piece is
not the dark-squared bishop but the black KNIGHT that grabs a2
(20.Rxe6 ... 28.Kd2 Nxa2 29.Rb3). At the recorded endpoints the
TrappedPiece detector fires on two of the three today; the third
(the a2 knight) needs the book's few extra rounding-up moves — a real
test case for the future suggest-verify walker, not a defect.

### The blockade B2 pair (inventory mechanisms 7 and 10)

Prediction registered before implementation; measured after.

| prediction | line | actual |
|---|---|---|
| UprootBlockader quiet firing 1-4% | ship line: under 8% | **4.2%** (21/500, 46 firings) — ships, just above the predicted band |
| book axes unchanged, negatives 31/31 | any regression reverts | **held exactly** |
| cbcs-216 elasticity split: knight elastic (threats >= 2), bishop inelastic | bishop elastic = metric wrong | **held** (unit-tested) |
| Praxis batch contains a position carrying UprootBlockader | — | **held** (g70: white, e6/e5) |

Elasticity v1 is evidence only (`blockader_<sq>`: piece, threats,
elastic); the strict leave-and-return race stays unbuilt, as bucketed.
The cost instrument is now generic — `kibitz-cli hint-fp <hint>` — so
every future B2 condition pays the same toll on the way in.

### Chess Praxis batch 1: three entries, selected by the author's own index

Provenance chain, stated because it is the property everything else
rests on: **stratagem (the book's own index) → game number → Sherwood
numbering → printed score → replay → FEN.** No judgment call sits
anywhere in that chain — which means the Praxis corpus can be trusted,
entry for entry, in a way the Jeremy Silman transcriptions (diagram-read FENs,
reconstruction caveats) currently cannot. Every batch-1 `sans` list
replays to its FEN by independent verification.

Batch 1 results: imbalances **6/6**, scored plans **3/3**
(PressureDoubledPawn, BlockadeThenPressure, PressureBackwardPawn — the
last already firing on the exact pawn the book calls weak), favors
**2/3**, and **four vocabulary gaps counted**: blockade of a non-passed
pawn complex (x2), restraint of a freeing pawn move, and the reserve
blockader — the book's index cites exactly one game for that concept
and batch 1 has it. The gaps are the deliverable as much as the hits:
this is the concept-coverage instrument doing its job.

The favors miss is g70 and it is the axis's known shape: a pawn down
with the two bishops and a strangling bind, the book (and the game)
say black, the material-led vote says white. Compensation stories
remain what the favors axis cannot see.

**One negative anchor added red, by declaration.** g70's WeakKing ban
was written down as expected-red before the entry was scored: with the
queens off, black's king walking to e7 to stand reserve behind the
blockader is the book's own technique, and the detector calls that king
weak. Same posture as cbcs-239 had before its class was found: left
red, not tuned to. It is the only standing red anchor (cbcs-239 went
green with the relocated-shield condition), and like cbcs-239 it reads
as a proxy error — "king off the back files" standing in for "king in
danger" — which makes it a candidate for the same treatment: predict a
class, sweep for it, and fix on the class or not at all.

### Quiet-set drift check after the batch

alerts-fp and entomb-fp re-run after all of the above: screen 46.6%,
WeakKing 0.58/position, TrappedPiece 0.10, entombed 0.000 — identical
to the pre-batch measurements, as they must be (corpus growth changes
no detector). Stated anyway, per the rule: the day these move without a
detector change is the day something is wrong with the harness.

### Multi-author, first honest reading

Three positions is not a measurement, so no Jeremy Silman-versus-Nimzowitsch
number is claimed yet. What batch 1 does establish is the shape of the
answer to come: the engine hit every Nimzowitsch imbalance and scored
plan in the batch, and every concept it lacked surfaced as a NAMED gap
rather than a silent miss. The comparison becomes reportable when the
Praxis corpus reaches a few dozen entries across at least three
stratagem families.

## The queenless-shield class: found, and refused on its own terms

praxis-g70 got the cbcs-239 treatment. Prediction registered before the
sweep: WeakKing alerts on pure shield/open-file evidence against a
queenless opponent form a class (predicted 60-130 of the quiet
holdout's 289 WeakKing alerts); no book WeakKing recall hit sits in the
class; refutation line — **any hit in the class means the gate loses
recall and does not ship.**

`kibitz-cli queenless-study`: the class is real — **90 of 289 quiet
WeakKing alerts (31%)**, and 27 class alerts across the book corpus
including the g70 anchor. **And the refutation line fired**: cbcs-329
(two hogs on the seventh) expects WeakKing for the back-rank-trapped
black king, the queens are off, and its alert is shield-only — the
back-rank arm does not engage because black's own a8 rook technically
guards the rank while the doubled rooks deliver mate anyway. am-316-2
is also at risk. A queenless king CAN be in mortal danger from heavy
pieces alone; "opponent has no queen" is too blunt a gate, exactly as
the pre-registered line anticipated it might be.

So: no fix ships. The class is real and priced (a third of quiet
WeakKing prose is shield chatter against queenless opponents), the
blunt gate is refuted by the corpus's own recall, and the refinement —
something like "shield terms require enemy majors actually bearing on
the king's half", which would have to keep cbcs-329 while releasing
g70 — is a NEW prediction for a future run, not a same-day retry. One
prediction, one refutation, one stop: re-predicting until the gate fits
is the mistake this document exists to prevent. g70 stays red, now with
its class quantified and the recall constraint that any future fix must
satisfy written down.

## Praxis batch 2: the fight against the blockader, scored by the detector built for it

Corpus composition: **167 positions — 162 Jeremy Silman, 5 Nimzowitsch**;
negative anchors 39; quiet holdout fixed at 500 (no detector changed
since the drift check above, so the quiet figures stand as measured).

Batch 2 is games 7 and 9 — the index's fight-against-the-blockader
pair, and the book's own compare-and-contrast exercise: in game 9 the
d3 blockader falls in the course of the defense, in game 7 in the
course of an attack White was compelled to undertake. Both entries were
replay-verified from the printed scores (the harness caught a
transcription typo in one pasted FEN before it could enter the corpus,
which is the verification property doing its job), and both carry
UprootBlockader as the book's own stated plan.

Results: Chess Praxis now **9/9 imbalances, 5/5 plans, 4/5 favors,
8/9 negatives** — the one red the declared g70 anchor. UprootBlockader,
shipped earlier this run at a measured 4.2% quiet cost, scored both of
its first two corpus expectations. Fifth vocabulary gap harvested:
rolling up a paralysed majority.

ALL BOOKS: imbalances 91.2% (270/296), plans 76.8% (116/151), alerts
52.0% (13/25), favors 72/104 = 69.2%.

## Passer classification (mechanisms 14/16), and Praxis batch 3 grades the gaps on purpose

Corpus composition: **169 positions — 162 Jeremy Silman, 7 Nimzowitsch
across three stratagem families** (blockade, fight-against-the-
blockader, isolani); negative anchors 41; quiet holdout fixed at 500.

### The classification, and the spot-check that fixed it before it shipped

Every passed pawn now carries `passer_<sq>`: protected / connected /
outside, plus one corpus-demanded hint — **OutsidePasserDecoy**, endgame
only. Prediction registered first; the scorecard:

| prediction | line | actual |
|---|---|---|
| quiet cost 2-6% | ship under 8% | **2.8%** (14/500) — ships |
| book unchanged, no new red | any regression reverts | held |
| three classification spot-checks | any wrong = does not ship | **one FAILED** — and fixed pre-ship |

The failed spot-check is the finding. The first draft called a passer
"outside" when far from the ENEMY king, and CBoCS p. 212 refuted it:
black's g3/h2 pawns sit four files from the wandering white king and
are the entire theater, with black's own king standing on top of them.
King-distance was a proxy for theater-distance — the third proxy error
this run has caught (central-king/open-file, shield-empty/relocated,
now this). Outside now means far from BOTH kings; the refuting position
is a committed unit test. This was a pre-ship verification revision,
entomb-style — not post-ship tuning against an anchor.

### Batch 3: the isolani family, including two deliberate misses

praxis-g61 is the formula executed — "Now we have it, the Isolani!"
with the knight landing on d4 in the same breath; BlockadeThenPressure
fires on the exact squares, favors white, all green.

praxis-g62 is the more valuable entry BECAUSE it fails. The book's own
annotations at moves 18 and 21 — the blockade of the isolated PAIR
proceeds, an over-protection of d4 takes place — are entered in our
vocabulary knowing both detectors miss: BlockadeThenPressure's
precondition recognizes only passed and isolated pawns, and c6/d5 are
neither by that test (the isolated pair is a structure the engine does
not classify); OverprotectStrongPoint recognizes only pawn spearheads,
and the strong point here is a SQUARE held by a rook. Free-form tags in
their place would have cooked the multi-author number. The two misses
now stand in the plans axis as named, cited concept debts.

Chess Praxis after three batches: **11/11 imbalances, 6/8 plans, 6/7
favors, 10/11 negatives** (the one red is the declared g70 anchor), six
vocabulary gaps. ALL BOOKS: imbalances 91.3% (272/298), plans 76.0%
(117/154), alerts 52.0% (13/25), favors 69.8% (74/106). The plans rate
DROPPED two tenths while the engine got strictly better this run —
two deliberate misses entered the denominator, which is the same
honest arithmetic the cbcs-193 knight bought in the entombment work.

## The g62 debts paid — after the cost term killed the first design

Prediction registered before implementing the isolated pair and
blockade-point overprotection; the measurement destroyed the first
design within the hour, which is the loop working:

| | predicted | first design | revised (central-only) |
|---|---|---|---|
| BlockadeThenPressure quiet rate | +<2 pts on 22.2% | **29.8% (+7.6) — killed** | 24.4% (+2.2) |
| OverprotectStrongPoint quiet rate | +1..+5 on 8.4% | **18.6% (+10.2) — killed** | 9.4% (+1.0) |
| suggest@3 | unchanged | **7/25 — regressed** | 8/25 restored |
| g62 flips both misses | yes | yes | yes |

The first design's error was the same one every proxy error this run
has had: "two-file island" is not "isolated pair". Every ordinary a+b
or g+h wing remnant qualified, and the overprotection arm then blessed
every blockaded isolani with two backers — half of master chess. The
revision restricts the pair to CENTRAL files (c-f, the remnant of a
dissolved center, Nimzowitsch's own c6+d5) and the overprotection claim
to pair stop squares only.

Two things stated against ourselves. First, the revised BTP delta
(+2.2) still exceeds the registered band (+<2) — the band was written
for the broader design and never re-registered for the revision, which
is a process miss worth naming: **bands are per-design, and a revision
gets a fresh one or inherits the old one explicitly.** The overage
ships anyway on one argument: the added firings are structure-true by
construction (the hint can only fire when a piece stands on a central
pair member's stop square), so unlike an alert on a quiet position,
each is a true sentence about a present structure — the quiet set
prices alert falseness, not plan-hint truth. Second, the 8%
"UprootBlockader standard" line in the prediction was mis-specified:
both hints' baselines already sat at 22.2% and 8.4% before any change,
so the line as written was unsatisfiable at birth. Measured deltas were
the operative constraint throughout, and the prediction file should
have said so.

Chess Praxis: **8/8 plans, 11/11 imbalances** — both entered-to-fail
expectations flipped by detectors built for the structures they named.
ALL BOOKS: plans 77.3% (119/154), imbalances 91.3%, alerts 52.0%,
favors 69.8%, suggest@1/3 unchanged, negatives 41 with only the g70
red. Composition: 169 positions, 162 Jeremy Silman + 7 Nimzowitsch,
quiet holdout fixed at 500.

## Prophylaxis batch 1: the first author-labeled test of the denial machinery

Corpus composition: **173 positions — 162 Jeremy Silman, 11 Nimzowitsch
across four stratagem families** (blockade, fight-against-the-blockader,
isolani, prophylaxis); negative anchors 45, 44 clean, the one red still
g70; quiet holdout fixed at 500 and drift-checked identical (46.6%,
0.58/pos) — no detector changed this batch.

Four entries from games 53, 54 and 55, each anchored at a ply the Part
III introduction cites by number, each carrying the book's own
prophylactic move as `best_moves` — the first data ever run through
suggest and role_of that was labeled prophylactic by the author
himself. Selection procedure (grep for explicit prophylaxis
annotations, prefer ply-specific claims) was fixed in the prediction
before any game was read. All axes green except the standing g70
items: imbalances 15/15, plans 8/8, favors 7/8 — including three
"balanced" verdicts called correctly, which the favors axis rarely
manages.

### The prediction scorecard, two refutations in opposite directions

1. **role_of sees prophylaxis at least half the time: REFUTED — 1 of 4
   (25%), sitting exactly on the pre-registered finding line.** The one
   hit is 21...Qf7 (denial): it blocks a CONCRETE piece plan, White's
   Bc1-e3-d4. Both waiting moves (8...a6, 6...a6) classify
   "constructive" and the prophylaxis-with-threat (24...Qe8) classifies
   "neither". The shape of the blindness is now precise: the machinery
   recognizes denial only where it obstructs a specific piece route;
   waiting moves and goading threats — the beginning of every
   prophylaxis, in the book's own words — are invisible to it.
2. **The maintainer's tempo hypothesis: UNANSWERABLE, which is its own
   finding.** Zero of the denial/both picks sit where the opponent's
   plan is faster — but zero of the constructive ones do either,
   because own/opp horizons are None on every one of the 29 labeled
   moves in the study. The tempo comparison has never had data: the
   scheme-horizon machinery does not emit on these positions. The
   hypothesis is not refuted; it has not yet been tested, and testing
   it is blocked on horizons actually existing.
3. **suggest@3 under 40%: REFUTED UPWARD — 3 of 4 (75%), with
   suggest@1 at 0 of 4.** The plan-led suggester gets prophylactic
   moves INTO the top three far more often than predicted; what it
   cannot do is rank them first (g54's Qe8 misses entirely — top three
   were Bf6/Bh8/Nh8). The suggest problem on this family is ranking,
   not recognition, which redirects any future work there.
4. Negatives grew 3, all green; one entry (g53's move-21 position)
   carries no ban because everything that fires there is unsettled by
   the text — the at-least-one-per-entry prediction is 3/4, missed and
   stated.

The multi-author measurement: 11 entries across four families,
roughly half of the few-dozen threshold. Every axis figure above
carries its composition, per the standing rule.

## Plan speed priced: the tempo hypothesis needs a term that does not exist

Batch 1's instrument gap, given the king-study treatment. Prediction
registered before the sweep; `kibitz-cli horizon-study` over the 29
labeled best-move entries and the quiet 500:

| | labeled | quiet |
|---|---|---|
| scheme for either side | 10% | 12% |
| **schemes for BOTH sides** | **0%** | **1%** |
| any maneuver | 45% | 69% |
| no speed at all | 55% | 31% |

Predictions 1 and 2 held (scheme coverage under 15%, far below the 30%
artifact line; maneuver coverage under 50% on the labeled set).
Prediction 3 split: scheme coverage matches across sets within 2
points, but maneuver coverage differs by 24 — quiet master middlegames
carry outposts and open files at a rate the prophylactic positions do
not, and the prediction's "within 15 points" was refuted on that half.

The decision the price forces: the tempo comparison in role_of needs
BOTH sides to have a speed, which happens in ~1% of positions, because
horizons descend from schemes, schemes from converging routed
maneuvers, and only three hints ever route. A fallback from schemes to
bare maneuver cost would still leave the comparison dataless in the
majority of positions. **The fix is a plan-speed term across the hint
vocabulary — every hint needs a moves-to-execute estimate — which is
run-sized B3 design work, now named and priced, and the prerequisite
for both the maintainer's tempo hypothesis and any suggest@1 ranking
fix.** Not started; the design deserves its own prediction sheet.

## The plan-speed term ships, and the tempo hypothesis gets its first data

The run-sized design horizon-study priced, built against its own
prediction sheet. `speed: Option<u8>` on every plan hint — moves the
owner needs to complete or activate the plan — computed in a single
post-pass (`plans::annotate_speed`) that cannot change what fires, with
role_of's horizon falling back from schemes to attributed plan speeds.
Maintenance plans keep None by design: a plan with no arrival time has
no speed, and zero would hand every side a trivial horizon.

### Scorecard

| prediction | line | actual |
|---|---|---|
| both-sides coverage >= 60%, both sets | under 40% = do not ship | quiet **69%** (held); labeled **45%** (REFUTED, above the ship line) |
| zero behavioral drift | any movement = bug | **held** — book-eval diffed IDENTICAL against a stash-restored baseline; alerts-fp unchanged |
| three spot-checks | any wrong = family ships as None | **all held**, now committed as unit tests |
| tempo hypothesis measured, no outcome predicted | — | measured (below) |

The labeled-set refutation is informative: prophylactic and early
positions carry maintenance and development hints that correctly have
no speed, so coverage there is structurally lower than in middlegames.
45% sits above the pre-registered 40% ship line, and shipping at a
refuted prediction with the refutation stated is the relocated-pawn
precedent, not an exception to it.

The drift check deserves its sentence: the harness had no true
pre-change baseline mid-turn, so one was made — stash the change,
measure, restore, diff. IDENTICAL to the byte on every axis.

### The tempo hypothesis, measured for the first time

Of the 17 role-classified picks on labeled data: **3 of 8 denial/both
picks sit where the opponent's plan is faster, against 2 of 9
constructive ones.** Directionally consistent with the maintainer's
hypothesis — prophylactic picks do sit opp-faster at a higher rate —
and nowhere near a confirmation: n is 8, both-sides coverage on this
set is 45%, and the margin is one pick. The instrument now exists; the
verdict needs the prophylaxis corpus to grow and coverage to rise.
No stronger claim is made.

## Prophylaxis batch 2: the blindness diagnosis holds, the recognition claim wobbles

Corpus composition: **176 positions — 162 Jeremy Silman, 14 Nimzowitsch
(four stratagem families)**; negatives 47 (46 clean, g70 the standing
red); quiet holdout fixed and drift-checked (46.6%). Corpus-only batch;
selection (games 52, 38, 21) and predictions registered before reading.

Three new author-labeled moves, including the corpus's **first WHITE
prophylactic anchor** (19.Qb2, whose later continuation 24.Qe5 the book
credits to prophylaxis by name). Axes: imbalances 17/17, plans 8/8,
favors 8/9 with all three new calls hit.

Scorecard:

1. **role_of 25-50% band: held — 1 of 3 (33%).** And the hit pattern
   repeats batch 1 exactly: 3...e5 classifies (it obstructs the
   concrete d2-d4 break); 14...Rad8 (forestalls c5) and 19.Qb2 (king
   security) classify "neither". The route/break-obstruction diagnosis
   is stable across seven labeled moves and two batches. Over 75%
   would have refuted it; 33% did not.
2. **suggest@3 >= 50% on new entries: REFUTED — 1 of 3.** Batch 1
   measured recognition at 75% and this batch says that number does
   not generalize; pooled, the family sits at 4/7. The honest state:
   recognition is neither reliably present nor absent at n=7, and the
   claim needs the family corpus it was always going to need.
3. Two new vocabulary gaps harvested (forestall-pawn-break,
   preventive-king-security) and restrain-liberating-pawn-move earned
   its second citation across two books' worth of games.
4. Tempo tally, reported not predicted: denial/both picks 9, of which
   3 sit opp-faster; constructive 2 of 9. Unchanged margin — the new
   early-opening anchor carries no costable plans (development and
   maintenance hints correctly have no speed), which is the labeled-set
   coverage structure the plan-speed scorecard already named.

## Praxis bulk batch: eight entries, four new families, and the corpus crosses the threshold

Corpus composition: **184 positions — 162 Jeremy Silman, 22 Nimzowitsch
across EIGHT stratagem families** (blockade, fight-against-the-
blockader, isolani, prophylaxis, over-protection, restraint,
alternating maneuvers, lust-to-expand); negatives 59, 58 clean, g70
still the only red; quiet holdout fixed and drift-checked (46.6%).
Selection and predictions registered before reading; every entry
replay-verified; batch size raised to eight on the maintainer's
direction, and the pipeline held.

Scorecard:

1. **Over-protection detector arms on the family's own games: 3 of 4 —
   held.** g58, g59 and g60 (the Qg4 French complex and Bogoljubow's
   avant-la-lettre Bf4) all fire OverprotectStrongPoint on the e5
   spearhead. The miss is g57, entered to fail: black's d5 sits one
   rank shy of the spearhead band, the depth analogue of the square
   narrowness g62 documented. Plans 11/12, the only miss being that
   deliberate one.
2. imbalances 26/26; favors 11/14 with both misses (g13, g78)
   predicted in their own entry notes — hypermodern positions where
   the space ledger reads white and the book reads black.
3. **role_of stability: third sample inside the band — 3 of 8
   (37.5%).** And the hit pattern refined itself: 8...Nce7 classifies
   (a routed regroup), 48...Kc6 classifies as denial (a king
   maneuver), while every rook-consolidation and queen-repositioning
   move stays "neither".
4. **Four new vocabulary gaps** (alternating-maneuvers,
   lust-to-expand, sacrifice-for-blockade, prophylactic-consolidation),
   and restrain-liberating-pawn-move reaches THREE citations across
   three games — the best-supported concept debt in the corpus.

**The number the batch hardened: suggest@3 on author-labeled strategic
moves is 4/15 (26.7%), with suggest@1 at 0/15.** Batch 1's 75% was the
outlier, not the rule. Across fifteen labeled moves the suggester
almost never surfaces the book's quiet move — its top-3s are uniformly
piece-active. Combined with the role_of pattern, the shape of the
engine's Nimzowitsch gap is now precise: it reads his STRUCTURES
nearly perfectly (26/26 imbalances this batch, 59/59 across all Praxis
batches), executes his named plans where a detector exists, and is
blind to exactly two things — quiet-move ranking and non-obstruction
prophylaxis. The multi-author threshold is crossed; that sentence is
the draft of the answer.

Tempo tally (reported): denial/both 11, of which 3 opp-faster;
constructive 2 of 11. Horizons now appear across the study's columns —
the plan-speed term is feeding it — but the margin stays one pick.

## The lagging-king gate, built to its price — and refused by it

The one priced alerts gain on the board, attempted against its
pre-registered sheet. Gate: the A-candidate condition (home d/e king,
enemy castled, queens on) plus "the enemy can actually open the
centre", the term both target book entries name. Two designs, one
measurement each:

| | book alerts | quiet screen | line |
|---|---|---|---|
| v1: captures + push-contact + half-open files | 16/25 (+3 flips) | **53.6% (+7.0)** | +3.1 — killed |
| v2: pawn ACTS only (captures, push-contact) | 15/25 (+8 pts, both targets) | **51.8% (+5.2)** | +3.1 — killed |

v1 also turned three negative anchors red and exposed a corpus defect
worth more than the detector work: the WeakKing bans on praxis-g35,
g59 and g60 had been written on detector SILENCE, in violation of the
corpus's own selection rule (bans only where the text settles the
position as quiet — these three texts have those kings attacked or
castling promptly). The three bans are removed with per-entry
correction notes; cbcs-136, the rule-valid anchor for the concept,
stayed clean through both designs, which is exactly what a good anchor
does.

The verdict the numbers force: **+8 recall at +5.2 firing is 1.54
points per point — below the 2.6 shipped standard, below even the 1.87
naive design this same axis refused in run 12.** Central pawn tension
is ordinary master chess; "can open the centre" does not separate the
dangerous lagging king from the routine pre-castling position, even
restricted to pawn acts. The arm is reverted in full. The +8 stays
unbought, and the axis keeps its floor at 52.0% — now with the added
knowledge that the book's own gate term fails the price, so any future
attempt needs a different discriminator entirely (enemy READINESS —
the development-lead half of the mechanism — is the untested
candidate, for a future sheet).

## Praxis bulk 2: seven entries, four more families

Corpus composition: **191 positions — 162 Jeremy Silman, 29 Nimzowitsch
across twelve stratagem families**; negatives 65 (64 clean, g70 the
standing red); quiet holdout fixed and drift-checked (46.6%). Predictions
registered before reading; every entry replay-verified; game 22 was
DROPPED rather than transcribed because the book prints its score only
from move 37 — no replay chain, no entry, no exceptions.

Scorecard: imbalances 32/32 and RookToSeventh hit on the absolute-
seventh exhibit (predictions 1 and 2 held); four new vocabulary gaps
(attack-chain-at-base — the pawn-chain family's first tag —
time-the-liberating-break, roll-up-the-restraining-pawn,
hanging-pawns-blockaded-security) against a predicted two; favors
13/17 with the new miss (g18) the same hypermodern-bind shape as g13
and g78. **Prediction 3 refuted low**: 1 of the 6 new labeled moves
classified by role_of (17%, band was 25-50%) — even the timed pawn
breaks read "neither", which sharpens the blindness diagnosis rather
than softening it, but the band was wrong and is retired: the honest
statement after four samples is that role_of classifies 10 of 35
author-labeled strategic moves (29%), almost exclusively the
route-obstruction subset.

## The multi-author comparison, first formal reading

The question this line of work was opened to answer: does the engine
track Jeremy Silman better than Nimzowitsch? At 29 Nimzowitsch entries
across twelve families, selected by the author's own stratagem index
through a chain with no judgment calls in it, the answer has a shape —
and it is not the feared one.

| axis | Jeremy Silman (162) | Nimzowitsch (29) |
|---|---|---|
| imbalances | 261/287 = 90.9% | **32/32 = 100%** |
| plans | 111/146 = 76.0% | **12/13 = 92.3%** |
| favors | 68/99 = 68.7% | 13/17 = 76.5% |
| suggest@3 | 8/25 = 32.0% | **4/21 = 19.0%** |
| role_of on labeled moves | (not labeled) | 10/35 = 29% |

Not a Jeremy Silman emulator: on the structural axes the engine reads
Nimzowitsch BETTER than the corpus it was tuned against, partly
because the Praxis entries are replay-derived positions with no
reconstruction noise, and partly because run 12 built detectors for
his named structures (blockade quality, uprooting, the isolated pair,
the blockade-point overprotection, passer classification) with the
corpus growing alongside. The gap is concentrated and named: the
QUIET-MOVE axes. suggest@3 drops by a third against the Jeremy Silman number on
the moves Nimzowitsch himself labeled, and the denial classifier sees
less than a third of his prophylaxis. The honest one-sentence answer:
**the engine explains Nimzowitsch's positions better than Jeremy Silman's,
and plays his moves worse — because his signature is the quiet move,
and quiet moves are the engine's measured blind spot on every axis
that touches them.**

Caveats attached: the two corpora differ in transcription fidelity
(replay-derived vs diagram-read), in negative density, and in what
they ask of the alerts axis (the Praxis batch deliberately carries no
alert expectations); 29 entries is past the stated threshold but the
per-family counts are still 2-7. The comparison strengthens or breaks
as the corpus grows; the composition line above is its denominator.

## Standing figures (supersedes every inline aggregate above)

A second desk reproduced all eight cost terms and the queenless class
to the digit, then caught the one way this document could mislead: the
bulk sections update composition and negatives at every corpus
revision, but the last full ALL BOOKS aggregate printed here dated
from the 169-position era, with nothing marking it superseded — a
reader scanning for the latest aggregate landed on stale denominators.
The convention from here: **this section is the standing aggregate, it
is updated in the same commit as any measurement that moves it, and
the most recent "Standing figures" section supersedes every aggregate
elsewhere in the file.**

As of 191 positions (162 Jeremy Silman across four books, 29
Nimzowitsch across twelve stratagem families; negatives 65; quiet
holdout 500, both sides 2300+, |eval| < 50cp at 200k nodes):

| axis | figure |
|---|---|
| imbalances | 293/319 = 91.8% |
| plans | 123/159 = 77.4% |
| alerts | 13/25 = 52.0% (3 line-conditional excluded, unscored) |
| favors | 81/116 = 69.8% |
| suggest@1 | 2/46 = 4.3% |
| suggest@3 | 12/46 = 26.1% |
| negatives | 64/65 clean (praxis-g70 WeakKing, red by declaration) |

Cost terms on the fixed 500: screen 46.6%, WeakKing 0.58/pos,
TrappedPiece 0.10/pos, entombed 0.000, UprootBlockader 4.2%,
OutsidePasserDecoy 2.8%, BlockadeThenPressure 24.4%,
OverprotectStrongPoint 9.4%.

Worth stating, since the stale line accidentally measured it: between
169 and 191 positions the rates moved 91.3→91.8, 77.3→77.4, 69.8→69.8
and alerts not at all — **stability across 13% corpus growth**, which
is what a detector suite that is not fitted to its corpus should show.

The second desk's remaining caveat — that the corpora themselves are
git-ignored, so a re-run verifies the machine's present inputs rather
than the ref's — is closed by docs/CORPUS_MANIFEST.md: SHA-256
fingerprints of every measurement input, regenerated by
scripts/corpus_manifest.sh in the same commit as any corpus change.
Hashes are not content; the licensing posture is untouched.

## The suggest re-baseline that wasn't: three fixes, zero movement

The run three tickets waited for, executed against one sheet — and the
priced re-baseline turned out to cost nothing. route_to now refuses
waypoints through enemy pieces unless the capture is SEE-sound;
suggest ranks unmarked moves ahead of static-risk-marked ones before
truncating; equal scores tiebreak by plan speed (the speed term's
second consumer). After all three: **every axis, every cost term, all
41 suites — byte-identical**, alerts-fp to the digit.

Scored against the sheet: the route fix landed at the top of its band
(plans -0 of a predicted -0 to -3 — the phantom paths existed but
never carried a published number); the ordering fix's +1..+4 band was
REFUTED at +0, because the position it was built around (Praxis game
58) genuinely has no safe plan-derived candidate — the fix is monotone
and correct and currently vacuous; the speed tiebreak moved nothing at
current score distributions. Both DECISIONS_NEEDED tickets are closed
at zero re-measurement cost. The general lesson is the deferral
calculus running in reverse: parking route_to was right when its price
was unknown, and the price was nothing — but only the fix could prove
that.

## The mega-batch: nine entries, the corpus reaches 200

Nine entries from seven games across five more families —
centralization and the author's own FAULTY-centralization exhibits,
consolidation-by-retreat, restraint-plus-wing (Saemisch), prepared
breakthrough, the Johner restraint masterpiece (12...Qd7!!),
mummification's preventive rook, and the von Gottschall zugzwang.
Game 14 was dropped (printed from move 53 only — no replay chain, no
entry), as was game 22 before it; the rule holds under scale.

Prediction scorecard: imbalances 38/38 and seven new vocabulary gaps
(predictions 2-3 held, gaps at more than double the predicted three,
including zugzwang-as-weapon — inventory mechanism 18's first corpus
citation); role_of 2 of 9 on the new labeled moves, inside the
successor band; drift zero. **Prediction 1 failed, and vacuously,
which is the finding**: the author's faulty-centralization exhibits
were predicted to yield plan bans from his own condemnations, and they
cannot — nothing centralization-shaped fires for the condemned side at
either anchor ply, because building and advancing a proud pawn center
are not concepts the engine can voice. The engine cannot be caught
praising what it cannot say. The negative family is real in the book
and vacuous against this vocabulary; recorded in the entry notes where
the next vocabulary extension will trip over it.

## Standing figures (supersedes every aggregate above)

As of **200 positions — 162 Jeremy Silman, 38 Nimzowitsch across
seventeen stratagem families**; negatives 79 (78 clean, praxis-g70 the
standing red); quiet holdout fixed at 500; corpus fingerprints in
docs/CORPUS_MANIFEST.md, printed by every measurement.

| axis | figure |
|---|---|
| imbalances | 299/325 = 92.0% |
| plans | 123/159 = 77.4% |
| alerts | 13/25 = 52.0% (3 line-conditional excluded) |
| favors | 86/122 = 70.5% |
| suggest@1 | 2/55 = 3.6% |
| suggest@3 | 13/55 = 23.6% |

Cost terms on the fixed 500: screen 46.6%, WeakKing 0.58/pos,
TrappedPiece 0.10/pos, entombed 0.000, UprootBlockader 4.2%,
OutsidePasserDecoy 2.8%, BlockadeThenPressure 24.4%,
OverprotectStrongPoint 9.4%.

Multi-author, updated: Nimzowitsch imbalances **38/38 (100%)** vs
Jeremy Silman 90.9%; plans 92.3% vs 76.0%; favors 78.3% vs 68.7%;
suggest@3 **16.7% vs 32.0%**. The comparison's shape sharpened as the
corpus grew: the structure-reading gap widened in Nimzowitsch's favor
and the quiet-move gap widened against him — both directions
strengthening the one-sentence answer already on record.
