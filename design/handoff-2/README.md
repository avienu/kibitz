# Handoff: Silman — round 2 (home + remaining screens)

## Overview
Round 1 specified the board, the evidence-overlay language, the tokens and the game view. **Round 2 fills in every other surface** inside that same shell: Home (two directions + recommendation), Database, Opening tree, Position search, Profile, Opponent prep, Tactics, Openings SRS, Endgames, Settings, Help & first-run tour.

Round 1's README (`design_handoff_silman_board_gameview`) remains the source of truth for: design tokens, typefaces and their roles, board rendering, evidence overlays, the nav rail, the status strip, and the legacy-vs-fresh analysis rule. **This document only specifies what is new**, and every screen below is built from round-1 parts.

## About the Design Files
`reference/Silman Screens.dc.html` is a **clickable design reference written in HTML** — not production code. It uses a streaming-preview authoring runtime (`<x-dc>`, `sc-for`, `dc-import`) that does not exist in your app; ignore that machinery. Recreate the screens in the Silman front end (React + TypeScript, Tauri) with its existing patterns, and render every board with the real **chessground** instance. The prototype's own board renderer (`reference/Board.dc.html`) exists only because chessground was unavailable in the prototype.

All data in the file is fixture data written to be plausible (players, ECO codes, counts, prose). Do not ship any of it.

## Fidelity
**High fidelity** for Home, Database, Profile, Opponent prep and Tactics — colors, type, spacing, and interaction states are final. **Solid but simplified** for Opening tree, Position search, Openings SRS, Endgames, Settings and Help: layout, hierarchy and component choices are final; some inner content is representative rather than exhaustive.

---

## The shell (unchanged from round 1)
`rail 204px | main column`, main column = `header (13px 20px, var(--panel), bottom border)` → `scrollable content` → `status strip`. Content region is **1336 × ~838** at the 1540 × 952 reference size, and every screen below lives inside it. The header always carries: title (`600 15px Public Sans`), subtitle (`400 11.5px`, `var(--faint)`), optional segmented control, then right-aligned secondary buttons.

**Rail additions in round 2.** COACH gains a **Home** item at the top of the group (Home is a coach surface, not a separate concept). Everything else matches round 1, including the DATA IN / OUT group that gives the CLI-only features a home.

**Screen-level pattern budget.** Round 2 introduces exactly **five** new reusable components; everything else is round-1 parts (panel card, segmented control, list row, progress cell, prose block, board):
1. **Data table** — header row + rows on a shared `grid-template-columns`, `9px 14px` cell padding, `1px solid var(--line)` row separators, hover `var(--panel2)`. Used by Database, Position search, Opening tree, Prep fingerprint, Master games.
2. **Stat tile** — `padding 12px`, `background var(--panel2)`, `radius 7px`, mono value at `700 20–24px`, mono caption `9.5px/0.14em` in `var(--faint)`. Used by Profile, SRS, Home band.
3. **Baseline bar** — `7px` track (`var(--panel3)`), value fill (`var(--good)`/`var(--bad)` at 0.75 opacity), optional 1px baseline tick at the peer value. Used by Structure report, Prep fingerprint, Endgame tiers, Woodpecker.
4. **Workflow stepper** — numbered chips in the header strip, each showing its chosen value once passed. Used only by Opponent prep.
5. **Evidence pane** — 420px right aside listing the supporting games behind whatever claim is selected. Used by Profile (and reusable by Prep).
No other new patterns. Anything else that looks new is a round-1 component with different content.

---

## Screen: Home — the strategic decision

### Direction A — coach-first (RECOMMENDED)
Content padding `22px 24px 26px`.
1. **Greeting row** — `600 22px Source Serif 4` date, plus a `400 13px` serif clause naming the next real commitment (“Club night Thursday — no prep started for R. Halvorsen yet.”).
2. **Three action cards**, `grid-template-columns: repeat(3,1fr)`, gap `14px`, each `padding 16px`, `background var(--panel)`, `1px solid var(--line)`, `radius 8px`:
   - **Continue** (carries `box-shadow: inset 2px 0 0 var(--accent)` — the single accented card on the screen): last game, where you stopped, what is unreviewed, primary button “Resume review”.
   - **Due today**: two mono numerals at `700 28px` with `Public Sans` unit labels, one serif line naming the specific lapse, two secondary buttons.
   - **Prep an opponent**: search field + Go, with a recents line.
3. **Findings panel** (`1.55fr`) beside a **right column** (`1fr`) holding “New since Friday” and “Running”.
   - Findings panel opens with a serif paragraph naming the two dominant weaknesses in plain language, then four rows: role dot (`var(--bad)`/`var(--good)`), label, mono value, and a `var(--faint)` evidence count. Clicking a row opens Profile **with that claim already selected**.
   - “New since Friday”: source tag (mono `600 9.5px`, uppercase, coloured per source) + title + result.
   - “Running”: the round-1 progress cell, plus one serif line that states the engine is cold.
4. **Recommendation callout** (design-review artifact, not a shipping component): `inset 2px 0 0 var(--violet)`, serif prose.

### Direction B — library-first
Same shell; content is a four-tile summary band (`repeat(4,1fr)`, mono value + label, each tile navigates) above the filter bar and the full game table. It is literally the Database screen with a band on top — which is the argument against it.

### Recommendation: ship A
The user’s week is three recurring jobs — prep before club night, review freshly synced games, clear the daily queues. A puts all three one click from launch; B makes the two time-boxed jobs (training, prep) require navigation every day, while the database is the one surface always reached deliberately anyway.

**What B does better:** it makes 121k games feel present, is instantly legible to anyone coming from ChessBase or SCID, and needs no “today” logic to be correct — A is only as good as its due-counts and findings. **Mitigations if you ship A:** keep Database as the first rail item, keep the Continue card first in reading order, and make A degrade to a short honest list when nothing is due (never pad it with invented widgets).

---

## Screen: Database
Filter bar (chips: Player · Event · Date · ECO · Result · Source, right-aligned mono range readout) → **inline job row** → table → pagination row.

- **Inline job row** — same visual grammar as the status-strip progress cell, promoted into the screen that owns the job: `inset 2px 0 0 var(--info)`, mono label `ANNOTATING DATABASE`, 5px track, mono detail (`41% · 49,780 / 121,438 · ~2 h 10 m left`), Pause button. Use this exact row for db-wide annotate, fresh-analysis passes, imports and syncs.
- **Table columns**: `26px | 1.6fr white | 1.6fr black | 58px result | 1.2fr event | 92px date | 64px ECO | 96px source | 84px analysis`.
  - Duplicate flag `⑂` in column 1 (`var(--faint)`), explained in the footer line — duplicates are linked to their higher-priority copy, never deleted.
  - Source tag colours: personal `var(--accent)`, TWIC `var(--info)`, Lichess `var(--violet)`, chess.com `var(--good)` — mono `600 9.5px`, uppercase, `0.08em`.
  - Analysis column follows the round-1 legacy rule: fresh = `500 10.5px` mono `var(--info)` (“fresh d24”); legacy = italic, `opacity 0.75`, `var(--faint)` (“legacy 2011”); none = `—`.
- Row click opens the game view.

## Screen: Opening tree
Left (flex) = move table `74px move | 84px games | 1fr W/D/L | 78px avg Elo | 74px perf`; the W/D/L cell is a 9px stacked bar (good / faint@0.5 / bad). Perf is signed, coloured. Right aside `520px` = board at `size 472` + “Games reaching this position” list. One serif paragraph explains transposition-awareness. Follows the displayed position — the same board component, no new pattern.

## Screen: Position search
Left `560px` on `var(--desk)`: board at `size 472` (drag pieces to set the position), FEN field + “Paste FEN”, one hint line. Right (flex): results header with a mono pill (`1,204 GAMES · 31 ms` — show the real timing, it is a product claim), filter chips, then the Database table minus the analysis column.

## Screen: Profile
Segmented control in the header switches subject (`You` / opponent name) — one screen, two subjects.

Layout: content (flex) + **Evidence aside 420px**.
- Serif lede, `400 15px/1.6`, max-width 840px, stating the two dominant findings in prose.
- **Motif matrix** (`1.25fr`): columns `1fr motif | 66px missed | 66px allowed | 74px vs peers`, mono right-aligned numerals, over-baseline values in `var(--bad)`, under-baseline in `var(--good)`.
- **Structure report** (`1fr`): baseline bars, one per pawn-structure family, with the peer baseline tick at 50%.
- **Phase accuracy**: three stat tiles (ACPL + error breakdown).
- **Conversion & defence**: two stat tiles with signed peer deltas.
- **Evidence aside**: mono count pill, serif paragraph explaining what the list is, then supporting games (title, red mono ply, faint date). Footer: “Every number on this screen opens its supporting games here; opening one jumps straight to the ply that produced the claim.” Bottom bar: primary “Train this weakness” (seeds the tactics queue) + “Open game”.
- **Every number is a control.** Clicking any motif row, structure bar, phase tile or rate tile re-targets the aside. This is the profile’s whole trust argument — do not ship a number that cannot be drilled.

## Screen: Opponent prep (workflow)
Header strip below the app header holds the **stepper**: `① Opponent → ② Fingerprint → ③ Weak lines → ④ Master games`, each passed step showing its chosen value (`R. Halvorsen`, `as Black`, `Alapin`, `5 games`). Active step: `background var(--panel3)`, accent numeral badge. Steps are re-clickable.

Right aside `520px` persists across all four steps: board at `size 472` showing the position under discussion, plus a **profile finding about this opponent** (violet mono label, serif prose) and two buttons — “Open his profile”, “Study in game view”. This is where profile findings surface inline.

- **Step 1** — name field + `Search local` / `Fetch from Lichess` / `Fetch from chess.com`; results list (name, Elo, games+span, source tag); first row accented as the selection. Serif footnote states local-first and that fetching analyses nothing.
- **Step 2** — colour segmented control + fingerprint table: `64px ECO | 1.4fr opening | 78px share | 1fr score (bar + mono) | 84px avg Elo | 1fr book exit`. Weak entries use `var(--bad)` bar and score.
- **Step 3** — ranked line cards: mono rank in accent, name, mono move sequence, red mono score, then a serif paragraph of *why* it is weak (specific, cites counts). Top card accented.
- **Step 4** — master games table (`white | black | result | event | year | plies`), with a serif line explaining the ranking rule. Row click opens the game view.

## Screen: Tactics
`230px mode column | board column (var(--desk)) | 400px reasoning aside`.
- **Mode column**: five modes as selectable blocks (name + mono badge + serif one-liner) — Weakness-targeted (default), Rated drill, Motif filter, Heisman speed drill, Woodpecker cycles. Below: a Woodpecker cycle panel of three baseline bars (cycle time, accuracy).
- **Board column**: a `640px`-wide meta row above the board — side to move, streak, rating with signed delta, and a `700 15px` mono clock in `var(--accent)` (the clock is the only oversized numeral on the screen; it is only present in timed modes). Board at `size 560`, flipped to the solver’s side, **no evidence overlays** — never pre-highlight the solution. Below: Hint `H`, Skip `S`, Give up `G`, plus the keyboard hint line.
- **Reasoning aside**: `WHY THIS PUZZLE` + coach/neutral segmented control (same voice system as Explain), serif headline + body, then a mono facts block (`MOTIF` / `SOURCE` / `RATING` at a 78px label column). Footer: queue progress cell (`7 / 12`).

## Screen: Openings SRS
`292px repertoire column | board column | 340px session aside`.
- Repertoire column: colour segmented control, then lines with due counts (`var(--accent)` when due) and total positions; “Import PGN or Lichess study” button; serif footnote on FSRS and the new-card cap.
- Board column: meta row (line name + your colour, `LAPSE ×2` pill in `var(--bad)`, due/done counts), board at `size 560` with only the last-move highlight, then the answer field (“type your move…” / or play it on the board) and the **grade row**: Again `1` / Hard `2` / Good `3` / Easy `4`, each button showing its next interval in small mono (`<1 m`, `2 d`, `9 d`, `21 d`) and coloured bad / dim / good / info. Keyboard `1–4` grades; `⏎` submits.
- Session aside: four stat tiles + a serif paragraph naming the specific branch being lapsed and what Silman will do about it.

## Screen: Endgames
`300px curriculum | board column | 380px feedback aside`.
- Curriculum: tier blocks with `n / m` counts and a completion baseline bar; complete tiers use `var(--good)`.
- Board column: mono header (`LUCENA · BUILD THE BRIDGE`, `TABLEBASE TRUTH · 5 PIECES` in `var(--good)`), board at `size 560` with the plan’s key square marked in violet (round-1 key-square wedge), objective line, Restart / Show the idea.
- Feedback aside: one row per move — `no | SAN | verdict | note`. Verdicts are mono `700 9.5px/0.1em`: `WINNING` (`var(--good)`), `SLOWER` (`var(--accent)`, with the DTZ cost stated), `THROWS` (`var(--bad)`), `ENGINE` (`var(--faint)`, the defender’s reply). Grading is against the tablebase, never an engine score — say so in the closing serif paragraph.

## Screen: Settings
Single column, `max-width 1080px`. Grouped cards; group header is a mono uppercase strip on `var(--panel2)`. Each row: `230px label+help | 1fr value | 200px action`, `12px 16px` padding, `1px solid var(--line)` separators. Label `500 12.5px`, help `400 11.5px Source Serif 4` `var(--faint)`, value in a read-only field (`var(--panel2)`, `1px solid var(--line)`, radius 6, mono for paths/keys/numbers), action as a bordered ghost button.
Groups and rows: **Engine & analysis** (engine path, node budget, spawn policy — the engine-off default is stated here in words), **Coach** (default voice, LLM verbaliser key with the grounding/fallback note), **Data** (database path, account syncs, TWIC schedule, tablebase path), **Appearance** (theme, board treatment, piece set).

## Screen: Help & first-run tour
`250px TOC | reader | 340px tour rail`.
- Reader column: `max-width 660px`, `h2` at `600 24px Source Serif 4`, a mono sub-line naming the surface, shortcut and CLI equivalent, body at `400 15px/1.68 Source Serif 4`. CLI blocks use the info-accented card (`inset 2px 0 0 var(--info)`) with mono `12.5px/1.7`.
- Tour rail (shown on first run, replayable from Help): a card on `var(--desk)` with `1px solid var(--accent)`, mono `FIRST-RUN TOUR` + `2 / 6`, serif body, Next / Skip tour. **One card per rail group**, anchored beside it, never covering the thing it points at.

---

## Interactions & Behavior (new in round 2)
- **Rail navigation** is the whole app’s router; the active item carries `inset 2px 0 0 var(--accent)` and `var(--panel3)`.
- **Claim → evidence** (Profile, Home findings): selecting a number sets the aside subject; Home findings navigate to Profile with that claim pre-selected; an evidence row opens the game view at the ply that produced the claim.
- **Prep stepper**: forward on selection, free backward navigation, selections persist and are shown in the step chips.
- **Engine-off stays visible everywhere**: the status strip reads `ENGINE IDLE` unless something is actually running; prep explicitly says fetching analyses nothing; Settings names the spawn policy.
- **Keyboard**: Tactics `⏎` submit, `H` hint, `S` skip, `G` give up. SRS `1–4` grade, `⏎` submit. Game view per round 1. The status strip’s right cell shows the active screen’s key hints.
- **Batch progress**: status-strip cell always; the owning screen additionally shows the inline job row.

## State Management
```ts
type AppState = {
  screen: 'home'|'database'|'tree'|'search'|'profile'|'prep'|'tactics'|'srs'|'endgame'|'settings'|'help'|'game';
  theme: 'dark'|'light';
  home: 'A'|'B';            // review-only switch; ship A
  profileSubject: 'self'|{ playerId: string };
  claim: ClaimId;           // drives the Profile evidence aside
  prepStep: 1|2|3|4;
  prepOpponent?: PlayerId; prepColor: 'white'|'black'; prepLine?: LineId;
  tacticMode: 'weak'|'rated'|'motif'|'speed'|'wood';
  voice: 'coach'|'neutral'; // shared with Explain
};
```
`voice` and `theme` are app-level settings, not per-screen. Job/engine state is a global subscription feeding both the status strip and any owning screen’s inline row.

## Design Tokens
Unchanged from round 1 — both theme sets, the three typefaces and their roles, radii, spacing scale and evidence hues. Round 2 adds **no new tokens**. Source-tag colours reuse existing roles (`accent`/`info`/`violet`/`good`); verdict colours reuse `good`/`accent`/`bad`/`faint`.

## Assets
`reference/pieces/*.svg` — cburnett, from `lichess-org/lila@master:public/piece/cburnett/`; use the app’s chessground copy. Fonts as round 1 (bundle locally, the app is offline-capable). No other imagery.

## Screenshots
`screenshots/01-home-a.png` — Home, Direction A (recommended).
`screenshots/02-home-b.png` — Home, Direction B.
`screenshots/03-database.png` · `04-profile.png` · `05-prep.png` · `06-tactics.png` · `07-srs.png` · `08-endgames.png` · `09-tree.png` · `10-position-search.png` · `11-settings.png` · `12-help-tour.png`.
Screenshots are scaled captures of the reference HTML — the written values here are authoritative.

## Files
- `reference/Silman Screens.dc.html` — all round-2 screens, clickable via the rail.
- `reference/Board.dc.html` — round-1 board renderer, included so the reference runs; replace with chessground.
- `reference/pieces/` — piece SVGs, reference only.

## Still not designed
Nothing from the inventory is now without a home. Deferred by the product, not by design: Maia predictable-mistake profiling, cloud sync, mobile, collaborative studies, the video/lesson catalogue, and the ChessBase-format converter.
