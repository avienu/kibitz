# DECISIONS_NEEDED.md

Parked decisions that change documented behavior, the license boundary, or
user-visible product behavior. Work continued on other tracks.

Run-2 status: items 1–2 are now the ONLY blockers for full Phase 1
acceptance (annotation storage + editing UI). Everything else in Phase 1 is
built and verified around them. New context per item below.

1. **PGN annotation import policy (user-visible).** The importers store the
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

2. **Games containing null moves are skipped (user-visible).** Encoding v1
   has no null-move byte. Decide: reserve byte 255 for null in encoding v2
   (recommended; fold into the same bump as item 1), or keep rejecting.
   *Run-2 data*: zero null-move games in Lichess 2013-01, TWIC 1650, or
   any of the ten real SCID bases' mainlines — the risk is confined to
   engine-annotated variation lines, which strengthens folding this into
   the annotation decision.

3. **Duplicate-detection signature definition (schema detail with product
   consequences).** Implemented as FNV-1a over (White, Black, Date, Result,
   normalized) + a separate hash of the move sequence; a game is a duplicate
   only if BOTH match. Event/site were deliberately excluded so the same game
   re-exported from different tools still dedups. Consequence: the same moves
   played by the same players on the same day in *different events* would be
   flagged as duplicates. Acceptable? (TWIC vs SCID overlap testing in the
   full Phase 1 acceptance will tell.)

4. **Elo top-4-bits in .si4 (cleanroom gap).** The community documentation
   does not say what the top nibble of the Elo fields means (likely a rating
   type). si4-read masks it off. All header dumps of the ten real bases
   produced sensible ratings with the mask, so this stays low-priority.

5. ~~si4 `.sg4` movetext decoding gaps~~ **RESOLVED empirically in run 2**
   (docs/SI4_FORMAT_NOTES.md §6): rook table transposed in the community
   doc, pawn double-push = code 15, null move = 0x00, swap-remove piece
   lists, coded non-standard tags. Validated on 7,905/7,905 real games.
   Remaining unknowns (Elo nibble, pre-v400 entries) don't block import.

6. **NEW — opening-tree root-position latency.** The tree query from the
   START position aggregates every game (956 ms at 121k games) and will
   blow the 1 s budget on a megabase. Options: materialized cache table for
   shallow positions (invalidate on import), or accept slower root queries.
   Non-blocking now; will matter at Phase 1 full-scale acceptance.

7. **NEW — ficsgames.org usage posture.** The FICS client works via the
   site's download CGI, but robots.txt disallows /cgi-bin and the service
   is volunteer-run with bandwidth quotas. Current behavior: strictly
   serial, descriptive User-Agent, generous timeout, no tests hit it by
   default. Decide whether to (a) keep the client with a prominent
   "be considerate / consider emailing the maintainer" notice, (b) ask
   ficsgames.org for blessing (contact: fics.ludens@gmail.com), or (c)
   drop to manual-download-only like ICC.
