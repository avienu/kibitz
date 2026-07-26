# SI4_FORMAT_NOTES.md — cleanroom reference for SCID .si4/.sn4/.sg4

This document is the sole basis for `crates/si4-read`. It was compiled from
community *documentation only*; no SCID source code was consulted (cleanroom
requirement, CLAUDE.md §5). Where the documentation is ambiguous or silent,
that is flagged here rather than resolved by reading SCID.

## Sources

1. https://github.com/bkshrader/si4spec — primary detailed spec (README).
   Contains hyperlinks to SCID/Stockfish source as "reference decoder"
   pointers; those links were NOT followed.
2. https://github.com/asdfjkl/si4spec — original spec (header only).
3. https://scidvspc.sourceforge.net/doc/Formats.htm — official user docs
   (file roles, limits).
4. https://sourceforge.net/p/scid/wiki/ — AboutScidFormat, FileFormats.
5. https://scidb.sourceforge.net/help/en/Database-Formats.html — notes that
   si4 text encoding is undeclared in-file (Latin-1 vs UTF-8).

Conventions: all multi-byte integers big-endian. Bit 0 = least significant.

## 1. Index file (.si4)

### Header — 182 bytes at offset 0

| Offset | Len | Field |
|---|---|---|
| 0x00 | 8 | Magic `"Scid.si\0"` (`53 63 69 64 2E 73 69 00`) |
| 0x08 | 2 | Version, uint16 = 400 |
| 0x0A | 4 | Database type (usually 0) |
| 0x0E | 3 | Number of games, uint24 (max 16,777,214 = 2^24−2) |
| 0x11 | 3 | Auto-load game number (semantics conflict between specs; ignore) |
| 0x14 | 108 | Description, NUL-terminated, last byte must be 0 |
| 0x80 | 54 | Custom flag names: 6 × 9 bytes, each NUL-terminated |
| 0xB6 | | Index entries follow (total header = 182) |

### Index entry — 47 bytes per game (v400)

| Off | Len | Field |
|---|---|---|
| 0 | 4 | Offset of game record in .sg4 (uint32) |
| 4 | 2 | Game record length, low 16 bits |
| 6 | 1 | bit 7 = length bit 16; bit 6 unused; bits 5–0 custom flags |
| 7 | 2 | Flags: bit 0 custom start (FEN), 1 promotion, 2 underpromotion, 3 delete-mark, 4 white-opening, 5 black-opening, 6 middlegame, 7 endgame, 8 novelty, 9 pawn structure, 10 tactics, 11 K-side, 12 Q-side, 13 brilliancy, 14 blunder, 15 user |
| 9 | 1 | bits 7–4 = White ID bits 19–16; bits 3–0 = Black ID bits 19–16 |
| 10 | 2 | White ID low 16 bits (IDs are 20-bit namebase indices) |
| 12 | 2 | Black ID low 16 bits |
| 14 | 1 | bits 7–5 Event ID bits 18–16; bits 4–2 Site ID bits 18–16; bits 1–0 Round ID bits 17–16 |
| 15 | 2 | Event ID low |
| 17 | 2 | Site ID low |
| 19 | 2 | Round ID low |
| 21 | 2 | bits 15–12 result (0 `*`, 1 `1-0`, 2 `0-1`, 3 `1/2-1/2`); bits 11–8 NAG-count code, 7–4 comment-count code, 3–0 variation-count code (0–10 exact; 11→~15, 12→~20, 13→~30, 14→~40, 15→50+) |
| 23 | 2 | ECO code (see §4) |
| 25 | 4 | Dates (see §4) |
| 29 | 2 | White Elo (12 bits used; high-bit purpose undocumented) |
| 31 | 2 | Black Elo |
| 33 | 1 | Stored-line code (opening lookup table; redundant, readers may skip) |
| 34 | 3 | Final material signature: bits 23–22 WQ, 21–20 WR, 19–18 WB, 17–16 WN, 15–12 WP, 11–10 BQ, 9–8 BR, 7–6 BB, 5–4 BN, 3–0 BP |
| 37 | 1 | Ply count bits 7–0 |
| 38 | 9 | bits 71–70 ply count bits 9–8; bits 69–64 home-pawn nibble count; bits 63–0 up to 16 home-pawn nibbles, MSB first |

## 2. Name file (.sn4)

### Header — 36 bytes

Magic `"Scid.sn\0"`, 4 unused bytes, then eight uint24 fields: name counts
(player, event, site, round) followed by max frequencies (player, event,
site, round). Limits: players 2^20−1, events/sites 2^19−1, rounds 2^18−1.

### Body

Sections in order Player, Event, Site, Round; names alphabetical and
front-coded. Record: ID (2 or 3 bytes — never 1; width from section size),
Frequency (1/2/3 bytes, width from section max-frequency), Length (1),
PrefixLength (1, absent on first record of each section), Suffix bytes
(Length − PrefixLength, no terminator). Text encoding undeclared (Latin-1 or
UTF-8; sniff).

## 3. Game file (.sg4) — summary

No documented file header; index Offset addresses the file directly. Games
never cross 131,072-byte block boundaries (max record 131,072 bytes = 17-bit
length). Record: non-standard tags {len,name,len,value}… 0-terminated; flags
byte; optional NUL-terminated FEN (if custom-start); move list (1 byte/move,
high nibble = piece number 0–15, low nibble = piece-specific move code; queen
diagonal moves take a second byte = dest square + 64); markers 11=NAG (next
byte), 12=comment (text in trailing comment section), 13=start variation,
14=end variation, 15=end of game; comments as NUL-terminated strings in
traversal order. Full per-piece code tables in the cached bkshrader README
(see scratchpad archive) — to be transcribed here when full .sg4 decoding is
implemented (Phase 1).

## 4. Encodings

- **Date** (uint32): bits 19–0 game date `(year<<9)|(month<<5)|day`;
  bits 31–20 event date: bits 31–29 YearMod (0 = unknown, else EventYear =
  GameYear + YearMod − 4), 28–25 month, 24–20 day. Zero components = unknown.
- **ECO** (uint16): 0 = none; else `1 + base*131 + sub` where
  `base = (letter−'A')*100 + number` and `sub = 0` for bare `L##`, else
  `1 + (suffix_letter−'a')*5 + digit` (digit 0 if absent, else 1–4).
  Anchors: 0x0001=A00, 0x0083=A00z4, 0x0084=A01, 0xFFDC=E99z4.
- **Result**: 0 `*`, 1 `1-0`, 2 `0-1`, 3 `1/2-1/2`.

## 5. Known documentation gaps (do NOT resolve by reading SCID source)

1. Auto-load-game semantics (the two specs contradict each other).
2. Elo field top 4 bits (rating type?) undocumented.
3. Count-code table holes (raw counts 11–12, 45–49 unmapped).
4. Pawn piece numbers 8–15 / file assignment implied, not stated; no explicit
   two-square pawn advance code.
5. Null move encoding undocumented.
6. Queen vs rook rank/file convention appears flipped between tables; bishop
   downward-diagonal sign unstated.
7. Marker bytes 11–15 vs king move codes: unambiguous only because king codes
   stop at 10; not spelled out.
8. Possible special encodings of non-standard tags beyond {len,name,len,value}.
9. Text encoding (names, comments, FEN) undeclared in-file.
10. .sg4 padding/block-boundary rules and whether offset 0 is the first game.
11. .sn4 ID width threshold (keyed to count or max frequency?) ambiguous.
12. Pre-v400 46-byte index entries unspecified.

Anything hitting these gaps against real user databases must be reported in
RUN_REPORT.md rather than resolved from SCID source.

## 6. Empirical findings (2026-07-25, run 2)

Gaps resolved by hypothesis-testing against the maintainer's real databases
(9 bases, 7,905 games, validated by full decode: every move legal, mainline
ply count equal to the index's, final material equal to the index
signature). No SCID source consulted.

1. **Non-standard tags** (gap §5.8): a first byte ≥ 0xF1 is a single-byte
   tag *code* for a common tag (0xF3 = Annotator observed), followed by
   {value_len, value}. Bytes 0x01–0xF0 are a literal name length as
   documented. 0x00 terminates the list.
2. **Pawn numbering / double push** (gap §5.4): pawns are numbered 8..=15
   for files a..h at the standard start; pawn move code 15 is the
   two-square advance.
3. **Rook table is transposed in the community doc** (gap §5.6): codes 0–7
   are to-FILE (same rank), 8–15 are to-RANK+8 (same file) — making rook
   and queen encodings mutually consistent. The queen table is correct as
   documented (bit3=1 → to-rank; bit3=0 → to-file; same-file → diagonal
   with destination square = next byte − 64).
4. **Null move** (gap §5.5): byte 0x00 (piece 0, code 0).
5. **Variations**: there is no start-of-game marker; marker 13 opens a
   variation branching from the position *before* the preceding move;
   consecutive variations on the same move each rewind to that point.
6. **Piece numbers are swap-remove list indices**, not fixed identities:
   initial order K, Ra, Nb, Bc, Q, Bf, Ng, Rh, pawns a..h (custom starts:
   FEN scan order with the king swapped to 0); on capture, the last list
   element moves into the freed slot, so numbers are reused mid-game.
7. **Empty games** (0 plies) may carry a stale full-material default
   (0x6A86A8) in the index's final-material field.

Still open: meaning of Elo top nibble (§5.2); count-code table holes
(§5.3); pre-v400 entries (§5.12).
