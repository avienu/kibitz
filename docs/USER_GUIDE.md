# Silman User Guide

This guide covers everything you can click in the app and every feature that
is currently CLI-only. It is also available inside the app: press
**Help & tour** at the bottom of the navigation rail.

---

## The window at a glance

- **Navigation rail** (left edge) — the SILMAN wordmark, a line showing the
  open database ("scid.sqlite · 121,438 games"), four capability groups
  (**STUDY**, **COACH**, **TRAIN**, **DATA IN / OUT**), and a footer with
  **Settings** and **Help & tour**. Rail items carry live badges (game
  count, cards due, job counts…) — a badge is real data or absent, never a
  fake number. Below 1280 px window width the rail collapses to icons
  (hover an icon for its label and badge). Minimum window size is
  1180×760.
- **Main column** — a header bar, the active view, and the **status strip**
  along the bottom.
- The **Game view** is the centrepiece: eval bar + board + move controls on
  the left, the Explain panel above the Moves panel on the right.

On first launch a one-time overlay points out the rail groups; dismiss it
with **Got it**. At startup the app automatically re-opens the last
database you used (the path is remembered; the default is
`testdata/corpus/scid.sqlite`, resolved from the repository root).

### The rail, item by item

- **STUDY** — **Database** (badge: game count), **Game**, **Opening tree**,
  **Position search**.
- **COACH** — **Explain** (a toggle, not a page — switches the Explain
  panel on/off; badge shows "on"/"off"), **Profile** (badge: "N findings"
  once a profile is built), **Opponent prep**.
- **TRAIN** — **Openings SRS** (badge: "N due"), **Tactics** (badge:
  attempts, or puzzle count before any attempt), **Endgames**.
- **DATA IN / OUT** — **Import PGN / SCID**, **TWIC ingest**,
  **Account syncs**, **Jobs** (badge: "N running" / "N pending" / done
  count).
- Footer — **Settings**, **Help & tour** (this guide).

---

## The Game view

### Header bar

- **Title block** — "White — Black" with the result, and a meta line: site,
  year, ECO, ply count, and the game's identity (`database #N` or
  `pasted PGN`).
- **walnut | instrument** — switches the board treatment (same control as
  in Settings).
- **Annotate** — the *static* Silman annotation pass over the loaded
  database game: imbalance comments plus tactical alerts from the WSUI
  screen. **No engine runs**; each fired alert enqueues a bounded engine
  confirmation job for the Jobs queue.
- **Re-analyze** — enqueues one bounded engine evaluation per mainline
  position. Nothing runs yet: "N eval jobs enqueued — run them from Jobs".
- **Export PGN** — renders the game (with annotations) as PGN in a modal
  with **Copy** / **Close**.

These three buttons are enabled only for games loaded from the database.

### Board column

- **Eval bar** (left of the board) — per-ply evaluation from the game's
  *stored* analyses; fresh silman engine rows are preferred over legacy
  SCID-imported ones. The fill is White's share, anchored at the bottom.
  With no stored analysis for the ply it shows an empty track and a muted
  "—" (never a fake 0.0). On a forced mate the bar pins to the winning
  side and the readout shows **#N** in that side's colour (the mate
  distance comes from the position's explanation; a bare "#" appears when
  only the stored mate sentinel is known). Hover the bar for the eval
  source — engine name, depth/nodes, fresh vs legacy import.
- **The board** — resizes with the window (the grid snaps to multiples of
  8, never below 496 px). Yellow wash marks the last move. Clicking a
  square selects it (see the Explain panel below); clicking it again
  clears the selection.
- **Move controls** — |◀ ◀ Prev · Next ▶ ▶| buttons, a "ply N / M" pill,
  and **Flip**.

### Keyboard map (Game view)

- **← / →** — one ply back / forward
- **↑ / ↓** — jump five plies back / forward
- **Home / End** — start / end of the game
- **f** — flip the board
- **e** — explain the current position

Global to the Game view, but ignored while you are typing in a text field,
while a modifier key is held, and while a dialog (help, tour, promotion
picker) is open.

### Explain panel

The **Explain** rail item (COACH) toggles the panel on or off. While it is
on, the free static screen runs automatically at **every ply** — no
keypress, and still no engine: the tactical screen is static analysis.

- Positions where the screen fires (**TACTICAL SCREEN FIRED**, or
  **FORCED MATE**) narrate immediately.
- **QUIET POSITION** plies keep the empty state — "No screen has fired on
  this position…" — until you explicitly ask with **E** or the
  **Explain position** button. The empty state also lists which plies
  already have cached explanations.

Panel anatomy:

- **Verdict pill** — the tag plus the eval readout (e.g.
  "TACTICAL SCREEN FIRED  +2.6", "FORCED MATE  #5").
- **Coach | Neutral** — narration voice. Switching is instant: both voices
  arrive pre-rendered with every explanation. (The choice also lives in
  Settings and is stored in the open database.)
- **Sentences** — each has a role dot and a kind label (TACTICAL ALERT /
  IMBALANCE / PLAN). **Hovering a sentence isolates its board evidence**:
  only that sentence's marks show, at full intensity.
- **Square filtering** — click a board square to filter the prose to
  sentences that reference that square (the rest fade); the footer shows
  "filtered to d7". Click the same square again to clear. Stepping to
  another ply clears both hover and selection.
- **Footer** — "Static screen · no engine spawned", the active voice, and
  the selection state.

### Evidence overlay language

One shared vocabulary, identical in both themes and both board treatments:

- **Red ring** — alert target (the piece/square in tactical danger).
- **Amber corner wedge + arrow** — attacker (arrows always point
  attacker → target).
- **Blue corner wedge** — defender.
- **Green square wash** — imbalance evidence.
- **Violet corner wedge** — key square / plan target.
- **Yellow square wash** — the last move played.
- **Neutral ring** — your selected square.

Evidence renders at 44 % intensity by default and at 100 % for the hovered
sentence.

### Moves panel

A move-pair grid: number column, White's and Black's moves per row, NAG
glyphs on the moves, and the per-move stored evals (muted = legacy import;
hover any eval for its engine). Comments render as serif rows under their
move; variations render as tagged rows — **FRESH** (named after one of the
game's fresh engine identities), **LEGACY** (imported engine lines /
pre-2020 years), or plain human lines.

- **full | hover | hidden** — how comments and variations display (hover
  dims them until pointed at; hidden removes them). Also in Settings.
- Click a move to jump the board there.

**Editing** (database games only):

- **Click the current move again — or right-click any move —** to open the
  annotation popover: NAG choices **! !! ? ?? !? ?!**, **clear**, and
  **comment** (starts a new comment under that move).
- **Click a comment** to edit it in place: **Enter** commits,
  **Esc** cancels, an **empty text deletes** the comment.
- **× on a variation row** deletes that variation.
- **Enter a variation from the board:** play a legal move that differs
  from the mainline move — an "Add … as a variation of …?" offer appears
  with **Add as variation** / **Dismiss**. (Playing the mainline move just
  advances.)
- **Save / Revert** — edits are local until saved; the Save button
  highlights while there are unsaved changes.

Below the Moves panel, the repertoire footer — "Mainline → repertoire" (or
"Line (first N plies) → repertoire" after stepping forward) with
**as White** / **as Black** — sends the current line to the Openings SRS
trainer. Re-adding a line never duplicates cards.

### Promotion picker

Wherever a board accepts moves — Game-view variations, Openings SRS,
Tactics, Endgames — dragging a pawn to the last rank opens a picker:
choose **Q / R / B / N** by click or keys **1–4**; **Esc** cancels.
Underpromotions are fully supported, including tactics puzzles whose
solution underpromotes.

### When does the engine run?

Stockfish is **off by default** and never runs behind your back:

1. **Annotate** and the Explain panel are static analysis — no engine.
2. **Re-analyze** and Annotate's confirmation checks only *enqueue* jobs.
3. The engine actually runs when you press **Run pending jobs** in the
   Jobs view (or the CLI `run-jobs`). The status strip's engine dot and
   batch progress show it happening.

---

## STUDY views

### Database

- **Path field + Open** — open a silman SQLite database (created and
  filled from the command line — see "CLI-only features"). The last path
  is remembered and auto-opened at launch; the window title shows the
  database filename. A summary line reports games / players / positions /
  sources.
- **Opening tree** and **Position search** sections (as below), plus:
- **Games list** — filter by player-name substring, 50 games per page with
  a **◀ Prev / Next ▶** pager; columns White, Elo, Black, Elo, Result,
  Date, ECO, Event. Click a row to load the game into the Game view (with
  its stored annotations and evals).

### Opening tree

Every move played from the position **currently on the board**, across the
whole database: move, game count, a W/D/L results bar (hover for exact
counts), average Elo, and performance rating. Clicking a row **advances
the loaded game one ply** if the game continues with that move; otherwise
a hint explains. The tree follows the board, so stepping through a game
walks you down the tree. (Also shown inside the Database view; the rail
item focuses it.)

### Position search

Games reaching the position currently on the board — the total, and up to
ten rows with **load** buttons that open the game at that position. For an
arbitrary *typed* FEN, use the CLI `find-fen`.

---

## COACH views

### Explain

A toggle for the Game view's Explain panel (documented above), not a page.
The badge shows whether it is on.

### Profile

A corpus-wide strengths/weaknesses report for one player (needs an open
database).

- **Player name field** — exact name, suggestions after two characters.
- **Build profile** — renders: the summary (games, score %, engine eval
  coverage); **Accuracy by phase (ACPL)** with blunders / mistakes /
  inaccuracies for opening / middlegame / endgame; the **Motif matrix**
  (opportunities, taken, missed, allowed — with clickable example game ids
  that open the game); **Pawn structures & piece placement**;
  **Openings (ECO)**; and **Conversion & defense** (winning positions
  ≥ +2.00 converted, worse positions ≤ −1.00 held).

ACPL and conversion need stored engine evaluations — run **Re-analyze**
plus the Jobs worker on the player's games first. The built profile feeds
the rail badge ("N findings"), the Opponent-prep weaknesses strip, and the
Tactics weakness-weighted drill.

### Opponent prep

Rank an opponent's weakest opening spots (needs an open database):

- Opponent name (suggestions as you type), **as White / as Black** (which
  of *their* repertoires to attack), **Build prep**. A spot needs 3+ of
  their games and an under-50 % score.
- Each card: rank, weakness score (higher = better target), games / their
  score % / ply reached, a **leaves book** badge on their book-exit
  points, the moves they actually play there, and clickable **master
  games** that reached the exact position — loading one opens it *at the
  prep position*.
- With a profile built for the same player, a **Profile weaknesses** strip
  appears above the cards.

---

## TRAIN views

### Openings SRS (Repertoire Trainer)

Spaced-repetition review of your opening repertoires, scheduled with
FSRS-4.5. Each card is a position where it is your color's turn plus the
repertoire move you must know there.

Building a repertoire:

- **From a loaded game:** the "→ repertoire: as White / as Black" footer
  under the Moves panel (see the Game view). Lines land in the default
  repertoire for that color ("main (white)" / "main (black)").
- **From a PGN study (CLI-only):**

```
cargo run --release -p silman-db --bin silman-cli -- --db mygames.sqlite \
  import-repertoire study.pgn white --name "main"
```

  `--name` defaults to `main`; re-importing is idempotent (positions that
  already have a card are left untouched — first move in wins).

Reviewing:

- **White / Black** buttons switch repertoires, each showing its due
  count; the rail badge and the status strip's "Openings SRS · N due
  today" nudge show the combined total.
- **Start review** starts a session over the due queue (up to 100 cards).
  A board appears beside the panel, oriented to your color; the prompt
  gives the moves so far ("Start position" for a first move) and asks for
  **your move** — play it on that board.
  - **Correct:** the move plays and you grade yourself **Good** or
    **Easy** (Easy pushes the next review further out).
  - **Wrong:** the card lapses immediately (graded Again), a green arrow
    shows the expected move, and the message gives the answer and when the
    card returns ("again in <1d"). Press **Continue**.
- **End session** aborts at any point; after the last card a summary shows
  "N reviewed — X correct, Y to relearn".
- The queue table lists the next due cards (up to 20): line prefix, due
  date (or "new"), and repetition count with lapses.

Intervals display as "13d" / "3mo" / "1.5y".

### Tactics

Puzzle drills over the Lichess puzzle database, imported into the open
database. The puzzle board lives in this view, and **no engine is
involved** — solving is checked against the stored solution line. The
summary line shows your tactics rating, rated attempt count, and imported
puzzle count.

Importing puzzles — in the **Import puzzles** section: CSV path (default
`testdata/corpus/lichess_db_puzzle.csv`), optional **min popularity**
cutoff (Lichess popularity is −100..100; the field defaults to 50), then
**Import CSV**. Download `lichess_db_puzzle.csv` from
database.lichess.org (CC0). The same import exists on the command line
with an extra row cap:

```
cargo run --release -p silman-db --bin silman-cli -- --db mygames.sqlite \
  import-puzzles lichess_db_puzzle.csv --min-popularity 50 --max-rows 100000
```

Solving flow — press **Next puzzle**: the opponent's setup move plays
after a beat, the clock starts, and you play every solution move on the
puzzle board (opponent replies play automatically).

- A **wrong move fails the puzzle** — the answer and full solution are
  shown; no retries on a rated attempt.
- An **alternate checkmate is accepted**: if your differing move mates,
  the puzzle counts as solved.
- **Underpromotions work**: pawn-to-last-rank moves open the promotion
  picker (Q/R/B/N, keys 1–4), so solutions requiring an underpromotion
  can be entered normally.
- **Give up** reveals the solution and records a failed attempt.
- After finishing, the puzzle's themes are revealed and **◀ / ▶** replay
  the solution; the outcome line shows your time and any rating change.

The five modes:

- **Rated (±100 of your rating)** — an unsolved puzzle near your rating;
  the band widens (up to ±1000) if nothing is left.
- **Motif filter** — pick a **theme** from the dropdown (with puzzle
  counts) and drill only that motif.
- **Weakness-weighted (from your profile)** — needs your profile (COACH →
  Profile). Puzzles target the motifs your games suffer from most, and
  every serve shows **"Why this puzzle"** — e.g. "picked because your
  games allow many exposed-king tactics (4 allowed, 2 missed in your
  profile) — this puzzle's themes […] train that motif".
- **Woodpecker cycle** — solve the *same* fixed set repeatedly. Create a
  named set of N puzzles, **Start cycle**, and watch per-cycle **Stats**
  (attempts, solved, accuracy, total and average time) improve.
- **Speed (easy, against the clock)** — deliberately easy puzzles (rated
  300–900 points below you) for fast pattern recognition; already-solved
  puzzles stay in the pool. The session line tracks solved/attempts and
  average time.

The tactics rating is Elo-style against fixed puzzle ratings: K = 40 for
your first 30 rated attempts, then K = 20. Only rated, motif and weakness
attempts move it — Woodpecker and speed record history only.

### Endgames

A tiered curriculum of classic theoretical endgame positions (27 drills),
played out against an automatic opponent. **No engine is involved.**

Tiers — the table shows each tier's rating band and your mastered count:

- **Essentials** (up to ~1000) — the two basic mates, the square of the
  pawn, king-and-pawn fundamentals.
- **Building technique** (~1000–1500) — opposition, key squares, spare
  tempi, queen vs pawn.
- **Rook endings and tempo play** (~1500–1900) — Lucena, Philidor, and
  the other rook endings that decide practical games.

**Open** a tier to list its drills (title, material, goal, attempts,
mastery progress); **Start** launches one. You play the side to move —
"Win with White" / "Hold the draw with Black" — with an instruction
explaining the idea. The opponent replies automatically:

- **"Opponent: tablebase (optimal replies)"** — where Syzygy tables cover
  the piece count, replies are provably result-optimal and every one of
  your moves is checked: a move that forfeits the theoretical result fails
  the drill immediately.
- **"Opponent: heuristic sparring partner"** — without tables, a
  deterministic shallow-search partner defends sensibly but is not an
  oracle; only actual game endings (checkmate / stalemate / draw) are
  detected.

**Give up** ends an attempt; then **Retry** or **Back to drills**.
Promotion moves open the standard picker. A drill is **mastered** after 2
clean completions ("1/2 clean" → "mastered ✓").

Tablebases: the Syzygy directory resolves from the `SILMAN_SYZYGY`
environment variable, else by walking up to `testdata/syzygy`. The repo
script

```
scripts/fetch_syzygy_test_files.sh
```

downloads the complete 3-man set (under 100 KB) — enough for the test
suite, but most drills have more pieces and fall back to the heuristic;
point `SILMAN_SYZYGY` at a 3-4-5-man set for tablebase-verified play
across the whole curriculum. The note at the top of the view always states
which opponent you are getting.

---

## DATA IN / OUT views

### Import PGN / SCID

- **PGN** — paste into the text area and **Load**, **Open file…** for a
  `.pgn`/`.txt` file, or **Sample game** (Anderssen–Kieseritzky, London
  1851). Loading switches to the Game view.
- **SCID (.si4)** — imports through the command line for now:
  `silman-cli import-si4 <base>` converts a `.si4`/`.sg4`/`.sn4` base into
  the SQLite database the app opens. Legacy engine analysis is preserved
  and tagged — never deleted, only superseded by fresh analysis.

Note: pasting a PGN loads it for *viewing* — storing games in the
database is a CLI import.

### TWIC ingest / Account syncs

Placeholder screens: both capabilities live in the data layer and the CLI
(`twic-sync`; `lichess-sync` / `chesscom-sync` / `fics-sync` — see
"CLI-only features"). The screens reserve the rail entry and will grow
status displays when the desktop surface lands.

### Jobs

The engine job queue — the **only** place the engine actually runs from
the app:

- Counts table: pending / running / done / failed.
- **Run pending jobs** — starts the worker (disabled while running or when
  nothing is pending). When the batch finishes, confirm-verdicts fold back
  into stored annotations (each tactical alert becomes confirmed, refuted,
  or unclear).
- "last engine:" shows the most recent engine identity.

Everything the engine does goes through this queue — annotate
confirmations, re-analyze passes, batch evals. Nothing runs until you
start the worker.

---

## Status strip

Always visible along the bottom:

- **Engine dot** — "ENGINE RUNNING" / "ENGINE IDLE", with the engine
  identity and node budget.
- **JOBS** — pending / running / done / failed counts (polled every few
  seconds).
- **ENGINE JOBS progress bar** — appears while a batch runs, with percent
  complete.
- **Message cell** — transient app status (load results, save
  confirmations…).
- **"Openings SRS · N due today"** (right edge) — appears when repertoire
  cards are due; clicking it jumps to the Openings SRS view.

---

## Settings

- **Theme** — Dark (default) / Light. Persists across sessions.
- **Board treatment** — Studio Walnut (default) / Instrument.
- **Narration voice** — Coach / Neutral. Also stored in the open
  database; annotations regenerate in the new voice on the next annotate
  pass.
- **Annotation display** — full / hover / hidden (same control as the
  Moves panel).
- **Engine binary (optional override)** — leave empty to auto-resolve; the
  "using:" line shows which binary would run. Resolution order: this
  override, the `SILMAN_STOCKFISH` environment variable, a repo-local
  `tools/` binary, then `stockfish` on PATH.
- **Search nodes per analysis** — the node budget for engine jobs
  (default 2,000,000).

---

## Deep links

A URL hash of the form

```
#game=123&ply=24&theme=light&treatment=instrument&voice=neutral
```

applies once at startup, after the database opens: theme, board treatment,
and voice switch, and game #123 loads at ply 24. Handy for demos and
docs; any subset of the parameters works.

---

## End-to-end workflows

### Import games and browse them

1. CLI: `import-pgn` / `import-si4` (or a sync command) into a `.sqlite`
   file — see below.
2. STUDY → Database → enter the path → **Open** (subsequent launches
   auto-open it).
3. Filter, click a game, step through it in the Game view.

### Annotate a game and confirm tactics with the engine

1. Load a game from the Database view.
2. Header bar → **Annotate** — instant static comments; engine checks are
   queued.
3. DATA IN / OUT → Jobs → **Run pending jobs** — verdicts fold back
   automatically when the batch finishes.
4. Edit by hand in the Moves panel (comments, NAGs, variations) and
   **Save**.
5. **Export PGN** to take the annotated game elsewhere.

### Build evals, then profile a player

1. For each game of interest: **Re-analyze** (header bar), then run the
   queue from Jobs (or CLI `reanalyze-game` + `run-jobs` for batches).
2. COACH → Profile → name → **Build profile** — ACPL and conversion now
   have data.

### Prep for an opponent

1. Open the database containing their games.
2. (Optional) Build their profile first for the weaknesses strip.
3. COACH → Opponent prep → name → **as White** / **as Black** →
   **Build prep**.
4. Click a master game on a card to study the critical position.

### Build and train an opening repertoire

1. Load a model game, step to where your line ends, and use
   "→ repertoire: **as White** / **as Black**" under the Moves panel — or
   bulk-import a PGN study with the CLI `import-repertoire`.
2. TRAIN → Openings SRS → pick the color → **Start review**.
3. Play each prompted move; grade **Good** / **Easy** when right, read the
   arrow and **Continue** when wrong.
4. Come back when the rail badge (or the status-strip nudge) shows cards
   due — FSRS schedules the rest.

### Train tactics against your own weaknesses

1. TRAIN → Tactics → **Import CSV** (once) with the Lichess puzzle dump.
2. COACH → Profile → build your own profile.
3. Tactics → mode **Weakness-weighted (from your profile)** →
   **Next puzzle** — each serve explains why it was picked.

---

## CLI-only features

Everything below has **no UI entry point** (exceptions are noted inline) —
it exists only in the developer CLI (`silman-cli`) or the validation harness
(`wsui-validate`). Run them from the repository root. The general form is:

```
cargo run --release -p silman-db --bin silman-cli -- --db <path.sqlite> <subcommand> [args]
```

`--db` defaults to `silman.sqlite` in the current directory; the database is
created and migrated automatically. (A built binary works the same:
`silman-cli --db <path.sqlite> <subcommand> [args]`.)

### Database creation & import (CLI-only)

- **Create / migrate a database**

```
cargo run --release -p silman-db --bin silman-cli -- --db mygames.sqlite init
```

- **Import a PGN file** (streaming; malformed games are skipped; provenance
  is recorded with every source):

```
cargo run --release -p silman-db --bin silman-cli -- --db mygames.sqlite \
  import-pgn games.pgn --source-name "My games" --origin "local file" \
  --license "personal data" --kind personal
```

  `--kind` is one of `personal|twic|online|other` and controls duplicate
  priority.

- **Import a SCID database** (pass the base path of the `.si4`/`.sg4`/`.sn4`
  triple; inline comments, NAGs, and variations are preserved):

```
cargo run --release -p silman-db --bin silman-cli -- --db mygames.sqlite \
  import-si4 /path/to/scidbase --source-name "SCID import" --kind personal
```

- **Import a repertoire PGN** for the Openings SRS trainer — every mainline
  move of the chosen color becomes a training card (the UI can only add
  lines from a loaded game, one at a time). `--name` defaults to `main`;
  re-import is idempotent:

```
cargo run --release -p silman-db --bin silman-cli -- --db mygames.sqlite \
  import-repertoire study.pgn white --name "main"
```

- **Import Lichess puzzles** for the Tactics trainer. The Tactics view's
  "Import CSV" button does the same import; only `--max-rows` (stop after
  importing that many puzzles) is CLI-exclusive. `--min-popularity` skips
  puzzles below that Lichess popularity (−100..100):

```
cargo run --release -p silman-db --bin silman-cli -- --db mygames.sqlite \
  import-puzzles lichess_db_puzzle.csv --min-popularity 50 --max-rows 100000
```

### Downloading games (CLI-only)

- **TWIC incremental sync** — downloads The Week in Chess issues to *your*
  machine for personal use (TWIC data must never be redistributed; a notice
  prints on first run). `--from` is required the first time:

```
cargo run --release -p silman-db --bin silman-cli -- --db mygames.sqlite \
  twic-sync --from 1580 --max-issues 5
```

  Later runs resume where the last one stopped:

```
cargo run --release -p silman-db --bin silman-cli -- --db mygames.sqlite twic-sync
```

- **Lichess user games** (resumable):

```
cargo run --release -p silman-db --bin silman-cli -- --db mygames.sqlite \
  lichess-sync SomeUsername
```

- **chess.com monthly archives** (resumable):

```
cargo run --release -p silman-db --bin silman-cli -- --db mygames.sqlite \
  chesscom-sync SomeUsername
```

- **FICS games via ficsgames.org** (volunteer-run archive — keep requests
  occasional; personal use only). Whole year, or one month with `--month`:

```
cargo run --release -p silman-db --bin silman-cli -- --db mygames.sqlite \
  fics-sync SomeUsername 2025 --month 6
```

### Queries & reports (CLI-only)

- **Repertoire fingerprint** — per-color openings, most-visited positions,
  and book deviations for a player (`--json` for the full record):

```
cargo run --release -p silman-db --bin silman-cli -- --db mygames.sqlite \
  fingerprint "Carlsen, Magnus"
```

- **List matching player names** (the UI only offers autocomplete inside
  Prep/Profile):

```
cargo run --release -p silman-db --bin silman-cli -- --db mygames.sqlite \
  players carlsen
```

- **Find games by FEN** — games reaching an arbitrary typed position, with
  query timing (the UI's Position search only queries the position
  currently on the board):

```
cargo run --release -p silman-db --bin silman-cli -- --db mygames.sqlite \
  find-fen "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1"
```

- **LLM-verbalized explanation** — the static analysis explained via the
  Anthropic API, post-validated with automatic fallback to template prose
  (needs `$ANTHROPIC_API_KEY` or `--api-key`):

```
cargo run --release -p silman-db --bin silman-cli -- \
  explain-llm "r1bqkbnr/pppp1ppp/2n5/4p3/2B1P3/5N2/PPPP1PPP/RNBQK2R b KQkq - 3 3"
```

### CLI equivalents of app features

These duplicate UI functionality, useful for scripting and batch work
(`opening-tree`, `explain`, `stats` also exist and mirror the Opening
tree view, the Explain panel, and the database summary line):

```
cargo run --release -p silman-db --bin silman-cli -- --db mygames.sqlite export-pgn 123
cargo run --release -p silman-db --bin silman-cli -- --db mygames.sqlite annotate-game 123
cargo run --release -p silman-db --bin silman-cli -- --db mygames.sqlite reanalyze-game 123 --nodes 200000
cargo run --release -p silman-db --bin silman-cli -- --db mygames.sqlite run-jobs --max-jobs 100
cargo run --release -p silman-db --bin silman-cli -- --db mygames.sqlite profile "Carlsen, Magnus" --json
```

`run-jobs` needs an engine binary (set `SILMAN_STOCKFISH` if it is not on
PATH) and, like the Jobs view, folds verdicts back into annotations when
the jobs finish.

### WSUI validation harness (CLI-only)

`wsui-validate` measures the tactical screen's precision/recall against
Lichess puzzles (positives) and engine-quiet positions (negatives); results
are recorded in `docs/VALIDATION.md`.

- Build the quiet-position set from an imported master-game database:

```
cargo run --release -p silman-db --bin wsui-validate -- \
  --build-quiet-from mygames.sqlite --per-class 500 > quiet_fens.txt
```

- Run the validation (train/holdout split, holdout numbers reported):

```
cargo run --release -p silman-db --bin wsui-validate -- \
  --puzzles lichess_db_puzzle.csv --quiet quiet_fens.txt --per-class 2000
```

- Emit a small committed fixture subset from the full puzzle dump:

```
cargo run --release -p silman-db --bin wsui-validate -- \
  --puzzles lichess_db_puzzle.csv --emit-fixture 500 > puzzles_sample.csv
```

---

## Where things are stored

- Everything lives in the SQLite database you open — games, annotations,
  engine evaluations, the job queue, repertoire cards and their review
  history, imported puzzles with your attempts and tactics rating, endgame
  drill progress, the narration-voice setting, and the provenance (source,
  license, date) of every import.
- UI preferences (database path, theme, board treatment, Explain on/off,
  annotation display, narration-voice fallback, engine path, node budget,
  the first-run flag) persist in the app's local storage.
