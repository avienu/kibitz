# Silman User Guide

This guide covers everything you can click in the app and every feature that is
currently CLI-only. It is also available inside the app: press the **Help**
button at the right end of the tab bar.

---

## The window at a glance

The window has two columns.

- **Left column** — the board, move navigation, the status line, the
  **Engine** panel, and the **Explain (static, no engine)** panel. This column
  is always visible.
- **Right column** — four tabs plus Help:
  - **Load PGN** — paste or open a PGN file to review a game.
  - **Database** — open a SQLite database, browse games, opening tree.
  - **Opponent Prep** — rank an opponent's weakest opening spots.
  - **Player Profile** — a full strengths/weaknesses report for one player.
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
"Annotating a game" below). For pasted PGNs the board is display-only.

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
weaknesses strip.

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

---

## CLI-only features

Everything below has **no UI entry point** — it exists only in the developer
CLI (`silman-cli`) or the validation harness (`wsui-validate`). Run them from
the repository root. The general form is:

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
  engine evaluations, the job queue, and the provenance (source, license,
  date) of every import.
- UI preferences (database path, engine path, node budget, comment display
  mode, the first-run flag) persist in the app's local storage.
