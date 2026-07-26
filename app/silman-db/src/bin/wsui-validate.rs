//! WSUI validation harness (docs/SILMAN_ENGINE_SPEC.md, validation plan).
//!
//! Positives: Lichess puzzle positions (CC0) — the position AFTER applying
//! the setup move from the puzzle CSV, i.e. the moment a tactic exists.
//! Negatives: engine-quiet positions (|eval| < 50 cp) sampled from
//! imported master games.
//!
//! The set is shuffled deterministically and split train/holdout; any
//! threshold tuning happens on the train half, and ONLY holdout numbers
//! are reported (and recorded in docs/VALIDATION.md).

use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;

use clap::Parser;
use cozy_chess::Board;
use silman_core::record::Severity;
use silman_core::wsui::{screen, WsuiConfig};

#[derive(Parser)]
struct Args {
    /// Lichess puzzle CSV (full dump or the committed fixture subset).
    #[arg(long)]
    puzzles: PathBuf,
    /// Quiet-position FEN list (one per line), built by `--build-quiet`.
    #[arg(long)]
    quiet: PathBuf,
    /// Cap on positions drawn from each class.
    #[arg(long, default_value_t = 2000)]
    per_class: usize,
    /// Deterministic shuffle seed.
    #[arg(long, default_value_t = 0xC0FFEE)]
    seed: u64,
    /// Instead of validating, build the quiet set: sample from this db.
    #[arg(long)]
    build_quiet_from: Option<PathBuf>,
    /// Engine node budget for quiet filtering.
    #[arg(long, default_value_t = 250_000)]
    quiet_nodes: u64,
    /// Emit a puzzle fixture subset of this size to stdout and exit.
    #[arg(long)]
    emit_fixture: Option<usize>,
}

/// Minimal deterministic RNG (xorshift64*) — no dependency needed.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545F4914F6CDD1D)
    }
    fn shuffle<T>(&mut self, v: &mut [T]) {
        for i in (1..v.len()).rev() {
            let j = (self.next() % (i as u64 + 1)) as usize;
            v.swap(i, j);
        }
    }
}

/// Parse one puzzle CSV line into the position where the tactic exists.
fn puzzle_position(line: &str) -> Option<Board> {
    // PuzzleId,FEN,Moves,...  (no quoted commas in these columns)
    let mut cols = line.split(',');
    let _id = cols.next()?;
    let fen = cols.next()?;
    let moves = cols.next()?;
    let mut board: Board = fen.parse().ok()?;
    let setup = moves.split_whitespace().next()?;
    let mv: cozy_chess::Move = setup.parse().ok()?;
    board.try_play(mv).ok()?;
    Some(board)
}

fn fired(board: &Board, cfg: &WsuiConfig) -> bool {
    screen(board, cfg).screen_fired
}

fn rate(hits: usize, total: usize) -> f64 {
    if total == 0 {
        0.0
    } else {
        hits as f64 / total as f64 * 100.0
    }
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    if let Some(n) = args.emit_fixture {
        // Deterministic sample of the big CSV for the committed fixture.
        let f = std::fs::File::open(&args.puzzles)?;
        let mut rng = Rng(args.seed | 1);
        let mut kept: Vec<String> = Vec::new();
        for (i, line) in BufReader::new(f).lines().enumerate() {
            let line = line?;
            if i == 0 {
                continue;
            }
            // Reservoir sample.
            if kept.len() < n {
                kept.push(line);
            } else {
                let j = (rng.next() % (i as u64)) as usize;
                if j < n {
                    kept[j] = line;
                }
            }
        }
        let mut out = std::io::stdout().lock();
        for l in kept {
            writeln!(out, "{l}")?;
        }
        return Ok(());
    }

    if let Some(db_path) = &args.build_quiet_from {
        return build_quiet(&args, db_path);
    }

    // Load positives.
    let f = std::fs::File::open(&args.puzzles)?;
    let mut positives: Vec<Board> = Vec::new();
    for (i, line) in BufReader::new(f).lines().enumerate() {
        let line = line?;
        if i == 0 && line.starts_with("PuzzleId") {
            continue;
        }
        if let Some(b) = puzzle_position(&line) {
            positives.push(b);
        }
    }
    // Load negatives.
    let f = std::fs::File::open(&args.quiet)?;
    let mut negatives: Vec<Board> = Vec::new();
    for line in BufReader::new(f).lines() {
        let line = line?;
        if let Ok(b) = line.trim().parse() {
            negatives.push(b);
        }
    }

    let mut rng = Rng(args.seed | 1);
    rng.shuffle(&mut positives);
    rng.shuffle(&mut negatives);
    positives.truncate(args.per_class);
    negatives.truncate(args.per_class);

    let (pos_train, pos_hold) = positives.split_at(positives.len() / 2);
    let (neg_train, neg_hold) = negatives.split_at(negatives.len() / 2);

    // Threshold grid, tuned on TRAIN only.
    let mut candidates = Vec::new();
    for fire in [Severity::Low, Severity::Medium, Severity::High] {
        for (see_med, see_high) in [(60, 200), (100, 300), (150, 400)] {
            candidates.push(WsuiConfig {
                fire_threshold: fire,
                see_medium: see_med,
                see_high,
                king_zone_surplus: 2,
            });
        }
    }
    let mut best: Option<(f64, WsuiConfig, f64, f64)> = None;
    for cfg in candidates {
        let recall = rate(
            pos_train.iter().filter(|b| fired(b, &cfg)).count(),
            pos_train.len(),
        );
        let fp = rate(
            neg_train.iter().filter(|b| fired(b, &cfg)).count(),
            neg_train.len(),
        );
        // Balanced objective: maximize recall − false-positive rate.
        let objective = recall - fp;
        eprintln!(
            "train: fire>={:?} see {}/{} -> recall {recall:.1}% fp {fp:.1}% (obj {objective:.1})",
            cfg.fire_threshold, cfg.see_medium, cfg.see_high
        );
        if best.as_ref().map(|(o, ..)| objective > *o).unwrap_or(true) {
            best = Some((objective, cfg, recall, fp));
        }
    }
    let (_, cfg, train_recall, train_fp) = best.expect("candidates nonempty");

    // Holdout report.
    let tp = pos_hold.iter().filter(|b| fired(b, &cfg)).count();
    let fp = neg_hold.iter().filter(|b| fired(b, &cfg)).count();
    let recall = rate(tp, pos_hold.len());
    let fp_rate = rate(fp, neg_hold.len());
    let precision = if tp + fp == 0 {
        0.0
    } else {
        tp as f64 / (tp + fp) as f64 * 100.0
    };
    println!(
        "chosen config (tuned on train): fire>={:?} see_medium={} see_high={} zone={}",
        cfg.fire_threshold, cfg.see_medium, cfg.see_high, cfg.king_zone_surplus
    );
    println!(
        "train:   recall {train_recall:.1}%  false-positive rate {train_fp:.1}%  (n={}+{})",
        pos_train.len(),
        neg_train.len()
    );
    println!(
        "holdout: recall {recall:.1}%  false-positive rate {fp_rate:.1}%  precision {precision:.1}%  (n={}+{})",
        pos_hold.len(),
        neg_hold.len()
    );
    Ok(())
}

/// Sample candidate positions from master games in the db, keep the
/// engine-quiet ones, print FENs to stdout.
fn build_quiet(args: &Args, db_path: &std::path::Path) -> anyhow::Result<()> {
    use silman_db::engine::{resolve_engine_path, Engine};
    let conn = silman_db::db::open(db_path)?;
    let engine_path =
        resolve_engine_path().ok_or_else(|| anyhow::anyhow!("no engine binary found"))?;
    let mut engine = Engine::spawn(&engine_path)?;

    let mut stmt = conn.prepare(
        "SELECT movetext, start_fen FROM games
         WHERE COALESCE(white_elo,0) >= 2300 AND COALESCE(black_elo,0) >= 2300
           AND ply_count >= 40 AND start_fen IS NULL",
    )?;
    let games: Vec<(Vec<u8>, Option<String>)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<Result<_, _>>()?;
    eprintln!("candidate games: {}", games.len());

    let mut rng = Rng(args.seed | 1);
    let mut out = std::io::stdout().lock();
    let mut emitted = 0usize;
    let mut order: Vec<usize> = (0..games.len()).collect();
    rng.shuffle(&mut order);
    for gi in order {
        if emitted >= args.per_class {
            break;
        }
        let (movetext, _) = &games[gi];
        let start = Board::default();
        let Ok(moves) = silman_db::movebin::decode_game(&start, movetext) else {
            continue;
        };
        if moves.len() < 30 {
            continue;
        }
        // One random middlegame ply per game.
        let ply = 16 + (rng.next() as usize % (moves.len().saturating_sub(20)));
        let mut b = start.clone();
        for &mv in &moves[..ply] {
            b.play(mv);
        }
        let fen = b.to_string();
        let line = engine.eval_nodes(&fen, args.quiet_nodes)?;
        if line.mate.is_none() && line.score_cp.abs() < 50 {
            writeln!(out, "{fen}")?;
            emitted += 1;
            if emitted % 100 == 0 {
                eprintln!("quiet: {emitted}");
            }
        }
    }
    eprintln!("emitted {emitted} quiet positions");
    Ok(())
}
