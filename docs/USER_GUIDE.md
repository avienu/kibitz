# Silman User Guide

This guide covers everything you can click in the app and every feature that is
currently CLI-only. It is also available inside the app: press the **Help**
button at the right end of the tab bar.

---

## The window at a glance

The window has two columns.

- **Left column** — the board, move navigation, the status line, the
  **Engine** panel, and the **Explain (static, no engine)** panel. This column
  is always visible (during a Train review session the board shows the
  training position instead of the loaded game).
- **Right column** — seven tabs plus Help:
  - **Load PGN** — paste or open a PGN file to review a game.
  - **Database** — open a SQLite database, browse games, opening tree.
  - **Opponent Prep** — rank an opponent's weakest opening spots.
  - **Player Profile** — a full strengths/weaknesses report for one player.
  - **Train** — the Repertoire Trainer: spaced-repetition review of your
    opening lines. A badge on the tab shows how many cards are due.
  - **Tactics** — puzzle drills: rated, motif-filtered, weakness-weighted,
    Woodpecker cycles, and a speed drill.
  - **Endgames** — a tiered curriculum of classic theoretical positions,
    played out against a tablebase or heuristic opponent.
  - **Help** — opens this guide.

On first launch a one-time overlay points out the tabs; dismiss it with
**Got it** (it will not reappear; the flag is stored in your browser storage).

---

## Board and move navigation

- **|<** — jump to the start of the game.
- **◀ Prev** / **Next ▶** — step one ply back / forward.
- **>|** — jump to the end of the game.
- The **ply counter** (e.g. `ply 12/85`) shows where you are.
- **Keyboard:** Left/Right arrow keys step through the game (ignored while
  you are typing in a text field).
- Clicking any move in the move list jumps to it.

**Moving pieces on the board** is enabled only for games loaded from the
database (because board input is how you enter variations — see
"Annotating a game" below) and during Train review sessions. For pasted
PGNs the board is display-only.

**Line → repertoire:** whenever a game with moves is loaded, a row under
the navigation buttons offers to send the current line to the Repertoire
Trainer: at ply 0 it reads "Mainline → repertoire", after stepping forward
"Line (first N plies) → repertoire". Press **as White** or **as Black** to
add a training card for every position in that line where the chosen color
is to move (the other side's moves only provide context). The status line
reports how many cards were new and how many positions were already
covered — re-adding a line never duplicates cards. See the Train tab
section below.

---

## Load PGN tab

- **PGN text area** — paste PGN here.
- **Load** — parse the pasted text and load the game.
- **Open file…** — pick a `.pgn` (or `.txt`) file; it is loaded immediately.
- **Sample game** — loads a built-in sample (Anderssen–Kieseritzky, London
  1851) so you can try the app with no data.

The status line under the board reports what loaded (players and ply count)
or the parse error.

---

## Database tab

### Opening a database

- **Path field + Open button** — path to a silman SQLite database. The
  default is `testdata/corpus/scid.sqlite`, resolved relative to the
  repository root; the path you enter is remembered between sessions.
  After opening, a summary line shows games / players / positions / sources,
  and the window title shows the database filename.

Databases are **created and filled from the command line** (PGN import,
SCID .si4 import, TWIC/Lichess/chess.com/FICS sync) — see "CLI-only
features" below. The app opens and browses an existing database.

### Opening tree (current position)

Shows every move played from the position **currently on the board**, across
the whole database:

- **Move** — the move in SAN.
- **Games** — how many games continued with it.
- **W / D / L** — a white/grey/black results bar (hover for exact counts).
- **Elo** — average rating of the players who chose the move.
- **Perf** — performance rating of the move.

Clicking a tree row **advances the loaded game one ply** if that game
continues with the clicked move; otherwise a hint explains (e.g. "The loaded
game continues Nf3 here, not d4."). The tree follows the board, so stepping
through a game walks you down the tree.

Below the tree, "**N games reach this position**" lists up to ten of them,
each with a **load** button that opens that game.

### Games list

- **Filter field** — case-insensitive substring match on either player's
  name; the total updates as you type.
- **Games table** — White, Elo, Black, Elo, Result, Date, ECO, Event.
  Click a row to load the game (with its stored annotations and evals).
- **◀ Prev / Next ▶ pager** — 50 games per page.

---

## Working with a database game

When a game is loaded **from the database**, two extra areas appear in the
right column: the game-tools row and the annotated move list.

### Game tools row

- **Annotate game** — runs the *static* Silman annotation pass: positional
  imbalance comments plus tactical alerts from the WSUI screen. **This does
  not run the engine.** For each fired tactical alert it *enqueues* a bounded
  engine confirmation job; the summary line reports positions analyzed,
  comments added, and engine checks enqueued.
- **Re-analyze game** — enqueues one bounded engine evaluation per mainline
  position. Nothing runs yet: the jobs sit in the queue until you press
  **Run engine jobs**.
- **Run engine jobs** — the user-initiated engine entry point. Starts the
  background worker, which runs every pending job and then **folds the
  verdicts back** into the stored annotations (each tactical alert becomes
  confirmed, refuted, or unclear). The jobs strip below the buttons shows
  `pending / running / done / failed` counts, auto-refreshing every two
  seconds while work remains; when the run finishes, the game reloads itself
  so fresh evals and rewritten comments appear.
- **Export PGN** — renders the game (with annotations) as PGN, copies it to
  the clipboard, and opens a modal with the text (**Copy** / **Close**).

### The engine-off principle

Stockfish is **off by default** and never runs behind your back. It runs in
exactly three situations:

1. You press **Analyze** in the Engine panel (one position, node-limited).
2. You press **Run engine jobs** (executes the queue you built with
   Annotate game / Re-analyze game).
3. You run the CLI `run-jobs` command.

"Annotate game" and "Explain position" are static analysis — no engine
process is started.

### Annotating a game

The **Moves & annotations** panel replaces the plain move list for database
games. Mainline moves flow inline; comments appear muted; variations appear
in parentheses on indented lines.

- **Click a move** to jump the board there and reveal its edit controls:
  - **✎** — edit or add a comment on that move. Type in the comment box and
    press **Set comment** (an empty text deletes the comment) or **Cancel**.
  - **!?** — cycle the move's NAG: none / ! / ? / !! / ?? / !? / ?!.
- **× on a comment** — delete that comment.
- **× after an opening parenthesis** — delete that variation.
- **Entering a variation:** with the game at the position you want to vary,
  move a piece on the board. The mainline move simply advances the game; any
  *other* legal move pops up "Add … as a variation of …?" with
  **Add as variation** / **Dismiss** buttons. Pawns promote to a queen
  automatically.
- **full / hover / hidden** toggle (top of the panel) — how comments are
  displayed: in full, collapsed to a `°` marker with the text in a tooltip,
  or hidden. The choice persists between sessions.
- **Save / Revert** — annotation edits are local until you press **Save**
  ("unsaved changes" is shown while the panel is dirty). **Revert** discards
  them and restores the last saved state.

Next to moves you may see small numbers: stored engine evaluations in pawns
from White's point of view. Bright values come from silman's own engine
runs; muted values are legacy evaluations imported from SCID (hover for the
engine name).

---

## Engine panel (left column)

- **Analyze** — run Stockfish on the position currently on the board, up to
  the configured node budget. Live output shows the score (from the side
  shown), depth summary, principal variation, and finally the best move.
- **Stop** — halt the running search.
- **nodes** — node budget per analysis (default 2,000,000; persisted).
- **engine path (optional override)** — leave empty to auto-resolve; the
  "using:" line below always shows which binary would run. Resolution order:
  this override, then the `SILMAN_STOCKFISH` environment variable, then a
  repo-local `tools/` binary, then `stockfish` on PATH.

Navigating to another position stops any running search.

---

## Explain (static, no engine)

- **Explain position** — runs the silman-core static analyzer on the current
  position and renders coach-style prose: material and Silman imbalances,
  and any tactical alerts from the WSUI screen. Evidence is drawn on the
  board: **red** = alert targets, **orange** = attackers, **green** =
  imbalance evidence.
- **Clear** — remove the explanation and the board shapes.
- **voice** — narration style for the prose: **Coach** (default) or
  **Neutral**. Changing it clears any shown explanation (the old prose is in
  the old voice). The choice is stored in the open database (and locally,
  for when no database is open).

This is instant and engine-free; it is the same analysis "Annotate game"
applies to every position.

---

## Opponent Prep tab

Prepare against a specific opponent using the games in the open database
(open one in the Database tab first).

- **Opponent name field** — exact name; suggestions appear after two
  characters.
- **as White / as Black** — which of *their* repertoires to attack.
- **Build prep** — ranks the positions they keep reaching and score badly
  in. A spot needs **3+ of their games and an under-50% score** to qualify.

Each result card shows:

- **#rank** and the **weakness score** (higher = better prep target).
- Games count, their score percentage, and the ply by which the position is
  reached.
- A **leaves book** badge if the spot is one of their book-exit points.
- **plays here:** the moves they actually play in that position.
- **Master games** that reached the exact position — click one to load it
  on the board *at the prep position*.

If you have built a profile for the same player in the Player Profile tab, a
**Profile weaknesses** strip appears above the cards: their top three motif
weaknesses and two worst-scoring pawn structures.

---

## Player Profile tab

A corpus-wide strengths/weaknesses report for one player (open a database
first).

- **Player name field** — exact name, with suggestions as you type.
- **Build profile** — scans their games and renders:
  - **Summary** — games, score %, and *engine eval coverage* (what fraction
    of their moves have stored evaluations).
  - **Accuracy by phase (ACPL)** — average centipawn loss, blunders,
    mistakes, and inaccuracies for opening / middlegame / endgame.
  - **Motif matrix** — per tactical motif: opportunities, taken, missed,
    and allowed (against them), with clickable example game ids (**#123**)
    that open the game in the Database tab.
  - **Pawn structures & piece placement** — recurring structure flags with
    their score in those games, plus examples.
  - **Openings (ECO)** — score by ECO code, with examples.
  - **Conversion & defense** — how often they converted winning positions
    (≥ +2.00) and held worse ones (≤ −1.00).

ACPL and conversion need stored engine evaluations. If coverage is 0%, run
**Re-analyze game** + **Run engine jobs** on their games first (or the CLI
`reanalyze-game` / `run-jobs` in a batch).

The built profile survives tab switches and also feeds the Opponent Prep
weaknesses strip and the Tactics tab's weakness-weighted drill.

---

## Train tab (Repertoire Trainer)

Spaced-repetition review of your opening repertoires, scheduled with
FSRS-4.5. Uses the open database (open one in the Database tab first); each
card is a position where it is your color's turn plus the repertoire move
you must know there.

### Building a repertoire

- **From a loaded game:** use the **"→ repertoire: as White / as Black"**
  row under the board (see "Board and move navigation" above). Lines land in
  the default repertoire for that color ("main (white)" / "main (black)").
- **From a PGN study (CLI-only):** import a whole PGN file, one card per
  mainline move of the training color:

```
cargo run --release -p silman-db --bin silman-cli -- --db mygames.sqlite \
  import-repertoire study.pgn white --name "main"
```

  `--name` defaults to `main`; re-importing is idempotent (positions that
  already have a card are left untouched — first move in wins).

### Reviewing

- **White / Black** buttons switch repertoires; each shows its due count,
  and the panel reports "N due of M cards". The **Train tab itself carries
  a badge** with the combined due total.
- **Start review** (enabled when cards are due) starts a session over the
  due queue (up to 100 cards). The **main board takes over**: it flips to
  your color and shows the card's position; the prompt gives the moves so
  far ("Start position" for a first move) and asks for **your move**.
- **Play your repertoire move on the board.**
  - **Correct:** the board plays the move and asks "How well did you know
    it?" — grade yourself **Good** or **Easy** (Easy pushes the next review
    further out).
  - **Wrong:** the card lapses immediately (graded Again), a green arrow
    shows the expected move, and the message gives the answer and when the
    card comes back ("The repertoire move is Nf3 — again in <1d"). Press
    **Continue**.
- **End session** aborts at any point. After the last card a summary shows
  "N reviewed — X correct, Y to relearn", with **Back to queue**.
- Below the controls, the queue table lists the next due cards (up to 20):
  the line prefix, the due date (or "new" for unseen cards), and the
  repetition count with lapses.

Intervals display as days/months/years ("13d", "3mo", "1.5y"). New cards
are marked "new" in the session header and the queue.

---

## Tactics tab

Puzzle drills over the Lichess puzzle database, imported into the open
database (open one in the Database tab first). The puzzle board lives in
this tab, independent of the game in the left column, and **no engine is
involved anywhere** — solving is checked against the stored solution line.

The summary line shows your **tactics rating**, rated attempt count, and
how many puzzles are imported.

### Importing puzzles

In the **Import puzzles** section: enter the CSV path (default
`testdata/corpus/lichess_db_puzzle.csv`), optionally a **min popularity**
cutoff (Lichess popularity is −100..100; the field defaults to 50), and
press **Import CSV**. Download `lichess_db_puzzle.csv` from
database.lichess.org (CC0; it may be bundled freely). The full 5M-row dump
takes a few minutes and imports in constant memory.

The same import exists on the command line, with an extra row cap:

```
cargo run --release -p silman-db --bin silman-cli -- --db mygames.sqlite \
  import-puzzles lichess_db_puzzle.csv --min-popularity 50 --max-rows 100000
```

### Solving flow

Press **Next puzzle**: the opponent's setup move plays after a beat, the
clock starts, and you play every move of the solution on the puzzle board
(the board is oriented to your side; opponent replies play automatically).

- A **wrong move fails the puzzle** — the answer and the full solution line
  are shown. There are no retries on a rated attempt.
- An **alternate checkmate is accepted**: if your differing move delivers
  mate, the puzzle counts as solved.
- **Underpromotion caveat:** board input auto-promotes to a queen, so a
  solution that requires an underpromotion cannot be entered — the attempt
  shows as failed with the solution revealed (unless queening happens to
  mate too, which the alternate-mate rule accepts).
- **Give up** reveals the solution and records a failed attempt.
- After finishing, the puzzle's themes are revealed and **◀ / ▶** replay
  the solution. The outcome line shows your time and any rating change.

### The five modes

- **Rated (±100 of your rating)** — an unsolved puzzle near your rating;
  the band starts at ±100 and widens (up to ±1000) if nothing is left.
- **Motif filter** — pick a **theme** from the dropdown (each shows its
  puzzle count) and drill only that motif.
- **Weakness-weighted (from your profile)** — needs your profile (build it
  in the Player Profile tab first). Puzzles are chosen against the motifs
  your games suffer from most, and every serve shows **"Why this puzzle"**
  — e.g. "picked because your games allow many exposed-king tactics (4
  allowed, 2 missed in your profile) — this puzzle's themes […] train that
  motif".
- **Woodpecker cycle** — solve the *same* fixed set repeatedly, aiming for
  faster and cleaner cycles. In the **Woodpecker sets** section, name a set
  and give it a size, press **Create set**, then **Start cycle**. The
  session line tracks puzzle x/y, solved count, and total time; **Stats**
  lists every past cycle with attempts, solved, accuracy, total and average
  time.
- **Speed (easy, against the clock)** — deliberately easy puzzles (rated
  300–900 points below you) for fast pattern recognition. Already-solved
  puzzles stay in the pool — repetition is the point. The session line
  tracks solved/attempts and average time.

### The tactics rating

An Elo-style rating updated against the fixed puzzle ratings: K = 40 for
your first 30 rated attempts, then K = 20. Only **rated, motif and
weakness** attempts move it — Woodpecker cycles and speed drills are
repetition training and record history only.

---

## Endgames tab

A tiered curriculum of classic theoretical endgame positions (27 drills),
played out on this tab's own board against an automatic opponent. Uses the
open database for progress tracking; **no engine is involved**.

### Tiers and drills

The tier table shows each tier's rating band and your mastered count:

- **Essentials** (up to ~1000) — the two basic mates, the square of the
  pawn, king-and-pawn fundamentals.
- **Building technique** (~1000–1500) — opposition, key squares, spare
  tempi, queen vs pawn.
- **Rook endings and tempo play** (~1500–1900) — Lucena, Philidor, and the
  other rook endings that decide practical games.

**Open** a tier to list its drills: title (hover for the full instruction),
material (e.g. KQvK), goal, your attempt count, and mastery progress.
**Start** launches a drill.

### Playing a drill

You play the side to move of the drill position — the task line says
"Win with White" or "Hold the draw with Black" — and the instruction
explains the idea. The opponent replies automatically:

- **Tablebase opponent** ("Opponent: tablebase (optimal replies)"): where
  Syzygy tables cover the piece count, replies are provably
  result-optimal, and every move of yours is checked — a move that
  forfeits the theoretical result (a win thrown away, a draw lost) fails
  the drill immediately.
- **Heuristic opponent** ("Opponent: heuristic sparring partner"): without
  tables, a deterministic shallow-search sparring partner defends
  sensibly but is not an oracle, and only actual game endings
  (checkmate / stalemate / draw) are detected.

**Give up** ends the attempt; after an attempt, **Retry** or **Back to
drills**. The board input auto-promotes to a queen; the curriculum is
designed so that no drill needs an underpromotion.

### Mastery

A drill is **mastered** after 2 clean completions (solving without
failing in between); progress shows as "1/2 clean", then "mastered" with a
✓ in the drill list. The summary line tracks total drills mastered.

### Getting tablebases

The Syzygy directory resolves from the `SILMAN_SYZYGY` environment
variable, else by walking up from the working directory to
`testdata/syzygy`. The repo script

```
scripts/fetch_syzygy_test_files.sh
```

downloads the complete 3-man set (under 100 KB, from the Lichess mirror)
into `testdata/syzygy/` — enough for the test suite, but most curriculum
drills have more pieces and then fall back to the heuristic opponent. For
tablebase-verified play across the whole curriculum, point `SILMAN_SYZYGY`
at a downloaded 3-4-5-man set. The note at the top of the tab always
states which opponent you are getting.

---

## End-to-end workflows

### Import games and browse them

1. CLI: `import-pgn` / `import-si4` (or a sync command) into a `.sqlite`
   file — see below.
2. App → Database tab → enter the path → **Open**.
3. Filter, click a game, step through it.

### Annotate a game and confirm tactics with the engine

1. Load a game from the Database tab.
2. **Annotate game** — instant static comments; engine checks are queued.
3. **Run engine jobs** — the queue runs; verdicts fold back automatically.
4. The game reloads: alerts are now marked confirmed/refuted/unclear.
5. Edit by hand (comments, NAGs, variations) and **Save**.
6. **Export PGN** to take the annotated game elsewhere.

### Build evals, then profile a player

1. Load each game of interest → **Re-analyze game** → **Run engine jobs**
   (or CLI `reanalyze-game` + `run-jobs` for batches).
2. Player Profile tab → name → **Build profile** — ACPL and conversion now
   have data.

### Prep for an opponent

1. Open the database containing their games.
2. (Optional) Build their profile first for the weaknesses strip.
3. Opponent Prep tab → name → **as White** or **as Black** → **Build prep**.
4. Click a master game on a card to study the critical position.

### Build and train an opening repertoire

1. Load a model game (Load PGN or Database tab), step to where your line
   ends, and press "→ repertoire: **as White**/**as Black**" — or bulk-import
   a PGN study with the CLI `import-repertoire`.
2. Train tab → pick the color → **Start review**.
3. Play each prompted move on the board; grade **Good**/**Easy** when right,
   read the arrow and press **Continue** when wrong.
4. Come back when the tab badge shows cards due — FSRS schedules the rest.

### Train tactics against your own weaknesses

1. Tactics tab → **Import CSV** (once) with the Lichess puzzle dump.
2. Player Profile tab → build your own profile.
3. Tactics tab → mode **Weakness-weighted (from your profile)** →
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

- **Import a repertoire PGN** for the Train tab — every mainline move of
  the chosen color becomes a training card (the UI can only add lines from
  a loaded game, one at a time). `--name` defaults to `main`; re-import is
  idempotent:

```
cargo run --release -p silman-db --bin silman-cli -- --db mygames.sqlite \
  import-repertoire study.pgn white --name "main"
```

- **Import Lichess puzzles** for the Tactics tab. The Tactics tab's
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
  query timing (the UI shows this only for the position currently on the
  board):

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
(`opening-tree`, `explain`, `stats` also exist and mirror the Database tab
tree, the Explain panel, and the database summary line):

```
cargo run --release -p silman-db --bin silman-cli -- --db mygames.sqlite export-pgn 123
cargo run --release -p silman-db --bin silman-cli -- --db mygames.sqlite annotate-game 123
cargo run --release -p silman-db --bin silman-cli -- --db mygames.sqlite reanalyze-game 123 --nodes 200000
cargo run --release -p silman-db --bin silman-cli -- --db mygames.sqlite run-jobs --max-jobs 100
cargo run --release -p silman-db --bin silman-cli -- --db mygames.sqlite profile "Carlsen, Magnus" --json
```

`run-jobs` needs an engine binary (set `SILMAN_STOCKFISH` if it is not on
PATH) and, like the UI button, folds verdicts back into annotations when the
jobs finish.

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
- UI preferences (database path, engine path, node budget, comment display
  mode, narration voice fallback, the first-run flag) persist in the app's
  local storage.
