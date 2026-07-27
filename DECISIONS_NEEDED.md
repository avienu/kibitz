# DECISIONS_NEEDED.md

Parked decisions that change documented behavior, the license boundary, or
user-visible product behavior. Work continued on other tracks.

Run-7 note: no parked decisions. All four run-6 judgment calls stand as
ruled (ECO names now in scope and implemented; the other three
provisionally ratified, untouched). New judgment calls documented in
RUN_REPORT.md run-7 "Honest omissions" — the pattern is uniform: where
the design shows a number the backend cannot yet source (peer
baselines, fingerprint avg-Elo, tactics due count), the UI shows an
explained absence instead of an invented value. Run-7 screenshots are
in (docs/screenshots/run7/, all twelve shots on the real database).

Run-6 note: no parked decisions; the design system is in whole. Four
judgment calls documented for review in RUN_REPORT.md run-6 ("Honest
deviations"): heuristic variation provenance, mate-distance sentinel in
analyses, no live-analysis surface in the new shell (engine-off
reading), ECO-name lookup absent. All have clean run-7 fixes if you
want them.

Run-5 note: no new parked decisions from the core feedback items. Two
judgment calls made on the data and documented for review rather than
parked: (a) the WSUI firing-rule study kept the incumbent solo rule as
default (it wins the balanced objective; the table in VALIDATION.md
shows exactly what any stricter rule costs — flip `WsuiConfig::rule`
if you weigh FP heavier than recall); (b) after verdict fold-back the
narrator regenerates without the annotate-time comment cap (verdicts
sharpen prose, they don't add noise; say the word and it inherits the
cap instead).

Run-4 note: all four maintainer verdicts implemented and regression-
tested; no new parked decisions. Open judgment items listed in
RUN_REPORT.md run-4 section: full-corpus fresh ACPL pass (~2-3h engine
time, one command), NAG multi-glyph display, imports/network-sync UI.

Run-3 note: no new parked decisions. Item 4 closed empirically; item 6's
root-cache remains deferred (scale condition not met); new judgment calls
(WSUI default thresholds, confirm-verdict fold-back into annotations) are
listed in RUN_REPORT.md as review items, not blockers.

**2026-07-25 (late): the maintainer ruled on items 1, 2, 3 and 7; 6 was
accepted as-is for now. All rulings are implemented and tested. Remaining
open items: 4 (low priority) and the deferred half of 6.** Item statuses
below record the decisions for the history.

1. **DECIDED 2026-07-25 — implemented as encoding v2.** Inline escape-token
   stream (the SCID-convergent design the maintainer specified): move
   indices 0–217, tokens COMMENT (varint + UTF-8), NAG, VAR_START/VAR_END
   (nestable), NULL_MOVE, END, and a reserved ESCAPE at the top of the byte
   range. Single-pass, ~1 byte/move for unannotated games, annotations
   physically local, cannot desynchronize. One-shot v1→v2 migration on db
   open (121k games in ~13 s). Round-trip test now asserts FULL token-level
   semantic equality. Original problem statement (for history):
   PGN annotation import policy (user-visible).** The importers store the
   mainline only: variations, comments and NAGs are parsed/decoded and
   *discarded* (with counts reported). Options: (a) extend the movetext
   encoding with variation/comment markers (si4-style byte stream), (b) a
   separate `annotations` table keyed by (game_id, ply, path). Preference:
   (a) + comments in a side table with an FTS5 index (searchable
   annotations), structured `%clk/%eval/%cal` parsing, and a strict
   round-trip test corpus — see the "best-in-world" design discussed with
   the maintainer on 2026-07-25; needs an encoding-version bump.
   *Run-2 scale of the loss on real data*: importing mypages+twictest alone
   dropped 16,653 comments, 29,576 NAGs and 8,680 variations. The sg4
   decoder already extracts all of it (comments, NAG values, variation
   trees walk correctly) — storage is the only missing piece.

2. **DECIDED 2026-07-25 — implemented.** Null move = dedicated escape
   token, applied as side-to-move flip + en-passant clear via
   cozy-chess's null_move(); a null played while in check (legal in PGN
   variations, unrepresentable as a cozy position) truncates that line
   gracefully instead of failing the game. Original problem statement:
   Games containing null moves are skipped (user-visible).** Encoding v1
   has no null-move byte. Decide: reserve byte 255 for null in encoding v2
   (recommended; fold into the same bump as item 1), or keep rejecting.
   *Run-2 data*: zero null-move games in Lichess 2013-01, TWIC 1650, or
   any of the ten real SCID bases' mainlines — the risk is confined to
   engine-annotated variation lines, which strengthens folding this into
   the annotation decision.

3. **DECIDED 2026-07-25 — implemented.** Rule confirmed: move-sequence
   hash + normalized players/date/result. Sources now carry a kind with
   priority personal > twic > online > other; the kept copy is upgraded to
   the highest-priority source's headers, and the losing copy is recorded
   in the `duplicates` link table (never deleted). Original problem
   statement: Duplicate-detection signature definition.** Implemented as FNV-1a over (White, Black, Date, Result,
   normalized) + a separate hash of the move sequence; a game is a duplicate
   only if BOTH match. Event/site were deliberately excluded so the same game
   re-exported from different tools still dedups. Consequence: the same moves
   played by the same players on the same day in *different events* would be
   flagged as duplicates. Acceptable? (TWIC vs SCID overlap testing in the
   full Phase 1 acceptance will tell.)

4. **CLOSED (run 3, empirical):** the top nibble is zero in all 95,066
   rating fields across the ten real databases (incl. 39,628 ICC games).
   Documented in SI4_FORMAT_NOTES.md §6.8 as documented-unknown-but-absent;
   the 12-bit mask loses nothing on available data.

5. ~~si4 `.sg4` movetext decoding gaps~~ **RESOLVED empirically in run 2**
   (docs/SI4_FORMAT_NOTES.md §6): rook table transposed in the community
   doc, pawn double-push = code 15, null move = 0x00, swap-remove piece
   lists, coded non-standard tags. Validated on 7,905/7,905 real games.
   Remaining unknowns (Elo nibble, pre-v400 entries) don't block import.

6. **ACCEPTED for now (2026-07-25):** live with ~1 s root queries; the fix
   when a megabase lands is a materialized aggregate for the first N plies
   (Phase 5-adjacent optimization). Original: opening-tree root latency.** The tree query from the
   START position aggregates every game (956 ms at 121k games) and will
   blow the 1 s budget on a megabase. Options: materialized cache table for
   shallow positions (invalidate on import), or accept slower root queries.
   Non-blocking now; will matter at Phase 1 full-scale acceptance.

7. **DECIDED 2026-07-25 — option (a), same posture as TWIC:**
   user-initiated personal download, provenance-tagged, never
   redistributed; the CLI prints a personal-use/bandwidth notice before
   each sync. Original problem statement: ficsgames.org usage posture.** The FICS client works via the
   site's download CGI, but robots.txt disallows /cgi-bin and the service
   is volunteer-run with bandwidth quotas. Current behavior: strictly
   serial, descriptive User-Agent, generous timeout, no tests hit it by
   default. Decide whether to (a) keep the client with a prominent
   "be considerate / consider emailing the maintainer" notice, (b) ask
   ficsgames.org for blessing (contact: fics.ludens@gmail.com), or (c)
   drop to manual-download-only like ICC.
