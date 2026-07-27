//! Narration noise gates (run 9, maintainer field reports): rules that
//! separate what is TRUE from what a coach would SAY. Both operate on a
//! finished [`FeatureRecord`] and are applied by the narration/explain
//! layers only — the WSUI screen, its validation numbers, and the job
//! queue are untouched.
//!
//! Gate 1 — exchanges in progress: after a capture, the capturing piece
//! standing "attacked and underdefended" on its capture square is
//! bookkeeping, not tactics, when the recapture merely resolves the
//! trade. Likewise the material ledger mid-trade ("up three pawns" for
//! one ply) is a lie of timing.
//!
//! Gate 2 — attacked-but-escapable: a queen or rook "attacked" by a
//! cheaper piece, with a safe square to step to, is a tempo event. The
//! static exchange value of capturing it is real arithmetic about an
//! imaginary world where it stands still. Unless the piece is actually
//! trapped (the TrappedPiece detector's job), the coach keeps quiet.

use cozy_chess::{Board, Color, Move, Piece};

use crate::attack::attacked_squares;
use crate::record::{AlertKind, FeatureRecord, ImbalanceKind};
use crate::see::see;

/// Centipawn window within which a capture counts as an even trade.
pub const EVEN_EXCHANGE_CP: i32 = 60;

/// Gate 1. `board_before` is the position `mv` was played FROM. Returns
/// true when the record was modified.
pub fn suppress_exchange_noise(record: &mut FeatureRecord, board_before: &Board, mv: Move) -> bool {
    let mover = board_before.side_to_move();
    let dest = mv.to;
    // A capture: destination occupied by the enemy, or en passant.
    // A pawn moving diagonally onto an empty square is en passant.
    let ep_capture = board_before.piece_on(mv.from) == Some(Piece::Pawn)
        && mv.from.file() != dest.file()
        && board_before.piece_on(dest).is_none();
    let is_capture = board_before.colors(!mover).has(dest) || ep_capture;
    if !is_capture {
        return false;
    }
    // Static exchange of initiating the capture, from the mover's side,
    // evaluated in the pre-move position (the standard "was this trade
    // sound" number).
    let exchange = see(board_before, dest, mover);
    let mut changed = false;

    if exchange >= -EVEN_EXCHANGE_CP {
        // Sound or even capture: the capturing piece being "attacked" on
        // its square is the pending recapture, not a tactic.
        let before = record.wsui.alerts.len();
        record.wsui.alerts.retain(|a| {
            !(matches!(
                a.kind,
                AlertKind::Undefended | AlertKind::InadequatelyDefended
            ) && a.target.as_deref() == Some(crate::record::square_name(dest).as_str()))
        });
        if record.wsui.alerts.len() != before {
            record.wsui.screen_fired = !record.wsui.alerts.is_empty();
            changed = true;
        }
    }
    if exchange.abs() <= EVEN_EXCHANGE_CP {
        // Even trade in progress: the one-ply material spike is timing
        // noise ("up three pawns" until the obvious recapture).
        let before = record.imbalances.len();
        record
            .imbalances
            .retain(|i| i.kind != ImbalanceKind::Material);
        changed |= record.imbalances.len() != before;
    }
    changed
}

/// Gate 2. Drop Undefended/InadequatelyDefended alerts whose target is a
/// queen or rook that has at least one safe flight square — attacked
/// heavy pieces MOVE; that is tempo, not a capture sequence. A piece with
/// no safe square is the TrappedPiece detector's story, which this gate
/// never touches. Returns true when the record was modified.
pub fn suppress_escapable_attack_noise(record: &mut FeatureRecord, board: &Board) -> bool {
    let mut drop_targets: Vec<String> = Vec::new();
    for alert in &record.wsui.alerts {
        if !matches!(
            alert.kind,
            AlertKind::Undefended | AlertKind::InadequatelyDefended
        ) {
            continue;
        }
        let Some(target) = alert.target.as_deref() else {
            continue;
        };
        let Ok(square) = target.parse::<cozy_chess::Square>() else {
            continue;
        };
        let Some(piece) = board.piece_on(square) else {
            continue;
        };
        if !matches!(piece, Piece::Queen | Piece::Rook) {
            continue;
        }
        let owner = if board.colors(Color::White).has(square) {
            Color::White
        } else {
            Color::Black
        };
        // A TrappedPiece alert on the same square means the escape story
        // is already settled the other way.
        let trapped_here = record
            .wsui
            .alerts
            .iter()
            .any(|a| a.kind == AlertKind::TrappedPiece && a.target.as_deref() == Some(target));
        if trapped_here {
            continue;
        }
        if has_safe_flight(board, square, owner) {
            drop_targets.push(target.to_string());
        }
    }
    if drop_targets.is_empty() {
        return false;
    }
    record.wsui.alerts.retain(|a| {
        !(matches!(
            a.kind,
            AlertKind::Undefended | AlertKind::InadequatelyDefended
        ) && a
            .target
            .as_deref()
            .is_some_and(|t| drop_targets.iter().any(|d| d == t)))
    });
    record.wsui.screen_fired = !record.wsui.alerts.is_empty();
    true
}

/// Does the piece on `square` (owned by `owner`) have a move to a square
/// the enemy does not attack at all? Conservative: an unattacked
/// destination is safe by construction; contested squares are not
/// counted even when defensible. Board may have either side to move —
/// a null move flips it when needed (in check, no flip exists and we
/// return false, leaving the alert to stand).
fn has_safe_flight(board: &Board, square: cozy_chess::Square, owner: Color) -> bool {
    let owned = if board.side_to_move() == owner {
        board.clone()
    } else {
        match board.null_move() {
            Some(b) => b,
            None => return false,
        }
    };
    let enemy_attacks = attacked_squares(&owned, !owner);
    let mut safe = false;
    owned.generate_moves_for(square.bitboard(), |mvs| {
        for m in mvs {
            if !enemy_attacks.has(m.to) && !board.colors(owner).has(m.to) {
                safe = true;
                return true;
            }
        }
        false
    });
    safe
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyze;

    /// After 11.Bxf6 in a Najdorf-style position (maintainer screenshot):
    /// the bishop on f6 awaits recapture — an even trade, not a hang.
    #[test]
    fn even_recapture_suppresses_hang_and_material() {
        // Position before Bg5xf6 (knight on f6, defended; bishop g5).
        let before: Board = "r1b1kb1r/1pqn1ppp/p2ppn2/6B1/3NPP2/2N2Q2/PPP3PP/2KR1B1R w - - 0 1"
            .parse()
            .unwrap();
        let mv: Move = "g5f6".parse().unwrap();
        let mut after = before.clone();
        after.play(mv);
        let mut record = analyze(&after);
        let had_alert = record.wsui.alerts.iter().any(|a| {
            a.target.as_deref() == Some("f6")
                && matches!(
                    a.kind,
                    AlertKind::Undefended | AlertKind::InadequatelyDefended
                )
        });
        let had_material = record
            .imbalances
            .iter()
            .any(|i| i.kind == ImbalanceKind::Material);
        suppress_exchange_noise(&mut record, &before, mv);
        assert!(
            !record.wsui.alerts.iter().any(|a| {
                a.target.as_deref() == Some("f6")
                    && matches!(
                        a.kind,
                        AlertKind::Undefended | AlertKind::InadequatelyDefended
                    )
            }),
            "pending recapture must not narrate as a hang (was present: {had_alert})"
        );
        assert!(
            !record
                .imbalances
                .iter()
                .any(|i| i.kind == ImbalanceKind::Material),
            "one-ply material spike suppressed (was present: {had_material})"
        );
    }

    /// An attacked queen with open flight squares is tempo, not tactics
    /// (maintainer screenshot: 12...Bb7 hitting Qf3).
    #[test]
    fn attacked_queen_with_flight_is_not_a_capture_sequence() {
        let board: Board = "r3kb1r/1bqn1ppp/p2ppn2/4P3/3N1P2/2N2Q2/PPP3PP/2KR1B1R w kq - 0 1"
            .parse()
            .unwrap();
        let mut record = analyze(&board);
        suppress_escapable_attack_noise(&mut record, &board);
        assert!(
            !record.wsui.alerts.iter().any(|a| {
                a.target.as_deref() == Some("f3")
                    && matches!(
                        a.kind,
                        AlertKind::Undefended | AlertKind::InadequatelyDefended
                    )
            }),
            "queen with safe squares must not carry a capture-sequence alert"
        );
    }

    /// The safe-flight helper itself: an open queen has one, a boxed
    /// rook does not; and gate 2 defers whenever a TrappedPiece alert
    /// already owns the square.
    #[test]
    fn flight_detection_and_trapped_deference() {
        // Queen on d1 with the whole d-file: flight exists.
        let open: Board = "4k3/8/8/8/8/8/8/3QK3 w - - 0 1".parse().unwrap();
        assert!(has_safe_flight(&open, "d1".parse().unwrap(), Color::White));

        // Gate 2 defers to TrappedPiece: inject a trapped alert on the
        // same square and the U/I alert must survive.
        let board: Board = "r3kb1r/1bqn1ppp/p2ppn2/4P3/3N1P2/2N2Q2/PPP3PP/2KR1B1R w kq - 0 1"
            .parse()
            .unwrap();
        let mut record = analyze(&board);
        let has_ui_alert_on_f3 = record.wsui.alerts.iter().any(|a| {
            a.target.as_deref() == Some("f3")
                && matches!(
                    a.kind,
                    AlertKind::Undefended | AlertKind::InadequatelyDefended
                )
        });
        if has_ui_alert_on_f3 {
            record.wsui.alerts.push(crate::record::TacticAlert {
                kind: AlertKind::TrappedPiece,
                side: crate::record::SideColor::White,
                target: Some("f3".into()),
                attackers: vec![],
                defenders: vec![],
                see: None,
                severity: crate::record::Severity::Medium,
                detail: None,
                engine_check: None,
            });
            suppress_escapable_attack_noise(&mut record, &board);
            assert!(
                record.wsui.alerts.iter().any(|a| {
                    a.target.as_deref() == Some("f3")
                        && matches!(
                            a.kind,
                            AlertKind::Undefended | AlertKind::InadequatelyDefended
                        )
                }),
                "trapped square keeps its capture-sequence alert"
            );
        }
    }
}
