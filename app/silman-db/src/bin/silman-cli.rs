//! silman-cli: developer CLI over the silman database core.

use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;

use clap::{Parser, Subcommand};
use silman_db::import::{import_pgn, SourceInfo};
use silman_db::query::{find_fen, stats};

#[derive(Parser)]
#[command(name = "silman-cli", about = "silman database developer CLI")]
struct Cli {
    /// Path to the SQLite database (created if missing).
    #[arg(long, global = true, default_value = "silman.sqlite")]
    db: PathBuf,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Create or migrate the database.
    Init,
    /// Import a PGN file (streaming; malformed games are skipped).
    ImportPgn {
        file: PathBuf,
        /// Provenance: human-readable source name.
        #[arg(long, default_value = "manual import")]
        source_name: String,
        /// Provenance: origin (URL or description).
        #[arg(long, default_value = "local file")]
        origin: String,
        /// Provenance: license of the imported data.
        #[arg(long, default_value = "unknown")]
        license: String,
    },
    /// Import a SCID database (.si4/.sg4/.sn4 base path).
    ImportSi4 {
        base: PathBuf,
        #[arg(long, default_value = "SCID import")]
        source_name: String,
        #[arg(long, default_value = "local SCID database")]
        origin: String,
        #[arg(long, default_value = "personal data")]
        license: String,
    },
    /// List games that reached the given FEN, with query timing.
    FindFen { fen: String },
    /// Show the opening tree (moves, W/D/L, perf) for a FEN.
    OpeningTree { fen: String },
    /// Print database summary counts.
    Stats,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let conn = silman_db::db::open(&cli.db)?;

    match cli.command {
        Command::Init => {
            println!("database ready at {}", cli.db.display());
        }
        Command::ImportPgn {
            file,
            source_name,
            origin,
            license,
        } => {
            let source = SourceInfo {
                name: source_name,
                origin,
                license,
            };
            let reader = BufReader::with_capacity(1 << 20, File::open(&file)?);
            let st = import_pgn(&conn, &source, reader)?;
            println!(
                "imported {} games ({} duplicates skipped, {} failed), {} positions indexed in {:.2?} ({:.0} games/s)",
                st.games_imported,
                st.duplicates_skipped,
                st.games_failed,
                st.positions_indexed,
                st.elapsed,
                (st.games_imported + st.duplicates_skipped + st.games_failed) as f64
                    / st.elapsed.as_secs_f64()
            );
            for f in &st.failures {
                eprintln!("  skipped: {f}");
            }
        }
        Command::ImportSi4 {
            base,
            source_name,
            origin,
            license,
        } => {
            let source = SourceInfo {
                name: source_name,
                origin,
                license,
            };
            let st = silman_db::import_si4::import_si4(&conn, &source, &base)?;
            println!(
                "imported {} games ({} duplicates, {} failed, {} empty, {} with null moves) \
                 in {:.2?}",
                st.base.games_imported,
                st.base.duplicates_skipped,
                st.base.games_failed,
                st.empty_skipped,
                st.null_move_skipped,
                st.base.elapsed
            );
            if st.comments_dropped + st.nags_dropped + st.variations_dropped > 0 {
                println!(
                    "NOTE: {} comments, {} NAGs, {} variations present in the source were NOT \
                     stored (annotation storage pending encoding v2 — see DECISIONS_NEEDED.md)",
                    st.comments_dropped, st.nags_dropped, st.variations_dropped
                );
            }
            for f in &st.base.failures {
                eprintln!("  failed: {f}");
            }
        }
        Command::OpeningTree { fen } => {
            let (tree, elapsed) = silman_db::query::opening_tree(&conn, &fen)?;
            println!(
                "{:<8} {:>7} {:>6} {:>6} {:>6} {:>8} {:>6}",
                "move", "games", "+W", "=D", "-B", "avg-elo", "perf"
            );
            for m in &tree {
                println!(
                    "{:<8} {:>7} {:>6} {:>6} {:>6} {:>8} {:>6}",
                    m.san,
                    m.count,
                    m.white_wins,
                    m.draws,
                    m.black_wins,
                    m.avg_elo.map_or("-".into(), |e| e.to_string()),
                    m.perf.map_or("-".into(), |p| p.to_string()),
                );
            }
            println!("{} distinct moves in {:.3?}", tree.len(), elapsed);
        }
        Command::FindFen { fen } => {
            let (hits, elapsed) = find_fen(&conn, &fen)?;
            for h in &hits {
                println!(
                    "#{}  {} - {}  {}  {}  {}  (ply {})",
                    h.game_id, h.white, h.black, h.event, h.date, h.result, h.ply
                );
            }
            println!("{} game(s) in {:.3?}", hits.len(), elapsed);
        }
        Command::Stats => {
            let s = stats(&conn)?;
            println!(
                "games: {}  players: {}  positions: {}  sources: {}",
                s.games, s.players, s.positions, s.sources
            );
        }
    }
    Ok(())
}
