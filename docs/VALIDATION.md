# VALIDATION.md — WSUI screen precision/recall

Per docs/SILMAN_ENGINE_SPEC.md (validation plan). First measured 2026-07-25
(run 3). Harness: `app/silman-db/src/bin/wsui-validate.rs`.

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
cargo run --release -p silman-db --bin wsui-validate -- \
  --build-quiet-from <db.sqlite> --per-class 500 > quiet_fens.txt
cargo run --release -p silman-db --bin wsui-validate -- \
  --puzzles lichess_db_puzzle.csv --quiet quiet_fens.txt --per-class 2000
```

## Results (2026-07-25, silman-core @ run-3 detectors)

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

## Firing-rule study (2026-07-26, run 5, silman-core @ run-5 detectors)

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
