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
copied, and it is the wrong precedent. Flagged to the maintainer.

**Remediated on the maintainer's ruling (run 12): 35 entries rewritten
as factual claims in our own words, 6 judged clean under the stated
criterion (titular captions as identifiers, bare moves/results, own
reconstruction notes), citations kept throughout.** The Nimzowitsch
corpus is built to this standard from its first entry.

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

## Step 0 addendum — source assessment (run 12, second pass)

### Source A: the two My System scans

| | `My System.pdf` (302pp) | `nimzowitsch - my system.pdf` (270pp) |
|---|---|---|
| edition | **Quality Chess 2007** ("New Translation", 2nd print, from the 2005 Rattman German edition) | 21st Century Edition (Hays) — **byte-identical (same SHA-1) to the Dropbox copy** the chapter-4 inventory was built from |
| text | crisp digital-quality bilevel (ScanKromsator); OCR trivially viable | photocopy-grade, grainy; OCR marginal |
| diagrams | numbered continuously, sharp glyphs, every square unambiguous — at p. 250 the count is at diagram 490, total ≈ 520 | Part 1-2 numbered (ch. 4 spans diagrams 38-61); games-section diagrams unnumbered, muddier but readable; total est. 250-300 |
| page geometry | PDF page = printed page (probe-verified) | printed page = PDF page − 10 |

Both are image-only (pdftotext extracts nothing); both are readable by
page-image reading, which is how the chapter-4 inventory was produced.

**Chosen: Quality Chess 2007**, on every axis — sharper diagrams for any
future diagram-to-FEN work, cleaner prose for concept reading, and full
SAN game scores typeset in the text (which mostly removes the
diagram-to-FEN problem for in-game positions; see scoping below). Copied
to `testdata/private/sources/my-system-qc2007.pdf` — verified
git-ignored. Nothing from either file goes into a tracked file.

One consequence flagged: the chapter-4 inventory above cites 21st
Century Edition pages (it predates the QC copy). Future corpus entries
key to the QC edition; the inventory's mechanism list is
edition-independent.

### Source B: Chess Praxis via the chessgames.com collection

Assessed at human scale — the collection page plus one game page,
nothing harvested.

- Collection: "Book: Chess Praxis (Nimzowitsch)", compiled by member
  Baby Hawk, keyed to the **Robert Sherwood translation's** game
  numbering. **100 games, numbered 1-109 with nine gaps** (30, 56, 76,
  79, 88, 96, 99, 104, 106 missing).
- **Annotations are present as described**, and provenance is
  structurally distinguishable, which was the load-bearing question:
  book-derived notes are embedded in the game score itself, opened by
  an explicit "Notes by Nimzowitsch" marker and closed by a footer
  attribution line ("Annotations by Aron Nimzowitsch"); member
  kibitzing is a separate section of dated, usernamed posts below the
  score. The two cannot be confused by structure alone.
- **Two verification duties remain per entry.** First, the footer says
  the site holds 48 Nimzowitsch-annotated games TOTAL — roughly half
  the collection's games will have no book notes on the site, so the
  attribution line must be checked per game, never assumed from
  collection membership. Second, the site's annotation TEXT is a
  transcription with its own defects — on the sampled game a member
  correctly points out the note "Ne5" should read "Nc5" — so site notes
  are a secondary witness; where an expectation label depends on a
  precise claim, the book is the authority.
- Licensing posture unchanged: annotations are read to derive a short
  factual label in our own words; no annotation text is copied into any
  file, tracked or not.

Why B matters: position and judgment arrive PAIRED, in replayable SAN,
with zero diagram-reading — and it supplies the second author for the
does-the-engine-only-track-one-author measurement at a fraction of
Source A's transcription cost. A supplies the vocabulary; B supplies
the positions.

## Diagram-to-FEN: scoped and priced, NOT started

Three paths, cheapest first. The finding is that the hard version of
this problem is mostly avoidable.

**Path A — Chess Praxis positions (Source B): no diagram-to-FEN at
all.** Game scores are historical facts in replayable SAN. Transcribe
the SAN (or take the position from the site's viewer at the annotated
ply), replay with the existing `san` module, emit the FEN. Deterministic
verification for free: an illegal or mis-transcribed line fails to
replay. Cost per entry: minutes. This is the recommended corpus path.

**Path B — My System diagrams that sit inside quoted games (most of
them): replay, don't read.** The QC edition prints full SAN from move
one; the games are also historical and exist in open databases. Replay
to the diagram's ply and the FEN is derived, not transcribed — the
diagram serves only as a checksum. Same verification property as A.

**Path C — constructed/schematic diagrams (no game behind them):
visual read + harness.** Nimzowitsch's ideal-position sketches (the
united-passers schema, blockade skeletons) are typically under a dozen
men. Read the diagram from the page image, emit FEN, verify with the
harness we already trust: cozy_chess legality parse, material sanity,
side-to-move from the surrounding text, double-read on any entry whose
first read fails a check. Error source is my transcription; the checks
are deterministic.

**Path D — a CV pipeline (page raster → board detection → grid split →
per-square classifier → FEN): priced and deferred.** Two to three runs
of engineering: segmentation is easy on the QC scan (strong borders,
clean hatching), but the per-square classifier needs labeled glyphs for
this exact figurine font, which do not exist — the honest bootstrap is
Path B's replay-verified FENs providing free square labels, which means
the pipeline is built AFTER the manual corpus, not instead of it. Break-
even is several hundred diagrams — i.e. digitizing multiple books
wholesale, which is not the current deliverable. Do not build it for
one chapter.

Recommendation standing until countermanded: A and B for corpus
positions, C for the handful of schematics, D deferred.

## Step 0 second addendum — a third source changes the plan

`~/Downloads/0a9cc761332e7f9fc1739a4bccbbbf46.pdf` turns out to be **My
System & Chess Praxis, the New In Chess 2016 combined volume, Robert
Sherwood translation — with a full text layer.** pdftotext extracts
~1MB of clean prose across 1030 text pages, figurine SAN included. It
contains, in one file:

- **My System** (Sherwood)
- **Chess Praxis** (Sherwood — the very translation the chessgames.com
  collection's game numbering is keyed to)
- **The Blockade**, Nimzowitsch's separate monograph, as Appendix Two —
  a book this document reported absent from the machine
- the 1911-1914 chess-revolution history article
- **Nimzowitsch's own Index of Stratagems in Chess Praxis** — every
  named mechanism mapped to the games that illustrate it, by the
  author, for the whole book

Copied (PDF + extracted text) to `testdata/private/sources/`, verified
git-ignored. NIC 2016, all rights reserved: the licensing posture is
unchanged and the text layer changes nothing about it — read to derive
labels, never copy prose into tracked files.

**Consequences, in order of size:**

1. **The primary source is now this volume, for everything.** The QC
   2007 scan drops to a fallback for anything needing the printed
   diagrams; the Niridha scan is now redundant twice over.
2. **The stratagem index is a whole-book concept inventory for Chess
   Praxis, authored by Nimzowitsch.** The chapter-at-a-time inventory
   method stays (My System is where mechanisms are DEFINED), but for
   Praxis the mechanism list and its illustrating games arrive
   pre-paired: stratagem → game number → Sherwood numbering →
   chessgames.com collection → replayable score → FEN. The provenance
   chain has no weak link and no judgment calls in it.
3. **Step 2's price falls again.** SAN in this volume is machine-
   extractable text, so Path B (replay, don't read) can be driven from
   the book itself — parse the printed score, replay with the existing
   san module, emit FEN with the diagram as checksum. The constructed-
   schematic residue (Path C) is all that ever needs eyes on a diagram,
   and a text-layer grep can now locate every schematic first.
4. **The multi-author measurement gets its denominator cheaper**: the
   index's stratagem→games mapping selects high-value Praxis entries
   (blockade: 8 games; prophylaxis: 30+; over-protection: 8) without
   reading the whole book to find them.

Step 2 remains not started; nothing above begins it.

## Mechanism status updates

- **Mechanism 7 (blockader quality), the CHOOSING side: shipped** as
  the `ChooseBlockader` hint (see docs/VALIDATION.md, "ChooseBlockader:
  mechanism 7's choosing side"). Both Diagram 145/146 corpus citations
  graduated; quiet cost 1.4%. What shipped is the piece-type
  preference the chapter states (knight on the stop square, or the
  nearest knight by empty-board distance when the stop is empty — the
  road-can-be-opened proxy that Diagram 145's own g7 pawn forced).
  The threat-radiation half of mechanism 7 already ships as the
  `blockader_<sq>` elasticity evidence.
- **Mechanism 8 (elasticity in the strict sense) stays B3** —
  leave-and-return race, unattempted, deliberately.
- **Liberation restraint (Praxis stratagem, 7 combined citations):
  attempted and refused** on its own sheet — lever geometry plus a
  space gate is not liberation; needs a mobility-delta release term
  (B3). Full record in docs/VALIDATION.md.


## Source-hunt closure: the Freymann elasticity exhibit

The run-11 open item — locate the NIC edition's My System Part 3 for
the Freymann elasticity example — is closed. The game
(Nimzowitsch-von Freyman, Vilnius 1912) is printed with a full score
as My System **Game 13** in the NIC 2016 combined volume, and is now
in the corpus as ms-g13-blockader-and-anti-blockader. No further
Part 3 source is needed for it.
