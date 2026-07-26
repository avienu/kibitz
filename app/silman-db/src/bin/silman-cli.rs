//! silman-cli: developer CLI over the silman database core.

use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;

use clap::{Parser, Subcommand};
use silman_db::import::{import_pgn, SourceInfo, SourceKind};
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
        /// Source kind for duplicate priority: personal|twic|online|other.
        #[arg(long, default_value = "personal")]
        kind: String,
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
        /// Source kind for duplicate priority: personal|twic|online|other.
        #[arg(long, default_value = "personal")]
        kind: String,
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
    /// Statically annotate a game (Silman template comments + queued
    /// engine confirmations for fired screens; engine does NOT run).
    AnnotateGame {
        game_id: i64,
        #[arg(long, default_value_t = 200_000)]
        confirm_nodes: u64,
        #[arg(long, default_value_t = 12)]
        max_comments: u32,
    },
    /// Run pending engine jobs (spawns the engine; user-initiated), then
    /// fold confirm-verdicts back into stored annotations.
    RunJobs {
        #[arg(long, default_value_t = 100)]
        max_jobs: u32,
    },
    /// Enqueue a fresh full-game re-analysis (legacy evals retained).
    ReanalyzeGame {
        game_id: i64,
        #[arg(long, default_value_t = 200_000)]
        nodes: u64,
    },
    /// Analyze one FEN statically and print the coach prose + record JSON.
    Explain { fen: String },
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
            kind,
        } => {
            let source = SourceInfo {
                name: source_name,
                origin,
                license,
                kind: SourceKind::from_str_lossy(&kind),
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
            kind,
        } => {
            let source = SourceInfo {
                name: source_name,
                origin,
                license,
                kind: SourceKind::from_str_lossy(&kind),
            };
            let st = silman_db::import_si4::import_si4(&conn, &source, &base)?;
            println!(
                "imported {} games ({} duplicates, {} failed, {} empty) in {:.2?}",
                st.base.games_imported,
                st.base.duplicates_skipped,
                st.base.games_failed,
                st.empty_skipped,
                st.base.elapsed
            );
            if st.comments_stored + st.nags_stored + st.variations_stored > 0 {
                println!(
                    "stored inline: {} comments, {} NAGs, {} variations (encoding v2)",
                    st.comments_stored, st.nags_stored, st.variations_stored
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
            println!(
                "ficsgames.org is a volunteer-run, bandwidth-limited archive. Downloads are for \
                 your personal use only and must never be redistributed; keep requests occasional \
                 and consider contacting the maintainer (fics.ludens@gmail.com) if you rely on it."
            );
            let fetcher = silman_db::net::UreqFetcher;
            let report = silman_db::net::fics::sync_user(&conn, &fetcher, &username, year, month)?;
            println!("{report:?}");
        }
        Command::ExportPgn { game_id } => {
            print!("{}", silman_db::export::export_pgn(&conn, game_id)?);
        }
        Command::AnnotateGame {
            game_id,
            confirm_nodes,
            max_comments,
        } => {
            let r =
                silman_db::annotate::annotate_game(&conn, game_id, confirm_nodes, max_comments)?;
            println!(
                "analyzed {} positions: {} screens fired, {} confirm jobs queued, {} comments added",
                r.positions_analyzed, r.screens_fired, r.jobs_enqueued, r.comments_added
            );
        }
        Command::RunJobs { max_jobs } => {
            let path = silman_db::engine::resolve_engine_path()
                .ok_or_else(|| anyhow::anyhow!("no engine binary found (set SILMAN_STOCKFISH)"))?;
            silman_db::jobs::reset_running(&conn)?;
            let r = silman_db::jobs::run_pending(&conn, &path, max_jobs)?;
            let f = silman_db::annotate::fold_back(&conn)?;
            println!(
                "jobs done: {}, failed: {}; folded {} verdicts ({} confirmed, {} refuted, {} unclear)",
                r.done, r.failed, f.folded, f.confirmed, f.refuted, f.unclear
            );
        }
        Command::ReanalyzeGame { game_id, nodes } => {
            let n = silman_db::jobs::enqueue_reanalyze(&conn, game_id, nodes)?;
            println!(
                "queued {n} re-analysis positions for game {game_id}; run `run-jobs` to execute"
            );
        }
        Command::Explain { fen } => {
            let board: cozy_chess::Board =
                fen.parse().map_err(|e| anyhow::anyhow!("bad FEN: {e:?}"))?;
            let record = silman_core::analyze(&board);
            println!("{}", silman_verbalize::verbalize(&record));
            println!("---");
            println!("{}", serde_json::to_string_pretty(&record)?);
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
