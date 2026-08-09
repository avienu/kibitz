//! kibitz-cli: developer CLI over the kibitz database core.

use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;

use clap::{Parser, Subcommand};
use kibitz_db::import::{import_pgn, SourceInfo, SourceKind};
use kibitz_db::query::{find_fen, stats};

#[derive(Parser)]
#[command(name = "kibitz-cli", about = "kibitz database developer CLI")]
struct Cli {
    /// Path to the SQLite database (created if missing).
    #[arg(long, global = true, default_value = "kibitz.sqlite")]
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
        #[arg(long, default_value_t = kibitz_db::fingerprint::DEFAULT_MAX_PLIES)]
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
    /// Statically annotate a game (Kibitz template comments + queued
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
    /// Analyze one FEN and explain it via the Anthropic LLM verbalizer;
    /// output is post-validated and falls back to template prose on any
    /// hallucination or transport failure.
    ExplainLlm {
        fen: String,
        /// Anthropic API key (defaults to $ANTHROPIC_API_KEY).
        #[arg(long)]
        api_key: Option<String>,
    },
    /// Full player profile (motifs, structures, ACPL where evals exist).
    Profile {
        player: String,
        #[arg(long, default_value_t = 2000)]
        max_games: u32,
        #[arg(long)]
        json: bool,
    },
    /// Import a PGN file as a Repertoire Trainer repertoire for one color:
    /// every mainline move of that color becomes a training card.
    ImportRepertoire {
        file: PathBuf,
        /// Training color: white|black.
        color: String,
        /// Repertoire name (created if missing; re-import is idempotent).
        #[arg(long, default_value = "main")]
        name: String,
        /// Provenance: human-readable source name.
        #[arg(long, default_value = "repertoire import")]
        source_name: String,
        /// Provenance: origin (URL or description).
        #[arg(long, default_value = "local file")]
        origin: String,
        /// Provenance: license of the imported data.
        #[arg(long, default_value = "personal data")]
        license: String,
    },
    /// Import the Lichess puzzle database CSV (CC0) for the tactics
    /// trainer — streaming with batched transactions; the 5M-row dump
    /// imports in constant memory.
    ImportPuzzles {
        file: PathBuf,
        /// Skip puzzles whose Popularity (-100..100) is below this value.
        #[arg(long)]
        min_popularity: Option<i64>,
        /// Stop after importing this many puzzles (post-filter).
        #[arg(long)]
        max_rows: Option<u64>,
        #[arg(long, default_value = "lichess-puzzles")]
        source_name: String,
        #[arg(long, default_value = "https://database.lichess.org/#puzzles")]
        origin: String,
        #[arg(long, default_value = "CC0-1.0")]
        license: String,
    },
    /// Print database summary counts.
    Stats,
    /// Fit the who-stands-better weights against decisive master games
    /// in the local database. Reports holdout accuracy; changes nothing.
    FavorsFit {
        #[arg(long, default_value_t = 4000)]
        samples: usize,
        #[arg(long, default_value_t = 0xC0FFEE)]
        seed: u64,
        /// Label by what an engine makes of the POSITION rather than by
        /// who won the game. Slow-acting imbalances are under-credited by
        /// outcomes settled thirty moves later; this asks whether the
        /// assessment was right instead.
        #[arg(long)]
        engine: bool,
        #[arg(long, default_value_t = 200_000)]
        nodes: u64,
    },
    /// Classify each book-recommended move as denial or construction,
    /// with the tempo comparison, to settle how prophylaxis should rank.
    ProphylaxisStudy {
        #[arg(default_value = "testdata/private/book-trials")]
        path: PathBuf,
    },
    /// Alert false-positive rate over engine-quiet master positions —
    /// the cost term the book corpus cannot measure.
    AlertsFp {
        #[arg(default_value = "testdata/corpus/quiet_fens.txt")]
        path: PathBuf,
    },
    /// Entombed-piece firing rate over the same engine-quiet master
    /// positions — the cost term for the imbalance, which buys no engine
    /// time but does move the material ledger.
    EntombFp {
        #[arg(default_value = "testdata/corpus/quiet_fens.txt")]
        path: PathBuf,
        /// Print every firing position, so the hits can be read rather
        /// than counted. A rate is not a verdict.
        #[arg(long)]
        dump: bool,
    },
    /// Is "shield file empty" a proxy for "shield pawn traded away"?
    /// Lists every position where a shield pawn is absent while an
    /// adjacent file holds a doubled pawn, and whether WeakKing fired.
    ShieldStudy {
        #[arg(num_args = 1.., default_values = [
            "testdata/private/book-trials",
            "testdata/corpus/quiet_fens.txt",
        ])]
        paths: Vec<PathBuf>,
    },
    /// Coverage of plan speed: how often schemes/maneuvers (and thus
    /// horizons) exist at all — prices the tempo-hypothesis prerequisite.
    HorizonStudy {
        #[arg(num_args = 1.., default_values = [
            "testdata/private/book-trials",
            "testdata/corpus/quiet_fens.txt",
        ])]
        paths: Vec<PathBuf>,
    },
    /// Class study for the praxis-g70 red anchor: WeakKing alerts on
    /// pure shield evidence against a queenless opponent.
    QueenlessStudy {
        #[arg(num_args = 1.., default_values = [
            "testdata/private/book-trials",
            "testdata/corpus/quiet_fens.txt",
        ])]
        paths: Vec<PathBuf>,
    },
    /// Firing rate of one plan hint over the engine-quiet master set —
    /// the generic cost term for new plan-hint conditions.
    HintFp {
        hint: String,
        #[arg(default_value = "testdata/corpus/quiet_fens.txt")]
        path: PathBuf,
        #[arg(long)]
        dump: bool,
    },
    /// Price the four remaining WeakKing misses: how common are the
    /// lagging-king and sector-funnel conditions, and does WeakKing
    /// already fire there?
    KingStudy {
        #[arg(num_args = 1.., default_values = [
            "testdata/private/book-trials",
            "testdata/corpus/quiet_fens.txt",
        ])]
        paths: Vec<PathBuf>,
    },
    /// Compare suggest-verify gate settings: jobs, plans, plans per job.
    GateStudy {
        #[arg(default_value = "testdata/private/book-trials")]
        path: PathBuf,
    },
    /// Partition the missed book alerts into engine-off cost, static gap,
    /// and screen defect — the split that decides whether 31.2% can move.
    AlertsStudy {
        #[arg(default_value = "testdata/private/book-trials")]
        path: PathBuf,
    },
    /// Score the analyzer against a private book-trial corpus
    /// (testdata/private/book-trials). Path may be a file or directory.
    BookEval {
        #[arg(default_value = "testdata/private/book-trials")]
        path: PathBuf,
        /// Print every individual miss.
        #[arg(long)]
        verbose: bool,
    },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let conn = kibitz_db::db::open(&cli.db)?;

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
            let st = kibitz_db::import_si4::import_si4(&conn, &source, &base)?;
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
            let (tree, elapsed) = kibitz_db::query::opening_tree(&conn, &fen)?;
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
            let fp = kibitz_db::fingerprint::player_fingerprint(&conn, &player, max_plies)?;
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
                        let ctx = kibitz_db::fingerprint::example_game_at(&conn, d.hash_before)
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
            for name in kibitz_db::fingerprint::matching_players(&conn, &pattern)? {
                println!("{name}");
            }
        }
        Command::TwicSync { from, max_issues } => {
            let fetcher = kibitz_db::net::UreqFetcher;
            let report = kibitz_db::twic::sync(
                &conn,
                &fetcher,
                &kibitz_db::twic::TwicOptions { from, max_issues },
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
            let fetcher = kibitz_db::net::UreqFetcher;
            let report = kibitz_db::net::lichess::sync_user(&conn, &fetcher, &username)?;
            println!("{report:?}");
        }
        Command::ChesscomSync { username } => {
            let fetcher = kibitz_db::net::UreqFetcher;
            let report = kibitz_db::net::chesscom::sync_user(&conn, &fetcher, &username)?;
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
            let fetcher = kibitz_db::net::UreqFetcher;
            let report = kibitz_db::net::fics::sync_user(&conn, &fetcher, &username, year, month)?;
            println!("{report:?}");
        }
        Command::ExportPgn { game_id } => {
            print!("{}", kibitz_db::export::export_pgn(&conn, game_id)?);
        }
        Command::AnnotateGame {
            game_id,
            confirm_nodes,
            max_comments,
        } => {
            let r =
                kibitz_db::annotate::annotate_game(&conn, game_id, confirm_nodes, max_comments)?;
            println!(
                "analyzed {} positions: {} screens fired, {} confirm jobs queued, \
                 {} suggest-verify jobs queued, {} comments added",
                r.positions_analyzed,
                r.screens_fired,
                r.jobs_enqueued,
                r.suggest_jobs_enqueued,
                r.comments_added
            );
        }
        Command::RunJobs { max_jobs } => {
            let path = kibitz_db::engine::resolve_engine_path()
                .ok_or_else(|| anyhow::anyhow!("no engine binary found (set KIBITZ_STOCKFISH)"))?;
            kibitz_db::jobs::reset_running(&conn)?;
            let r = kibitz_db::jobs::run_pending(&conn, &path, max_jobs)?;
            let f = kibitz_db::annotate::fold_back(&conn)?;
            println!(
                "jobs done: {}, failed: {}; folded {} verdicts ({} confirmed, {} refuted, {} unclear)",
                r.done, r.failed, f.folded, f.confirmed, f.refuted, f.unclear
            );
        }
        Command::ReanalyzeGame { game_id, nodes } => {
            let n = kibitz_db::jobs::enqueue_reanalyze(&conn, game_id, nodes)?;
            println!(
                "queued {n} re-analysis positions for game {game_id}; run `run-jobs` to execute"
            );
        }
        Command::Explain { fen } => {
            let board: cozy_chess::Board =
                fen.parse().map_err(|e| anyhow::anyhow!("bad FEN: {e:?}"))?;
            let record = kibitz_core::analyze(&board);
            let voice = kibitz_db::narrate::narration_voice(&conn)?;
            println!("{}", kibitz_verbalize::verbalize_voiced(&record, voice));
            println!("---");
            println!("{}", serde_json::to_string_pretty(&record)?);
        }
        Command::ExplainLlm { fen, api_key } => {
            use kibitz_verbalize::llm::{LlmVerbalizer, VerbalizeMode};
            let board: cozy_chess::Board =
                fen.parse().map_err(|e| anyhow::anyhow!("bad FEN: {e:?}"))?;
            let record = kibitz_core::analyze(&board);
            let transport = kibitz_db::net::llm::AnthropicTransport::resolve(api_key)?;
            let voice = kibitz_db::narrate::narration_voice(&conn)?;
            let out = LlmVerbalizer::with_voice(transport, voice).verbalize_checked(&record);
            match &out.mode {
                VerbalizeMode::Llm => println!("mode: llm"),
                VerbalizeMode::TemplateFallback(reason) => {
                    println!("mode: template-fallback ({reason})");
                }
            }
            println!("{}", out.text);
        }
        Command::Profile {
            player,
            max_games,
            json,
        } => {
            let p = kibitz_db::profile::build_profile(&conn, &player, max_games)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&p)?);
            } else {
                println!(
                    "== {} — {} games, {:.1}% score, eval coverage {:.1}%",
                    p.player, p.games, p.score_pct, p.eval_coverage_pct
                );
                for (label, a) in [
                    ("opening", &p.acpl_opening),
                    ("middlegame", &p.acpl_middlegame),
                    ("endgame", &p.acpl_endgame),
                ] {
                    if a.moves > 0 {
                        println!(
                            "  ACPL {label:<11} {:>6.1}  ({} moves: {} blunders, {} mistakes, {} inaccuracies)",
                            a.acpl, a.moves, a.blunders, a.mistakes, a.inaccuracies
                        );
                    }
                }
                // Compact evidence form: game id @ producing ply, "3759@p34".
                fn eg(examples: &[kibitz_profile::Example]) -> String {
                    examples
                        .iter()
                        .map(|e| e.to_string())
                        .collect::<Vec<_>>()
                        .join(", ")
                }
                println!("  motif matrix (opportunities taken/missed | allowed against self):");
                for m in p.motifs.iter().take(8) {
                    println!(
                        "    {:<22} opp {:>4}  taken {:>4}  missed {:>4} (e.g. {})  allowed {:>4} (e.g. {})",
                        m.kind, m.opportunities, m.taken, m.missed, eg(&m.example_missed), m.allowed, eg(&m.example_allowed)
                    );
                }
                println!("  structures:");
                for s in p.structures.iter().take(8) {
                    println!(
                        "    {:<22} {:>4} games  {:>5.1}%  (e.g. {})",
                        s.flag,
                        s.games,
                        s.score_pct,
                        eg(&s.examples)
                    );
                }
                println!("  openings:");
                for e in p.eco.iter().take(8) {
                    println!(
                        "    {:<4} {:>4} games  {:>5.1}%  (e.g. {})",
                        e.eco,
                        e.games,
                        e.score_pct,
                        eg(&e.examples)
                    );
                }
                println!(
                    "  conversion: reached +2.00 in {} games, converted {}; reached -1.00 in {}, held {}",
                    p.conversion.winning_reached,
                    p.conversion.converted_wins,
                    p.conversion.losing_reached,
                    p.conversion.held
                );
            }
        }
        Command::ImportRepertoire {
            file,
            color,
            name,
            source_name,
            origin,
            license,
        } => {
            let color = match color.as_str() {
                "white" => kibitz_profile::Color::White,
                "black" => kibitz_profile::Color::Black,
                other => anyhow::bail!("color must be white or black, got {other:?}"),
            };
            let source = SourceInfo {
                name: source_name,
                origin,
                license,
                kind: SourceKind::Personal,
            };
            let reader = BufReader::with_capacity(1 << 20, File::open(&file)?);
            let st =
                kibitz_db::repertoire::import_pgn_repertoire(&conn, color, &name, &source, reader)?;
            println!(
                "read {} lines ({} failed): {} new cards, {} positions already covered, {} plies walked",
                st.games_read,
                st.games_failed,
                st.line.cards_added,
                st.line.cards_existing,
                st.line.plies_walked
            );
            for f in &st.failures {
                eprintln!("  failed: {f}");
            }
        }
        Command::ImportPuzzles {
            file,
            min_popularity,
            max_rows,
            source_name,
            origin,
            license,
        } => {
            let source = SourceInfo {
                name: source_name,
                origin,
                license,
                kind: SourceKind::Other,
            };
            let opts = kibitz_db::tactics::PuzzleImportOptions {
                min_popularity,
                max_rows,
            };
            let reader = BufReader::with_capacity(1 << 20, File::open(&file)?);
            let st = kibitz_db::tactics::import_puzzles_csv(&conn, &source, reader, &opts)?;
            println!(
                "imported {} puzzles ({} duplicates skipped, {} filtered out, {} malformed) in {:.2?} ({:.0} rows/s)",
                st.imported,
                st.duplicates_skipped,
                st.filtered_out,
                st.malformed,
                st.elapsed,
                (st.imported + st.duplicates_skipped + st.filtered_out) as f64
                    / st.elapsed.as_secs_f64().max(1e-9)
            );
        }
        Command::FavorsFit {
            samples,
            seed,
            engine,
            nodes,
        } => {
            let label = if engine {
                kibitz_db::favorsfit::Label::Engine
            } else {
                kibitz_db::favorsfit::Label::Outcome
            };
            kibitz_db::favorsfit::run_labelled(&conn, samples, seed, label, nodes)?;
        }
        Command::AlertsFp { path } => {
            kibitz_db::bookeval::alerts_fp(&path)?;
        }
        Command::EntombFp { path, dump } => {
            kibitz_db::bookeval::entomb_fp(&path, dump)?;
        }
        Command::ShieldStudy { paths } => {
            kibitz_db::bookeval::shield_study(&paths)?;
        }
        Command::HorizonStudy { paths } => {
            kibitz_db::bookeval::horizon_study(&paths)?;
        }
        Command::QueenlessStudy { paths } => {
            kibitz_db::bookeval::queenless_study(&paths)?;
        }
        Command::HintFp { hint, path, dump } => {
            kibitz_db::bookeval::hint_fp(&path, &hint, dump)?;
        }
        Command::KingStudy { paths } => {
            kibitz_db::bookeval::king_study(&paths)?;
        }
        Command::GateStudy { path } => {
            let corpora = kibitz_db::bookeval::load(&path)?;
            kibitz_db::bookeval::gate_study(&corpora)?;
        }
        Command::AlertsStudy { path } => {
            let corpora = kibitz_db::bookeval::load(&path)?;
            kibitz_db::bookeval::alerts_study(&corpora)?;
        }
        Command::ProphylaxisStudy { path } => {
            let corpora = kibitz_db::bookeval::load(&path)?;
            kibitz_db::bookeval::prophylaxis_study(&corpora)?;
        }
        Command::BookEval { path, verbose } => {
            let corpora = kibitz_db::bookeval::load(&path)?;
            let mut totals = kibitz_db::bookeval::Report::default();
            for corpus in &corpora {
                let r = kibitz_db::bookeval::eval_corpus(corpus);
                kibitz_db::bookeval::print_report(&corpus.book, &r, verbose);
                totals.positions += r.positions;
                totals.imbalance.hits += r.imbalance.hits;
                totals.imbalance.total += r.imbalance.total;
                totals.plans.hits += r.plans.hits;
                totals.plans.total += r.plans.total;
                totals.alerts.hits += r.alerts.hits;
                totals.alerts.total += r.alerts.total;
                totals.favors.hits += r.favors.hits;
                totals.favors.total += r.favors.total;
                totals.suggest_top1.hits += r.suggest_top1.hits;
                totals.suggest_top1.total += r.suggest_top1.total;
                totals.suggest_top3.hits += r.suggest_top3.hits;
                totals.suggest_top3.total += r.suggest_top3.total;
                for (t, n) in r.vocabulary_gaps {
                    *totals.vocabulary_gaps.entry(t).or_default() += n;
                }
            }
            if corpora.len() > 1 {
                kibitz_db::bookeval::print_report("ALL BOOKS", &totals, false);
            }
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
