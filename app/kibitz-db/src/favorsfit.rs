//! Fitting the who-stands-better weights against real games.
//!
//! The favors vote used one weight for every imbalance kind, which is
//! obviously wrong — a Minor lean in Development is not the same claim as
//! a Minor lean in Material. Guessing better numbers by hand would just
//! be a nicer guess, so this fits them.
//!
//! **Ground truth is the game result, not an engine.** A centipawn score
//! answers "who is winning if both sides play perfectly from here", and
//! that is not the question Silman's verdicts answer. "Who has the easier
//! game to play" is a practical claim, and the practical evidence is what
//! actually happened when two strong players played it out. It is a noisy
//! label at any single position and an honest one in aggregate.
//!
//! Train/holdout is fixed-seed and the holdout is scored once, per the
//! discipline established for the WSUI screen in docs/VALIDATION.md.

use anyhow::Result;
use cozy_chess::Board;
use kibitz_core::record::{Favors, ImbalanceKind, Magnitude};
use rusqlite::Connection;

/// One labelled position, reduced to what the vote actually sees.
pub struct Sample {
    /// Per-kind signed magnitude factor: + for White, - for Black.
    pub features: [i32; 8],
    /// +1 White won, -1 Black won.
    pub label: i32,
}

const KINDS: [ImbalanceKind; 8] = [
    ImbalanceKind::Material,
    ImbalanceKind::PawnStructure,
    ImbalanceKind::MinorPieces,
    ImbalanceKind::SquaresOutposts,
    ImbalanceKind::FilesDiagonals,
    ImbalanceKind::Space,
    ImbalanceKind::Development,
    ImbalanceKind::Initiative,
];

fn kind_index(k: ImbalanceKind) -> usize {
    KINDS.iter().position(|x| *x == k).unwrap_or(0)
}

/// xorshift, so a run is reproducible from its seed alone.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

/// Sample middlegame positions from decisive master games and label each
/// by who went on to win.
pub fn collect(conn: &Connection, want: usize, seed: u64) -> Result<Vec<Sample>> {
    let mut stmt = conn.prepare(
        "SELECT movetext, result FROM games
         WHERE COALESCE(white_elo,0) >= 2300 AND COALESCE(black_elo,0) >= 2300
           AND ply_count >= 40 AND start_fen IS NULL AND result IN (1, 2)",
    )?;
    let games: Vec<(Vec<u8>, i64)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<Result<_, _>>()?;
    eprintln!("decisive master games: {}", games.len());

    let mut rng = Rng(seed | 1);
    let mut order: Vec<usize> = (0..games.len()).collect();
    for i in (1..order.len()).rev() {
        order.swap(i, (rng.next() % (i as u64 + 1)) as usize);
    }

    let mut out = Vec::new();
    for gi in order {
        if out.len() >= want {
            break;
        }
        let (movetext, result) = &games[gi];
        let start = Board::default();
        let Ok(moves) = crate::movebin::decode_game(&start, movetext) else {
            continue;
        };
        if moves.len() < 30 {
            continue;
        }
        // A middlegame ply: past the book, before the technical phase.
        let span = moves.len().saturating_sub(24);
        if span == 0 {
            continue;
        }
        let ply = 20 + (rng.next() as usize % span);
        let mut b = start.clone();
        for &mv in &moves[..ply] {
            b.play(mv);
        }
        let record = kibitz_core::analyze(&b);
        let mut features = [0i32; 8];
        for imb in &record.imbalances {
            let f = match imb.magnitude {
                Magnitude::Minor => 1,
                Magnitude::Clear => 2,
                Magnitude::Winning => 4,
            };
            features[kind_index(imb.kind)] += match imb.favors {
                Favors::White => f,
                Favors::Black => -f,
                Favors::Balanced => 0,
            };
        }
        if features.iter().all(|f| *f == 0) {
            continue; // nothing to learn from a reading with no lean
        }
        out.push(Sample {
            features,
            label: if *result == 1 { 1 } else { -1 },
        });
        if out.len() % 500 == 0 {
            eprintln!("sampled {}", out.len());
        }
    }
    Ok(out)
}

fn accuracy(samples: &[Sample], w: &[i32; 8]) -> f64 {
    let hits = samples
        .iter()
        .filter(|s| {
            let lean: i32 = s.features.iter().zip(w).map(|(f, w)| f * w).sum();
            // A vote of zero picks nobody and is scored as a miss, the
            // same way the book harness scores it.
            lean.signum() == s.label
        })
        .count();
    hits as f64 / samples.len().max(1) as f64
}

/// Coordinate ascent over integer weights. Deliberately not a gradient
/// method: the objective is accuracy of a SIGN, which is not
/// differentiable, and eight coordinates over a small grid is exhaustive
/// enough to be honest about.
pub fn fit(train: &[Sample]) -> [i32; 8] {
    let mut w = [10i32; 8];
    let candidates = [0, 2, 4, 6, 8, 10, 14, 18, 24, 30];
    let mut best = accuracy(train, &w);
    for _pass in 0..6 {
        let mut improved = false;
        for i in 0..8 {
            let keep = w[i];
            let mut best_v = keep;
            for &c in &candidates {
                w[i] = c;
                let a = accuracy(train, &w);
                if a > best + 1e-9 {
                    best = a;
                    best_v = c;
                    improved = true;
                }
            }
            w[i] = best_v;
        }
        if !improved {
            break;
        }
    }
    w
}

pub fn run(conn: &Connection, want: usize, seed: u64) -> Result<()> {
    let samples = collect(conn, want, seed)?;
    if samples.len() < 100 {
        anyhow::bail!("only {} samples; need at least 100", samples.len());
    }
    // Fixed split, holdout scored once.
    let cut = samples.len() / 2;
    let (train, holdout) = samples.split_at(cut);
    let baseline = [10i32; 8];
    let fitted = fit(train);

    println!(
        "samples: {} (train {}, holdout {})",
        samples.len(),
        train.len(),
        holdout.len()
    );
    println!("\nweights");
    for (i, k) in KINDS.iter().enumerate() {
        println!(
            "  {:<16} {:>3}  (was {})",
            format!("{k:?}"),
            fitted[i],
            baseline[i]
        );
    }
    println!("\naccuracy vs game result");
    println!(
        "  train    uniform {:.1}%   fitted {:.1}%",
        accuracy(train, &baseline) * 100.0,
        accuracy(train, &fitted) * 100.0
    );
    println!(
        "  HOLDOUT  uniform {:.1}%   fitted {:.1}%",
        accuracy(holdout, &baseline) * 100.0,
        accuracy(holdout, &fitted) * 100.0
    );
    println!("\nPaste into kibitz_core::verdict::KIND_WEIGHT only if the HOLDOUT improved.");
    Ok(())
}
