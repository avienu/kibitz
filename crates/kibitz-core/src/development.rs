//! Development tracker (run 11): the PRIOR side of the dream system.
//!
//! Every dream the imbalance detectors produce is derived from pawn
//! structure — which is fog at move 5. This module voices the classical
//! opening principles as dreams-under-uncertainty: the maintainer's
//! framing, verbatim — "the knight already knows where it wants to go;
//! the bishop doesn't yet — that's why the knight moves first." It is a
//! function over the MOVE SEQUENCE, not just a position, because "this
//! piece already moved twice" needs history; called with an empty move
//! list it still reports everything a bare position can show (sleeping
//! minors, castling state, queen sortie, unclaimed center) and stays
//! silent about wandering.
//!
//! The tracker's findings surface as ordinary [`Imbalance`] records
//! (kind `Development`, favors = the side that OWNS the dream) carrying
//! the run-11 prior plan hints — `CompleteDevelopment`,
//! `CastleIntoSafety`, `ClaimTheCenter`, plus the misplay observations
//! `QueenAheadOfHerArmy` and `SamePieceWandering` — so narration,
//! explain and suggestions all inherit them through the same machinery
//! (additive tokens; the record schema stays v3).
//!
//! Principles follow Jeremy Silman, The Complete Book of Chess
//! Strategy, pp. 3-6 (opening strategy / castling / development /
//! fianchetto), expressed as detection rules only — never book prose:
//! develop the whole army, castle quickly, the queen no further than
//! the second or third rank early on.

use std::cmp::Reverse;
use std::collections::BTreeMap;

use cozy_chess::{Board, Color, File, Move, Piece, Rank, Square};
use serde_json::json;

use crate::record::{Favors, FeatureRecord, Imbalance, ImbalanceKind, Magnitude, PlanHint};

/// The opening gate closes at this fullmove number even when development
/// is unfinished (a middlegame with a buried bishop is a minor-piece
/// story, not a development lecture).
pub const OPENING_MOVE_LIMIT: u32 = 13;

/// A queen at or beyond this relative rank (1-based) counts as a sortie
/// while development is unfinished. Silman's rule of thumb (CBOCS p. 5):
/// the queen belongs no further than the second or third rank early on —
/// so the fourth rank and beyond is "ahead of her army".
pub const QUEEN_SORTIE_RANK: u32 = 4;

/// One side's development ledger.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SideDevelopment {
    /// Minor pieces still on their home squares that have never moved.
    pub sleeping_minors: Vec<Square>,
    /// Minors currently on the board minus the sleepers.
    pub developed_minors: u32,
    /// Castling performed (from the move list), or a castled-looking king
    /// when no history is available.
    pub castled: bool,
    /// Any castling right remains.
    pub castling_available: bool,
    /// Uncastled king still on the central files (d/e/f).
    pub king_in_center: bool,
    /// King square + rook square of the preferred available castle.
    pub castle_squares: Option<(Square, Square)>,
    /// Queen out beyond [`QUEEN_SORTIE_RANK`] while at least two minors
    /// sleep.
    pub queen_sortie: Option<Square>,
    /// A non-pawn, non-king piece that moved at least twice while at
    /// least two minors sleep: (current square, times moved). The queen
    /// is not double-reported when the sortie already names her.
    pub wanderer: Option<(Square, u32)>,
    /// Still-home center pawns (d/e files).
    pub center_pawns_home: Vec<Square>,
    /// The unplayed two-square center-pawn advances still available.
    pub center_advances: Vec<Square>,
    /// Rough development tempo: developed minors + two for castling.
    pub tempo: i32,
}

/// The full development report for a position reached by `moves`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DevelopmentReport {
    pub white: SideDevelopment,
    pub black: SideDevelopment,
    /// The opening-phase gate: the tracker reports only while the game
    /// still has opening character — before move [`OPENING_MOVE_LIMIT`]
    /// ends, or until both sides are castled AND fully developed,
    /// whichever comes first (and never in an endgame).
    pub in_opening: bool,
}

impl DevelopmentReport {
    pub fn side(&self, color: Color) -> &SideDevelopment {
        match color {
            Color::White => &self.white,
            Color::Black => &self.black,
        }
    }
}

fn back_rank(color: Color) -> Rank {
    match color {
        Color::White => Rank::First,
        Color::Black => Rank::Eighth,
    }
}

/// 1-based rank from `color`'s point of view.
fn rel_rank(color: Color, sq: Square) -> u32 {
    match color {
        Color::White => sq.rank() as u32 + 1,
        Color::Black => 8 - sq.rank() as u32,
    }
}

/// Compute the development report for the position reached by playing
/// `moves` from `start`. Pure and engine-free; O(moves).
pub fn track(start: &Board, moves: &[Move]) -> DevelopmentReport {
    let mut board = start.clone();
    // Times the piece CURRENTLY standing on a square has moved.
    let mut moved: BTreeMap<Square, u32> = BTreeMap::new();
    let mut did_castle = [false, false];

    for &mv in moves {
        let side = board.side_to_move();
        let piece = board.piece_on(mv.from);
        let times = moved.remove(&mv.from).unwrap_or(0) + 1;
        moved.remove(&mv.to);
        // En passant: the captured pawn stands beside the destination.
        if piece == Some(Piece::Pawn)
            && mv.from.file() != mv.to.file()
            && board.piece_on(mv.to).is_none()
        {
            moved.remove(&Square::new(mv.to.file(), mv.from.rank()));
        }
        // cozy-chess castling is king-onto-own-rook.
        if piece == Some(Piece::King) && board.colors(side).has(mv.to) {
            did_castle[side as usize] = true;
            let (king_file, rook_file) = if mv.to.file() > mv.from.file() {
                (File::G, File::F)
            } else {
                (File::C, File::D)
            };
            moved.insert(Square::new(king_file, mv.from.rank()), times);
            moved.insert(Square::new(rook_file, mv.from.rank()), 1);
        } else {
            moved.insert(mv.to, times);
        }
        board.play(mv);
    }

    let side_report = |color: Color| -> SideDevelopment {
        let back = back_rank(color);
        let minor_homes = [
            (File::B, Piece::Knight),
            (File::G, Piece::Knight),
            (File::C, Piece::Bishop),
            (File::F, Piece::Bishop),
        ];
        let mut sleeping_minors: Vec<Square> = minor_homes
            .iter()
            .map(|&(file, piece)| (Square::new(file, back), piece))
            .filter(|&(sq, piece)| {
                board.colored_pieces(color, piece).has(sq) && !moved.contains_key(&sq)
            })
            .map(|(sq, _)| sq)
            .collect();
        sleeping_minors.sort();
        let minors = (board.colors(color)
            & (board.pieces(Piece::Knight) | board.pieces(Piece::Bishop)))
        .len();
        let developed_minors = minors - sleeping_minors.len() as u32;

        let king = board.king(color);
        // With no history, a king two-plus files off e on its back rank
        // reads as castled (same heuristic as the imbalance detector).
        let castled = did_castle[color as usize]
            || ((king.file() as i8 - File::E as i8).abs() >= 2 && king.rank() == back);
        let rights = board.castle_rights(color);
        let castling_available = rights.short.is_some() || rights.long.is_some();
        let king_in_center = !castled && (king.file() as i8 - File::E as i8).abs() <= 1;
        let castle_squares = rights
            .short
            .or(rights.long)
            .map(|rook_file| (king, Square::new(rook_file, back)));

        let sleeping = sleeping_minors.len();
        let queen_sortie = (sleeping >= 2)
            .then(|| {
                board
                    .colored_pieces(color, Piece::Queen)
                    .into_iter()
                    .find(|&q| rel_rank(color, q) >= QUEEN_SORTIE_RANK)
            })
            .flatten();
        let wanderer = if sleeping >= 2 {
            board
                .colors(color)
                .into_iter()
                .filter(|&sq| {
                    !matches!(board.piece_on(sq), Some(Piece::Pawn | Piece::King))
                        && (queen_sortie != Some(sq))
                })
                .filter_map(|sq| moved.get(&sq).map(|&n| (sq, n)))
                .filter(|&(_, n)| n >= 2)
                .max_by_key(|&(sq, n)| (n, Reverse(sq)))
        } else {
            None
        };

        let mut center_pawns_home = Vec::new();
        let mut center_advances = Vec::new();
        let pawn_rank = match color {
            Color::White => Rank::Second,
            Color::Black => Rank::Seventh,
        };
        let step: i8 = match color {
            Color::White => 1,
            Color::Black => -1,
        };
        for file in [File::D, File::E] {
            let home = Square::new(file, pawn_rank);
            if !board.colored_pieces(color, Piece::Pawn).has(home) || moved.contains_key(&home) {
                continue;
            }
            center_pawns_home.push(home);
            let one = home.try_offset(0, step);
            let two = home.try_offset(0, 2 * step);
            if let (Some(one), Some(two)) = (one, two) {
                if !board.occupied().has(one) && !board.occupied().has(two) {
                    center_advances.push(two);
                }
            }
        }

        let tempo = developed_minors as i32 + if castled { 2 } else { 0 };
        SideDevelopment {
            sleeping_minors,
            developed_minors,
            castled,
            castling_available,
            king_in_center,
            castle_squares,
            queen_sortie,
            wanderer,
            center_pawns_home,
            center_advances,
            tempo,
        }
    };

    let white = side_report(Color::White);
    let black = side_report(Color::Black);
    let done = |s: &SideDevelopment| s.sleeping_minors.is_empty() && s.castled;
    let in_opening = board.fullmove_number() as u32 <= OPENING_MOVE_LIMIT
        && !(done(&white) && done(&black))
        && crate::imbalance::phase(&board) != crate::record::Phase::Endgame;
    DevelopmentReport {
        white,
        black,
        in_opening,
    }
}

/// The five prior plan-hint tokens (run 11). Static suggestion mappers
/// exist for the first three; the misplay observations are voice-only.
pub const PRIOR_HINTS: &[&str] = &[
    "CompleteDevelopment",
    "CastleIntoSafety",
    "ClaimTheCenter",
    "QueenAheadOfHerArmy",
    "SamePieceWandering",
];

/// Is `hint` one of the development-prior tokens? Used by the suggester
/// to keep the prophylaxis machinery away from them: "deny the opponent
/// their development" degenerates into nonsense moves at static depth.
pub fn is_prior_hint(hint: &str) -> bool {
    PRIOR_HINTS.contains(&hint)
}

fn sq(square: Square) -> String {
    crate::record::square_name(square)
}

fn sqs(squares: &[Square]) -> Vec<String> {
    squares.iter().map(|&s| sq(s)).collect()
}

/// The prior imbalances for a report: one `Development` imbalance per
/// side that still has development dreams, favors = the side that OWNS
/// the plans (the dreamer, not the leader — the who-is-ahead story stays
/// with the position-only development detector). Empty outside the
/// opening gate.
pub fn imbalances(report: &DevelopmentReport) -> Vec<Imbalance> {
    if !report.in_opening {
        return Vec::new();
    }
    let mut out = Vec::new();
    for (dev, favors, suffix) in [
        (&report.white, Favors::White, "white"),
        (&report.black, Favors::Black, "black"),
    ] {
        let mut plans: Vec<PlanHint> = Vec::new();
        let mut evidence = BTreeMap::new();
        if !dev.sleeping_minors.is_empty() {
            evidence.insert(
                format!("sleeping_minors_{suffix}"),
                json!(sqs(&dev.sleeping_minors)),
            );
            plans.push(PlanHint {
                hint: "CompleteDevelopment".into(),
                squares: sqs(&dev.sleeping_minors),
            });
        }
        if !dev.castled {
            if let Some((king, rook)) = dev.castle_squares.filter(|_| dev.castling_available) {
                if dev.king_in_center {
                    evidence.insert(format!("king_in_center_{suffix}"), json!("available"));
                }
                plans.push(PlanHint {
                    hint: "CastleIntoSafety".into(),
                    squares: vec![sq(king), sq(rook)],
                });
            } else if dev.king_in_center {
                evidence.insert(format!("king_in_center_{suffix}"), json!("lost"));
            }
        }
        if !dev.center_advances.is_empty() {
            evidence.insert(
                format!("center_unclaimed_{suffix}"),
                json!(sqs(&dev.center_advances)),
            );
            plans.push(PlanHint {
                hint: "ClaimTheCenter".into(),
                squares: sqs(&dev.center_advances),
            });
        }
        if let Some(queen) = dev.queen_sortie {
            evidence.insert(format!("queen_sortie_{suffix}"), json!(sq(queen)));
            plans.push(PlanHint {
                hint: "QueenAheadOfHerArmy".into(),
                squares: vec![sq(queen)],
            });
        }
        if let Some((square, times)) = dev.wanderer {
            evidence.insert(
                format!("wanderer_{suffix}"),
                json!({ "square": sq(square), "times": times }),
            );
            plans.push(PlanHint {
                hint: "SamePieceWandering".into(),
                squares: vec![sq(square)],
            });
        }
        if plans.is_empty() {
            continue;
        }
        let misplay = dev.queen_sortie.is_some()
            || dev.wanderer.is_some()
            || (dev.king_in_center && !dev.castling_available);
        let magnitude = if dev.sleeping_minors.len() >= 2 || misplay {
            Magnitude::Clear
        } else {
            Magnitude::Minor
        };
        out.push(Imbalance {
            kind: ImbalanceKind::Development,
            favors,
            magnitude,
            evidence,
            plans,
        });
    }
    out
}

/// Fold the prior into an analyzed record: append the prior imbalances,
/// restore the magnitude ordering, and re-synthesize the composite plans
/// so the new hints join the convergence machinery. A no-op outside the
/// opening gate. Additive only — nothing already in the record is
/// removed, and the schema stays v3.
pub fn augment(record: &mut FeatureRecord, report: &DevelopmentReport) {
    let add = imbalances(report);
    if add.is_empty() {
        return;
    }
    record.imbalances.extend(add);
    record.imbalances.sort_by_key(|i| Reverse(i.magnitude));
    record.composite_plans = crate::plans::synthesize(&record.imbalances);
}

#[cfg(test)]
mod tests {
    //! Cited golden tests on public-domain classics (FEN/moves +
    //! citation only, never book prose).

    use super::*;

    fn moves(ucis: &[&str]) -> (Board, Vec<Move>) {
        let start = Board::default();
        let mut board = start.clone();
        let mut out = Vec::new();
        for uci in ucis {
            let mv: Move = uci.parse().unwrap();
            assert!(board.is_legal(mv), "illegal {uci}");
            board.play(mv);
            out.push(mv);
        }
        (start, out)
    }

    fn names(squares: &[Square]) -> Vec<String> {
        squares.iter().map(|&s| sq(s)).collect()
    }

    fn hints_of(report: &DevelopmentReport, favors: Favors) -> Vec<String> {
        imbalances(report)
            .iter()
            .filter(|i| i.favors == favors)
            .flat_map(|i| i.plans.iter().map(|p| p.hint.clone()))
            .collect()
    }

    /// The very start: everyone is asleep, both sides dream of the
    /// center, nobody has misplayed.
    #[test]
    fn initial_position_everyone_sleeps() {
        let (start, mv) = moves(&[]);
        let r = track(&start, &mv);
        assert!(r.in_opening);
        assert_eq!(r.white.sleeping_minors.len(), 4);
        assert_eq!(r.black.sleeping_minors.len(), 4);
        assert!(r.white.queen_sortie.is_none());
        assert!(r.white.wanderer.is_none());
        // The d/e advances are blocked at the start by nothing — both
        // two-square pushes are dreams.
        assert_eq!(names(&r.white.center_advances), ["d4", "e4"]);
        assert_eq!(names(&r.black.center_advances), ["d5", "e5"]);
        assert_eq!(r.white.tempo, 0);
    }

    /// Morphy vs Duke Karl / Count Isouard, Paris Opera 1858, after
    /// 9.Bg5: White's whole army is out with the king ready to castle
    /// long; Black's b8-knight and f8-bishop are still dreaming.
    #[test]
    fn opera_game_development_with_tempo() {
        let (start, mv) = moves(&[
            "e2e4", "e7e5", "g1f3", "d7d6", "d2d4", "c8g4", "d4e5", "g4f3", "d1f3", "d6e5", "f1c4",
            "g8f6", "f3b3", "d8e7", "b1c3", "c7c6", "c1g5",
        ]);
        let r = track(&start, &mv);
        assert!(r.in_opening);
        assert!(r.white.sleeping_minors.is_empty(), "{r:?}");
        assert_eq!(names(&r.black.sleeping_minors), ["b8", "f8"]);
        assert!(r.white.tempo > r.black.tempo, "{r:?}");
        let black_hints = hints_of(&r, Favors::Black);
        assert!(black_hints.contains(&"CompleteDevelopment".to_string()));
        assert!(black_hints.contains(&"CastleIntoSafety".to_string()));
        let white_hints = hints_of(&r, Favors::White);
        assert!(white_hints.contains(&"CastleIntoSafety".to_string()));
        assert!(!white_hints.contains(&"CompleteDevelopment".to_string()));
    }

    /// Scholar's-mate-adjacent early queen (1.e4 e5 2.Qh5 — the Wayward
    /// Queen Attack): the queen is out beyond the third rank while all
    /// four minors sleep. Rule per Jeremy Silman, The Complete Book of
    /// Chess Strategy, p. 5 (the queen no further than the second or
    /// third rank early on).
    #[test]
    fn wayward_queen_sortie_fires() {
        let (start, mv) = moves(&["e2e4", "e7e5", "d1h5"]);
        let r = track(&start, &mv);
        assert_eq!(r.white.queen_sortie.map(sq), Some("h5".into()));
        let hints = hints_of(&r, Favors::White);
        assert!(
            hints.contains(&"QueenAheadOfHerArmy".to_string()),
            "{hints:?}"
        );
        // The queen is the sortie, not additionally the wanderer.
        assert!(r.white.wanderer.is_none());
    }

    /// Counter-anchor for the same rule: a queen on the second/third
    /// rank is within Silman's bound — no sortie (1.e4 e5 2.Nf3 Nc6
    /// 3.Qe2, an old Chigorin handling).
    #[test]
    fn queen_on_second_rank_is_no_sortie() {
        let (start, mv) = moves(&["e2e4", "e7e5", "g1f3", "b8c6", "d1e2"]);
        let r = track(&start, &mv);
        assert!(r.white.queen_sortie.is_none(), "{r:?}");
        let hints = hints_of(&r, Favors::White);
        assert!(!hints.contains(&"QueenAheadOfHerArmy".to_string()));
    }

    /// Emanuel Lasker, Common Sense in Chess (1896), first lecture: do
    /// not move the same piece twice in the opening. The Blackburne
    /// Shilling pattern (1.e4 e5 2.Nf3 Nc6 3.Bc4 Nd4) moves the c6
    /// knight again while three Black pieces sleep — the wanderer is
    /// named on d4.
    #[test]
    fn same_piece_wandering_names_the_knight() {
        let (start, mv) = moves(&["e2e4", "e7e5", "g1f3", "b8c6", "f1c4", "c6d4"]);
        let r = track(&start, &mv);
        assert_eq!(r.black.wanderer, Some((Square::D4, 2)));
        let hints = hints_of(&r, Favors::Black);
        assert!(
            hints.contains(&"SamePieceWandering".to_string()),
            "{hints:?}"
        );
    }

    /// Fianchetto counts as development (Jeremy Silman, The Complete
    /// Book of Chess Strategy, p. 6): after 1.b3 e5 2.Bb2 the c1-bishop
    /// has left home and only three White pieces still sleep.
    #[test]
    fn fianchetto_is_development() {
        let (start, mv) = moves(&["b2b3", "e7e5", "c1b2"]);
        let r = track(&start, &mv);
        assert_eq!(names(&r.white.sleeping_minors), ["b1", "f1", "g1"]);
        assert_eq!(r.white.developed_minors, 1);
    }

    /// Castling is tracked through cozy-chess's king-onto-rook encoding,
    /// and a castled side loses the CastleIntoSafety dream.
    #[test]
    fn castling_resolves_the_king_dream() {
        // Four Knights development race: 1.e4 e5 2.Nf3 Nc6 3.Nc3 Nf6
        // 4.Bb5 Bb4 5.O-O.
        let (start, mv) = moves(&[
            "e2e4", "e7e5", "g1f3", "b8c6", "b1c3", "g8f6", "f1b5", "f8b4", "e1h1",
        ]);
        let r = track(&start, &mv);
        assert!(r.white.castled);
        assert!(!r.white.king_in_center);
        let white_hints = hints_of(&r, Favors::White);
        assert!(!white_hints.contains(&"CastleIntoSafety".to_string()));
        assert!(white_hints.contains(&"CompleteDevelopment".to_string())); // c1 sleeps
        let black_hints = hints_of(&r, Favors::Black);
        assert!(black_hints.contains(&"CastleIntoSafety".to_string()));
        assert_eq!(
            imbalances(&r)
                .iter()
                .find(|i| i.favors == Favors::Black)
                .and_then(|i| i.plans.iter().find(|p| p.hint == "CastleIntoSafety"))
                .map(|p| p.squares.clone()),
            Some(vec!["e8".to_string(), "h8".to_string()])
        );
    }

    /// The opening gate: once both sides are castled and fully
    /// developed the tracker falls silent — the dreams came true.
    #[test]
    fn gate_closes_when_both_sides_are_done() {
        // Four Knights, both sides complete development and castle:
        // 1.e4 e5 2.Nf3 Nc6 3.Nc3 Nf6 4.Bb5 Bb4 5.O-O O-O 6.d3 d6
        // 7.Bg5 Bxc3 8.bxc3 Bg4.
        let (start, mv) = moves(&[
            "e2e4", "e7e5", "g1f3", "b8c6", "b1c3", "g8f6", "f1b5", "f8b4", "e1h1", "e8h8", "d2d3",
            "d7d6", "c1g5", "b4c3", "b2c3", "c8g4",
        ]);
        let r = track(&start, &mv);
        assert!(!r.in_opening, "{r:?}");
        assert!(imbalances(&r).is_empty());
    }

    /// The gate also closes on the move clock: sleepers at move 20 are a
    /// minor-piece story for the imbalance detectors, not a development
    /// lecture. (Position-only invocation: a reconstructed middlegame.)
    #[test]
    fn gate_closes_on_the_move_clock() {
        let board: Board = "r1bq1rk1/pp3ppp/2n2n2/3p4/8/2NBPN2/PPP2PPP/R1BQ1RK1 w - - 0 20"
            .parse()
            .unwrap();
        let r = track(&board, &[]);
        assert!(!r.in_opening);
    }

    /// Position-only invocation (no history): sleeping minors, castling
    /// state and the unclaimed center still report; wandering cannot.
    #[test]
    fn position_only_tracking_reports_what_a_fen_can_show() {
        // After 1.e4 e5 2.Qh5 as a bare FEN.
        let board: Board = "rnbqkbnr/pppp1ppp/8/4p2Q/4P3/8/PPPP1PPP/RNB1KBNR b KQkq - 1 2"
            .parse()
            .unwrap();
        let r = track(&board, &[]);
        assert!(r.in_opening);
        assert_eq!(r.white.sleeping_minors.len(), 4);
        assert_eq!(r.white.queen_sortie, Some(Square::H5));
        assert!(r.white.wanderer.is_none());
        assert_eq!(names(&r.white.center_advances), ["d4"]);
    }

    /// Augment folds the prior into a record and re-synthesizes the
    /// composites; outside the gate it is a no-op.
    #[test]
    fn augment_is_additive_and_gated() {
        let (start, mv) = moves(&["e2e4", "e7e5", "g1f3"]);
        let mut board = start.clone();
        for &m in &mv {
            board.play(m);
        }
        let mut record = crate::analyze(&board);
        let before = record.imbalances.len();
        augment(&mut record, &track(&start, &mv));
        assert!(record.imbalances.len() > before);
        assert!(record
            .imbalances
            .iter()
            .any(|i| i.plans.iter().any(|p| p.hint == "CompleteDevelopment")));

        // Gate closed: nothing changes.
        let closed: Board = "r1bq1rk1/pp3ppp/2n2n2/3p4/8/2NBPN2/PPP2PPP/R1BQ1RK1 w - - 0 20"
            .parse()
            .unwrap();
        let mut record = crate::analyze(&closed);
        let snapshot = record.clone();
        augment(&mut record, &track(&closed, &[]));
        assert_eq!(record, snapshot);
    }
}
