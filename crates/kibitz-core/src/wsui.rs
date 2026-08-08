//! Stage 1 — the WSUI tactical screen (docs/KIBITZ_ENGINE_SPEC.md).
//!
//! Four static detectors run for BOTH sides (side to move first), no
//! search: **W**eak king, **S**talemated (trapped) pieces, **U**ndefended
//! (loose) pieces, **I**nadequately defended pieces. Output is a list of
//! [`TacticAlert`]s; if any alert reaches the configured severity
//! threshold the screen "fires" and the app layer may enqueue a bounded
//! engine confirmation job. Everything here must stay microsecond-cheap.

use cozy_chess::{
    get_king_moves, get_knight_moves, get_pawn_attacks, BitBoard, Board, Color, File, Piece, Rank,
    Square,
};

use crate::attack::{attacked_squares, attackers_of, pinned_piece_covers, pinned_pieces};
use crate::record::{square_name, AlertKind, Severity, TacticAlert, WsuiReport};
use crate::see::{piece_value, see};

/// How alert lists become a fire/no-fire decision (run-5 feedback item 5:
/// each variant was evaluated against the puzzle/quiet validation sets;
/// the table lives in docs/VALIDATION.md).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FiringRule {
    /// Any single alert at or above `fire_threshold` (the original rule).
    AnyAtOrAbove,
    /// At least two alerts at or above `fire_threshold`.
    PairAtOrAbove,
    /// One High alert, OR two alerts of DISTINCT kinds at or above
    /// `fire_threshold`.
    HighSoloOrTwoDistinct,
    /// Severity-weighted sum (Low=1, Medium=2, High=4) reaches `fire_at`.
    WeightedScore { fire_at: u32 },
}

/// Tunable thresholds (benchmarked/tuned by the Phase 3 validation
/// harness; defaults are the tuned values recorded in docs/VALIDATION.md).
#[derive(Debug, Clone)]
pub struct WsuiConfig {
    /// Minimum severity at which the screen fires.
    pub fire_threshold: Severity,
    /// The decision rule combining alerts into a firing decision.
    pub rule: FiringRule,
    /// SEE gain (cp) for an I-alert to be medium / high.
    pub see_medium: i32,
    pub see_high: i32,
    /// King-zone attacker-minus-defender surplus for a W-alert.
    pub king_zone_surplus: i32,
}

impl Default for WsuiConfig {
    fn default() -> Self {
        Self {
            fire_threshold: Severity::Medium,
            rule: FiringRule::AnyAtOrAbove,
            see_medium: 100,
            see_high: 300,
            king_zone_surplus: 2,
        }
    }
}

/// Apply the configured firing rule to a sorted alert list.
fn decide(alerts: &[TacticAlert], cfg: &WsuiConfig) -> bool {
    let at_or_above = || alerts.iter().filter(|a| a.severity >= cfg.fire_threshold);
    match cfg.rule {
        FiringRule::AnyAtOrAbove => at_or_above().next().is_some(),
        FiringRule::PairAtOrAbove => at_or_above().count() >= 2,
        FiringRule::HighSoloOrTwoDistinct => {
            alerts.iter().any(|a| a.severity >= Severity::High) || {
                let kinds: std::collections::BTreeSet<_> = at_or_above().map(|a| a.kind).collect();
                kinds.len() >= 2
            }
        }
        FiringRule::WeightedScore { fire_at } => {
            let weight = |s: Severity| match s {
                Severity::Low => 1u32,
                Severity::Medium => 2,
                Severity::High => 4,
            };
            alerts.iter().map(|a| weight(a.severity)).sum::<u32>() >= fire_at
        }
    }
}

fn names(bb: BitBoard) -> Vec<String> {
    bb.into_iter().map(square_name).collect()
}

/// Run the full screen for both sides; side to move's problems first.
/// What each detector produced, per side, before anything downstream
/// looks at the result.
///
/// The screen does not arbitrate — all three detectors append to one vec
/// and it is only sorted by severity — so a kind missing from the output
/// genuinely did not fire. That is worth being able to SHOW rather than
/// assert, because "TrappedPiece is insensitive" and "TrappedPiece fired
/// and lost" have opposite fixes, and lowering a threshold on a detector
/// that is already firing produces no change and reads as blindness.
///
/// `trapped_skipped` is the one silent exit in the whole screen:
/// `detect_trapped` needs a board it can generate moves on, and for the
/// side NOT to move that means a null move, which is unavailable when the
/// mover is in check. The detector then returns having examined nothing.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScreenTrace {
    pub undefended: usize,
    pub inadequate: usize,
    pub trapped: usize,
    pub weak_king: usize,
    /// Sides for which detect_trapped exited without examining anything.
    pub trapped_skipped: Vec<crate::record::SideColor>,
}

pub fn screen_trace(board: &Board, cfg: &WsuiConfig) -> ScreenTrace {
    let stm = board.side_to_move();
    let mut t = ScreenTrace::default();
    for side in [stm, !side_of(stm)] {
        let mut ud = Vec::new();
        detect_undefended_and_inadequate(board, side, cfg, &mut ud);
        t.undefended += ud
            .iter()
            .filter(|a| a.kind == crate::record::AlertKind::Undefended)
            .count();
        t.inadequate += ud
            .iter()
            .filter(|a| a.kind == crate::record::AlertKind::InadequatelyDefended)
            .count();

        let mut tr = Vec::new();
        detect_trapped(board, side, &mut tr);
        t.trapped += tr.len();
        if board.side_to_move() != side && board.null_move().is_none() {
            t.trapped_skipped.push(side.into());
        }

        let mut wk = Vec::new();
        detect_weak_king(board, side, cfg, &mut wk);
        t.weak_king += wk.len();
    }
    t
}

fn side_of(c: Color) -> Color {
    c
}

pub fn screen(board: &Board, cfg: &WsuiConfig) -> WsuiReport {
    let stm = board.side_to_move();
    let mut alerts = Vec::new();
    for side in [stm, !stm] {
        detect_undefended_and_inadequate(board, side, cfg, &mut alerts);
        let _ = detect_trapped(board, side, &mut alerts);
        detect_weak_king(board, side, cfg, &mut alerts);
    }
    // Most severe first within the stable side order.
    alerts.sort_by_key(|a| std::cmp::Reverse(a.severity));
    let screen_fired = decide(&alerts, cfg);
    WsuiReport {
        alerts,
        screen_fired,
    }
}

/// U + I: loose pieces and unfavourable capture sequences, with pin-aware
/// defender counting and overload detection.
fn detect_undefended_and_inadequate(
    board: &Board,
    side: Color,
    cfg: &WsuiConfig,
    alerts: &mut Vec<TacticAlert>,
) {
    let enemy = !side;
    let occ = board.occupied();
    let pinned = pinned_pieces(board, side);
    let enemy_pinned = pinned_pieces(board, enemy);
    // Track (defender -> defended attacked targets) for overload detection.
    let mut sole_defender_targets: Vec<(Square, Square)> = Vec::new();

    let own_pieces = board.colors(side)
        & (board.pieces(Piece::Knight)
            | board.pieces(Piece::Bishop)
            | board.pieces(Piece::Rook)
            | board.pieces(Piece::Queen));

    for sq in own_pieces {
        // Absolutely-pinned attackers only count along their pin ray.
        let raw_attackers = attackers_of(board, sq, enemy, occ);
        let mut attackers = BitBoard::EMPTY;
        for a in raw_attackers {
            if !enemy_pinned.has(a) || pinned_piece_covers(board, enemy, a, sq) {
                attackers |= a.bitboard();
            }
        }
        // Pin-aware defender count: a pinned defender only counts if the
        // target lies on its pin ray.
        let raw_defenders = attackers_of(board, sq, side, occ);
        let mut defenders = BitBoard::EMPTY;
        for d in raw_defenders {
            if !pinned.has(d) || pinned_piece_covers(board, side, d, sq) {
                defenders |= d.bitboard();
            }
        }

        let home_rank = match side {
            Color::White => Rank::First,
            Color::Black => Rank::Eighth,
        };
        if defenders.is_empty() {
            // U — loose piece. A merely-loose piece still sitting on its
            // back rank is undeveloped, not tactically loose — skip.
            if attackers.is_empty() && sq.rank() == home_rank {
                continue;
            }
            let severity = if attackers.is_empty() {
                Severity::Low // merely loose
            } else {
                Severity::Medium // currently attacked and undefended
            };
            alerts.push(TacticAlert {
                kind: AlertKind::Undefended,
                side: side.into(),
                target: Some(square_name(sq)),
                attackers: names(attackers),
                defenders: vec![],
                see: None,
                severity,
                detail: Some(
                    if attackers.is_empty() {
                        "loose"
                    } else {
                        "attacked-and-undefended"
                    }
                    .to_string(),
                ),
                engine_check: None,
            });
            continue;
        }

        // I — attackers outnumber (pin-aware) defenders AND the static
        // exchange favours the attacker.
        if !attackers.is_empty() {
            let gain = see(board, sq, enemy);
            if gain > 0 {
                let severity = if gain >= cfg.see_high {
                    Severity::High
                } else if gain >= cfg.see_medium {
                    Severity::Medium
                } else {
                    Severity::Low
                };
                alerts.push(TacticAlert {
                    kind: AlertKind::InadequatelyDefended,
                    side: side.into(),
                    target: Some(square_name(sq)),
                    attackers: names(attackers),
                    defenders: names(defenders),
                    see: Some(gain),
                    severity,
                    detail: None,
                    engine_check: None,
                });
            }
            if defenders.len() == 1 {
                let d = defenders.into_iter().next().expect("len 1");
                sole_defender_targets.push((d, sq));
            }
        }
    }

    // Overload: one piece is the sole defender of >= 2 attacked targets.
    sole_defender_targets.sort_by_key(|(d, _)| *d as u8);
    let mut i = 0;
    while i < sole_defender_targets.len() {
        let d = sole_defender_targets[i].0;
        let targets: Vec<Square> = sole_defender_targets[i..]
            .iter()
            .take_while(|(dd, _)| *dd == d)
            .map(|(_, t)| *t)
            .collect();
        i += targets.len();
        if targets.len() >= 2 {
            alerts.push(TacticAlert {
                kind: AlertKind::InadequatelyDefended,
                side: side.into(),
                target: Some(square_name(d)),
                attackers: vec![],
                defenders: targets.iter().copied().map(square_name).collect(),
                see: None,
                severity: Severity::Medium,
                detail: Some("overloaded-defender".to_string()),
                engine_check: None,
            });
        }
    }
}

/// S — pieces with no safe square, including the attackable check.
/// Whether a detector actually examined the position, as distinct from
/// examining it and finding nothing. A detector returning an empty list
/// for both is an accessor you cannot audit: the caller sees zero and
/// cannot tell a clean bill of health from a skipped scan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scanned {
    Yes,
    /// The side is not to move and the mover is in check, so no null-move
    /// board exists to generate replies on. Nothing was examined.
    NoNullMove,
}

fn detect_trapped(board: &Board, side: Color, alerts: &mut Vec<TacticAlert>) -> Scanned {
    let enemy = !side;
    let occ = board.occupied();
    let own_pieces = board.colors(side)
        & (board.pieces(Piece::Knight)
            | board.pieces(Piece::Bishop)
            | board.pieces(Piece::Rook)
            | board.pieces(Piece::Queen));

    // Legal destinations per from-square. Only meaningful when it's this
    // side's turn; for the side not to move we use a null-move board (if
    // in check this whole detector is moot for that side).
    let probe_board = if board.side_to_move() == side {
        Some(board.clone())
    } else {
        board.null_move()
    };
    let Some(probe) = probe_board else {
        return Scanned::NoNullMove;
    };

    let mut dest_map: Vec<(Square, BitBoard)> = Vec::new();
    probe.generate_moves(|pm| {
        dest_map.push((pm.from, pm.to));
        false
    });

    for sq in own_pieces {
        let piece = board.piece_on(sq).expect("occupied");
        let dests = dest_map
            .iter()
            .filter(|(f, _)| *f == sq)
            .map(|(_, to)| *to)
            .fold(BitBoard::EMPTY, |a, b| a | b);
        // A destination is safe if the enemy cannot win material by
        // capturing the piece there (SEE from the enemy's side <= 0 on a
        // board where the piece stands on the destination).
        let mut safe = 0;
        for to in dests {
            let mut b2 = probe.clone();
            let mv = cozy_chess::Move {
                from: sq,
                to,
                promotion: None,
            };
            if b2.try_play(mv).is_err() {
                continue;
            }
            if see_on_moved(&b2, to, enemy) <= 0 {
                safe += 1;
                break;
            }
        }
        if safe > 0 {
            continue;
        }
        let attacked = !attackers_of(board, sq, enemy, occ).is_empty();
        let attackable = attackable_in_one(board, sq, enemy);
        if !attacked && !attackable {
            continue;
        }
        // An unattacked piece "trapped" on its own back two ranks is just
        // undeveloped or boxed in by its own army (home bishops, a Be7
        // behind Qd8/Rf8); real traps are either attacked now or stuck in
        // hostile territory.
        let home_ranks = match side {
            Color::White => Rank::First.bitboard() | Rank::Second.bitboard(),
            Color::Black => Rank::Eighth.bitboard() | Rank::Seventh.bitboard(),
        };
        if !attacked && home_ranks.has(sq) {
            continue;
        }
        let severity = if attacked && see(board, sq, enemy) > 0 {
            if piece_value(piece) >= 500 {
                Severity::High
            } else {
                Severity::Medium
            }
        } else if piece_value(piece) >= 320 {
            Severity::Medium
        } else {
            Severity::Low
        };
        alerts.push(TacticAlert {
            kind: AlertKind::TrappedPiece,
            side: side.into(),
            target: Some(square_name(sq)),
            attackers: names(attackers_of(board, sq, enemy, occ)),
            defenders: names(attackers_of(board, sq, side, occ)),
            see: attacked.then(|| see(board, sq, enemy)),
            severity,
            detail: Some(
                if attacked {
                    "trapped-and-attacked"
                } else {
                    "trapped-attackable"
                }
                .to_string(),
            ),
            engine_check: None,
        });
    }
    Scanned::Yes
}

/// SEE against the piece that just moved to `sq` (it is now the victim).
fn see_on_moved(board_after: &Board, sq: Square, attacker: Color) -> i32 {
    see(board_after, sq, attacker)
}

/// Can any enemy piece legally move to a square from which it would attack
/// `sq` next move? (Cheap one-ply "the trap can be sprung" check.)
fn attackable_in_one(board: &Board, sq: Square, enemy: Color) -> bool {
    // Squares from which each piece type attacks `sq` (symmetry of the
    // attack tables). Sliders use current occupancy.
    let occ = board.occupied();
    let from_knight = get_knight_moves(sq);
    let from_king = get_king_moves(sq);
    let from_rook = cozy_chess::get_rook_moves(sq, occ);
    let from_bishop = cozy_chess::get_bishop_moves(sq, occ);
    let from_pawn = get_pawn_attacks(sq, !enemy); // pawns of `enemy` attack sq from these

    let probe = if board.side_to_move() == enemy {
        Some(board.clone())
    } else {
        board.null_move()
    };
    let Some(probe) = probe else { return false };
    let mut found = false;
    probe.generate_moves(|pm| {
        let want = match pm.piece {
            Piece::Knight => from_knight,
            Piece::King => from_king,
            Piece::Rook => from_rook,
            Piece::Bishop => from_bishop,
            Piece::Queen => from_rook | from_bishop,
            Piece::Pawn => from_pawn,
        };
        if !(pm.to & want).is_empty() {
            found = true;
        }
        found
    });
    found
}

/// W — weak king: zone pressure, pawn-shield defects, open files toward
/// the king, back-rank vulnerability.
fn detect_weak_king(board: &Board, side: Color, cfg: &WsuiConfig, alerts: &mut Vec<TacticAlert>) {
    let enemy = !side;
    let king = board.king(side);
    let zone = get_king_moves(king) | king.bitboard();
    let occ = board.occupied();

    // Zone pressure: per-square attacker/defender counts (king excluded
    // from defenders — it cannot defend against a mating attack).
    let mut attack_count = 0i32;
    let mut defend_count = 0i32;
    for sq in zone {
        attack_count += attackers_of(board, sq, enemy, occ).len() as i32;
        defend_count +=
            (attackers_of(board, sq, side, occ) & !board.pieces(Piece::King)).len() as i32;
    }
    let surplus = attack_count - defend_count;

    // Pawn shield (only meaningful for a king on the back two ranks).
    let back_ranks = match side {
        Color::White => Rank::First.bitboard() | Rank::Second.bitboard(),
        Color::Black => Rank::Eighth.bitboard() | Rank::Seventh.bitboard(),
    };
    let mut shield_defects: Vec<String> = Vec::new();
    let mut open_files_at_king: Vec<String> = Vec::new();
    // Shield defects are only meaningful for a king that has left the
    // central files (castled or manually tucked away); an uncastled king
    // on d/e with its center pawns advanced is normal opening life.
    let flank_king = !matches!(king.file(), File::D | File::E);
    if back_ranks.has(king) && flank_king {
        let kf = king.file() as i8;
        for df in -1..=1i8 {
            let f = kf + df;
            if !(0..8).contains(&f) {
                continue;
            }
            let file = File::index(f as usize);
            let own_pawns_on_file = board.colored_pieces(side, Piece::Pawn) & file.bitboard();
            let enemy_pawns_on_file = board.colored_pieces(enemy, Piece::Pawn) & file.bitboard();
            if own_pawns_on_file.is_empty() {
                shield_defects.push(format!("{}-file shield pawn missing", file_char(file)));
                if enemy_pawns_on_file.is_empty() {
                    open_files_at_king.push(file_char(file).to_string());
                }
            } else {
                // Advanced shield pawn (beyond the third rank).
                let advanced = own_pawns_on_file
                    .into_iter()
                    .all(|p| rank_distance_from_home(side, p.rank()) > 2);
                if advanced {
                    shield_defects.push(format!("{}-file shield pawn advanced", file_char(file)));
                }
            }
        }
    }

    // Back-rank vulnerability: king confined to the back rank by its own
    // pieces/pawns, enemy has a major piece, and no own rook/queen guards
    // the back rank.
    let back_rank = match side {
        Color::White => Rank::First,
        Color::Black => Rank::Eighth,
    };
    let mut back_rank_weak = false;
    if king.rank() == back_rank {
        let escape_rank = match side {
            Color::White => Rank::Second,
            Color::Black => Rank::Seventh,
        };
        let escapes = get_king_moves(king) & escape_rank.bitboard();
        let enemy_attacks = attacked_squares(board, enemy);
        let all_blocked = escapes
            .into_iter()
            .all(|s| board.colors(side).has(s) || enemy_attacks.has(s));
        let enemy_majors =
            board.colors(enemy) & (board.pieces(Piece::Rook) | board.pieces(Piece::Queen));
        let own_majors_on_rank = board.colors(side)
            & (board.pieces(Piece::Rook) | board.pieces(Piece::Queen))
            & back_rank.bitboard();
        if all_blocked && !enemy_majors.is_empty() && own_majors_on_rank.is_empty() {
            back_rank_weak = true;
        }
    }

    let mut score = 0;
    if surplus >= cfg.king_zone_surplus {
        score += 2;
    }
    score += shield_defects.len().min(2) as i32;
    if !open_files_at_king.is_empty() && has_enemy_major(board, enemy) {
        score += 1;
    }
    if back_rank_weak {
        score += 2;
    }
    if score == 0 {
        return;
    }
    let severity = match score {
        1 => Severity::Low,
        2 => Severity::Medium,
        _ => Severity::High,
    };
    // Suppress pure shield chatter in quiet positions: a lone low-severity
    // structural note with no enemy majors is noise.
    if severity == Severity::Low && !has_enemy_major(board, enemy) {
        return;
    }
    let mut details: Vec<String> = Vec::new();
    if surplus >= cfg.king_zone_surplus {
        details.push(format!("zone-pressure+{surplus}"));
    }
    details.extend(shield_defects);
    if !open_files_at_king.is_empty() {
        details.push(format!("open-files:{}", open_files_at_king.join(",")));
    }
    if back_rank_weak {
        details.push("back-rank".to_string());
    }
    alerts.push(TacticAlert {
        kind: AlertKind::WeakKing,
        side: side.into(),
        target: Some(square_name(king)),
        attackers: names(zone.into_iter().fold(BitBoard::EMPTY, |acc, s| {
            acc | attackers_of(board, s, enemy, occ)
        })),
        defenders: vec![],
        see: None,
        severity,
        detail: Some(details.join("; ")),
        engine_check: None,
    });
}

fn has_enemy_major(board: &Board, enemy: Color) -> bool {
    !(board.colors(enemy) & (board.pieces(Piece::Rook) | board.pieces(Piece::Queen))).is_empty()
}

fn file_char(f: File) -> char {
    (b'a' + f as u8) as char
}

/// Ranks advanced from the side's home rank (0 = back rank).
fn rank_distance_from_home(side: Color, rank: Rank) -> u8 {
    match side {
        Color::White => rank as u8,
        Color::Black => 7 - rank as u8,
    }
}

#[cfg(test)]
mod firing_rule_tests {
    use super::*;
    use crate::record::SideColor;

    fn alert(kind: AlertKind, severity: Severity) -> TacticAlert {
        TacticAlert {
            kind,
            side: SideColor::White,
            target: None,
            attackers: vec![],
            defenders: vec![],
            see: None,
            severity,
            detail: None,
            engine_check: None,
        }
    }

    fn cfg(rule: FiringRule) -> WsuiConfig {
        WsuiConfig {
            rule,
            ..WsuiConfig::default()
        }
    }

    #[test]
    fn rules_decide_as_specified() {
        let one_med = vec![alert(AlertKind::Undefended, Severity::Medium)];
        let two_med_same = vec![
            alert(AlertKind::Undefended, Severity::Medium),
            alert(AlertKind::Undefended, Severity::Medium),
        ];
        let two_med_distinct = vec![
            alert(AlertKind::Undefended, Severity::Medium),
            alert(AlertKind::TrappedPiece, Severity::Medium),
        ];
        let one_high = vec![alert(AlertKind::WeakKing, Severity::High)];
        let lows = vec![
            alert(AlertKind::Undefended, Severity::Low),
            alert(AlertKind::WeakKing, Severity::Low),
        ];

        let any = cfg(FiringRule::AnyAtOrAbove);
        assert!(decide(&one_med, &any));
        assert!(!decide(&lows, &any));

        let pair = cfg(FiringRule::PairAtOrAbove);
        assert!(!decide(&one_med, &pair));
        assert!(decide(&two_med_same, &pair));
        assert!(!decide(&one_high, &pair), "one alert is one alert");

        let hstd = cfg(FiringRule::HighSoloOrTwoDistinct);
        assert!(decide(&one_high, &hstd));
        assert!(!decide(&one_med, &hstd));
        assert!(
            !decide(&two_med_same, &hstd),
            "same kind twice is one story"
        );
        assert!(decide(&two_med_distinct, &hstd));

        let weighted = cfg(FiringRule::WeightedScore { fire_at: 4 });
        assert!(!decide(&one_med, &weighted)); // 2 < 4
        assert!(decide(&two_med_same, &weighted)); // 2+2
        assert!(decide(&one_high, &weighted)); // 4
        assert!(!decide(&lows, &weighted)); // 1+1
    }
}
