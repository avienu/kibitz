# DECISIONS_NEEDED.md

Parked decisions that change documented behavior, the license boundary, or
user-visible product behavior. Work continued on other tracks.

1. **PGN annotation import policy (user-visible).** The Phase 1 importer
   stores the mainline only: variations, comments and NAGs are parsed and
   *discarded*. ROADMAP Phase 1 later requires annotation editing in the game
   view, which implies storing them. Options: (a) extend the movetext
   encoding with variation/comment markers (si4-style byte stream), (b) a
   separate `annotations` table keyed by (game_id, ply, path). Preference:
   (a), it keeps one blob per game; needs an encoding-version bump.

2. **Games containing null moves are skipped (user-visible).** Encoding v1
   has no null-move byte (a null move is not in the legal-move ordering).
   Lichess 2013-01 contained none, but annotated/engine PGNs use `--`/`Z0`.
   Decide: reserve a byte value (e.g. 255) for null in encoding v2, or keep
   rejecting such games.

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
   type). si4-read masks it off. If real databases show nonzero top nibbles,
   we can only document observed values — resolving the semantics from SCID
   source is forbidden for the BSD crate.

5. **si4 `.sg4` movetext decoding gaps.** Several documented ambiguities
   (queen vs rook code table orientation, two-square pawn advance, null move,
   comment charset — docs/SI4_FORMAT_NOTES.md §5) can only be settled
   empirically against your real databases. Provide a small real .si4/.sg4/
   .sn4 set in testdata/private/ for the Phase 1 si4 importer work.
