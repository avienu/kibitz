# First Claude Code session — paste this as your opening instruction

Read CLAUDE.md, docs/ARCHITECTURE.md, docs/KIBITZ_ENGINE_SPEC.md, and
docs/ROADMAP.md in full before writing any code.

Then execute Phase 0 from ROADMAP.md, in this order:

1. Scaffold the cargo workspace exactly per the CLAUDE.md layout. Create
   LICENSE files (BSD-3-Clause in each crates/* member, GPL-3.0 in app/),
   docs/LICENSES.md with the initial dependency registry, and CI
   (GitHub Actions): fmt, clippy -D warnings, test, and a cargo-license job
   that FAILS if any crates/* member has a GPL/LGPL/AGPL dependency.
2. Pin cozy-chess in kibitz-core. Write the perft correctness suite
   (standard perft positions, depths per published tables) and a criterion
   benchmark for movegen + attack-map queries. Record results in
   docs/BENCHMARKS.md with hardware notes.
3. Scaffold the Tauri v2 app with chessground: render a board, load a PGN
   file, arrow-key through moves.
4. Implement the minimal UCI manager in app/src-tauri: engine registry entry
   for a user-provided Stockfish path, spawn, isready/position/go nodes,
   parse info lines, stream eval+PV to the UI for the displayed position.
5. Spike si4-read: parse the .si4 header and index records and the .sn4 name
   file; CLI that dumps game headers. Cleanroom rules per CLAUDE.md apply.

Constraints and reminders:
- Ask before adding ANY dependency not already named in the docs; state its
  license when proposing it.
- Small commits, conventional commit messages.
- If the cozy-chess benchmark misses the 2x-of-shakmaty bar, STOP and report
  rather than substituting a library.
- macOS is the dev target; keep everything Linux-buildable (CI runs Linux).

When Phase 0 acceptance criteria are met, stop and produce a short report:
what passed, benchmark numbers, any deviations proposed for the docs.
