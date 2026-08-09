//! Book-trial evaluation (run 8.5): score the static analyzer against a
//! PRIVATE corpus of positions transcribed from the maintainer's own
//! chess books (testdata/private/book-trials/*.json, git-ignored).
//!
//! Each corpus entry carries a FEN plus expectations expressed in OUR
//! vocabulary (ImbalanceKind names, PlanHint tokens, AlertKind names) —
//! never book prose. The harness reports recall per axis and surfaces
//! "vocabulary gaps": free-form expectation tags our engine has no hint
//! for yet. No engine, no network — pure static analysis.

use std::collections::BTreeMap;
use std::path::Path;

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Corpus {
    pub book: String,
    #[serde(default)]
    pub edition: String,
    pub positions: Vec<Entry>,
}

#[derive(Debug, Deserialize)]
pub struct Entry {
    pub id: String,
    #[serde(default)]
    pub citation: String,
    pub fen: String,
    /// Optional SAN move list from the standard start reaching `fen`
    /// (run 11): principle entries reconstruct exactly, and the
    /// development tracker needs the history for its wandering/tempo
    /// observations. A list that fails to replay to `fen` is ignored
    /// (the tracker then sees the bare position).
    #[serde(default)]
    pub sans: Vec<String>,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub confidence: String,
    #[serde(default)]
    pub expected: Expected,
    #[serde(default)]
    pub not_expected: NotExpected,
}

/// Negative assertions: things the analyzer must NOT claim here. The
/// counter-example positions (a "useful" doubled pawn, an open file with
/// no entry squares...) anchor precision while thresholds chase recall.
#[derive(Debug, Default, Deserialize)]
pub struct NotExpected {
    #[serde(default)]
    pub imbalances: Vec<String>,
    #[serde(default)]
    pub plan_tags: Vec<String>,
    /// Alert kinds this position must NOT produce (run 12, #11 follow-up).
    ///
    /// The alerts axis had no corpus-side negatives at all: `alerts_fp`
    /// over 500 engine-quiet master positions was its only cost term. That
    /// set is the honest one — nobody chose it with these detectors in
    /// mind, so it can surprise you — but it is anonymous. A ban written
    /// against a transcribed position says WHICH claim would be wrong and
    /// WHY, and it fails loudly in book-eval rather than moving a rate by
    /// a fraction of a point. Only one of the two can surprise you; both
    /// are worth having.
    #[serde(default)]
    pub alerts: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct Expected {
    #[serde(default)]
    pub favors: Option<String>,
    #[serde(default)]
    pub imbalances: Vec<String>,
    #[serde(default)]
    pub plan_tags: Vec<String>,
    #[serde(default)]
    pub alerts: Vec<String>,
    #[serde(default)]
    pub theme: String,
    /// Book-given best move(s) in SAN, when the transcription carried
    /// them — scored against kibitz-core::suggest (run 10).
    #[serde(default)]
    pub best_moves: Vec<String>,
    /// Expectations that hold only after a CONTINUATION is played — real
    /// chess content about a derived position, not this one (run 12).
    ///
    /// Three corpus entries expected TrappedPiece for a piece that gets
    /// snared several moves into a line the book is warning against. No
    /// static detector can see those from the diagram, and neither can an
    /// engine that has not played the bad move — so scoring them at
    /// position level punished the analyzer for not hallucinating. They
    /// are excluded from position-level scoring and reported separately;
    /// they become scorable when a suggest-then-verify harness can walk
    /// `line` and screen the resulting position.
    #[serde(default)]
    pub line_conditional: Vec<LineConditional>,
}

/// One line-gated expectation: play `line` from the entry's FEN, then the
/// `alerts` are expected to hold in the resulting position. An empty
/// `line` means the SAN has not been transcribed yet (needs the book) —
/// still excluded from scoring, still counted, flagged in the report.
#[derive(Debug, Default, Deserialize)]
pub struct LineConditional {
    #[serde(default)]
    pub line: Vec<String>,
    #[serde(default)]
    pub alerts: Vec<String>,
    #[serde(default)]
    pub note: String,
}

/// Known PlanHint tokens the engine can emit today. Expected plan tags
/// outside this list are vocabulary gaps, reported rather than scored.
/// Does Jeremy Silman recommend denial or construction, and when?
///
/// The engine gives a prophylactic move a bonus that can outrank
/// executing your own plan, and whether that ranking is right looked like
/// a design argument. It is not: the corpus carries the author's own
/// recommendation for every entry with `best_moves`, so the distribution
/// is measurable. The maintainer's hypothesis is that his prophylactic
/// picks cluster where the OPPONENT's plan is faster — which would make
/// the fix a tempo term rather than a weight.
pub fn prophylaxis_study(corpora: &[Corpus]) -> anyhow::Result<()> {
    println!(
        "{:<16} {:<8} {:<14} {:>8} {:>8} {:>9} {:>9}",
        "entry", "move", "role", "own str", "opp str", "own hzn", "opp hzn"
    );
    let (mut denial, mut construct, mut both, mut neither) = (0, 0, 0, 0);
    let mut denial_opp_faster = 0;
    let mut construct_opp_faster = 0;

    for corpus in corpora {
        for e in &corpus.positions {
            let Some(want) = e.expected.best_moves.first() else {
                continue;
            };
            let Ok(board) = e.fen.parse::<cozy_chess::Board>() else {
                continue;
            };
            let record = kibitz_core::analyze(&board);
            // Find the legal move whose SAN matches the book's.
            let mut found = None;
            board.generate_moves(|pm| {
                for mv in pm {
                    if kibitz_core::suggest::san(&board, mv) == *want {
                        found = Some(mv);
                        return true;
                    }
                }
                false
            });
            let Some(mv) = found else { continue };
            let r = kibitz_core::suggest::role_of(&record, &board, mv);
            let role = match (r.constructive.is_empty(), r.blocking.is_empty()) {
                (false, false) => "both",
                (false, true) => "constructive",
                (true, false) => "denial",
                (true, true) => "neither",
            };
            match role {
                "denial" => denial += 1,
                "constructive" => construct += 1,
                "both" => both += 1,
                _ => neither += 1,
            }
            // "Faster" means their cheapest scheme arrives sooner than ours,
            // or they have a plan on this square and we have none.
            let opp_faster = match (r.own_horizon, r.opp_horizon) {
                (Some(o), Some(t)) => t < o,
                (None, Some(_)) => true,
                _ => false,
            };
            if opp_faster {
                match role {
                    "denial" | "both" => denial_opp_faster += 1,
                    "constructive" => construct_opp_faster += 1,
                    _ => {}
                }
            }
            println!(
                "{:<16} {:<8} {:<14} {:>8} {:>8} {:>9} {:>9}",
                e.id,
                want,
                role,
                r.own_strength,
                r.opp_strength,
                r.own_horizon.map(|h| h.to_string()).unwrap_or("—".into()),
                r.opp_horizon.map(|h| h.to_string()).unwrap_or("—".into()),
            );
        }
    }
    println!(
        "\nclassified: {construct} constructive, {denial} denial, {both} both, \
{neither} neither (the engine recognises no role for those)"
    );
    println!(
        "of the denial/both picks, {denial_opp_faster} sit where the opponent's plan is FASTER"
    );
    println!("of the constructive picks, {construct_opp_faster} do");
    Ok(())
}

/// Why does the alerts axis sit at 31.2%, and can it move?
///
/// The number is uninterpretable on its own. A missed alert falls into
/// one of three quite different buckets and only one of them is a bug:
///
///   * ENGINE-OFF COST — the screen correctly declined to fire, and
///     seeing the alert needs a search. That is the stated product
///     principle, not a defect, and it is a ceiling rather than a gap.
///   * SILENT, SCREEN FIRED — the expected detector produced nothing, and
///     other detectors fired the screen anyway, so the engine WAS
///     consulted. Named precisely because the earlier label "screen
///     defect" was wrong: the screen behaved correctly. Three readings
///     were possible — no alert downstream of a consultation, an alert of
///     the wrong kind, or the right alert sorted out of view — and
///     screen_trace settles it: the detector reported zero. The screen
///     neither truncates nor suppresses, so nothing was sorted away.
///   * STATIC GAP — the screen did not fire and the expected alert is a
///     structural feature (a trapped piece, a thin king shelter) that a
///     detector could catch with the engine off.
pub fn alerts_study(corpora: &[Corpus]) -> anyhow::Result<()> {
    println!(
        "{:<20} {:<20} {:<7} {:<16} {:<7} we produced",
        "entry", "expected alert", "screen", "bucket", "det"
    );
    let (mut cost, mut defect, mut gap) = (0, 0, 0);
    for corpus in corpora {
        for e in &corpus.positions {
            let Ok(board) = e.fen.parse::<cozy_chess::Board>() else {
                continue;
            };
            let record = kibitz_core::analyze(&board);
            let got: Vec<String> = record
                .wsui
                .alerts
                .iter()
                .map(|a| format!("{:?}", a.kind))
                .collect();
            // Pre-arbitration counts. The screen does not arbitrate — all
            // three detectors append and it only sorts — so this
            // distinguishes "fired and lost" from "never fired", which
            // have opposite fixes.
            let trace =
                kibitz_core::wsui::screen_trace(&board, &kibitz_core::wsui::WsuiConfig::default());
            for want in &e.expected.alerts {
                if got.iter().any(|k| k == want) {
                    continue; // a hit, not a miss
                }
                let fired = record.wsui.screen_fired;
                // Structural kinds a static detector can own outright.
                let structural = want == "TrappedPiece" || want == "WeakKing";
                let bucket = if fired {
                    defect += 1;
                    "silent, screen fired"
                } else if structural {
                    gap += 1;
                    "static gap"
                } else {
                    cost += 1;
                    "engine-off cost"
                };
                let n = match want.as_str() {
                    "TrappedPiece" => trace.trapped,
                    "WeakKing" => trace.weak_king,
                    "Undefended" => trace.undefended,
                    _ => trace.inadequate,
                };
                println!(
                    "{:<20} {:<20} {:<7} {:<16} det={:<3} {}{}",
                    e.id,
                    want,
                    if fired { "fired" } else { "quiet" },
                    bucket,
                    n,
                    if trace.trapped_skipped.is_empty() {
                        ""
                    } else {
                        "TRAPPED-SKIPPED "
                    },
                    if got.is_empty() {
                        "nothing".to_string()
                    } else {
                        got.join(",")
                    }
                );
            }
        }
    }
    let total = cost + defect + gap;
    println!("\nmissed alerts: {total}");
    println!("  engine-off cost  {cost:>3}  screen correctly quiet; seeing it needs a search");
    println!("  static gap       {gap:>3}  screen quiet, but the feature is structural");
    println!(
        "  silent, screen fired {defect:>3}  the expected detector produced nothing; OTHER \
detectors fired the screen, so the engine was consulted anyway"
    );
    println!(
        "\nAll {} non-engine-off misses share one cause: the expected detector reported \
zero (screen_trace). They differ only in consequence — whether other evidence was \
enough to fire the screen.",
        defect + gap
    );
    Ok(())
}

/// Three candidate gates for enqueueing a bounded suggest-verify job,
/// over the same corpus. The deciding column is plans-per-job: coverage
/// bought per unit of engine time.
/// False-positive side of the alerts axis.
///
/// The book corpus scores alerts on RECALL only — its 14 negative
/// anchors cover imbalances and plans, and there are none for alerts. So
/// a sensitivity change that recovers five expectations and simultaneously
/// starts alerting on healthy positions scores as a clean +5, which is
/// how a gain and a regression come to look identical.
///
/// This is the denominator: engine-quiet positions from master games
/// (both sides 2300+, |eval| < 50cp at 200k nodes — the set built for the
/// WSUI validation). Nothing here is a tactic, so every alert is a cost
/// and every screen firing buys an engine job for nothing.
pub fn alerts_fp(path: &std::path::Path) -> anyhow::Result<()> {
    let text = std::fs::read_to_string(path)?;
    let fens: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    let cfg = kibitz_core::wsui::WsuiConfig::default();
    let (mut fired, mut weak_king, mut trapped, mut undef, mut inadeq) = (0, 0, 0, 0, 0);
    let mut n = 0;
    for fen in &fens {
        let Ok(board) = fen.parse::<cozy_chess::Board>() else {
            continue;
        };
        n += 1;
        let r = kibitz_core::wsui::screen(&board, &cfg);
        if r.screen_fired {
            fired += 1;
        }
        for a in &r.alerts {
            match a.kind {
                kibitz_core::record::AlertKind::WeakKing => weak_king += 1,
                kibitz_core::record::AlertKind::TrappedPiece => trapped += 1,
                kibitz_core::record::AlertKind::Undefended => undef += 1,
                kibitz_core::record::AlertKind::InadequatelyDefended => inadeq += 1,
            }
        }
    }
    let pct = |k: usize| k as f64 / n.max(1) as f64 * 100.0;
    println!("engine-quiet master positions: {n}");
    println!("  screen fires        {fired:>5}  ({:.1}%)", pct(fired));
    println!(
        "  WeakKing alerts     {weak_king:>5}  ({:.2} per position)",
        weak_king as f64 / n.max(1) as f64
    );
    println!(
        "  TrappedPiece        {trapped:>5}  ({:.2} per position)",
        trapped as f64 / n.max(1) as f64
    );
    println!("  Undefended          {undef:>5}");
    println!("  InadequatelyDefended{inadeq:>5}");
    println!("\nNothing here is a tactic. Every alert is a cost and every firing buys an engine job for nothing.");
    Ok(())
}

/// The same denominator, applied to the entombed-piece imbalance (#12).
///
/// Entombment costs no engine time — it is an imbalance, not a screen
/// alert — so the currency here is different: every firing on a healthy
/// master position is a false STATEMENT in the coach prose and a wrong
/// discount in the material ledger, which moves who the app says is
/// better. Same discipline as `alerts_fp` all the same: measure the cost
/// term before tuning the detector, not after.
pub fn entomb_fp(path: &std::path::Path, dump: bool) -> anyhow::Result<()> {
    use cozy_chess::{Color, Piece};
    let text = std::fs::read_to_string(path)?;
    let mut n = 0usize;
    let mut positions_firing = 0usize;
    let mut pieces = 0usize;
    let mut by_piece: BTreeMap<&'static str, usize> = BTreeMap::new();
    for fen in text.lines().filter(|l| !l.trim().is_empty()) {
        let Ok(board) = fen.parse::<cozy_chess::Board>() else {
            continue;
        };
        n += 1;
        let found: Vec<_> = [Color::White, Color::Black]
            .iter()
            .flat_map(|c| kibitz_core::entomb::entombed(&board, *c))
            .collect();
        if found.is_empty() {
            continue;
        }
        positions_firing += 1;
        pieces += found.len();
        if dump {
            println!("{fen}");
        }
        for e in found {
            if dump {
                println!("   {:?} on {}", e.piece, e.square);
            }
            let name = match e.piece {
                Piece::Knight => "knight",
                Piece::Bishop => "bishop",
                Piece::Rook => "rook",
                Piece::Queen => "queen",
                _ => "other",
            };
            *by_piece.entry(name).or_default() += 1;
        }
    }
    println!("engine-quiet master positions: {n}");
    println!(
        "  positions with an entombed piece {positions_firing:>5}  ({:.1}%)",
        positions_firing as f64 / n.max(1) as f64 * 100.0
    );
    println!(
        "  entombed pieces                  {pieces:>5}  ({:.3} per position)",
        pieces as f64 / n.max(1) as f64
    );
    for (k, v) in &by_piece {
        println!("    {k:<8} {v:>5}");
    }
    println!(
        "\nNothing here is in trouble. Every firing is a false claim in the prose \
         and a wrong discount in the material ledger."
    );
    Ok(())
}

/// A candidate enqueue rule: does this position deserve a bounded
/// suggest-verify job?
type Gate = fn(&kibitz_core::record::FeatureRecord, kibitz_core::record::Favors) -> bool;

pub fn gate_study(corpora: &[Corpus]) -> anyhow::Result<()> {
    let gates: [(&str, Gate); 3] = [
        ("any plan", |r, _| !r.composite_plans.is_empty()),
        ("for side to move", |r, stm| {
            r.composite_plans
                .iter()
                .any(|c| c.favors == stm || c.favors == kibitz_core::record::Favors::Balanced)
        }),
        ("converging (>=2 supports)", |r, stm| {
            r.composite_plans.iter().any(|c| {
                (c.favors == stm || c.favors == kibitz_core::record::Favors::Balanced)
                    && c.supporting.len() >= 2
            })
        }),
    ];
    println!(
        "{:<28} {:>6} {:>8} {:>14}",
        "gate", "jobs", "plans", "plans / job"
    );
    for (name, gate) in gates {
        let (mut jobs, mut plans) = (0usize, 0usize);
        for corpus in corpora {
            for e in &corpus.positions {
                let Ok(board) = e.fen.parse::<cozy_chess::Board>() else {
                    continue;
                };
                let record = kibitz_core::analyze(&board);
                let stm = match board.side_to_move() {
                    cozy_chess::Color::White => kibitz_core::record::Favors::White,
                    cozy_chess::Color::Black => kibitz_core::record::Favors::Black,
                };
                if gate(&record, stm) {
                    jobs += 1;
                    // What the user gets for that job: the plans the
                    // suggestions would be serving.
                    plans += record
                        .composite_plans
                        .iter()
                        .filter(|c| {
                            c.favors == stm || c.favors == kibitz_core::record::Favors::Balanced
                        })
                        .count();
                }
            }
        }
        println!(
            "{:<28} {:>6} {:>8} {:>14.2}",
            name,
            jobs,
            plans,
            plans as f64 / jobs.max(1) as f64
        );
    }
    Ok(())
}

/// Plan token a `Maneuver::reason` stands for, where it has one.
fn maneuver_token(reason: &str) -> Option<&'static str> {
    match reason {
        "opposition" => Some("TakeOpposition"),
        _ => None,
    }
}

const KNOWN_HINTS: &[&str] = &[
    "ManeuverKnightToOutpost",
    "ManeuverBishopToSupportPoint",
    "ManeuverRookToOpenFile",
    "UndermineDefender",
    "OverprotectStrongPoint",
    "TakeOpposition",
    "CreatePassedPawn",
    "HuntBishopPair",
    "TradeSquareDefender",
    "KeepBestPiece",
    "TradeOffAttacker",
    "TargetWeakPawn",
    "PressureBackwardPawn",
    "BlockadeWhitePasser",
    "BlockadeBlackPasser",
    "AdvanceQueensideMajority",
    "DoubleOnOpenFile",
    "BlockadeThenPressure",
    "KeepPositionClosed",
    "OpenPositionForBishops",
    "OpenPositionBeforeOpponentCompletes",
    "UseSpaceAvoidExchanges",
    // Run 8.5 vocabulary additions (book-trial tuning).
    "WingPawnStormClosedCenter",
    "MinorityAttack",
    "RookToSeventh",
    "RookBehindPasser",
    "PressureDoubledPawn",
    "TradeOrActivateBadBishop",
    "ActivateKingInEndgame",
    "RestrictKnight",
    "AdvanceCentralMajority",
    "OpenLinesTowardWeakKing",
    // Run 11: development-prior vocabulary (kibitz-core::development).
    "CompleteDevelopment",
    "CastleIntoSafety",
    "ClaimTheCenter",
    "QueenAheadOfHerArmy",
    "SamePieceWandering",
    // Run 12 (#12): entombment, an imbalance rather than an alert.
    "ActivateEntombedPiece",
    "KeepPieceEntombed",
    // Run 12, ruling 3: the blockade B2 pair from the Nimzowitsch ch. 4
    // inventory (mechanism 10; mechanism 7 is evidence, not a hint).
    "UprootBlockader",
    "OutsidePasserDecoy",
];

#[derive(Debug, Default)]
pub struct AxisScore {
    pub hits: u32,
    pub total: u32,
}

impl AxisScore {
    fn pct(&self) -> f64 {
        if self.total == 0 {
            100.0
        } else {
            100.0 * f64::from(self.hits) / f64::from(self.total)
        }
    }
}

#[derive(Debug, Default)]
pub struct Report {
    pub positions: u32,
    pub bad_fens: Vec<(String, String)>,
    pub imbalance: AxisScore,
    pub plans: AxisScore,
    pub alerts: AxisScore,
    pub favors: AxisScore,
    /// Suggestion hit-rate (run 10): the TOP suggestion matches one of
    /// the book's best moves / ANY of the top three does. One check per
    /// entry that carries best_moves.
    pub suggest_top1: AxisScore,
    pub suggest_top3: AxisScore,
    /// expectation tag -> occurrences, for tags the engine cannot emit.
    pub vocabulary_gaps: BTreeMap<String, u32>,
    /// (entry id, axis, expected item) for every miss.
    pub misses: Vec<(String, &'static str, String)>,
    /// Precision: negative assertions that FIRED anyway.
    pub false_fires: Vec<(String, &'static str, String)>,
    pub negative_checks: u32,
    /// Line-conditional expectations seen: (entry id, alert, line transcribed?).
    /// Counted and reported, never scored — see [`LineConditional`].
    pub line_conditional: Vec<(String, String, bool)>,
}

pub fn eval_corpus(corpus: &Corpus) -> Report {
    let mut r = Report::default();
    for e in &corpus.positions {
        let Ok(board) = e.fen.parse::<cozy_chess::Board>() else {
            r.bad_fens.push((e.id.clone(), e.fen.clone()));
            continue;
        };
        r.positions += 1;
        let record = kibitz_core::analyze(&board);

        // Development prior (run 11): fed with the entry's replayed
        // history when it reaches the FEN, else with the bare position.
        // Its hints/kind extend detection only — the favors vote stays
        // untouched (a to-do list is not an advantage lean).
        let development = {
            let start = cozy_chess::Board::default();
            let mut replay = start.clone();
            let mut moves: Vec<cozy_chess::Move> = Vec::new();
            for san in &e.sans {
                let Ok(mv) = crate::san::parse_san(&replay, san) else {
                    moves.clear();
                    break;
                };
                replay.play(mv);
                moves.push(mv);
            }
            if !moves.is_empty() && replay.same_position(&board) {
                kibitz_core::development::track(&start, &moves)
            } else {
                kibitz_core::development::track(&board, &[])
            }
        };
        let development = kibitz_core::development::imbalances(&development);

        let mut detected_kinds: Vec<String> = record
            .imbalances
            .iter()
            .map(|i| format!("{:?}", i.kind))
            .collect();
        if !development.is_empty() {
            detected_kinds.push("Development".to_string());
        }
        for want in &e.expected.imbalances {
            r.imbalance.total += 1;
            if detected_kinds.iter().any(|k| k == want) {
                r.imbalance.hits += 1;
            } else {
                r.misses.push((e.id.clone(), "imbalance", want.clone()));
            }
        }

        let mut detected_hints: Vec<String> = record
            .imbalances
            .iter()
            .flat_map(|i| i.plans.iter().map(|p| p.hint.clone()))
            .collect();
        detected_hints.extend(record.composite_plans.iter().flat_map(|c| c.hints.clone()));
        detected_hints.extend(
            development
                .iter()
                .flat_map(|i| i.plans.iter().map(|p| p.hint.clone())),
        );
        // Maneuvers are plans too. A reroute that carries its own owner
        // (opposition, which belongs to the side to move) is emitted as a
        // Maneuver rather than a PlanHint precisely so it cannot be
        // misattributed — the harness has to look there as well.
        for m in &record.maneuvers {
            if let Some(token) = maneuver_token(&m.reason) {
                detected_hints.push(token.to_string());
            }
        }
        for want in &e.expected.plan_tags {
            if KNOWN_HINTS.contains(&want.as_str()) {
                r.plans.total += 1;
                if detected_hints.iter().any(|h| h == want) {
                    r.plans.hits += 1;
                } else {
                    r.misses.push((e.id.clone(), "plan", want.clone()));
                }
            } else {
                *r.vocabulary_gaps.entry(want.clone()).or_default() += 1;
            }
        }

        let detected_alerts: Vec<String> = record
            .wsui
            .alerts
            .iter()
            .map(|a| format!("{:?}", a.kind))
            .collect();
        for want in &e.expected.alerts {
            r.alerts.total += 1;
            if detected_alerts.iter().any(|k| k == want) {
                r.alerts.hits += 1;
            } else {
                r.misses.push((e.id.clone(), "alert", want.clone()));
            }
        }

        for lc in &e.expected.line_conditional {
            for a in &lc.alerts {
                r.line_conditional
                    .push((e.id.clone(), a.clone(), !lc.line.is_empty()));
            }
        }

        for banned in &e.not_expected.imbalances {
            r.negative_checks += 1;
            if detected_kinds.iter().any(|k| k == banned) {
                r.false_fires
                    .push((e.id.clone(), "imbalance", banned.clone()));
            }
        }
        for banned in &e.not_expected.plan_tags {
            r.negative_checks += 1;
            if detected_hints.iter().any(|h| h == banned) {
                r.false_fires.push((e.id.clone(), "plan", banned.clone()));
            }
        }
        for banned in &e.not_expected.alerts {
            r.negative_checks += 1;
            if detected_alerts.iter().any(|k| k == banned) {
                r.false_fires.push((e.id.clone(), "alert", banned.clone()));
            }
        }

        // Suggestion hit-rate (run 10) against transcribed book moves.
        // The harness runs NO engine, so the whole-board static veto
        // governs (run 11): marked candidates are dropped exactly as a
        // no-engine consumer would drop them. This is the honest number
        // for what users see without verification — tactical book moves
        // that statics must veto now count as misses (see
        // docs/VALIDATION.md).
        if !e.expected.best_moves.is_empty() {
            let suggestions: Vec<_> = kibitz_core::suggest::suggest(&record, &board)
                .into_iter()
                .filter(|s| s.static_risk.is_none())
                .collect();
            let hit = |s: &kibitz_core::suggest::Suggestion| {
                e.expected
                    .best_moves
                    .iter()
                    .any(|bm| san_matches(&s.san, bm))
            };
            r.suggest_top1.total += 1;
            if suggestions.first().is_some_and(hit) {
                r.suggest_top1.hits += 1;
            }
            r.suggest_top3.total += 1;
            if suggestions.iter().any(hit) {
                r.suggest_top3.hits += 1;
            } else {
                r.misses.push((
                    e.id.clone(),
                    "suggest",
                    format!(
                        "{:?} (we said {:?})",
                        e.expected.best_moves,
                        suggestions
                            .iter()
                            .map(|s| s.san.clone())
                            .collect::<Vec<_>>()
                    ),
                ));
            }
        }

        if let Some(want) = e.expected.favors.as_deref() {
            if want != "balanced" {
                r.favors.total += 1;
                // The product's own verdict, not a harness-local vote:
                // kibitz_core::verdict is what the app shows and what the
                // fit in `favors-fit` tunes.
                let lean = kibitz_core::verdict::lean(&record.imbalances);
                let ours = if lean > 0 {
                    "white"
                } else if lean < 0 {
                    "black"
                } else {
                    "balanced"
                };
                if ours == want {
                    r.favors.hits += 1;
                } else {
                    r.misses
                        .push((e.id.clone(), "favors", format!("{want} (we said {ours})")));
                }
            }
        }
    }
    r
}

/// SAN comparison tolerant of decoration: check/mate/annotation marks are
/// stripped from both sides before comparing.
fn san_matches(ours: &str, book: &str) -> bool {
    fn strip(s: &str) -> &str {
        s.trim_end_matches(['+', '#', '!', '?'])
    }
    strip(ours) == strip(book)
}

/// Load every *.json corpus under `path` (file or directory).
pub fn load(path: &Path) -> anyhow::Result<Vec<Corpus>> {
    let mut out = Vec::new();
    let files: Vec<std::path::PathBuf> = if path.is_dir() {
        let mut v: Vec<_> = std::fs::read_dir(path)?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|x| x == "json"))
            .collect();
        v.sort();
        v
    } else {
        vec![path.to_path_buf()]
    };
    for f in files {
        let corpus: Corpus = serde_json::from_str(&std::fs::read_to_string(&f)?)
            .map_err(|e| anyhow::anyhow!("{}: {e}", f.display()))?;
        out.push(corpus);
    }
    Ok(out)
}

pub fn print_report(book: &str, r: &Report, verbose: bool) {
    println!("== {book} — {} positions", r.positions);
    if !r.bad_fens.is_empty() {
        println!("  UNPARSEABLE FENS: {}", r.bad_fens.len());
        for (id, fen) in &r.bad_fens {
            println!("    {id}: {fen}");
        }
    }
    let line = |name: &str, s: &AxisScore| {
        if s.total > 0 {
            println!(
                "  {name:<12} {:>3}/{:<3} = {:>5.1}%",
                s.hits,
                s.total,
                s.pct()
            );
        }
    };
    line("imbalances", &r.imbalance);
    line("plans", &r.plans);
    line("alerts", &r.alerts);
    line("favors", &r.favors);
    line("suggest@1", &r.suggest_top1);
    line("suggest@3", &r.suggest_top3);
    if !r.vocabulary_gaps.is_empty() {
        println!("  vocabulary gaps (no matching hint yet):");
        for (tag, n) in &r.vocabulary_gaps {
            println!("    {tag} ×{n}");
        }
    }
    if r.negative_checks > 0 {
        println!(
            "  negatives    {}/{} clean",
            r.negative_checks - r.false_fires.len() as u32,
            r.negative_checks
        );
        for (id, axis, what) in &r.false_fires {
            println!("  FALSE-FIRE {axis:<9} {id}: {what}");
        }
    }
    if !r.line_conditional.is_empty() {
        println!(
            "  line-conditional {} expectation(s) excluded from position-level scoring \
             (await suggest-verify):",
            r.line_conditional.len()
        );
        for (id, alert, has_line) in &r.line_conditional {
            println!(
                "    {id}: {alert}{}",
                if *has_line {
                    ""
                } else {
                    "  [line not yet transcribed]"
                }
            );
        }
    }
    if verbose {
        for (id, axis, want) in &r.misses {
            println!("  MISS {axis:<9} {id}: expected {want}");
        }
    }
}

/// Firing rate of one plan hint over the engine-quiet set — the
/// entomb-fp discipline made generic, so every future B2 condition gets
/// its cost term from the same instrument (run 12, ruling 3).
///
/// The currency is prose: a plan hint runs no engine, so each firing on
/// a healthy master position is a sentence of advice about a plan that
/// is not there. Measure before wiring into KNOWN_HINTS, not after.
pub fn hint_fp(path: &std::path::Path, hint: &str, dump: bool) -> anyhow::Result<()> {
    let text = std::fs::read_to_string(path)?;
    let (mut n, mut firing, mut count) = (0usize, 0usize, 0usize);
    for fen in text.lines().filter(|l| !l.trim().is_empty()) {
        let Ok(board) = fen.parse::<cozy_chess::Board>() else {
            continue;
        };
        n += 1;
        let record = kibitz_core::analyze(&board);
        let hits = record
            .imbalances
            .iter()
            .flat_map(|i| i.plans.iter())
            .filter(|p| p.hint == hint)
            .count()
            + record
                .composite_plans
                .iter()
                .flat_map(|c| c.hints.iter())
                .filter(|h| h.as_str() == hint)
                .count();
        if hits > 0 {
            firing += 1;
            count += hits;
            if dump {
                println!("{fen}");
            }
        }
    }
    println!("engine-quiet master positions: {n}");
    println!(
        "  positions where {hint} fires {firing:>5}  ({:.1}%)",
        firing as f64 / n.max(1) as f64 * 100.0
    );
    println!("  total firings                {count:>5}");
    Ok(())
}

/// The four remaining WeakKing static-gap misses, priced (run 12, #10).
///
/// am-324-2, htryc-375-128, htryc-388-182, htryc-391-200 are all silent
/// today — no WeakKing at any severity — and the hypothesis under test
/// (recorded before this function was written) is that they are NOT one
/// class but a 2+2 split:
///
///   A — the LAGGING KING: king still on its home-rank d/e square while
///   the enemy king is already castled, queens on. Shield intact, no
///   open file, nothing hits the zone: every current arm is correctly
///   silent, because the exposure is temporal.
///
///   B — the SECTOR FUNNEL: castled flank king whose sector the enemy
///   out-forces by >= 300cp of travel-discounted force (force::force_in).
///   The zone-surplus arm counts pieces attacking zone squares NOW; a
///   funnel is pieces that can arrive.
///
/// This is a pricing study, not a detector: it reports how common each
/// condition is on each corpus and whether WeakKing already fires there.
/// The quiet-set frequency is the cost a real detector would start from.
pub fn king_study(paths: &[std::path::PathBuf]) -> anyhow::Result<()> {
    use cozy_chess::{Color, File, Piece, Rank};
    use kibitz_core::force::{force_in, Sector};

    // (label, fen): book entries keep their ids, quiet lines are numbered.
    let mut items: Vec<(String, String)> = Vec::new();
    for p in paths {
        if p.extension().and_then(|e| e.to_str()) == Some("json") || p.is_dir() {
            for c in load(p)? {
                items.extend(c.positions.into_iter().map(|e| (e.id, e.fen)));
            }
        } else {
            items.extend(
                std::fs::read_to_string(p)?
                    .lines()
                    .filter(|l| !l.trim().is_empty())
                    .enumerate()
                    .map(|(i, l)| (format!("quiet-{i}"), l.to_string())),
            );
        }
    }

    const MISSES: [&str; 4] = [
        "am-324-2",
        "htryc-375-128",
        "htryc-388-182",
        "htryc-391-200",
    ];
    // [book, quiet] x [candidates, weak-king-fires]
    let mut a = [[0usize; 2]; 2];
    let mut b = [[0usize; 2]; 2];
    let mut scanned = [0usize; 2];

    println!("book-corpus candidates:");
    for (label, fen) in &items {
        let Ok(board) = fen.parse::<cozy_chess::Board>() else {
            continue;
        };
        let quiet = label.starts_with("quiet-");
        let src = usize::from(quiet);
        scanned[src] += 1;
        let record = kibitz_core::wsui::screen(&board, &kibitz_core::wsui::WsuiConfig::default());
        let queens_on = !board.pieces(Piece::Queen).is_empty();
        for side in [Color::White, Color::Black] {
            let king = board.king(side);
            let enemy = !side;
            let eking = board.king(enemy);
            let home = match side {
                Color::White => Rank::First,
                Color::Black => Rank::Eighth,
            };
            let eback = match enemy {
                Color::White => Rank::First.bitboard() | Rank::Second.bitboard(),
                Color::Black => Rank::Eighth.bitboard() | Rank::Seventh.bitboard(),
            };
            let central = |sq: cozy_chess::Square| matches!(sq.file(), File::D | File::E);
            let enemy_castled = eback.has(eking) && !central(eking);

            // A: lagging king.
            let is_a = king.rank() == home && central(king) && enemy_castled && queens_on;
            // B: sector funnel at a castled flank king.
            let sector = Sector::of(king);
            let is_b = eback_own(side).has(king)
                && !central(king)
                && sector != Sector::Center
                && force_in(&board, enemy, sector) - force_in(&board, side, sector) >= 300;

            if !is_a && !is_b {
                continue;
            }
            let weak = record.alerts.iter().any(|al| {
                al.kind == kibitz_core::record::AlertKind::WeakKing
                    && al.side == kibitz_core::record::SideColor::from(side)
            });
            if is_a {
                a[src][0] += 1;
                a[src][1] += usize::from(weak);
            }
            if is_b {
                b[src][0] += 1;
                b[src][1] += usize::from(weak);
            }
            if !quiet {
                println!(
                    "  {label:<28} {:<6} {}{}{}  WeakKing: {}",
                    if side == Color::White {
                        "white"
                    } else {
                        "black"
                    },
                    if is_a { "A" } else { "" },
                    if is_a && is_b { "+" } else { "" },
                    if is_b { "B" } else { "" },
                    if weak { "fires" } else { "silent" },
                );
                if MISSES.contains(&label.as_str()) {
                    println!("    ^ one of the four misses");
                }
            }
        }
    }
    let pct = |k: usize, n: usize| k as f64 / n.max(1) as f64 * 100.0;
    println!("\nscanned: book {}, quiet {}", scanned[0], scanned[1]);
    for (name, m) in [("A lagging king", &a), ("B sector funnel", &b)] {
        println!("{name}:");
        println!(
            "  book  {:>4} candidates, WeakKing fires on {}",
            m[0][0], m[0][1]
        );
        println!(
            "  quiet {:>4} candidates ({:.1}% of positions), WeakKing fires on {}",
            m[1][0],
            pct(m[1][0], scanned[1]),
            m[1][1]
        );
    }
    Ok(())
}

/// `side`'s own back two ranks (the "has castled or stayed home" band).
fn eback_own(side: cozy_chess::Color) -> cozy_chess::BitBoard {
    use cozy_chess::Rank;
    match side {
        cozy_chess::Color::White => Rank::First.bitboard() | Rank::Second.bitboard(),
        cozy_chess::Color::Black => Rank::Eighth.bitboard() | Rank::Seventh.bitboard(),
    }
}

/// What fraction of positions have a plan SPEED at all?
///
/// Prophylaxis batch 1 found own/opp horizons None on every one of the
/// 29 author-labeled best-move positions — the tempo comparison in
/// role_of has never had data. Horizons come only from schemes; schemes
/// only form when routed maneuvers converge with composite plans; and
/// only three hints ever route. This study prices the gap: per position
/// and side, does a scheme exist, a maneuver, or neither. The answer
/// decides whether the fix is a role_of fallback (B2) or a plan-speed
/// term across the whole hint vocabulary (run-sized B3).
pub fn horizon_study(paths: &[std::path::PathBuf]) -> anyhow::Result<()> {
    let mut sets: Vec<(String, Vec<String>)> = Vec::new(); // (set name, fens)
    for p in paths {
        if p.extension().and_then(|e| e.to_str()) == Some("json") || p.is_dir() {
            let mut fens = Vec::new();
            for c in load(p)? {
                fens.extend(
                    c.positions
                        .into_iter()
                        .filter(|e| !e.expected.best_moves.is_empty())
                        .map(|e| e.fen),
                );
            }
            sets.push(("labeled best-move entries".into(), fens));
        } else {
            let fens = std::fs::read_to_string(p)?
                .lines()
                .filter(|l| !l.trim().is_empty())
                .map(str::to_string)
                .collect();
            sets.push(("quiet holdout".into(), fens));
        }
    }
    for (name, fens) in &sets {
        let (mut n, mut scheme_any, mut scheme_both, mut man_any, mut neither) =
            (0usize, 0usize, 0usize, 0usize, 0usize);
        for fen in fens {
            let Ok(board) = fen.parse::<cozy_chess::Board>() else {
                continue;
            };
            n += 1;
            let r = kibitz_core::analyze(&board);
            let sides = |f: kibitz_core::record::Favors| r.schemes.iter().any(|s| s.favors == f);
            let w = sides(kibitz_core::record::Favors::White);
            let b = sides(kibitz_core::record::Favors::Black);
            if w || b {
                scheme_any += 1;
            }
            if w && b {
                scheme_both += 1;
            }
            if !r.maneuvers.is_empty() {
                man_any += 1;
            }
            if !(w || b) && r.maneuvers.is_empty() {
                neither += 1;
            }
        }
        let pct = |k: usize| k as f64 / n.max(1) as f64 * 100.0;
        println!("{name}: {n} positions");
        println!(
            "  scheme for either side  {scheme_any:>4}  ({:.0}%)",
            pct(scheme_any)
        );
        println!("  schemes for BOTH sides  {scheme_both:>4}  ({:.0}%)  <- the tempo comparison needs this", pct(scheme_both));
        println!(
            "  any maneuver            {man_any:>4}  ({:.0}%)",
            pct(man_any)
        );
        println!(
            "  no speed at all         {neither:>4}  ({:.0}%)",
            pct(neither)
        );
    }
    Ok(())
}

/// Does shield anatomy predict anything once the queens are off?
///
/// The praxis-g70 red anchor: WeakKing at severity HIGH, on pure
/// shield/open-file evidence, against a king the book is walking up the
/// board as a reserve blockader — no queen on the board. Same proxy
/// shape as cbcs-239 ("shield file empty" standing in for "shield pawn
/// gone"): here "shield anatomy broken" stands in for "king in danger",
/// which without a queen it may not be.
///
/// Class: a WeakKing alert on side S where S's opponent is queenless
/// and the alert's own detail is shield/open-file ONLY — no zone
/// pressure, no back-rank. Reports the class size on the quiet holdout
/// and, on the book corpus, whether any RECALL HIT sits in the class
/// (the refutation condition: a gate that loses recall does not ship).
pub fn queenless_study(paths: &[std::path::PathBuf]) -> anyhow::Result<()> {
    use cozy_chess::{Color, Piece};
    let mut items: Vec<(String, String, Vec<String>)> = Vec::new(); // (label, fen, expected alerts)
    for p in paths {
        if p.extension().and_then(|e| e.to_str()) == Some("json") || p.is_dir() {
            for c in load(p)? {
                items.extend(
                    c.positions
                        .into_iter()
                        .map(|e| (e.id, e.fen, e.expected.alerts)),
                );
            }
        } else {
            items.extend(
                std::fs::read_to_string(p)?
                    .lines()
                    .filter(|l| !l.trim().is_empty())
                    .enumerate()
                    .map(|(i, l)| (format!("quiet-{i}"), l.to_string(), vec![])),
            );
        }
    }
    let (mut quiet_alerts, mut quiet_class) = (0usize, 0usize);
    let mut book_class: Vec<String> = Vec::new();
    let mut hits_in_class: Vec<String> = Vec::new();
    for (label, fen, expected) in &items {
        let Ok(board) = fen.parse::<cozy_chess::Board>() else {
            continue;
        };
        let r = kibitz_core::wsui::screen(&board, &kibitz_core::wsui::WsuiConfig::default());
        for a in &r.alerts {
            if a.kind != kibitz_core::record::AlertKind::WeakKing {
                continue;
            }
            let quiet = label.starts_with("quiet-");
            if quiet {
                quiet_alerts += 1;
            }
            let side = match a.side {
                kibitz_core::record::SideColor::White => Color::White,
                kibitz_core::record::SideColor::Black => Color::Black,
            };
            let enemy_queenless = board.colored_pieces(!side, Piece::Queen).is_empty();
            let detail = a.detail.clone().unwrap_or_default();
            let shield_only = !detail.contains("zone-pressure") && !detail.contains("back-rank");
            if enemy_queenless && shield_only {
                if quiet {
                    quiet_class += 1;
                } else {
                    book_class.push(format!("{label} ({:?})", a.side));
                    // A recall hit in the class: this entry EXPECTS
                    // WeakKing and the class alert may be the one
                    // satisfying it.
                    if expected.iter().any(|w| w == "WeakKing") {
                        hits_in_class.push(label.clone());
                    }
                }
            }
        }
    }
    println!("quiet holdout: {quiet_alerts} WeakKing alerts, {quiet_class} in the queenless-shield class ({:.0}%)",
        quiet_class as f64 / quiet_alerts.max(1) as f64 * 100.0);
    println!("book corpus class members:");
    for b in &book_class {
        println!("  {b}");
    }
    if hits_in_class.is_empty() {
        println!(
            "no book WeakKing EXPECTATION is satisfied by a class alert — gate loses no recall"
        );
    } else {
        println!("RECALL AT RISK — these entries expect WeakKing and hold a class alert:");
        for h in &hits_in_class {
            println!("  {h}");
        }
    }
    Ok(())
}

/// Was the shield pawn TRADED, or did it just relocate one file over?
///
/// `cbcs-239` is the first red negative anchor the corpus-side alert bans
/// produced: WeakKing reports "f-file shield pawn missing" on the
/// position Jeremy Silman uses to teach that doubled pawns can be an
/// asset. The f-pawn is not gone from in front of that king — it captured
/// onto e3, and the pawn count in front of g1 is unchanged.
///
/// That is the same shape of error as the one the run-12 WeakKing work
/// already fixed once: "central king" was a proxy for "open file", and
/// "shield file empty" may be a proxy for "shield pawn traded away". This
/// study asks whether the suspicion is a CLASS or one anchor. It reports
/// every (position, side) where the detector's own shield arm applies and
/// some shield file is empty while an adjacent file carries an own
/// DOUBLED pawn, and whether WeakKing fired.
///
/// It settles nothing by itself. A condition written off one anchor is
/// how a detector gets tuned to a corpus; a condition written off a class
/// is a condition.
pub fn shield_study(paths: &[std::path::PathBuf]) -> anyhow::Result<()> {
    use cozy_chess::{Color, File, Piece, Rank};

    let mut fens: Vec<String> = Vec::new();
    for p in paths {
        if p.extension().and_then(|e| e.to_str()) == Some("json") || p.is_dir() {
            for c in load(p)? {
                fens.extend(c.positions.into_iter().map(|e| e.fen));
            }
        } else {
            fens.extend(
                std::fs::read_to_string(p)?
                    .lines()
                    .filter(|l| !l.trim().is_empty())
                    .map(str::to_string),
            );
        }
    }

    let (mut scanned, mut candidates, mut fired, mut sole) = (0usize, 0usize, 0usize, 0usize);
    println!(
        "{:<70} {:<6} {:<9} {:<8} {:<6} screen",
        "fen", "side", "empty/dbl", "severity", "blame"
    );
    for fen in &fens {
        let Ok(board) = fen.parse::<cozy_chess::Board>() else {
            continue;
        };
        scanned += 1;
        let record = kibitz_core::wsui::screen(&board, &kibitz_core::wsui::WsuiConfig::default());
        for side in [Color::White, Color::Black] {
            let king = board.king(side);
            let back = match side {
                Color::White => Rank::First.bitboard() | Rank::Second.bitboard(),
                Color::Black => Rank::Eighth.bitboard() | Rank::Seventh.bitboard(),
            };
            // Exactly the detector's own gate: back two ranks, flank file.
            if !back.has(king) || matches!(king.file(), File::D | File::E) {
                continue;
            }
            let pawns = board.colored_pieces(side, Piece::Pawn);
            let on = |f: i8| -> u32 {
                if !(0..8).contains(&f) {
                    return 0;
                }
                (pawns & File::index(f as usize).bitboard()).len()
            };
            let kf = king.file() as i8;
            let mut hits: Vec<String> = Vec::new();
            for df in -1..=1i8 {
                let f = kf + df;
                if !(0..8).contains(&f) || on(f) > 0 {
                    continue;
                }
                // A doubled own pawn on either neighbour is the signature
                // of a shield pawn that captured sideways rather than one
                // that was traded off the board.
                for adj in [f - 1, f + 1] {
                    if on(adj) >= 2 {
                        hits.push(format!(
                            "{}->{}",
                            (b'a' + f as u8) as char,
                            (b'a' + adj as u8) as char
                        ));
                    }
                }
            }
            if hits.is_empty() {
                continue;
            }
            candidates += 1;
            let alert = record.alerts.iter().find(|a| {
                a.kind == kibitz_core::record::AlertKind::WeakKing
                    && a.side == kibitz_core::record::SideColor::from(side)
            });
            // Firing is not the same as firing BECAUSE of this. Only an
            // alert whose own detail names the relocated file as a
            // missing shield pawn would be touched by the candidate
            // condition, and only one that is otherwise bare would be
            // silenced by it — a king under real pressure keeps its alert.
            let detail = alert.and_then(|a| a.detail.clone()).unwrap_or_default();
            let blamed = hits.iter().any(|h| {
                let f = h.chars().next().unwrap_or('?');
                detail.contains(&format!("{f}-file shield pawn missing"))
            });
            // `detail` is the reason list joined with "; " (see
            // wsui::detect_weak_king). Anything in it that is not a
            // missing-shield note is an independent reason to alert.
            let other_reason = detail
                .split("; ")
                .any(|d| !d.trim().is_empty() && !d.contains("shield pawn missing"));
            if blamed {
                fired += 1;
                if !other_reason {
                    sole += 1;
                }
            }
            println!(
                "{fen:<70} {:<6} {:<9} {:<8} {:<6} {}",
                if side == Color::White {
                    "white"
                } else {
                    "black"
                },
                hits.join(","),
                match alert {
                    None => "silent".to_string(),
                    Some(a) => format!("{:?}", a.severity).to_lowercase(),
                },
                if blamed { "blamed" } else { "-" },
                if record.screen_fired {
                    "screen"
                } else {
                    "quiet"
                },
            );
        }
    }
    println!("\npositions scanned            {scanned:>5}");
    println!("relocated-shield candidates  {candidates:>5}");
    println!(
        "  WeakKing blames the file   {fired:>5}  ({:.0}%)",
        fired as f64 / candidates.max(1) as f64 * 100.0
    );
    println!(
        "  and has no other reason    {sole:>5}  ({:.0}%) — these are the alerts a \
         relocated-pawn condition would actually silence",
        sole as f64 / candidates.max(1) as f64 * 100.0
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Synthetic corpus (no book content): the Sveshnikov bind position
    /// our golden tests already cover must score on all axes.
    #[test]
    fn harness_scores_a_known_position() {
        let corpus = Corpus {
            book: "synthetic".into(),
            edition: String::new(),
            positions: vec![
                Entry {
                    id: "syn-1".into(),
                    citation: String::new(),
                    sans: vec![],
                    fen: "r1bqkb1r/pp3ppp/2np1n2/1N2p3/4P3/2N5/PPP2PPP/R1BQKB1R w KQkq - 0 7"
                        .into(),
                    kind: "example".into(),
                    confidence: "high".into(),
                    not_expected: NotExpected::default(),
                    expected: Expected {
                        favors: Some("white".into()),
                        imbalances: vec!["SquaresOutposts".into(), "PawnStructure".into()],
                        plan_tags: vec![
                            "ManeuverKnightToOutpost".into(),
                            "minority-attack".into(), // vocabulary gap
                        ],
                        alerts: vec![],
                        theme: String::new(),
                        best_moves: vec!["Nd5".into()],
                        line_conditional: vec![],
                    },
                },
                Entry {
                    id: "syn-bad-fen".into(),
                    citation: String::new(),
                    sans: vec![],
                    fen: "not a fen".into(),
                    kind: "example".into(),
                    confidence: "high".into(),
                    not_expected: NotExpected::default(),
                    expected: Expected::default(),
                },
            ],
        };
        let r = eval_corpus(&corpus);
        assert_eq!(r.positions, 1);
        assert_eq!(r.bad_fens.len(), 1);
        assert_eq!(r.imbalance.total, 2);
        assert_eq!(r.imbalance.hits, 2, "misses: {:?}", r.misses);
        assert_eq!(r.plans.total, 1);
        assert_eq!(r.plans.hits, 1);
        assert_eq!(r.vocabulary_gaps.get("minority-attack"), Some(&1));
        assert_eq!(r.favors.hits, 1);
        // Run 10: the Sveshnikov bind's book move Nd5 is our suggestion.
        assert_eq!(r.suggest_top3.total, 1);
        assert_eq!(r.suggest_top3.hits, 1, "misses: {:?}", r.misses);
    }
}
