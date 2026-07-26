//! silman-db: GPL-3.0 app-layer database core.
//!
//! Holds the SQLite schema/migrations, streaming PGN importer, binary move
//! encoding, and Zobrist position index. Lives in `app/` (GPL layer) per
//! ARCHITECTURE.md; the BSD crates must never depend on this.
