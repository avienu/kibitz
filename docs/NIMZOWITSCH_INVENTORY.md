# Nimzowitsch concept inventory — chapter 4, "The Passed Pawn"

Run 12. The sequencing rule this document exists to honor: **the
inventory comes before the corpus.** Threshold tuning on the existing
axes returned +9.4, then 0, then 0.2 points across its last three
attempts; every recent gain came from a new distinction. Nimzowitsch is
being added for concept coverage — mechanisms Jeremy Silman largely does
not name — and transcribing positions for mechanisms no detector can
express would manufacture failures that teach nothing. So: one chapter,
every named mechanism, each bucketed against the engine as it exists,
with the split predicted before the chapter was opened.

## Source and licensing

Source on this machine: **My System, 21st Century Edition** (Hays
Publishing, ed. Lou Hays — modernized English translation, algebraic
notation), PDF in the maintainer's Dropbox instruction library. No Chess
Praxis, no Die Blockade, no German original found on the machine.

The German original is public domain (Nimzowitsch died 1935). **This
edition's English text is not.** Corpus entries built from it must be
FEN + short factual expectation labels keyed to edition and page — the
htryc-379-140 pattern. No transcribed prose, ever.

**Audit of the existing corpus against that rule: it does not hold.**
41 of 162 entries carry notes with explicit `caption:` / `text:` /
`Book text:` / `book says` markers — 39 of 52 in The Complete Book of
Chess Strategy, 2 in the Endgame Course, none in The Amateur's Mind or
HTRYC. Some are diagram captions (short phrases, plausibly fine), but
several carry full sentences labeled as book text (cbcs-192, cbcs-216,
cbcs-217, cbcs-219, cbcs-236, cbcs-239 among others). Mitigation:
testdata/private is git-ignored and never ships. But the CBCS
transcription style is the one precedent the Nimzowitsch work might have
copied, and it is the wrong precedent. Flagged to the maintainer;
entries not rewritten without direction.

## Prediction (recorded before opening the chapter)

1. The chapter names 12-18 distinct mechanisms.
2. Split ≈ 35% / 30% / 35% across buckets 1/2/3; hard line fixed in
   advance: **bucket 3 under 50%**, else the chapter is not ready for
   transcription.
3. Named bucket-3 candidates: blockader utility; un-blockading
   (uprooting); gaoler economics; lust-to-expand as a threat term;
   qualitative majority (hedged toward bucket 2).
4. At least 2 mechanisms land in bucket 1 with no work.

## The inventory

Buckets: **B1** = expressible by an existing detector as-is. **B2** =
existing detector plus a new condition. **B3** = requires a detector
that does not exist. One bucket per mechanism (sub-parts noted).

| # | mechanism (edition page) | bucket | expression |
|---|---|---|---|
| 1 | healthy majority must yield a passer (31-32) | B1 | pawn_structure healthy-majority test (`healthy_majority_promises_a_passer` / crippled-majority silence) |
| 2 | the candidate rule — the free pawn advances first, others are supports (32) | B2 | CreatePassedPawn + candidate-file identification; suggest ranks the candidate push |
| 3 | the enemy passer must be blockaded (32-33) | B1 | BlockadeWhitePasser / BlockadeBlackPasser |
| 4 | the blockade square is a weak square to be occupied and exploited (35) | B1 | BlockadeThenPressure + the SquaresOutposts hole test |
| 5 | the passer's "lust to expand" — breakthrough sacrifices by the unblockaded passer (33-34, Alekhine-Treybal) | B3 | an advance-threat term: the sac's value is the position after it, which static hints cannot hold |
| 6 | the blockader is shielded from frontal attack by the pawn it stops (35) | B2 | blockade detection + shielded-square condition on the blockader |
| 7 | blockader quality — the piece must radiate threats FROM the square; knight ideal (37-38) | B2 | blockade hint + mobility/threat count of the blockader on its post (cbcs-178 already asks for this) |
| 8 | elasticity in the strict sense — leave and return in time, changing the blockade square (37-38, Rh4-h2) | B3 | a race calculation: can the blockader return before the pawn runs |
| 9 | the reserve blockading point — which of two squares to hold first (43-44) | B2 | endgame blockade / ActivateKingInEndgame + square-pair naming |
| 10 | uprooting the blockader — "Changez les blockeurs", negotiations (39-41) | B2 | route_to_attack / TradeOffAttacker machinery re-targeted at the blockading piece |
| 11 | crippling spreads to the rear — non-local paralysis, pieces tied to the pawn (36, Leonhardt-Nimzowitsch) | B3 | a tied-pieces / mobility-contagion measure; entomb covers only the terminal case |
| 12 | the King's frontal attack on the isolated passer, with the opposition (41-42) | B2 | king route_to at the square in front; TakeOpposition sub-part already B1 |
| 13 | the turning (enveloping) movement (41-42, 46) | B2 | king route_to targeting behind/beside the pawn; geometry distinguishes it from frontal |
| 14 | connected passers — blockade-impossibility; the advance rule between them (44-45) | B2 | passed-set adjacency condition (not classified today); the which-pawn-steps rule is a B3 sub-part |
| 15 | the protected passer's immunity — capturing loses by the square of the pawn (46) | B2 | protected passers already scored +15; the immunity concept needs square-of-the-pawn arithmetic |
| 16 | the outside passer as diversion trump (46-47) | B2 | distance-from-kings condition on the existing passed set; the decoy-sacrifice ending is a B3 sub-part |
| 17 | when the advance should be risked — the enumerated push rules (47-48) | B3 | policy over consequences; suggest-verify territory |
| 18 | Zugzwang as the blockader's ally (42, 49-50) | B3 | no zugzwang detector exists |

## Predicted versus actual

| | predicted | actual |
|---|---|---|
| mechanisms named | 12-18 | **18** (top of band) |
| B1 | ~35% (4-6) | **3 (17%)** |
| B2 | ~30% (4-6) | **10 (56%)** |
| B3 | ~35% (4-7) | **5 (28%)** |
| B3 under 50% | line | **held** |
| ≥2 in B1 as-is | yes | held (3) |

Two of the five named B3 candidates were wrong, and wrong in the
direction the prediction itself flagged as the interesting refutation:
**blockader quality** and **uprooting** are B2 — the existing machinery
(blockade hints, route_to_attack) reaches them with one condition each.
I underestimated the engine. Gaoler economics and lust-to-expand were B3
as predicted; qualitative majority landed B1/B2 as hedged.

## What the split means

The chapter is ready for transcription **after the B2 conditions exist,
not before**. Ten of eighteen mechanisms are one condition away — this
is the +9.4-shaped work (a new distinction on existing machinery), and
it is most of the chapter. Recommended order:

1. Build the cheap B2 conditions first: passer classification
   (connected / outside — protected is half-done), blockader quality,
   uprooting, shielded blockader. Each is a pre-registered
   prediction + entomb-fp-style cost term before wiring, per
   VALIDATION.md methodology.
2. Then transcribe the chapter's positions (FEN + labels only). His
   illustrative material is heavily continuation-based: expectations
   that hold only inside a line go to `line_conditional` from day one —
   the category exists precisely so this does not recreate the
   counterfactual mess at scale.
3. B3 detectors (advance-threat term, elasticity race, tied-pieces
   measure, push policy, zugzwang) are each their own run-sized design.
   None blocks transcription of the B1/B2 material.

## The multi-author measurement this enables

Once entries from two authors exist, score them SEPARATELY: does the
engine track Jeremy Silman better than Nimzowitsch on the same axes? If
it does, the product is a Jeremy Silman emulator — a fine product, but a
different claim than "explains positions in human terms," and nobody has
checked. The per-book breakdown book-eval already prints is the
instrument; it just needs a second author in the denominator.
