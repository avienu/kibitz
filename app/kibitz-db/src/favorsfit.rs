//! Fitting the who-stands-better weights against real games.
//!
//! The favors vote used one weight for every imbalance kind, which is
//! obviously wrong — a Minor lean in Development is not the same claim as
//! a Minor lean in Material. Guessing better numbers by hand would just
//! be a nicer guess, so this fits them.
//!
//! **Ground truth is the game result, not an engine.** A centipawn score
//! answers "who is winning if both sides play perfectly from here", and
//! that is not the question Jeremy Silman's verdicts answer. "Who has the easier
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
    /// Signed lean count per (kind, magnitude band): index `2k` is the
    /// kind's Minor readings, `2k + 1` its Clear-or-better ones. Split
    /// because they turned out to be different signals — every positional
    /// detector is 6-11 points more accurate when it commits than when it
    /// shrugs, while Material is equally accurate either way.
    pub features: [i32; 16],
    /// +1 White won, -1 Black won.
    pub label: i32,
}

/// Feature index for a kind's Minor band; `+ 1` gives Clear-or-better.
fn feat(kind: ImbalanceKind, magnitude: Magnitude) -> usize {
    kind_index(kind) * 2 + usize::from(magnitude != Magnitude::Minor)
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

/// How a sampled position gets its label.
#[derive(Clone, Copy, PartialEq)]
pub enum Label {
    /// Who went on to win. A PRACTICAL signal, and the honest answer to
    /// "who has the easier game" — but it is settled thirty moves later,
    /// so it systematically under-credits slow structural advantages
    /// relative to immediate ones. Pawn structure is the slowest-acting
    /// imbalance on the board and therefore the worst served by it.
    Outcome,
    /// What an engine thinks of the POSITION. Answers a different
    /// question — is the assessment right, rather than did they win — and
    /// is the fairer yardstick for anything that pays off slowly.
    Engine,
}

/// Sample middlegame positions from decisive master games and label each
/// by who went on to win, or by what an engine makes of the position.
pub fn collect(conn: &Connection, want: usize, seed: u64) -> Result<Vec<Sample>> {
    collect_labelled(conn, want, seed, Label::Outcome, 0)
}

pub fn collect_labelled(
    conn: &Connection,
    want: usize,
    seed: u64,
    label_kind: Label,
    nodes: u64,
) -> Result<Vec<Sample>> {
    let mut engine = if label_kind == Label::Engine {
        let path = crate::engine::resolve_engine_path()
            .ok_or_else(|| anyhow::anyhow!("no engine binary found"))?;
        Some(crate::engine::Engine::spawn(&path)?)
    } else {
        None
    };
    collect_inner(conn, want, seed, label_kind, nodes, &mut engine)
}

fn collect_inner(
    conn: &Connection,
    want: usize,
    seed: u64,
    label_kind: Label,
    nodes: u64,
    engine: &mut Option<crate::engine::Engine>,
) -> Result<Vec<Sample>> {
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
        let mut features = [0i32; 16];
        for imb in &record.imbalances {
            // The magnitude ladder is no longer baked in here: whether a
            // Winning reading is worth twice a Clear one is something the
            // fit should decide, not something the features should assume.
            let sign = match imb.favors {
                Favors::White => 1,
                Favors::Black => -1,
                Favors::Balanced => 0,
            };
            features[feat(imb.kind, imb.magnitude)] += sign;
        }
        if features.iter().all(|f| *f == 0) {
            continue; // nothing to learn from a reading with no lean
        }
        let label = match label_kind {
            Label::Outcome => {
                if *result == 1 {
                    1
                } else {
                    -1
                }
            }
            Label::Engine => {
                let line = engine
                    .as_mut()
                    .expect("engine present for Label::Engine")
                    .eval_nodes(&b.to_string(), nodes)?;
                // Engine scores are from the side to move; normalise to
                // White's point of view so the label means the same thing
                // as an outcome label does.
                let cp = if b.side_to_move() == cozy_chess::Color::White {
                    line.score_cp
                } else {
                    -line.score_cp
                };
                // A position the engine calls level has no side to be
                // right about; excluding it is not cherry-picking, it is
                // declining to grade an unanswered question.
                if line.mate.is_none() && cp.abs() < 30 {
                    continue;
                }
                if cp > 0 {
                    1
                } else {
                    -1
                }
            }
        };
        out.push(Sample { features, label });
        if out.len() % 500 == 0 {
            eprintln!("sampled {}", out.len());
        }
    }
    Ok(out)
}

fn accuracy(samples: &[Sample], w: &[i32; 16]) -> f64 {
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
pub fn fit(train: &[Sample]) -> [i32; 16] {
    let mut w = [10i32; 16];
    let candidates = [0, 2, 4, 6, 8, 10, 14, 18, 24, 30];
    let mut best = accuracy(train, &w);
    for _pass in 0..6 {
        let mut improved = false;
        for i in 0..16 {
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

/// Per-kind diagnostics: how often does each detector lean at all, and
/// when it does, is it right?
///
/// A fitted weight tells you what the vote should DO with a detector; it
/// does not tell you why. A kind can end up near zero for three quite
/// different reasons — it rarely leans, it leans at chance, or it leans
/// backwards — and only the third is a bug in the detector. This
/// separates them.
fn diagnose(samples: &[Sample]) {
    println!("\nper-kind diagnostics (n = {})", samples.len());
    println!(
        "  {:<16} {:>8} {:>10} {:>10}",
        "kind", "leans%", "correct%", "mean|mag|"
    );
    for (i, k) in KINDS.iter().enumerate() {
        let lean_of = |s: &Sample| s.features[i * 2] + s.features[i * 2 + 1];
        let leaning: Vec<&Sample> = samples.iter().filter(|s| lean_of(s) != 0).collect();
        if leaning.is_empty() {
            println!(
                "  {:<16} {:>8} {:>10} {:>10}",
                format!("{k:?}"),
                "0.0%",
                "—",
                "—"
            );
            continue;
        }
        let correct = leaning
            .iter()
            .filter(|s| lean_of(s).signum() == s.label)
            .count();
        let mean_mag: f64 =
            leaning.iter().map(|s| lean_of(s).abs() as f64).sum::<f64>() / leaning.len() as f64;
        println!(
            "  {:<16} {:>7.1}% {:>9.1}% {:>10.2}",
            format!("{k:?}"),
            leaning.len() as f64 / samples.len() as f64 * 100.0,
            correct as f64 / leaning.len() as f64 * 100.0,
            mean_mag
        );
    }
    // The decisive split. A detector whose Clear/Winning readings are much
    // sharper than its Minor ones has a CALIBRATION problem, not a
    // direction problem: it is right when it commits and noisy when it
    // shrugs, and the fix is to stop shrugging out loud.
    println!("\ncorrect% by magnitude (Minor / Clear+ )");
    for (i, k) in KINDS.iter().enumerate() {
        let acc = |band: usize| -> String {
            let v: Vec<&Sample> = samples
                .iter()
                .filter(|s| s.features[i * 2 + band] != 0)
                .collect();
            if v.len() < 30 {
                return format!("{:>12}", "—");
            }
            let c = v
                .iter()
                .filter(|s| s.features[i * 2 + band].signum() == s.label)
                .count();
            format!(
                "{:>7.1}% (n={})",
                c as f64 / v.len() as f64 * 100.0,
                v.len()
            )
        };
        println!("  {:<16} {}   {}", format!("{k:?}"), acc(0), acc(1));
    }

    println!("\n  leans%   = how often the detector picks a side at all");
    println!("  correct% = of those, how often it picked the winner");
    println!("             50% is a coin flip; BELOW 50% means it is inverted,");
    println!("             which is a bug in the detector, not in the vote.");
}

/// Same eight per-kind weights, plus ONE shared multiplier for
/// Clear-or-better readings.
///
/// The free 16-weight fit gained nothing on holdout (63.3% vs 63.1%) and
/// produced weights that contradicted the diagnostics — Development at
/// 30/30 on a detector that leans 4.5% of the time is coordinate ascent
/// fitting noise. Nine parameters expresses the same real finding (a
/// committed reading is worth more than a shrug) without inventing eight
/// independent estimates the data cannot support.
fn fit_shared_ladder(train: &[Sample]) -> ([i32; 16], i32) {
    let mut best = ([0i32; 16], 10, 0.0f64);
    for mult in [10, 15, 20, 25, 30, 40] {
        let mut kind_w = [10i32; 8];
        let expand = |kw: &[i32; 8], m: i32| {
            let mut w = [0i32; 16];
            for i in 0..8 {
                w[i * 2] = kw[i];
                w[i * 2 + 1] = kw[i] * m / 10;
            }
            w
        };
        let mut acc = accuracy(train, &expand(&kind_w, mult));
        for _pass in 0..6 {
            let mut improved = false;
            for i in 0..8 {
                let mut best_v = kind_w[i];
                for c in [0, 2, 4, 6, 8, 10, 14, 18, 24, 30] {
                    kind_w[i] = c;
                    let a = accuracy(train, &expand(&kind_w, mult));
                    if a > acc + 1e-9 {
                        acc = a;
                        best_v = c;
                        improved = true;
                    }
                }
                kind_w[i] = best_v;
            }
            if !improved {
                break;
            }
        }
        if acc > best.2 {
            best = (expand(&kind_w, mult), mult, acc);
        }
    }
    (best.0, best.1)
}

pub fn run(conn: &Connection, want: usize, seed: u64) -> Result<()> {
    run_labelled(conn, want, seed, Label::Outcome, 0)
}

pub fn run_labelled(
    conn: &Connection,
    want: usize,
    seed: u64,
    label_kind: Label,
    nodes: u64,
) -> Result<()> {
    let samples = collect_labelled(conn, want, seed, label_kind, nodes)?;
    if samples.len() < 100 {
        anyhow::bail!("only {} samples; need at least 100", samples.len());
    }
    // Fixed split, holdout scored once.
    let cut = samples.len() / 2;
    let (train, holdout) = samples.split_at(cut);
    // Baseline reproduces the shipped ladder: Minor 1, Clear+ 2, times a
    // uniform per-kind 10 — so "fitted" is measured against what the vote
    // did before, not against a straw man.
    let mut baseline = [0i32; 16];
    for i in 0..8 {
        baseline[i * 2] = 10;
        baseline[i * 2 + 1] = 20;
    }
    let fitted_free = fit(train);
    let (fitted, mult) = fit_shared_ladder(train);

    println!(
        "samples: {} (train {}, holdout {})",
        samples.len(),
        train.len(),
        holdout.len()
    );
    println!("\nweights (minor / clear+)");
    for (i, k) in KINDS.iter().enumerate() {
        println!(
            "  {:<16} {:>3} / {:<3}   (was {} / {})",
            format!("{k:?}"),
            fitted[i * 2],
            fitted[i * 2 + 1],
            baseline[i * 2],
            baseline[i * 2 + 1]
        );
    }
    println!("\naccuracy vs game result");
    println!(
        "  train    uniform {:.1}%   fitted {:.1}%",
        accuracy(train, &baseline) * 100.0,
        accuracy(train, &fitted) * 100.0
    );
    println!(
        "  HOLDOUT  uniform {:.1}%   free-16 {:.1}%   shared-ladder {:.1}%  (clear+ x{:.1})",
        accuracy(holdout, &baseline) * 100.0,
        accuracy(holdout, &fitted_free) * 100.0,
        accuracy(holdout, &fitted) * 100.0,
        mult as f64 / 10.0
    );
    diagnose(&samples);
    println!("\nPaste into kibitz_core::verdict::KIND_WEIGHT only if the HOLDOUT improved.");
    Ok(())
}
