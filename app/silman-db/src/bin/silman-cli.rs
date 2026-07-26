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
    /// Repertoire fingerprint for a player (exact name).
    Fingerprint {
        player: String,
        #[arg(long, default_value_t = silman_db::fingerprint::DEFAULT_MAX_PLIES)]
        max_plies: u16,
        /// Emit the full fingerprint as JSON instead of a summary.
        #[arg(long)]
        json: bool,
    },
    /// List player names matching a pattern.
    Players { pattern: String },
    /// Incrementally ingest TWIC issues (personal use; see first-run notice).
    TwicSync {
        /// Issue number to start from (required on first run).
        #[arg(long)]
        from: Option<u32>,
        #[arg(long, default_value_t = 5)]
        max_issues: u32,
    },
    /// Download and import a Lichess user's games (resumable).
    LichessSync { username: String },
    /// Download and import a chess.com user's monthly archives (resumable).
    ChesscomSync { username: String },
    /// Download and import a FICS user's games via ficsgames.org.
    FicsSync {
        username: String,
        year: u16,
        /// Month 1-12; omit for the whole year.
        #[arg(long)]
        month: Option<u8>,
    },
    /// Export one stored game as PGN to stdout.
    ExportPgn { game_id: i64 },
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
        Command::Fingerprint {
            player,
            max_plies,
            json,
        } => {
            let fp = silman_db::fingerprint::player_fingerprint(&conn, &player, max_plies)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&fp)?);
            } else {
                for (label, cf) in [("White", &fp.white), ("Black", &fp.black)] {
                    println!(
                        "== {} as {label}: {} games, {:.1}% score",
                        fp.player, cf.games, cf.score_pct
                    );
                    println!("  openings:");
                    for e in cf.eco_families.iter().take(8) {
                        println!(
                            "    {:<4} {:>4} games  {:>5.1}%",
                            e.eco, e.games, e.score_pct
                        );
                    }
                    println!("  most-visited positions:");
                    for p in cf.positions.iter().take(8) {
                        let moves: Vec<String> = p
                            .moves
                            .iter()
                            .take(4)
                            .map(|m| format!("{} x{} ({:.0}%)", m.san, m.count, m.score_pct))
                            .collect();
                        println!(
                            "    ply>={:<2} seen {:>3}x: {}",
                            p.min_ply,
                            p.count,
                            moves.join(", ")
                        );
                    }
                    println!("  book deviations (first exit per game):");
                    for d in cf.deviations.iter().take(8) {
                        let ctx = silman_db::fingerprint::example_game_at(&conn, d.hash_before)
                            .map(|(id, w, b)| format!("  e.g. game #{id} {w}-{b}"))
                            .unwrap_or_default();
                        println!(
                            "    ply {:<2} {:<8} x{} ({:.0}%){}",
                            d.ply, d.san, d.count, d.score_pct, ctx
                        );
                    }
                    println!();
                }
            }
        }
        Command::Players { pattern } => {
            for name in silman_db::fingerprint::matching_players(&conn, &pattern)? {
                println!("{name}");
            }
        }
        Command::TwicSync { from, max_issues } => {
            let fetcher = silman_db::net::UreqFetcher;
            let report = silman_db::twic::sync(
                &conn,
                &fetcher,
                &silman_db::twic::TwicOptions { from, max_issues },
            )?;
            if let Some(notice) = &report.first_run_notice {
                println!("{notice}");
            }
            for issue in &report.issues {
                println!("{issue:?}");
            }
            if report.up_to_date {
                println!("up to date");
            }
        }
        Command::LichessSync { username } => {
            let fetcher = silman_db::net::UreqFetcher;
            let report = silman_db::net::lichess::sync_user(&conn, &fetcher, &username)?;
            println!("{report:?}");
        }
        Command::ChesscomSync { username } => {
            let fetcher = silman_db::net::UreqFetcher;
            let report = silman_db::net::chesscom::sync_user(&conn, &fetcher, &username)?;
            println!("{report:?}");
        }
        Command::FicsSync {
            username,
            year,
            month,
        } => {
            let fetcher = silman_db::net::UreqFetcher;
            let report = silman_db::net::fics::sync_user(&conn, &fetcher, &username, year, month)?;
            println!("{report:?}");
        }
        Command::ExportPgn { game_id } => {
            print!("{}", silman_db::export::export_pgn(&conn, game_id)?);
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
