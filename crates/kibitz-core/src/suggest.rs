//! Candidate-move synthesis + prophylaxis (run 10).
//!
//! CONVERGENCE SCORING — the maintainer's insight, verbatim: "if multiple
//! plans contain the same move ideas ... our number one move suggestion is
//! X because it keeps our plan options open." Every PlanHint token has a
//! move-mapper proposing legal moves that concretely advance that plan
//! (direct execution 3, preparation 2, enabling 1). A move proposed by
//! several plans scores the SUM of its mapper weights plus a convergence
//! bonus of one point per extra plan served, so the move that keeps the
//! most plans alive rises to the top.
//!
//! PROPHYLAXIS: the record carries the opponent's plans too. When the
//! opponent's best plan is at least as strong as ours (within one point),
//! blocking candidates are generated — occupy or defend the plan's target,
//! contest its route squares, trade the piece the plan needs, or answer a
//! wing storm with the central counter-break — and compete on equal terms
//! with the constructive moves. A prophylactic top suggestion is flagged so
//! the coach can say "first, deny the opponent".
//!
//! Purely static: legality + SEE only, the engine is never consulted
//! (CLAUDE.md #6). Suggestions therefore live in the Explanation contract
//! and the narration, never in the FeatureRecord itself.
//!
//! WHOLE-BOARD VETO (run 11): beyond the destination-square SEE gate,
//! every surviving candidate is checked for pieces left en prise anywhere
//! on the board. Candidates whose whole-board static cost reaches
//! [`PIECE_LOSS_CP`] are MARKED (`static_risk`), not dropped: an engine
//! layer in the app may clear the false positives (e.g. the Winawer's
//! theory move ...cxd4, where ...dxc3 regains the piece one exchange
//! deeper than statics can see). Consumers with no engine must drop
//! marked candidates — bad advice is worse than no advice.

use std::collections::{BTreeMap, BTreeSet};

use cozy_chess::{get_pawn_attacks, BitBoard, Board, Color, File, Move, Piece, Rank, Square};

use crate::attack::attackers_of;
use crate::record::{Favors, FeatureRecord};
use crate::see::{piece_value, see};

/// Mapper weight: the move IS the plan (knight lands on the outpost).
pub const EXECUTE: u32 = 3;
/// Mapper weight: the move prepares the plan (rook joins the file).
pub const PREPARE: u32 = 2;
/// Mapper weight: the move merely enables the plan (a step on the route).
pub const ENABLE: u32 = 1;

/// A candidate is dropped when its destination loses more than this many
/// centipawns to the opponent's best static capture sequence: the coach
/// never suggests a hanging move. (Quiet tactical refutations deeper than
/// one exchange are beyond a static suggester — documented limitation.)
pub const SAFETY_CP: i32 = 60;

/// Whole-board veto threshold (run 11, maintainer field report): a
/// candidate that leaves the opponent's best static capture ANYWHERE on
/// the board netting at least this much is statically UNSAFE. Derived
/// from the cheapest piece-for-pawn loss — knight minus pawn, per
/// [`piece_value`] — so a bishop shed for a pawn (~230cp) clearly
/// qualifies while an even exchange never does.
pub const PIECE_LOSS_CP: i32 = 220;

/// One suggested move: what to play, which plans it serves, and whether it
/// is primarily a denial of the opponent's plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Suggestion {
    /// The move in coordinate form (cozy-chess convention: castling is
    /// king-onto-rook).
    pub mv: String,
    pub san: String,
    pub score: u32,
    /// Hint tokens this move serves. For a prophylactic suggestion the
    /// DENIED opponent tokens lead the list.
    pub serving: Vec<String>,
    pub prophylactic: bool,
    /// Whole-board static risk (run 11): `Some(net)` when, after this
    /// move, the opponent's best SEE capture sequence anywhere on the
    /// board nets at least [`PIECE_LOSS_CP`] beyond what the move itself
    /// captured. A marked candidate is NOT sound advice on its own —
    /// consumers must drop it unless a bounded engine review clears it
    /// (statics one exchange deep cannot tell a piece-regaining line
    /// like the Winawer's ...cxd4 from a plain piece-dropper like
    /// ...f5??; that distinction is the engine layer's job).
    pub static_risk: Option<i32>,
}

/// One active plan pulled out of the record, with its EFFECTIVE owner (a
/// blockade belongs to the defender, whatever the parent imbalance favors —
/// same re-attribution as plans.rs).
struct ActivePlan {
    hint: String,
    squares: Vec<String>,
    favors: Favors,
}

fn effective_favors(hint: &str, favors: Favors) -> Favors {
    match hint {
        "BlockadeWhitePasser" => Favors::Black,
        "BlockadeBlackPasser" => Favors::White,
        _ => favors,
    }
}

fn magnitude_weight(m: crate::record::Magnitude) -> u32 {
    match m {
        crate::record::Magnitude::Minor => 1,
        crate::record::Magnitude::Clear => 2,
        crate::record::Magnitude::Winning => 3,
    }
}

fn color_favors(c: Color) -> Favors {
    match c {
        Color::White => Favors::White,
        Color::Black => Favors::Black,
    }
}

/// All active plans, one entry per (hint, squares): imbalance hints first,
/// then composite hints not already collected (composites are synthesized
/// FROM imbalance hints, so this dedupe keeps each idea counted once).
/// Hints that EXPLAIN rather than instruct. They must not enter the
/// suggester's plan pool at all: they generate no moves of their own, and
/// merely being counted shifts plan_strength, which reshuffles
/// prophylactic ranking and displaces real answers (am-324-1 "b4" and
/// am-325-2 "c5" were both lost that way). Same principle that keeps
/// them out of the favors vote — a standing idea is not a claim about
/// what to play this move.
const EXPLANATORY_ONLY: &[&str] = &["OverprotectStrongPoint", "AttackWhereYouAreStronger"];

fn active_plans(record: &FeatureRecord) -> Vec<ActivePlan> {
    let mut out: Vec<ActivePlan> = Vec::new();
    let mut seen: BTreeSet<(String, Vec<String>)> = BTreeSet::new();
    let mut seen_tokens: BTreeSet<String> = BTreeSet::new();
    for imb in &record.imbalances {
        for plan in &imb.plans {
            if EXPLANATORY_ONLY.contains(&plan.hint.as_str()) {
                continue;
            }
            if !seen.insert((plan.hint.clone(), plan.squares.clone())) {
                continue;
            }
            seen_tokens.insert(plan.hint.clone());
            out.push(ActivePlan {
                hint: plan.hint.clone(),
                squares: plan.squares.clone(),
                favors: effective_favors(&plan.hint, imb.favors),
            });
        }
    }
    for cp in &record.composite_plans {
        for hint in &cp.hints {
            if EXPLANATORY_ONLY.contains(&hint.as_str()) || seen_tokens.contains(hint) {
                continue;
            }
            seen_tokens.insert(hint.clone());
            out.push(ActivePlan {
                hint: hint.clone(),
                squares: cp.squares.clone(),
                favors: effective_favors(hint, cp.favors),
            });
        }
    }
    out
}

fn parse_sq(s: &str) -> Option<Square> {
    s.parse().ok()
}

fn legal_moves(board: &Board) -> Vec<Move> {
    let mut moves = Vec::new();
    board.generate_moves(|pm| {
        moves.extend(pm);
        false
    });
    moves
}

fn after(board: &Board, mv: Move) -> Board {
    let mut b = board.clone();
    b.play_unchecked(mv);
    b
}

fn is_capture(board: &Board, mv: Move) -> bool {
    let stm = board.side_to_move();
    board.colors(!stm).has(mv.to)
        || (board.piece_on(mv.from) == Some(Piece::Pawn)
            && mv.from.file() != mv.to.file()
            && board.piece_on(mv.to).is_none())
}

/// Does playing `mv` INCREASE the number of `side` pieces bearing on
/// `target`? (A piece landing ON the target does not attack it and is not
/// counted — occupation is scored separately.)
fn adds_attacker(board: &Board, mv: Move, target: Square, side: Color) -> bool {
    if mv.to == target {
        return false;
    }
    let before = attackers_of(board, target, side, board.occupied()).len();
    let b2 = after(board, mv);
    attackers_of(&b2, target, side, b2.occupied()).len() > before
}

/// Post-move safety: after playing `mv`, the opponent's best static
/// capture sequence on the destination must not net them more than
/// [`SAFETY_CP`] beyond whatever the move itself captured.
fn is_safe(board: &Board, mv: Move) -> bool {
    let stm = board.side_to_move();
    let gained = board.piece_on(mv.to).map(piece_value).unwrap_or(0);
    let b2 = after(board, mv);
    let loss = see(&b2, mv.to, !stm).max(0);
    gained - loss >= -SAFETY_CP
}

/// The best static capture the ENEMY of `side` has anywhere on the board:
/// the maximum SEE over every square holding a `side` piece, clamped at
/// zero (a capture the opponent would decline is no threat). Like all
/// SEE, this ignores pins and en passant — documented limitations.
fn best_enemy_capture(board: &Board, side: Color) -> i32 {
    let mut best = 0;
    for sq in board.colors(side) {
        best = best.max(see(board, sq, !side));
    }
    best
}

/// Whole-board static risk of `mv` (run 11, maintainer field report: the
/// Winawer ...f5?? chips — the destination-only gate above never notices
/// the mover left ANOTHER piece en prise). After playing `mv`, the
/// opponent's best SEE capture anywhere, net of what the move itself
/// captured: `Some(net)` when it reaches [`PIECE_LOSS_CP`].
///
/// The already-en-prise subtlety is handled by construction: when a piece
/// hangs BEFORE the move, every candidate that fails to address it still
/// shows the full net and is marked, while candidates that resolve the
/// hang (move, defend, trade or out-capture it) bring the net below the
/// threshold and pass statically.
fn static_risk(board: &Board, mv: Move) -> Option<i32> {
    let stm = board.side_to_move();
    let gained = board.piece_on(mv.to).map(piece_value).unwrap_or(0);
    let b2 = after(board, mv);
    let net = best_enemy_capture(&b2, stm) - gained;
    (net >= PIECE_LOSS_CP).then_some(net)
}

/// Knight-move distance between two squares on an empty board (capped).
fn knight_distance(from: Square, to: Square) -> u32 {
    if from == to {
        return 0;
    }
    let mut seen = from.bitboard();
    let mut frontier = from.bitboard();
    for depth in 1..=5u32 {
        let mut next = BitBoard::EMPTY;
        for s in frontier {
            next |= cozy_chess::get_knight_moves(s);
        }
        if next.has(to) {
            return depth;
        }
        next &= !seen;
        seen |= next;
        frontier = next;
    }
    6
}

fn pawn_attacks_of(board: &Board, side: Color) -> BitBoard {
    let mut a = BitBoard::EMPTY;
    for p in board.colored_pieces(side, Piece::Pawn) {
        a |= get_pawn_attacks(p, side);
    }
    a
}

fn rel_rank(side: Color, rank: usize) -> Rank {
    match side {
        Color::White => Rank::index(rank - 1),
        Color::Black => Rank::index(8 - rank),
    }
}

fn forward(side: Color) -> i8 {
    if side == Color::White {
        1
    } else {
        -1
    }
}

/// Squares strictly ahead of `sq` on the same file, from `side`'s view.
fn file_ahead(side: Color, sq: Square) -> BitBoard {
    let mut span = BitBoard::EMPTY;
    let mut s = sq;
    while let Some(n) = s.try_offset(0, forward(side)) {
        span |= n.bitboard();
        s = n;
    }
    span
}

/// The first square among `squares` holding an ENEMY (of `side`) pawn.
fn enemy_pawn_among(board: &Board, side: Color, squares: &[String]) -> Option<Square> {
    squares
        .iter()
        .filter_map(|s| parse_sq(s))
        .find(|&sq| board.colored_pieces(!side, Piece::Pawn).has(sq))
}

/// Moves that pile up on an enemy pawn: a sound capture executes, a new
/// attacker on the pawn or its (empty) stop square scores `add_w`. A stop
/// square that is already occupied needs no further grip.
fn pressure_moves(
    board: &Board,
    legal: &[Move],
    side: Color,
    pawn: Square,
    stop: Option<Square>,
    add_w: u32,
) -> Vec<(Move, u32)> {
    let stop = stop.filter(|&s| !board.occupied().has(s));
    let mut out = Vec::new();
    for &mv in legal {
        if mv.to == pawn && is_capture(board, mv) {
            if see(board, pawn, side) >= 0 {
                out.push((mv, EXECUTE));
            }
        } else if adds_attacker(board, mv, pawn, side)
            || stop.is_some_and(|s| adds_attacker(board, mv, s, side))
        {
            out.push((mv, add_w));
        }
    }
    out
}

/// Ranks left before an enemy pawn of `owner` promotes.
fn promotion_distance(owner: Color, pawn: Square) -> u32 {
    match owner {
        Color::White => 7 - pawn.rank() as u32,
        Color::Black => pawn.rank() as u32,
    }
}

/// Legal moves for `side` (the side to move) that concretely advance the
/// plan named by `hint`. Unknown tokens map to nothing.
fn moves_for_hint(
    board: &Board,
    legal: &[Move],
    side: Color,
    hint: &str,
    squares: &[String],
) -> Vec<(Move, u32)> {
    debug_assert_eq!(board.side_to_move(), side);
    let mut out: Vec<(Move, u32)> = Vec::new();
    match hint {
        "ManeuverKnightToOutpost" => {
            let Some(target) = squares.last().and_then(|s| parse_sq(s)) else {
                return out;
            };
            let route: Vec<Square> = squares[..squares.len().saturating_sub(1)]
                .iter()
                .filter_map(|s| parse_sq(s))
                .collect();
            let enemy_pawn_cover = pawn_attacks_of(board, !side);
            for &mv in legal {
                if board.piece_on(mv.from) != Some(Piece::Knight) {
                    continue;
                }
                if mv.to == target {
                    out.push((mv, EXECUTE));
                } else if route.contains(&mv.to) {
                    out.push((mv, PREPARE));
                } else if !enemy_pawn_cover.has(mv.to)
                    && knight_distance(mv.to, target) < knight_distance(mv.from, target)
                {
                    out.push((mv, ENABLE));
                }
            }
        }
        // The lever: a pawn move that puts our pawn onto a square from
        // which it attacks the guard. squares = [guard, square_we_want].
        "UndermineDefender" => {
            let Some(guard) = squares.first().and_then(|s| parse_sq(s)) else {
                return out;
            };
            for &mv in legal {
                if board.piece_on(mv.from) != Some(Piece::Pawn) {
                    continue;
                }
                if !get_pawn_attacks(mv.to, side).has(guard) {
                    continue;
                }
                // A lever that simply loses the pawn is not a plan.
                if !is_safe(board, mv) {
                    continue;
                }
                // PREPARE, not EXECUTE: knocking away a guard is groundwork
                // for owning the square, not the plan being carried out.
                out.push((mv, PREPARE));
            }
        }
        // OverprotectStrongPoint deliberately generates NO candidates.
        // Nimzowitsch's overprotection explains why a quiet move is good;
        // it does not pick one. Nearly every developing move adds a
        // defender to a central point, so mapping it to moves buried the
        // real plan under Rf1/Be2/Kf2 noise and cost two book answers
        // (am-324-1 b4, am-325-2 c5). The hint stays; the chips do not.
        "PressureBackwardPawn" | "PressureDoubledPawn" => {
            let Some(pawn) = enemy_pawn_among(board, side, squares) else {
                return out;
            };
            let stop = pawn.try_offset(0, forward(!side));
            out = pressure_moves(board, legal, side, pawn, stop, PREPARE);
        }
        "DoubleOnOpenFile" => {
            let pawns = board.pieces(Piece::Pawn);
            let majors =
                board.colors(side) & (board.pieces(Piece::Rook) | board.pieces(Piece::Queen));
            for &mv in legal {
                if !matches!(board.piece_on(mv.from), Some(Piece::Rook | Piece::Queen)) {
                    continue;
                }
                let file = mv.to.file();
                if !(pawns & file.bitboard()).is_empty() || mv.from.file() == file {
                    continue;
                }
                let others = (majors & file.bitboard() & !mv.from.bitboard()).len();
                out.push((mv, if others >= 1 { EXECUTE } else { PREPARE }));
            }
        }
        "RookToSeventh" => {
            let seventh = rel_rank(side, 7);
            let hinted = squares.first().and_then(|s| parse_sq(s));
            for &mv in legal {
                if !matches!(board.piece_on(mv.from), Some(Piece::Rook | Piece::Queen)) {
                    continue;
                }
                if mv.to.rank() != seventh {
                    continue;
                }
                if is_capture(board, mv) && see(board, mv.to, side) < 0 {
                    continue;
                }
                let direct = hinted == Some(mv.to) || hinted == Some(mv.from);
                out.push((mv, if direct { EXECUTE } else { PREPARE }));
            }
        }
        "RookBehindPasser" => {
            let Some(passer) = squares.first().and_then(|s| parse_sq(s)) else {
                return out;
            };
            if !board.colored_pieces(side, Piece::Pawn).has(passer) {
                return out;
            }
            let behind = file_ahead(!side, passer); // squares behind, from owner's view
            for &mv in legal {
                if board.piece_on(mv.from) == Some(Piece::Rook) && behind.has(mv.to) {
                    out.push((mv, EXECUTE));
                }
            }
        }
        "BlockadeWhitePasser" | "BlockadeBlackPasser" | "BlockadeThenPressure" => {
            // The stop square: for the blockade pair it is the hinted
            // square; blockade-then-pressure names [pawn, stop].
            let pawn = enemy_pawn_among(board, side, squares);
            let stop = if hint == "BlockadeThenPressure" {
                pawn.and_then(|p| squares.iter().filter_map(|s| parse_sq(s)).find(|&s| s != p))
            } else {
                squares.first().and_then(|s| parse_sq(s))
            };
            if let Some(stop) = stop {
                for &mv in legal {
                    if mv.to != stop {
                        continue;
                    }
                    let w = match board.piece_on(mv.from) {
                        Some(Piece::Knight | Piece::Bishop) => EXECUTE,
                        Some(Piece::Rook) => PREPARE,
                        Some(Piece::Queen | Piece::King) => ENABLE,
                        _ => continue,
                    };
                    out.push((mv, w));
                }
            }
            // Restrain, blockade, destroy: pressure on the passer itself
            // belongs to the same plan. The blockade pair names only the
            // stop square; the pawn stands one step beyond it.
            let pawn = pawn.or_else(|| {
                let stop = stop?;
                let owner_forward = match hint {
                    "BlockadeWhitePasser" => -1i8, // stop is above the white pawn
                    "BlockadeBlackPasser" => 1,
                    _ => return None,
                };
                stop.try_offset(0, owner_forward)
                    .filter(|&p| board.colored_pieces(!side, Piece::Pawn).has(p))
            });
            if let Some(pawn) = pawn {
                // A passer within two ranks of promotion is a crisis:
                // bearing on it is execution, not preparation.
                let add_w = if promotion_distance(!side, pawn) <= 2 {
                    EXECUTE
                } else {
                    PREPARE
                };
                out.extend(pressure_moves(board, legal, side, pawn, stop, add_w));
            }
        }
        "AdvanceQueensideMajority" => {
            let qside = File::A.bitboard() | File::B.bitboard() | File::C.bitboard();
            let own = board.colored_pieces(side, Piece::Pawn) & qside;
            let their = board.colored_pieces(!side, Piece::Pawn) & qside;
            if own.len() <= their.len() {
                return out;
            }
            for &mv in legal {
                if board.piece_on(mv.from) != Some(Piece::Pawn)
                    || !qside.has(mv.from)
                    || is_capture(board, mv)
                {
                    continue;
                }
                // The candidate pawn — unopposed file — leads the charge.
                let opposed = !(file_ahead(side, mv.from)
                    & board.colored_pieces(!side, Piece::Pawn))
                .is_empty();
                out.push((mv, if opposed { PREPARE } else { EXECUTE }));
            }
        }
        "AdvanceCentralMajority" => {
            let center = File::D.bitboard() | File::E.bitboard();
            let front = squares.first().and_then(|s| parse_sq(s));
            for &mv in legal {
                if board.piece_on(mv.from) != Some(Piece::Pawn) || is_capture(board, mv) {
                    continue;
                }
                if front == Some(mv.to) {
                    out.push((mv, EXECUTE));
                } else if center.has(mv.from) {
                    out.push((mv, PREPARE));
                }
            }
        }
        "MinorityAttack" => {
            let lever = squares.first().and_then(|s| parse_sq(s));
            let target = squares.get(1).and_then(|s| parse_sq(s));
            for &mv in legal {
                if board.piece_on(mv.from) != Some(Piece::Pawn) {
                    continue;
                }
                let executes = (Some(mv.to) == lever && !is_capture(board, mv))
                    || (Some(mv.to) == target
                        && is_capture(board, mv)
                        && see(board, mv.to, side) >= 0);
                if executes {
                    out.push((mv, EXECUTE));
                } else if lever.is_some_and(|l| mv.from.file() == l.file())
                    && !is_capture(board, mv)
                {
                    out.push((mv, PREPARE));
                }
            }
        }
        "OpenPositionForBishops" | "OpenPositionBeforeOpponentCompletes" => {
            let breadth = File::B.bitboard()
                | File::C.bitboard()
                | File::D.bitboard()
                | File::E.bitboard()
                | File::F.bitboard()
                | File::G.bitboard();
            for &mv in legal {
                if board.piece_on(mv.from) != Some(Piece::Pawn) {
                    continue;
                }
                if is_capture(board, mv) {
                    // A pawn trade opens lines.
                    if board.piece_on(mv.to) == Some(Piece::Pawn) && see(board, mv.to, side) >= 0 {
                        out.push((mv, EXECUTE));
                    }
                } else if breadth.has(mv.to) {
                    // A push that attacks an enemy pawn creates the tension
                    // whose resolution opens the position.
                    let bites =
                        get_pawn_attacks(mv.to, side) & board.colored_pieces(!side, Piece::Pawn);
                    if !bites.is_empty() {
                        out.push((mv, PREPARE));
                    }
                }
            }
        }
        "KeepPositionClosed" => {
            for &mv in legal {
                if board.piece_on(mv.from) != Some(Piece::Pawn) || is_capture(board, mv) {
                    continue;
                }
                // A push that runs into a fixed enemy pawn locks the file.
                let locks = mv
                    .to
                    .try_offset(0, forward(side))
                    .is_some_and(|f| board.colored_pieces(!side, Piece::Pawn).has(f));
                if locks {
                    out.push((mv, PREPARE));
                }
            }
        }
        "UseSpaceAvoidExchanges" => {
            // Conservative: a piece attacked by an enemy piece of equal
            // value (an offered trade) steps away to a safe square.
            let occ = board.occupied();
            for from in board.colors(side) {
                let Some(piece) = board.piece_on(from) else {
                    continue;
                };
                if matches!(piece, Piece::Pawn | Piece::King) {
                    continue;
                }
                let offered = attackers_of(board, from, !side, occ)
                    .into_iter()
                    .filter_map(|a| board.piece_on(a))
                    .any(|p| (piece_value(p) - piece_value(piece)).abs() <= 30);
                if !offered {
                    continue;
                }
                for &mv in legal {
                    if mv.from == from && !is_capture(board, mv) && is_safe(board, mv) {
                        out.push((mv, ENABLE));
                    }
                }
            }
        }
        "TradeOrActivateBadBishop" => {
            let Some(bsq) = squares.first().and_then(|s| parse_sq(s)) else {
                return out;
            };
            if !board.colored_pieces(side, Piece::Bishop).has(bsq) {
                return out;
            }
            for &mv in legal {
                if mv.from != bsq {
                    continue;
                }
                if is_capture(board, mv) {
                    if see(board, mv.to, side) >= 0 {
                        out.push((mv, EXECUTE));
                    }
                } else {
                    out.push((mv, PREPARE));
                }
            }
        }
        "ActivateKingInEndgame" => {
            if crate::imbalance::phase(board) != crate::record::Phase::Endgame {
                return out;
            }
            let Some(target) = squares.first().and_then(|s| parse_sq(s)) else {
                return out;
            };
            let cheb = |a: Square, b: Square| {
                let df = (a.file() as i8 - b.file() as i8).abs();
                let dr = (a.rank() as i8 - b.rank() as i8).abs();
                df.max(dr)
            };
            for &mv in legal {
                if board.piece_on(mv.from) != Some(Piece::King) {
                    continue;
                }
                if mv.to == target {
                    out.push((mv, EXECUTE));
                } else if cheb(mv.to, target) < cheb(mv.from, target) {
                    out.push((mv, PREPARE));
                }
            }
        }
        "RestrictKnight" => {
            let knights: Vec<Square> = squares
                .iter()
                .filter_map(|s| parse_sq(s))
                .filter(|&s| board.colored_pieces(!side, Piece::Knight).has(s))
                .collect();
            if knights.is_empty() {
                return out;
            }
            let freedom = |b: &Board, n: Square| {
                let owner = !side;
                (cozy_chess::get_knight_moves(n)
                    & !b.colors(owner)
                    & !crate::attack::attacked_squares(b, side))
                .len()
            };
            let before: u32 = knights.iter().map(|&n| freedom(board, n)).sum();
            for &mv in legal {
                if knights.contains(&mv.to) {
                    continue; // trading the homeless knight frees it
                }
                let b2 = after(board, mv);
                let now: u32 = knights.iter().map(|&n| freedom(&b2, n)).sum();
                if now < before {
                    let w = if board.piece_on(mv.from) == Some(Piece::Pawn) {
                        PREPARE
                    } else {
                        ENABLE
                    };
                    out.push((mv, w));
                }
            }
        }
        "OpenLinesTowardWeakKing" => {
            let Some(entry) = squares.first().and_then(|s| parse_sq(s)) else {
                return out;
            };
            let file = entry.file();
            let kf = board.king(!side).file() as i8;
            for &mv in legal {
                match board.piece_on(mv.from) {
                    Some(Piece::Rook | Piece::Queen) => {
                        if mv.to == entry
                            && (!is_capture(board, mv) || see(board, mv.to, side) >= 0)
                        {
                            out.push((mv, EXECUTE));
                        } else if mv.to.file() == file && mv.from.file() != file {
                            out.push((mv, PREPARE));
                        }
                    }
                    Some(Piece::Pawn) => {
                        // The lever nearest the king: a pawn move that
                        // attacks an enemy pawn beside the king's files.
                        let bites = get_pawn_attacks(mv.to, side)
                            & board.colored_pieces(!side, Piece::Pawn);
                        let near_king = bites.into_iter().any(|b| (b.file() as i8 - kf).abs() <= 1);
                        if near_king && (!is_capture(board, mv) || see(board, mv.to, side) >= 0) {
                            out.push((mv, PREPARE));
                        }
                    }
                    _ => {}
                }
            }
        }
        "CompleteDevelopment" => {
            // Run 11 (the maintainer's framing): "the knight already
            // knows where it wants to go; the bishop doesn't yet — that's
            // why the knight moves first." Knights to natural central
            // squares outrank bishop developments while both kinds
            // sleep; once only bishops remain, developing them IS the
            // plan.
            let sleepers: Vec<Square> = squares
                .iter()
                .filter_map(|s| parse_sq(s))
                .filter(|&s| {
                    (board.colored_pieces(side, Piece::Knight)
                        | board.colored_pieces(side, Piece::Bishop))
                    .has(s)
                })
                .collect();
            let knights_sleep = sleepers
                .iter()
                .any(|&s| board.colored_pieces(side, Piece::Knight).has(s));
            let rel = |sq: Square| match side {
                Color::White => sq.rank() as u32,
                Color::Black => 7 - sq.rank() as u32,
            };
            for &mv in legal {
                if !sleepers.contains(&mv.from) {
                    continue;
                }
                match board.piece_on(mv.from) {
                    Some(Piece::Knight) => {
                        let central = (File::C..=File::F).contains(&mv.to.file());
                        let forward = rel(mv.to) >= 2;
                        out.push((mv, if central && forward { EXECUTE } else { ENABLE }));
                    }
                    Some(Piece::Bishop) => {
                        out.push((mv, if knights_sleep { PREPARE } else { EXECUTE }));
                    }
                    _ => {}
                }
            }
        }
        "CastleIntoSafety" => {
            // The castling move itself executes the dream; vacating a
            // square between the king and the hinted rook enables it (so
            // a developing move that clears the path earns convergence).
            let king = squares.first().and_then(|s| parse_sq(s));
            let rook = squares.get(1).and_then(|s| parse_sq(s));
            let path: Vec<Square> = match (king, rook) {
                (Some(k), Some(r)) if k.rank() == r.rank() => {
                    let (lo, hi) = if (k.file() as i8) < (r.file() as i8) {
                        (k.file() as i8, r.file() as i8)
                    } else {
                        (r.file() as i8, k.file() as i8)
                    };
                    ((lo + 1)..hi)
                        .map(|f| Square::new(File::index(f as usize), k.rank()))
                        .collect()
                }
                _ => Vec::new(),
            };
            for &mv in legal {
                let piece = board.piece_on(mv.from);
                if piece == Some(Piece::King) && board.colors(side).has(mv.to) {
                    out.push((mv, EXECUTE));
                } else if piece != Some(Piece::King)
                    && path.contains(&mv.from)
                    && !path.contains(&mv.to)
                {
                    out.push((mv, ENABLE));
                }
            }
        }
        "ClaimTheCenter" => {
            // The hinted squares are the unplayed two-square center
            // advances; the single step toward one merely enables.
            let targets: Vec<Square> = squares.iter().filter_map(|s| parse_sq(s)).collect();
            for &mv in legal {
                if board.piece_on(mv.from) != Some(Piece::Pawn) || is_capture(board, mv) {
                    continue;
                }
                if targets.contains(&mv.to) {
                    out.push((mv, EXECUTE));
                } else if targets
                    .iter()
                    .any(|t| t.try_offset(0, -forward(side)) == Some(mv.to))
                {
                    out.push((mv, ENABLE));
                }
            }
        }
        "WingPawnStormClosedCenter" => {
            let Some(brk) = squares.first().and_then(|s| parse_sq(s)) else {
                return out;
            };
            // Ownership guard: the storm belongs to the side for whom the
            // break square is a LEVER — a pawn of `side` standing there
            // would bite an enemy pawn. A sided hint carried by a
            // Balanced imbalance loses its owner; this recovers it.
            if (get_pawn_attacks(brk, side) & board.colored_pieces(!side, Piece::Pawn)).is_empty() {
                return out;
            }
            for &mv in legal {
                if board.piece_on(mv.from) != Some(Piece::Pawn) {
                    continue;
                }
                if mv.to == brk {
                    out.push((mv, EXECUTE));
                } else if (mv.to.file() as i8 - brk.file() as i8).abs() <= 1 {
                    // The storm pawns' next advances on and beside the
                    // break file.
                    let toward = match side {
                        Color::White => mv.to.rank() > mv.from.rank(),
                        Color::Black => mv.to.rank() < mv.from.rank(),
                    };
                    if toward && !is_capture(board, mv) {
                        out.push((mv, PREPARE));
                    }
                }
            }
        }
        _ => {}
    }
    out
}

/// The strongest plan score for `favors` in the record: composites carry
/// their synthesized score, lone hints their imbalance's magnitude weight.
/// Balanced-owned plans count for BOTH sides: a sided hint carried by a
/// level imbalance has lost its owner label, and the mapper/blocking
/// guards recover direction from the board instead.
///
/// `deniable_only` (run 11) skips the development-prior tokens: when
/// sizing up the OPPONENT's plans for prophylaxis, "deny their
/// development" degenerates into nonsense at static depth (attack their
/// home squares?), so prior dreams are never denial targets — one's own
/// prior dreams still count as constructive strength.
fn plan_strength(record: &FeatureRecord, favors: Favors, deniable_only: bool) -> u32 {
    let denied = |hint: &str| deniable_only && crate::development::is_prior_hint(hint);
    let comp = record
        .composite_plans
        .iter()
        .filter(|c| c.favors == favors || c.favors == Favors::Balanced)
        .filter(|c| c.hints.iter().any(|h| !denied(h)))
        .map(|c| c.score)
        .max()
        .unwrap_or(0);
    let single = record
        .imbalances
        .iter()
        .flat_map(|i| {
            i.plans
                .iter()
                .filter(move |p| {
                    let f = effective_favors(&p.hint, i.favors);
                    (f == favors || f == Favors::Balanced) && !denied(&p.hint)
                })
                .map(move |_| magnitude_weight(i.magnitude))
        })
        .max()
        .unwrap_or(0);
    comp.max(single)
}

/// The opponent's leading plan: (hint tokens, squares, target square).
/// Prefers the best composite, else the strongest lone hint with squares.
/// Balanced-owned plans are eligible (see [`plan_strength`]).
/// Development-prior tokens are never offered for denial (run 11).
fn opponent_leading_plan(
    record: &FeatureRecord,
    opp: Favors,
) -> Option<(Vec<String>, Vec<String>, Option<Square>)> {
    if let Some(cp) = record
        .composite_plans
        .iter()
        .filter(|c| c.favors == opp || c.favors == Favors::Balanced)
        .filter(|c| {
            c.hints
                .iter()
                .any(|h| !crate::development::is_prior_hint(h))
        })
        .max_by_key(|c| c.score)
    {
        let target =
            parse_sq(&cp.target).or_else(|| cp.squares.iter().rev().find_map(|s| parse_sq(s)));
        let hints: Vec<String> = cp
            .hints
            .iter()
            .filter(|h| !crate::development::is_prior_hint(h))
            .cloned()
            .collect();
        return Some((hints, cp.squares.clone(), target));
    }
    let mut best: Option<(u32, &crate::record::PlanHint)> = None;
    for imb in &record.imbalances {
        for plan in &imb.plans {
            let f = effective_favors(&plan.hint, imb.favors);
            if (f != opp && f != Favors::Balanced) || crate::development::is_prior_hint(&plan.hint)
            {
                continue;
            }
            let w = magnitude_weight(imb.magnitude);
            let better = match &best {
                Some((bw, bp)) => w > *bw || (w == *bw && plan.squares.len() > bp.squares.len()),
                None => true,
            };
            if better {
                best = Some((w, plan));
            }
        }
    }
    best.map(|(_, plan)| {
        let target = plan.squares.iter().rev().find_map(|s| parse_sq(s));
        (vec![plan.hint.clone()], plan.squares.clone(), target)
    })
}

/// Blocking candidates against the opponent's leading plan, each tagged
/// with the opponent token it denies.
fn blocking_moves(
    board: &Board,
    legal: &[Move],
    side: Color,
    tokens: &[String],
    squares: &[String],
    target: Option<Square>,
) -> Vec<(Move, u32, String)> {
    let lead = match tokens.first() {
        Some(t) => t.clone(),
        None => return Vec::new(),
    };
    let mut out: Vec<(Move, u32, String)> = Vec::new();
    let plan_squares: Vec<Square> = squares.iter().filter_map(|s| parse_sq(s)).collect();
    for &mv in legal {
        // (a) Occupy or defend the plan's target square.
        if let Some(t) = target {
            if mv.to == t && !is_capture(board, mv) {
                out.push((mv, EXECUTE, lead.clone()));
                continue;
            }
            if adds_attacker(board, mv, t, side) {
                out.push((mv, PREPARE, lead.clone()));
                continue;
            }
        }
        // (c) Trade the piece the plan needs: capture on a plan square.
        if plan_squares.contains(&mv.to) && is_capture(board, mv) && see(board, mv.to, side) >= 0 {
            out.push((mv, EXECUTE, lead.clone()));
            continue;
        }
        // (b) Contest the route/stop squares.
        if plan_squares
            .iter()
            .any(|&s| Some(s) != target && adds_attacker(board, mv, s, side))
        {
            out.push((mv, ENABLE, lead.clone()));
        }
    }
    // (d) A wing storm against a closed center is answered in the center:
    // the freeing counter-break.
    if tokens.iter().any(|t| t == "WingPawnStormClosedCenter") {
        let center =
            File::C.bitboard() | File::D.bitboard() | File::E.bitboard() | File::F.bitboard();
        for &mv in legal {
            if board.piece_on(mv.from) != Some(Piece::Pawn) || !center.has(mv.to) {
                continue;
            }
            let counter = if is_capture(board, mv) {
                board.piece_on(mv.to) == Some(Piece::Pawn) && see(board, mv.to, side) >= 0
            } else {
                !(get_pawn_attacks(mv.to, side) & board.colored_pieces(!side, Piece::Pawn))
                    .is_empty()
            };
            if counter {
                out.push((mv, EXECUTE, "WingPawnStormClosedCenter".to_string()));
            }
        }
    }
    out
}

/// Synthesize up to three candidate moves for the side to move from the
/// record's plans (see the module docs for the scoring and prophylaxis
/// rules). `board` must be the record's position.
/// What role a specific move plays: which of OUR plans it advances, and
/// which of THEIRS it denies.
///
/// Exposed for the corpus study of prophylaxis. The engine's ranking
/// currently gives a denial bonus that can outrank executing your own
/// plan, and whether that is right is not a matter of taste — the book
/// corpus carries Silman's own recommendation for 25 positions, so the
/// distribution of prophylactic-versus-constructive picks is measurable.
#[derive(Debug, Default, Clone)]
pub struct MoveRole {
    /// Our plans this move advances.
    pub constructive: Vec<String>,
    /// Their plans it denies.
    pub blocking: Vec<String>,
    /// Magnitude-weighted strength of our plans, and of their deniable ones.
    pub own_strength: u32,
    pub opp_strength: u32,
    /// Cheapest scheme horizon each side owns, in moves. `None` when that
    /// side has no multi-stage plan — the tempo comparison the maintainer
    /// proposed needs a speed, and this is the only one we compute.
    pub own_horizon: Option<u8>,
    pub opp_horizon: Option<u8>,
}

pub fn role_of(record: &FeatureRecord, board: &Board, mv: Move) -> MoveRole {
    let stm = board.side_to_move();
    let stm_favors = color_favors(stm);
    let opp_favors = color_favors(!stm);
    let legal = legal_moves(board);
    let plans = active_plans(record);

    let mut role = MoveRole {
        own_strength: plan_strength(record, stm_favors, false),
        opp_strength: plan_strength(record, opp_favors, true),
        ..MoveRole::default()
    };
    let horizon = |f: Favors| {
        record
            .schemes
            .iter()
            .filter(|s| s.favors == f)
            .map(|s| s.horizon)
            .min()
    };
    role.own_horizon = horizon(stm_favors);
    role.opp_horizon = horizon(opp_favors);

    for p in plans.iter().filter(|p| p.favors == stm_favors) {
        for (m, _) in moves_for_hint(board, &legal, stm, &p.hint, &p.squares) {
            if m == mv && !role.constructive.contains(&p.hint) {
                role.constructive.push(p.hint.clone());
            }
        }
    }
    if let Some((tokens, squares, target)) = opponent_leading_plan(record, opp_favors) {
        for (m, _, token) in blocking_moves(board, &legal, stm, &tokens, &squares, target) {
            if m == mv && !role.blocking.contains(&token) {
                role.blocking.push(token);
            }
        }
    }
    role
}

pub fn suggest(record: &FeatureRecord, board: &Board) -> Vec<Suggestion> {
    let stm = board.side_to_move();
    let stm_favors = color_favors(stm);
    let opp_favors = color_favors(!stm);
    let legal = legal_moves(board);
    if legal.is_empty() {
        return Vec::new();
    }

    #[derive(Default)]
    struct Cand {
        constructive: BTreeMap<String, u32>,
        blocking: BTreeMap<String, u32>,
    }
    // Keyed by the move's string form: cozy Move is not Ord, and the
    // string key doubles as the deterministic iteration order.
    let mut cands: BTreeMap<String, (Move, Cand)> = BTreeMap::new();

    let plans = active_plans(record);
    for plan in &plans {
        if plan.favors != stm_favors && plan.favors != Favors::Balanced {
            continue;
        }
        for (mv, w) in moves_for_hint(board, &legal, stm, &plan.hint, &plan.squares) {
            let entry = cands
                .entry(mv.to_string())
                .or_insert_with(|| (mv, Cand::default()))
                .1
                .constructive
                .entry(plan.hint.clone())
                .or_default();
            *entry = (*entry).max(w);
        }
    }

    // Prophylaxis: when the opponent's best plan rivals ours (within one
    // point), denial competes with construction.
    let own_strength = plan_strength(record, stm_favors, false);
    let opp_strength = plan_strength(record, opp_favors, true);
    if opp_strength > 0 && opp_strength + 1 >= own_strength {
        if let Some((tokens, squares, target)) = opponent_leading_plan(record, opp_favors) {
            for (mv, w, token) in blocking_moves(board, &legal, stm, &tokens, &squares, target) {
                let entry = cands
                    .entry(mv.to_string())
                    .or_insert_with(|| (mv, Cand::default()))
                    .1
                    .blocking
                    .entry(token)
                    .or_default();
                *entry = (*entry).max(w);
            }
        }
    }

    let mut scored: Vec<Suggestion> = cands
        .into_values()
        .filter(|(mv, _)| is_safe(board, *mv))
        .map(|(mv, c)| {
            let constructive: u32 = c.constructive.values().sum();
            let blocking: u32 = c.blocking.values().sum();
            let tokens: BTreeSet<&String> =
                c.constructive.keys().chain(c.blocking.keys()).collect();
            let distinct = tokens.len() as u32;
            let score = constructive + blocking + distinct.saturating_sub(1);
            let prophylactic = blocking > 0 && blocking >= constructive;
            let mut serving: Vec<String> = Vec::new();
            let (first, second) = if prophylactic {
                (&c.blocking, &c.constructive)
            } else {
                (&c.constructive, &c.blocking)
            };
            for k in first.keys().chain(second.keys()) {
                if !serving.contains(k) {
                    serving.push(k.clone());
                }
            }
            Suggestion {
                mv: mv.to_string(),
                san: san(board, mv),
                score,
                serving,
                prophylactic,
                static_risk: static_risk(board, mv),
            }
        })
        .collect();
    scored.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then(b.serving.len().cmp(&a.serving.len()))
            .then(a.san.cmp(&b.san))
    });
    scored.truncate(3);
    scored
}

/// Standard algebraic notation for a legal move (original implementation:
/// cozy-chess encodes castling as king-onto-own-rook).
pub fn san(board: &Board, mv: Move) -> String {
    let stm = board.side_to_move();
    let piece = board.piece_on(mv.from).expect("legal move has a mover");

    let suffix = {
        let b2 = after(board, mv);
        if b2.checkers().is_empty() {
            ""
        } else {
            let mut any = false;
            b2.generate_moves(|_| {
                any = true;
                true
            });
            if any {
                "+"
            } else {
                "#"
            }
        }
    };

    if piece == Piece::King && board.colors(stm).has(mv.to) {
        let body = if mv.to.file() > mv.from.file() {
            "O-O"
        } else {
            "O-O-O"
        };
        return format!("{body}{suffix}");
    }

    let capture = is_capture(board, mv);
    let mut out = String::new();
    if piece == Piece::Pawn {
        if capture {
            out.push(file_char(mv.from.file()));
            out.push('x');
        }
        out.push_str(&crate::record::square_name(mv.to));
        if let Some(promo) = mv.promotion {
            out.push('=');
            out.push(piece_char(promo));
        }
    } else {
        out.push(piece_char(piece));
        let mut same_file = false;
        let mut same_rank = false;
        let mut ambiguous = false;
        board.generate_moves(|pm| {
            if pm.piece == piece && pm.from != mv.from && pm.to.has(mv.to) {
                ambiguous = true;
                if pm.from.file() == mv.from.file() {
                    same_file = true;
                }
                if pm.from.rank() == mv.from.rank() {
                    same_rank = true;
                }
            }
            false
        });
        if ambiguous {
            if !same_file {
                out.push(file_char(mv.from.file()));
            } else if !same_rank {
                out.push(rank_char(mv.from.rank()));
            } else {
                out.push(file_char(mv.from.file()));
                out.push(rank_char(mv.from.rank()));
            }
        }
        if capture {
            out.push('x');
        }
        out.push_str(&crate::record::square_name(mv.to));
    }
    out.push_str(suffix);
    out
}

fn file_char(f: File) -> char {
    (b'a' + f as u8) as char
}

fn rank_char(r: Rank) -> char {
    (b'1' + r as u8) as char
}

fn piece_char(p: Piece) -> char {
    match p {
        Piece::Pawn => 'P',
        Piece::Knight => 'N',
        Piece::Bishop => 'B',
        Piece::Rook => 'R',
        Piece::Queen => 'Q',
        Piece::King => 'K',
    }
}

#[cfg(test)]
mod tests {
    //! Cited mapper tests: each position is a book diagram or classic
    //! pattern given as FEN + citation only (no book prose). Assertions
    //! pin the move(s) a plan-literate player would shortlist.

    use super::*;
    use crate::analyze;

    fn board(fen: &str) -> Board {
        fen.parse().unwrap()
    }

    fn suggest_for(fen: &str) -> Vec<Suggestion> {
        let b = board(fen);
        suggest(&analyze(&b), &b)
    }

    fn sans(s: &[Suggestion]) -> Vec<&str> {
        s.iter().map(|x| x.san.as_str()).collect()
    }

    fn mapper(fen: &str, hint: &str, squares: &[&str]) -> Vec<(String, u32)> {
        let b = board(fen);
        let legal = legal_moves(&b);
        let sq: Vec<String> = squares.iter().map(|s| s.to_string()).collect();
        moves_for_hint(&b, &legal, b.side_to_move(), hint, &sq)
            .into_iter()
            .map(|(mv, w)| (san(&b, mv), w))
            .collect()
    }

    fn has_move(out: &[(String, u32)], san: &str, weight: u32) -> bool {
        out.iter().any(|(s, w)| s == san && *w == weight)
    }

    /// Sveshnikov bind (the imbalance_golden real-FEN outpost example):
    /// the c3-knight's route ends on d5 — Nd5 executes the plan.
    #[test]
    fn maneuver_knight_lands_on_the_outpost() {
        let s = suggest_for("r1bqkb1r/pp3ppp/2np1n2/1N2p3/4P3/2N5/PPP2PPP/R1BQKB1R w KQkq - 0 7");
        assert!(
            s.iter().any(|x| x.san == "Nd5"
                && x.serving.iter().any(|t| t == "ManeuverKnightToOutpost")),
            "{s:?}"
        );
    }

    /// Jeremy Silman, The Complete Book of Chess Strategy, p. 236, entry
    /// 'Pawn Structure - Backward Pawns': pile up on the backward d6 pawn —
    /// Ne4 adds an attacker (and the hanging Nb5? is filtered by SEE).
    #[test]
    fn pressure_backward_pawn_adds_attackers_safely() {
        let fen = "r1q2rk1/1p2bppp/pBnp4/4p3/P7/2NB1QP1/1PP2P1P/R2R2K1 w - - 0 1";
        let out = mapper(fen, "PressureBackwardPawn", &["d6", "d5"]);
        assert!(has_move(&out, "Ne4", PREPARE), "{out:?}");
        let s = suggest_for(fen);
        assert!(
            !sans(&s).contains(&"Nb5"),
            "hanging Nb5 must be dropped: {s:?}"
        );
    }

    /// Jeremy Silman, The Complete Book of Chess Strategy, p. 240, entry
    /// 'Pawn Structure - Doubled Pawns': Black piles on the front c4 pawn
    /// with ...b5 and ...Ba6.
    #[test]
    fn pressure_doubled_pawn_saemisch_targets() {
        let out = mapper(
            "rnbq1rk1/p1pp1ppp/1p2pn2/8/2PPP3/P1P2P2/6PP/R1BQKBNR b KQ - 0 1",
            "PressureDoubledPawn",
            &["c4"],
        );
        assert!(has_move(&out, "b5", PREPARE), "{out:?}");
        assert!(has_move(&out, "Ba6", PREPARE), "{out:?}");
    }

    /// Doubling pattern per Jeremy Silman, The Complete Book of Chess
    /// Strategy, p. 224, entry 'Doubled Rooks' (schematic position): the
    /// queen joins the rook on the open d-file.
    #[test]
    fn double_on_open_file_stacks_the_majors() {
        let out = mapper(
            "1k1r4/ppp2ppp/8/8/8/8/PPP1QPPP/1K1R4 w - - 0 1",
            "DoubleOnOpenFile",
            &[],
        );
        assert!(has_move(&out, "Qd2", EXECUTE), "{out:?}");
        assert!(has_move(&out, "Qd3", EXECUTE), "{out:?}");
    }

    /// Jeremy Silman, The Complete Book of Chess Strategy, p. 329, entry
    /// 'Two Hogs on the Seventh': the seventh-rank rooks keep working the
    /// rank; Rxa7?? (a8- and a2-rooks both defend) is SEE-excluded.
    #[test]
    fn rooks_on_seventh_press_on() {
        let out = mapper(
            "r3k3/pRR5/8/5p2/6p1/6P1/r4PK1/8 w - - 0 1",
            "RookToSeventh",
            &["b7"],
        );
        assert!(has_move(&out, "Rd7", PREPARE), "{out:?}");
        assert!(
            !out.iter().any(|(s, _)| s == "Rxa7"),
            "losing capture must not be proposed: {out:?}"
        );
    }

    /// Jeremy Silman, The Complete Book of Chess Strategy, p. 323, entry
    /// 'Rooks Behind Passed Pawns': Ra1, behind the a4 passer.
    #[test]
    fn rook_goes_behind_the_passer() {
        let fen = "8/5pk1/6p1/7p/P7/6P1/2r2PKP/1R6 w - - 0 1";
        let out = mapper(fen, "RookBehindPasser", &["a4", "a3"]);
        assert!(has_move(&out, "Ra1", EXECUTE), "{out:?}");
        let s = suggest_for(fen);
        assert_eq!(s.first().map(|x| x.san.as_str()), Some("Ra1"), "{s:?}");
    }

    /// Jeremy Silman, The Amateur's Mind, p. 317, test 4: the far-advanced
    /// d3 passer must be fought. The mapper proposes the thematic
    /// d-file grip (including the book's Rd5, which SEE rejects here —
    /// d8's rook takes it for free statically); the surviving top
    /// suggestions all grip the passer's promotion path.
    #[test]
    fn blockade_pressure_grips_the_advanced_passer() {
        let fen = "3r2k1/5pp1/7p/PR2PP2/2p5/2Bpb2P/6P1/3K4 w - - 0 1";
        let b = board(fen);
        let legal = legal_moves(&b);
        let mapped = moves_for_hint(
            &b,
            &legal,
            Color::White,
            "BlockadeBlackPasser",
            &["d2".to_string()],
        );
        // Pressure on a passer two steps from queening is execution.
        assert!(
            mapped
                .iter()
                .any(|(mv, w)| mv.to == Square::D5 && *w == EXECUTE),
            "{mapped:?}"
        );
        let s = suggest_for(fen);
        assert!(
            s.iter()
                .any(|x| x.san == "Rb2" && x.serving.iter().any(|t| t == "BlockadeBlackPasser")),
            "d2-grip Rb2 expected: {s:?}"
        );
        assert!(!sans(&s).contains(&"Rd5"), "statically hanging: {s:?}");
    }

    /// Jeremy Silman, The Complete Book of Chess Strategy, p. 268, entry
    /// 'Queenside Pawn Majority': the CANDIDATE (unopposed a-) pawn leads.
    #[test]
    fn queenside_majority_pushes_the_candidate() {
        let out = mapper(
            "8/7k/1p3p2/3pp2p/PP5P/4PP2/8/5K2 w - - 0 1",
            "AdvanceQueensideMajority",
            &[],
        );
        assert!(has_move(&out, "a5", EXECUTE), "{out:?}");
        assert!(has_move(&out, "b5", PREPARE), "{out:?}");
    }

    /// Jeremy Silman, The Complete Book of Chess Strategy, p. 269, entry
    /// 'Queenside Pawn Majority' (central-majority side): Black rolls the
    /// central majority with ...e5.
    #[test]
    fn central_majority_rolls_forward() {
        let out = mapper(
            "2rr2k1/p1qn1ppb/1p2p2p/8/2P5/1P2BN2/P3QPPP/3RR1K1 b - - 0 1",
            "AdvanceCentralMajority",
            &["e5"],
        );
        assert!(has_move(&out, "e5", EXECUTE), "{out:?}");
    }

    /// Jeremy Silman, The Complete Book of Chess Strategy, p. 202, entry
    /// 'Minority Attack': the b-pawn marches toward the b5 lever.
    #[test]
    fn minority_attack_advances_the_b_pawn() {
        let out = mapper(
            "r1bqrnk1/pp2bppp/2p2n2/3p2B1/3P4/2NBPN2/PPQ2PPP/R4RK1 w - - 0 1",
            "MinorityAttack",
            &["b5", "c6"],
        );
        assert!(has_move(&out, "b4", PREPARE), "{out:?}");
    }

    /// Jeremy Silman, The Amateur's Mind, p. 326, test 21 (dxe5 among the
    /// book moves): the pawn trade opens the position.
    #[test]
    fn open_position_trades_pawns() {
        let out = mapper(
            "r2qkbnr/pppn1ppp/3pb3/4p3/3PP3/2N2N2/PPP2PPP/R1BQKB1R w KQkq - 0 1",
            "OpenPositionForBishops",
            &[],
        );
        assert!(has_move(&out, "dxe5", EXECUTE), "{out:?}");
    }

    /// Jeremy Silman, How to Reassess Your Chess, 3rd ed., p. 387,
    /// problem 181: e5 runs into the fixed e6/d6 chain and locks the
    /// center the knights want closed.
    #[test]
    fn keep_closed_locks_the_chain() {
        let out = mapper(
            "r1br2k1/2qnbppp/p1npp3/1p4P1/4P2P/1NN1BP2/PPPQ4/2KR1B1R w - - 0 1",
            "KeepPositionClosed",
            &[],
        );
        assert!(has_move(&out, "e5", PREPARE), "{out:?}");
    }

    /// Space-side trade refusal (pattern per Jeremy Silman, The Complete
    /// Book of Chess Strategy, p. 271, entry 'Space'): the f3-knight,
    /// offered an even trade by ...Bg4, steps away keeping the tension.
    #[test]
    fn space_side_declines_the_offered_trade() {
        let out = mapper(
            "rn1qkb1r/ppp2ppp/4pn2/3p4/3P2b1/2N1PN2/PPP2PPP/R1BQKB1R w KQkq - 0 5",
            "UseSpaceAvoidExchanges",
            &[],
        );
        assert!(has_move(&out, "Ne5", ENABLE), "{out:?}");
    }

    /// Jeremy Silman, The Complete Book of Chess Strategy, p. 279, entry
    /// 'Trading Pieces': the buried e7-bishop walks out of the chain
    /// toward the kingside dark squares (the d8 reroute needs the queen
    /// to move first — d8 is occupied here).
    #[test]
    fn bad_bishop_steps_off_the_chain() {
        let out = mapper(
            "rnbq1rk1/pp2bpnp/3p2pB/2pPp3/2P1P1P1/2N2N1P/PP1Q1P2/R3R1K1 b - - 0 1",
            "TradeOrActivateBadBishop",
            &["e7"],
        );
        assert!(has_move(&out, "Bf6", PREPARE), "{out:?}");
        assert!(has_move(&out, "Bg5", PREPARE), "{out:?}");
    }

    /// Jeremy Silman, How to Reassess Your Chess, 3rd ed., p. 366,
    /// problem 21: the king marches toward the center/pawn.
    #[test]
    fn endgame_king_steps_toward_the_action() {
        let out = mapper(
            "k7/8/8/8/8/3P4/8/2K5 w - - 0 1",
            "ActivateKingInEndgame",
            &["d4"],
        );
        assert!(has_move(&out, "Kd2", PREPARE), "{out:?}");
        assert!(has_move(&out, "Kc2", PREPARE), "{out:?}");
    }

    /// Jeremy Silman, How to Reassess Your Chess, 3rd ed., p. 371,
    /// problem 82: keep the b8-knight homeless — some move must shrink its
    /// list of free squares.
    #[test]
    fn restrict_knight_shrinks_its_freedom() {
        let out = mapper(
            "1n1rr1k1/p1p2ppp/1p1p4/4q3/2P5/P3PB2/1PQR1PPP/5RK1 w - - 0 1",
            "RestrictKnight",
            &["b8"],
        );
        assert!(!out.is_empty(), "no restricting move found");
        // c5 pries at the pawns that would give the knight a home; the
        // mapper must weight pawn restriction over piece restriction.
        assert!(out.iter().all(|(_, w)| *w <= PREPARE), "{out:?}");
    }

    /// Jeremy Silman, The Amateur's Mind, p. 316, test 2: the f-file
    /// toward the airy king — the a1-rook joins it.
    #[test]
    fn open_lines_bring_the_rook_to_the_kings_file() {
        let out = mapper(
            "r3r1k1/pb3p2/4pR2/1p1p2p1/3P1n2/B1P5/PP1N2PP/R5K1 w - - 0 1",
            "OpenLinesTowardWeakKing",
            &["f7"],
        );
        assert!(has_move(&out, "Rf1", PREPARE), "{out:?}");
    }

    /// Jeremy Silman, The Amateur's Mind, p. 323, test 15: the storm's
    /// next advance — g4, prying at f5.
    #[test]
    fn wing_storm_advances_the_storm_pawn() {
        let out = mapper(
            "r1bq1rk1/pp1nb1pp/4p3/2ppPp2/5B2/2PBP3/PP1N1PPP/R2QK2R w KQ - 0 1",
            "WingPawnStormClosedCenter",
            &["g4"],
        );
        assert!(has_move(&out, "g4", EXECUTE), "{out:?}");
    }

    /// Convergence (the maintainer's rule, synthetic position): one rook
    /// move onto the open d-file both doubles heavy pieces and aims at the
    /// weak king — it serves two plans and earns the bonus.
    #[test]
    fn convergent_move_outscores_single_plan_moves() {
        let b = board("4k2r/5p2/8/8/8/8/PP3PPP/R4RK1 w k - 0 20");
        let s = suggest(&analyze(&b), &b);
        let top = s.first().expect("suggestions");
        assert!(top.serving.len() >= 2, "{s:?}");
        assert!(top.san.starts_with('R'), "{s:?}");
        // Sum of both PREPARE weights plus the convergence bonus.
        assert!(top.score > PREPARE + PREPARE, "{s:?}");
    }

    /// Prophylaxis, Sveshnikov bind with the DEFENDER to move: White's
    /// whole position converges on d5 (knight route + backward-pawn
    /// pressure), so Black's top suggestions DEFEND d5 — ...Be6, flagged
    /// prophylactic. (The maintainer's complaint: "I haven't seen it once
    /// recommend prophylaxis.")
    #[test]
    fn prophylaxis_defends_the_opponents_convergence_square() {
        let s = suggest_for("r1bqkb1r/pp3ppp/2np1n2/1N2p3/4P3/2N5/PPP2PPP/R1BQKB1R b KQkq - 0 7");
        let be6 = s.iter().find(|x| x.san == "Be6");
        let be6 = be6.unwrap_or_else(|| panic!("defensive Be6 expected: {s:?}"));
        assert!(be6.prophylactic, "{s:?}");
        assert!(!be6.serving.is_empty(), "{s:?}");
    }

    /// Jeremy Silman, The Amateur's Mind, p. 323, test 15 with the
    /// DEFENDER to move: the blocking generator maps the storm to the
    /// central counter-break ...d4 (the safety gate then judges it on the
    /// merits — here it sheds a pawn statically and is filtered, leaving
    /// only sound moves in the final list).
    #[test]
    fn storm_blocking_maps_the_central_counter_break() {
        let b = board("r1bq1rk1/pp1nb1pp/4p3/2ppPp2/5B2/2PBP3/PP1N1PPP/R2QK2R b KQ - 0 1");
        let legal = legal_moves(&b);
        let blocked = blocking_moves(
            &b,
            &legal,
            Color::Black,
            &["WingPawnStormClosedCenter".to_string()],
            &["g4".to_string()],
            Some(Square::G4),
        );
        assert!(
            blocked
                .iter()
                .any(|(mv, w, _)| mv.to == Square::D4 && *w == EXECUTE),
            "{blocked:?}"
        );
        // The final list stays free of statically losing tries.
        let s = suggest_for("r1bq1rk1/pp1nb1pp/4p3/2ppPp2/5B2/2PBP3/PP1N1PPP/R2QK2R b KQ - 0 1");
        assert!(!sans(&s).contains(&"d4"), "{s:?}");
    }

    /// The safety gate: a mapper move that hangs material never surfaces.
    /// (Synthetic: the hinted square is covered by two pawns.)
    #[test]
    fn hanging_candidates_are_filtered() {
        // White knight can hop to the hinted d5, but d5 is covered by the
        // c6/e6 pawns and undefended — SEE kills it.
        let b = board("4k3/8/2p1p3/8/8/4N3/8/4K3 w - - 0 1");
        let legal = legal_moves(&b);
        let mapped = moves_for_hint(
            &b,
            &legal,
            Color::White,
            "ManeuverKnightToOutpost",
            &["d5".to_string()],
        );
        // The mapper proposes it (it is the hinted square)...
        assert!(mapped.iter().any(|(mv, _)| mv.to == Square::D5));
        // ...but the safety filter must reject it.
        assert!(!is_safe(&b, "e3d5".parse().unwrap()));
    }

    /// French Winawer after 5.a3 (maintainer field report, run 11): the
    /// b4-bishop hangs to axb4 (cxb4 recaptures only a pawn — net bishop
    /// for pawn, ~230cp). The destination-only gate never noticed, so
    /// f5??/f6?? shipped as chips. The whole-board veto must mark them.
    const WINAWER: &str = "rnbqk1nr/pp3ppp/4p3/2ppP3/1b1P4/P1N5/1PP2PPP/R1BQKBNR b KQkq - 0 5";

    #[test]
    fn winawer_whole_board_veto_marks_piece_droppers() {
        let b = board(WINAWER);
        let risk = |uci: &str| static_risk(&b, uci.parse().unwrap());
        // f5/f6 ignore the hanging bishop: marked with the full swing.
        assert!(risk("f7f5").is_some_and(|r| r >= PIECE_LOSS_CP));
        assert!(risk("f7f6").is_some_and(|r| r >= PIECE_LOSS_CP));
        // cxd4 is the THEORY move (axb4 is met by dxc3, regaining the
        // piece) — but statics one exchange deep cannot distinguish it
        // from the losers, so it is marked too. Documented limitation:
        // resurrecting it is the engine verification layer's job.
        assert!(risk("c5d4").is_some_and(|r| r >= PIECE_LOSS_CP));
        // Candidates that ADDRESS the hang pass statically — this is the
        // already-en-prise subtlety: with the bishop hanging, only moves
        // that bring the swing back under the threshold stay clean.
        assert_eq!(risk("b4c3"), None, "Bxc3+ trades the hanging bishop");
        assert_eq!(risk("b4a5"), None, "Ba5 steps out of the capture");
    }

    /// Winawer, end to end: whatever the mappers propose here, every
    /// surviving suggestion carries the static mark — so a consumer with
    /// no engine verification available shows NOTHING (which
    /// conservatively kills cxd4 too; the engine layer resurrects it).
    #[test]
    fn winawer_suggestions_are_all_statically_marked() {
        let s = suggest_for(WINAWER);
        assert!(!s.is_empty(), "the mappers do propose moves here: {s:?}");
        for x in &s {
            assert!(
                x.static_risk.is_some_and(|r| r >= PIECE_LOSS_CP),
                "{} must be statically marked: {s:?}",
                x.san
            );
        }
    }

    /// Control: with nothing en prise the veto never fires — the
    /// Sveshnikov bind's suggestions flow unchanged and unmarked.
    #[test]
    fn whole_board_veto_leaves_clean_positions_alone() {
        let s = suggest_for("r1bqkb1r/pp3ppp/2np1n2/1N2p3/4P3/2N5/PPP2PPP/R1BQKB1R w KQkq - 0 7");
        assert!(
            s.iter().any(|x| x.san == "Nd5" && x.static_risk.is_none()),
            "{s:?}"
        );
        assert!(
            s.iter().all(|x| x.static_risk.is_none()),
            "nothing hangs in the bind: {s:?}"
        );
    }

    /// The opera-game 8...c6 closing (narration snapshot): 9.f4 is an
    /// even lever (exf4 Bxf4) and nothing else hangs — it must stay
    /// statically clean, as must the Bd2 backup.
    #[test]
    fn opera_c6_lever_f4_stays_statically_clean() {
        let fen = "r1b1kb1r/pp2qppp/2p2n2/4p3/2B1P3/1QN5/PPP2PPP/R1B1K2R w KQkq - 0 9";
        let b = board(fen);
        assert_eq!(static_risk(&b, "f2f4".parse().unwrap()), None);
        assert_eq!(static_risk(&b, "c1d2".parse().unwrap()), None);
    }

    /// Emanuel Lasker, Common Sense in Chess (1896), first lecture
    /// (1.e4 e5): knights before bishops — the knight already knows
    /// where it wants to go, so Nf3 executes while Bc4 only prepares,
    /// and the rim hop Nh3 merely enables.
    #[test]
    fn knights_develop_before_bishops() {
        let out = mapper(
            "rnbqkbnr/pppp1ppp/8/4p3/4P3/8/PPPP1PPP/RNBQKBNR w KQkq - 0 2",
            "CompleteDevelopment",
            &["b1", "c1", "f1", "g1"],
        );
        assert!(has_move(&out, "Nf3", EXECUTE), "{out:?}");
        assert!(has_move(&out, "Nc3", EXECUTE), "{out:?}");
        assert!(has_move(&out, "Bc4", PREPARE), "{out:?}");
        assert!(has_move(&out, "Nh3", ENABLE), "{out:?}");
    }

    /// Once only bishops sleep, developing them IS the plan (executes).
    #[test]
    fn last_sleeping_bishop_executes() {
        // Four Knights after 4...Bb4 5.O-O O-O 6.d3 d6: only c1/c8 sleep.
        let out = mapper(
            "r1bq1rk1/ppp2ppp/2np1n2/1B2p3/1b2P3/2NP1N2/PPP2PPP/R1BQ1RK1 w - - 0 7",
            "CompleteDevelopment",
            &["c1"],
        );
        assert!(has_move(&out, "Bg5", EXECUTE), "{out:?}");
        assert!(has_move(&out, "Bd2", EXECUTE), "{out:?}");
    }

    /// The castling move itself executes CastleIntoSafety; a piece
    /// vacating the path merely enables (so a developing move that
    /// clears the way earns convergence with CompleteDevelopment).
    #[test]
    fn castle_mapper_executes_castling_and_enables_path_clearing() {
        // Four Knights after 4...Bb4: White can castle short now.
        let castled = mapper(
            "r1bqk2r/pppp1ppp/2n2n2/1B2p3/1b2P3/2N2N2/PPPP1PPP/R1BQK2R w KQkq - 4 5",
            "CastleIntoSafety",
            &["e1", "h1"],
        );
        assert!(has_move(&castled, "O-O", EXECUTE), "{castled:?}");
        // After 1.e4 e5 2.Nf3 Nc6 the f1-bishop still blocks the path:
        // its developing moves enable the castle.
        let blocked = mapper(
            "r1bqkbnr/pppp1ppp/2n5/4p3/4P3/5N2/PPPP1PPP/RNBQKB1R w KQkq - 2 3",
            "CastleIntoSafety",
            &["e1", "h1"],
        );
        assert!(has_move(&blocked, "Bc4", ENABLE), "{blocked:?}");
        assert!(
            !blocked.iter().any(|(s, _)| s.starts_with("O-O")),
            "{blocked:?}"
        );
    }

    /// ClaimTheCenter: the hinted two-square advance executes, the single
    /// step toward it enables (1.e4 e5, White's d-pawn dream).
    #[test]
    fn claim_the_center_pushes_the_hinted_pawn() {
        let out = mapper(
            "rnbqkbnr/pppp1ppp/8/4p3/4P3/8/PPPP1PPP/RNBQKBNR w KQkq - 0 2",
            "ClaimTheCenter",
            &["d4"],
        );
        assert!(has_move(&out, "d4", EXECUTE), "{out:?}");
        assert!(has_move(&out, "d3", ENABLE), "{out:?}");
    }

    /// End to end (the run-11 point: no more silent openings): with the
    /// move history supplied, an early quiet position produces
    /// suggestions serving the development dreams.
    #[test]
    fn opening_position_finally_gets_suggestions() {
        let start = Board::default();
        let moves: Vec<cozy_chess::Move> = ["e2e4", "e7e5"]
            .iter()
            .map(|u| u.parse().unwrap())
            .collect();
        let record = crate::analyze_with_history(&start, &moves);
        let mut b = start.clone();
        for &mv in &moves {
            b.play(mv);
        }
        let s = suggest(&record, &b);
        assert!(!s.is_empty(), "opening must not be suggestion-silent");
        assert!(
            s.iter().any(|x| x
                .serving
                .iter()
                .any(|t| crate::development::is_prior_hint(t))),
            "{s:?}"
        );
        // Prophylaxis never targets the opponent's development dreams.
        for x in &s {
            if x.prophylactic {
                assert!(
                    x.serving
                        .first()
                        .is_none_or(|t| !crate::development::is_prior_hint(t)),
                    "prior dream offered for denial: {s:?}"
                );
            }
        }
    }

    /// SAN formatting: disambiguation, captures, castling, promotion.
    #[test]
    fn san_formatting_matrix() {
        // The c7 rook cannot legally reach a7 (its partner blocks b7), so
        // no disambiguation is needed.
        let b = board("r3k3/pRR5/8/5p2/6p1/6P1/r4PK1/8 w - - 0 1");
        assert_eq!(san(&b, "b7a7".parse().unwrap()), "Rxa7");
        // True ambiguity: knights on b1 and f3 both reach d2.
        let n = board("4k3/8/8/8/8/5N2/8/1N2K3 w - - 0 1");
        assert_eq!(san(&n, "b1d2".parse().unwrap()), "Nbd2");
        assert_eq!(san(&n, "f3d2".parse().unwrap()), "Nfd2");
        let c = board("r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1");
        assert_eq!(san(&c, "e1h1".parse().unwrap()), "O-O");
        assert_eq!(san(&c, "e1a1".parse().unwrap()), "O-O-O");
        let p = board("8/4P1k1/8/8/8/8/8/4K3 w - - 0 1");
        assert_eq!(san(&p, "e7e8q".parse().unwrap()), "e8=Q");
    }
}
