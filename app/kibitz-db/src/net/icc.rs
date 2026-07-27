//! ICC (Internet Chess Club) games retrieval — documented stub.
//!
//! # Investigation result (July 2026): no scriptable HTTP path exists
//!
//! ICC exposes **no public or member-facing HTTP endpoint** for retrieving a
//! member's game history as PGN, so this module deliberately implements no
//! client. What was found:
//!
//! - `https://www.chessclub.com/` is a marketing site (Next.js); its nav
//!   links only to account/membership pages and the play client. No games,
//!   PGN, or API links anywhere.
//! - `https://play.chessclub.com/` is a single-page app driven by ICC's
//!   proprietary protocol over websockets; no REST endpoints are visible.
//! - ICC's support knowledge base (`support.chessclub.com`) documents PGN
//!   access only as an interactive, per-game flow: Analysis mode → Games
//!   tab → browse your games → per-game "upload & download PGN"
//!   (article 10538221281308, "How do I analyze games"). Full-text searches
//!   of the knowledge base for "api" and "telnet" return zero articles, and
//!   there is no bulk-export article.
//! - Article 13730571717276 confirms even viewing *another* player's game
//!   history is "not currently available".
//! - Historically, games were retrieved via ICC's telnet protocol (port
//!   5000, `spgn`/`history` commands) through their desktop clients
//!   (BlitzIn/Dasher had client-side "autosave PGN to this computer"
//!   options). The legacy documentation pages (`chessclub.com/help/pgn`,
//!   `/help/spgn`, `/help/history`) now all 301-redirect to the homepage.
//!   Per project rules, a telnet client is out of scope and is not
//!   implemented.
//!
//! # Manual path
//!
//! Export PGN from ICC's own interface, then import the file locally — see
//! [`MANUAL_IMPORT_INSTRUCTIONS`], which the CLI prints when a user asks
//! for ICC ingestion.

/// Instructions the CLI shows for ICC, since no automated client can exist.
pub const MANUAL_IMPORT_INSTRUCTIONS: &str = "\
ICC (chessclub.com) has no HTTP API for downloading your game history, so \
kibitz cannot fetch ICC games automatically. Manual path: open the ICC play \
client (play.chessclub.com or the desktop app), enter Analysis mode, open \
the Games tab, and use its PGN download for the games you want (desktop \
clients can also auto-save your games to a local PGN file as you play). \
Then import the saved file with: kibitz-cli import-pgn <file.pgn>";
