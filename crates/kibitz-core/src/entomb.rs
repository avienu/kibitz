//! Entombed pieces: a piece with no future.
//!
//! Jeremy Silman devotes an entry to this (The Complete Book of Chess Strategy,
//! p. 192-193, "Entombed Pieces"): a rook tied to a back-rank square by an
//! enemy bishop and pawn, two knights that cannot leave their own camp.
//! The nominal material count says one thing and the board says another,
//! because a piece that can never do anything is a piece you do not have.
//!
//! This was first attempted as a sensitivity setting of the WSUI
//! `TrappedPiece` alert and that was a category error — see
//! docs/VALIDATION.md, "TrappedPiece: the corpus wants a different
//! concept". Trapped is a TACTIC: a piece that is attacked, or about to
//! be, and will be lost in a few moves. The screen fires, the engine
//! confirms, the alert is either right or wrong within the horizon.
//! Entombed is a STRATEGIC property with no horizon at all: nobody is
//! going to win the b8-rook, it is simply not playing. Relaxing the
//! trapped gates to admit it cost 17.8 points of screen firing on quiet
//! master positions for 15.6 points of book recall, because the concept
//! those gates exclude — unattacked, at home, nowhere to go — is mostly
//! just an UNDEVELOPED piece.
//!
//! So the discriminator here is not attack, it is PERMANENCE:
//!
//! 1. The piece has no route (within [`crate::route::MAX_HOPS`]) to any
//!    square where it would do something — the enemy half of the board,
//!    or an enemy piece it can take without losing by the trade.
//! 2. No single pawn move by its owner changes that.
//!
//! Condition 2 is the whole distinction. A bishop on f1 in the starting
//! position satisfies (1) — it has no moves at all — and fails (2), because
//! e2-e4 hands it a diagonal. That bishop is undeveloped. The bishop on f1
//! in Jeremy Silman's Amateur's Mind p. 10 diagram satisfies both, because every
//! white pawn on the board is frozen. That bishop is entombed.
//!
//! Living in the imbalance layer rather than the screen means this costs
//! no engine time at all (CLAUDE.md #6) and carries an owner, so the
//! verbalizer can say whose problem it is.

use cozy_chess::{BitBoard, Board, Color, Piece, Rank, Square};

use crate::see::piece_value;

/// A piece its owner cannot give anything to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Entombed {
    pub square: Square,
    pub piece: Piece,
}

impl Entombed {
    /// What the ledger should knock off for it. Half, not all: the piece
    /// still guards what it stands next to, and structures unfreeze.
    pub fn discount_cp(&self) -> i32 {
        piece_value(self.piece) / 2
    }
}

/// The half of the board a piece has to reach to be doing something.
fn enemy_half(color: Color) -> BitBoard {
    match color {
        Color::White => {
            Rank::Fifth.bitboard()
                | Rank::Sixth.bitboard()
                | Rank::Seventh.bitboard()
                | Rank::Eighth.bitboard()
        }
        Color::Black => {
            Rank::Fourth.bitboard()
                | Rank::Third.bitboard()
                | Rank::Second.bitboard()
                | Rank::First.bitboard()
        }
    }
}

/// Squares `piece` can step to from `sq` over `occupied`.
///
/// [`crate::route`] has the same function privately, but its BFS cannot
/// be reused here: it treats an enemy-occupied square as a passable
/// waypoint whether or not the capture is sound, so a rook walled in by a
/// defended pawn "routes" straight through it. That is tolerable where
/// route_to is used today (its targets are empty outpost squares and its
/// callers only ever add plans), and it is fatal here, where the whole
/// claim is that no path exists.
fn steps(piece: Piece, sq: Square, occupied: BitBoard) -> BitBoard {
    match piece {
        Piece::Knight => cozy_chess::get_knight_moves(sq),
        Piece::Bishop => cozy_chess::get_bishop_moves(sq, occupied),
        Piece::Rook => cozy_chess::get_rook_moves(sq, occupied),
        Piece::Queen => {
            cozy_chess::get_bishop_moves(sq, occupied) | cozy_chess::get_rook_moves(sq, occupied)
        }
        Piece::King | Piece::Pawn => BitBoard::EMPTY,
    }
}

/// Every square an enemy pawn attacks right now.
fn pawn_cover(board: &Board, by: Color) -> BitBoard {
    board
        .colored_pieces(by, Piece::Pawn)
        .into_iter()
        .fold(BitBoard::EMPTY, |acc, p| {
            acc | cozy_chess::get_pawn_attacks(p, by)
        })
}

/// The largest cell a piece can pace and still be called entombed.
///
/// Reaching nowhere at all is the pure case (Jeremy Silman's b8-rook), but the
/// f1-bishop of Amateur's Mind p. 10 shuffles between g2, h3 and h1 and is
/// just as dead. Three is where those two meet. It matters far more than
/// it looks: the first version of this detector had no size test at all
/// and called an entombed piece in **51% of engine-quiet master positions**
/// — 362 of the 545 hits were ordinary back-rank rooks (docs/VALIDATION.md).
pub const MAX_CELL: u32 = 3;

/// What the piece can do, within [`crate::route::MAX_HOPS`].
struct Scope {
    /// Safe empty squares it can reach and hold, excluding where it stands.
    cell: BitBoard,
    /// It can reach the enemy half, or take something without losing by it.
    future: bool,
}

/// Where the piece on `from` can get to, and whether any of it is worth
/// having.
///
/// Both halves are deliberately generous, because between them they decide
/// whether we STAY SILENT. `future` counts any crossing into the enemy half
/// and any capture that is not simply a losing trade; `cell` counts every
/// square the piece could stand on safely, however pointless. The detector
/// should only speak when even a generous reading finds a small room and
/// no door.
fn scope(board: &Board, color: Color, piece: Piece, from: Square) -> Scope {
    let enemy = !color;
    let targets = enemy_half(color) | (board.colors(enemy) & !board.pieces(Piece::King));
    let occ = board.occupied() & !from.bitboard();
    let hostile_pawns = pawn_cover(board, enemy);
    let ours = piece_value(piece);

    // What counts as a wall. Enemy men and our own PAWNS do; our own
    // pieces do not, because they will move. Without this the detector
    // condemned a rook on f8 with its king on g8 and its queen on d8 —
    // 28.2% of engine-quiet master positions held an "entombed" piece and
    // 126 of the 207 were back-rank rooks penned in by their own army
    // (docs/VALIDATION.md). A cell is only a cell if its walls stay put.
    let walls = board.colors(enemy) | board.colored_pieces(color, Piece::Pawn);

    // Only PAWNS seal a square. Being covered by an enemy piece is a
    // reason not to go there this move, not a wall — the piece can be
    // driven off, traded, or simply have somewhere better to be. Counting
    // piece coverage as a wall is what condemned the c8-bishop of a
    // Paulsen Sicilian, whose one route out (…Bb5) happened to be watched
    // by a knight and a queen. Pawns are the only men that stay.
    let safe_empty = |sq: Square| !hostile_pawns.has(sq);
    let is_future = |sq: Square| match board.piece_on(sq) {
        // A capture is a future unless it just loses the piece: taking a
        // defended pawn with a knight is not a plan.
        Some(victim) => {
            piece_value(victim) >= ours
                || crate::attack::attackers_of(board, sq, enemy, occ).is_empty()
        }
        None => safe_empty(sq),
    };

    // A piece already standing in enemy territory is not entombed however
    // little it can do next — it has arrived. (A black rook on the second
    // rank was among the first false positives.)
    if enemy_half(color).has(from) {
        return Scope {
            cell: BitBoard::EMPTY,
            future: true,
        };
    }

    let mut cell = BitBoard::EMPTY;
    let mut seen = from.bitboard();
    let mut frontier = vec![from];
    for _ in 0..crate::route::MAX_HOPS {
        let mut next = Vec::new();
        for s in frontier {
            for n in steps(piece, s, walls) & !seen {
                seen |= n.bitboard();
                if targets.has(n) && !board.colors(color).has(n) && is_future(n) {
                    return Scope { cell, future: true };
                }
                // Only holdable squares behind no wall are transit. An
                // enemy piece we cannot safely take is a wall, not a
                // doorway; a friend standing there is neither.
                if !walls.has(n) && safe_empty(n) {
                    cell |= n.bitboard();
                    next.push(n);
                }
            }
        }
        if next.is_empty() {
            break;
        }
        frontier = next;
    }
    Scope {
        cell,
        future: false,
    }
}

/// Is this piece walled into a small cell with no way out of it?
fn is_entombed(board: &Board, color: Color, piece: Piece, from: Square) -> bool {
    let s = scope(board, color, piece, from);
    !s.future && s.cell.len() <= MAX_CELL
}

/// Every distinct pawn move `color` can make, as the position after it —
/// or `None` when the question cannot be asked at all.
///
/// An empty vector and `None` are different answers and the caller must
/// not confuse them: empty means the pawns are FROZEN, which is the
/// strongest possible evidence of entombment, while `None` means the
/// position could not be probed and nothing should be claimed.
///
/// Promotion choice is collapsed to one board per (from, to): the four
/// promotion pieces differ in strength, not in which squares they vacate,
/// and vacating is the only thing this test cares about.
fn pawn_move_boards(board: &Board, color: Color) -> Option<Vec<Board>> {
    // Move generation needs `color` to be the side to move. When it is not,
    // pass. If the mover is in check there is no null-move position, and
    // this side gets no permanence test at all — see `entombed`.
    let probe = if board.side_to_move() == color {
        board.clone()
    } else {
        board.null_move()?
    };
    let pawns = board.colored_pieces(color, Piece::Pawn);
    let mut seen: Vec<(Square, Square)> = Vec::new();
    let mut out = Vec::new();
    probe.generate_moves(|pm| {
        if !pawns.has(pm.from) {
            return false;
        }
        for to in pm.to {
            if seen.contains(&(pm.from, to)) {
                continue;
            }
            seen.push((pm.from, to));
            let mut after = probe.clone();
            let last_rank = match color {
                Color::White => Rank::Eighth,
                Color::Black => Rank::First,
            };
            let mv = cozy_chess::Move {
                from: pm.from,
                to,
                promotion: (to.rank() == last_rank).then_some(Piece::Queen),
            };
            if after.try_play(mv).is_ok() {
                out.push(after);
            }
        }
        false
    });
    Some(out)
}

/// Every entombed piece of `color`, in square order.
///
/// Returns empty — never a guess — when the permanence test cannot be run
/// (the side is not to move and its opponent is in check, so there is no
/// null-move board to generate pawn moves on). Silence is the right
/// failure here: an unverifiable entombment claim would discount material
/// and change who the app says is better.
pub fn entombed(board: &Board, color: Color) -> Vec<Entombed> {
    let candidates = board.colors(color)
        & (board.pieces(Piece::Knight)
            | board.pieces(Piece::Bishop)
            | board.pieces(Piece::Rook)
            | board.pieces(Piece::Queen));
    if candidates.is_empty() {
        return Vec::new();
    }
    let stuck: Vec<Entombed> = candidates
        .into_iter()
        .filter_map(|square| {
            let piece = board.piece_on(square)?;
            is_entombed(board, color, piece, square).then_some(Entombed { square, piece })
        })
        .collect();
    if stuck.is_empty() {
        return Vec::new();
    }
    // Only now is the expensive half worth running: most positions never
    // reach it, and the ones that do have one or two candidates.
    let Some(one) = pawn_move_boards(board, color) else {
        return Vec::new();
    };
    let survivors: Vec<Entombed> = stuck
        .into_iter()
        .filter(|e| {
            !one.iter()
                .any(|b| !is_entombed(b, color, e.piece, e.square))
        })
        .collect();
    if survivors.is_empty() {
        return survivors;
    }
    // Two pawn moves, not one. Fischer-Gadia (How to Reassess Your Chess
    // p. 379) is the position that forced this: the b3-bishop is boxed by
    // its own a2/c2 and Black's b5-pawn, and no SINGLE white pawn move
    // frees it — but c2-c4 followed by cxb5 does, and White is better in
    // the book precisely because his structure is still fluid. A cell you
    // can dig out of in two is not a tomb, and at depth 1 this detector
    // discounted Fischer's bishop and handed the position to Black
    // (docs/VALIDATION.md). The candidate set here is one or two pieces in
    // a couple of positions per thousand, so the squared cost is nothing.
    survivors
        .into_iter()
        .filter(|e| {
            !one.iter().any(|b1| {
                pawn_move_boards(b1, color).is_some_and(|two| {
                    two.iter()
                        .any(|b2| !is_entombed(b2, color, e.piece, e.square))
                })
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn tomb(fen: &str, color: Color) -> Vec<Square> {
        let b = Board::from_str(fen).expect("fen");
        entombed(&b, color).into_iter().map(|e| e.square).collect()
    }

    /// The starting position is the whole point of the permanence test:
    /// eight pieces with nothing to do and every one of them one pawn move
    /// from a future.
    #[test]
    fn nobody_is_entombed_in_the_starting_position() {
        let fen = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";
        assert!(tomb(fen, Color::White).is_empty());
        assert!(tomb(fen, Color::Black).is_empty());
    }

    /// Jeremy Silman, The Complete Book of Chess Strategy, p. 192,
    /// "Entombed Pieces" (diagram 183): the b7-pawn and c8-bishop tie
    /// Black's rook to b8 forever. It is not attacked and never will be —
    /// it is simply out of the game, which is why the WSUI screen is the
    /// wrong home for it.
    #[test]
    fn cbcs_192_the_entombed_rook() {
        assert_eq!(
            tomb("1rB5/1P6/p4k2/2p5/2P2KP1/8/8/8 w - - 0 1", Color::Black),
            vec![Square::B8]
        );
    }

    /// Jeremy Silman, The Complete Book of Chess Strategy, p. 193,
    /// "Entombed Pieces" (diagram 184). The book calls both white knights
    /// entombed and this detector calls NEITHER, deliberately — the
    /// prediction that it would catch h2 was recorded and refuted (see
    /// docs/VALIDATION.md, run 12 / #12).
    ///
    /// - b1 is not entombed because c3 is defended by the d2-pawn, so the
    ///   knight can pay a bishop for itself and go c3-e4-d6.
    /// - h2 is not entombed because **d2xe3** exists. That capture takes
    ///   away the very pawn that covers d2 and g3, and the box opens.
    ///
    /// The second one is the honest half. The permanence test asks whether
    /// ONE pawn move frees the piece and a capture is a pawn move; keeping
    /// it in means the detector is stricter than the book, which is the
    /// right direction for something that discounts material.
    #[test]
    fn cbcs_193_knights_are_boxed_in_but_the_box_has_a_capture_in_it() {
        assert!(tomb(
            "8/8/2p2k2/6p1/1b3pP1/4pP2/3P2KN/1N6 w - - 0 1",
            Color::White
        )
        .is_empty());
        // Take the escape hatch away — d2 on d3, nothing to capture — and
        // the h2-knight is entombed exactly as the text describes.
        assert_eq!(
            tomb("8/8/2p2k2/6p1/1b3pP1/3PpP2/6KN/1N6 b - - 0 1", Color::White),
            vec![Square::H2]
        );
    }

    /// Jeremy Silman, The Amateur's Mind, p. 10 (Bishops vs Knights): the
    /// f1-bishop is walled in behind pawns that cannot move, opposite a
    /// knight that dominates the board. This is the case that separates
    /// entombment from bad-bishop-ness — the bishop has two legal moves
    /// and neither leads anywhere, ever.
    #[test]
    fn am_10_the_entombed_bishop() {
        assert_eq!(
            tomb(
                "8/8/8/6p1/3n1pP1/2pPpP2/k1P1P3/3K1B2 w - - 0 1",
                Color::White
            ),
            vec![Square::F1]
        );
        // The knight that beats it is the opposite of entombed.
        assert!(tomb(
            "8/8/8/6p1/3n1pP1/2pPpP2/k1P1P3/3K1B2 w - - 0 1",
            Color::Black
        )
        .is_empty());
    }

    /// A bishop behind an unmoved but MOBILE pawn wall is undeveloped, not
    /// entombed: one push and it is on a diagonal. The distinction is the
    /// whole reason this detector exists rather than a looser TrappedPiece.
    #[test]
    fn a_mobile_pawn_wall_is_development_not_entombment() {
        // White to play, everything free: Bf1 has no moves but e2-e4 opens
        // the f1-a6 diagonal into Black's half.
        assert!(tomb("4k3/8/8/8/8/8/4P1P1/4KB2 w - - 0 1", Color::White).is_empty());
    }

    /// The same bishop with the wall frozen by enemy pawns IS entombed:
    /// e2 and g2 can neither push nor capture, so f1 is a cell.
    #[test]
    fn the_same_bishop_behind_a_frozen_wall_is_entombed() {
        assert_eq!(
            tomb("4k3/8/8/8/8/4p1p1/4P1P1/4KB2 w - - 0 1", Color::White),
            vec![Square::F1]
        );
    }
}
