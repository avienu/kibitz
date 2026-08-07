//! Piece routing: the shortest safe path a piece can take to a square it
//! wants, as an ordered sequence.
//!
//! [`crate::imbalance`] has had a knight-only BFS since run 3, but the
//! corpus keeps asking for bishop and rook regroupings (the book-eval
//! misses are full of Bh2, Bd6, Bf4 — see docs/VALIDATION.md). Routing is
//! the same search for every piece; only the step function differs, so it
//! lives here and takes the piece type as a parameter.
//!
//! Safety is judged over TIME, not against the current attack map. A
//! waypoint is transit: being kicked off it costs a tempo, not the plan,
//! so it only fails if an enemy pawn can be there BEFORE we arrive. The
//! destination is a home we mean to keep, and callers hold it to the
//! permanent hole test instead. See [`crate::pawn_contact`].

use cozy_chess::{
    get_bishop_moves, get_knight_moves, get_rook_moves, BitBoard, Board, Color, Piece, Square,
};

/// How many moves a route may take. Three hops covers Nc3-e2-d4 but not
/// the classical regroupings (Nb1-d2-f1-g3-f5 is four); beyond five a
/// static route outlives the position it was computed in.
pub const MAX_HOPS: u8 = 5;

/// One found route: the ordered waypoints between origin and destination
/// (exclusive of both) plus the destination reached.
#[derive(Debug, Clone, PartialEq)]
pub struct Route {
    pub via: Vec<Square>,
    pub to: Square,
}

impl Route {
    /// Moves the route costs, counting the arrival.
    pub fn moves(&self) -> u8 {
        self.via.len() as u8 + 1
    }
}

/// Squares `piece` can step to from `sq` over `occupied`. Sliders see
/// through nothing — a route step is a real move on the real board.
fn steps(piece: Piece, sq: Square, occupied: BitBoard) -> BitBoard {
    match piece {
        Piece::Knight => get_knight_moves(sq),
        Piece::Bishop => get_bishop_moves(sq, occupied),
        Piece::Rook => get_rook_moves(sq, occupied),
        Piece::Queen => get_bishop_moves(sq, occupied) | get_rook_moves(sq, occupied),
        // Kings route too (endgame marches), one square at a time.
        Piece::King => cozy_chess::get_king_moves(sq),
        Piece::Pawn => BitBoard::EMPTY,
    }
}

/// Shortest safe route for the piece standing on `from` to any square in
/// `targets`, or `None` if every target is unreachable within [`MAX_HOPS`].
///
/// - `target_ok` decides whether a destination is worth having; callers
///   own that policy (a hole test, an open-file entry square, …).
/// - Waypoints must be empty of our own pieces, not out-gunned by enemy
///   pieces, and not reachable by an enemy pawn before we arrive.
pub fn route_to(
    board: &Board,
    color: Color,
    piece: Piece,
    from: Square,
    targets: BitBoard,
    target_ok: &dyn Fn(Square) -> bool,
) -> Option<Route> {
    let enemy = !color;
    // The routing piece does not block its own path, nor defend the square
    // it is standing on.
    let occ = board.occupied() & !from.bitboard();
    let mut outgunned = BitBoard::EMPTY;
    for sq in !board.occupied() {
        let att = crate::attack::attackers_of(board, sq, enemy, occ).len();
        let def = (crate::attack::attackers_of(board, sq, color, occ) & !from.bitboard()).len();
        if att > def {
            outgunned |= sq.bitboard();
        }
    }
    let blocked = board.colors(color) | outgunned;
    let evict = crate::pawn_contact::evict_distance(board, enemy);
    let waypoint_ok = |sq: Square, hop: u8| {
        hop == 0 || !crate::pawn_contact::contested_within(&evict, sq, hop - 1)
    };

    let mut prev: [Option<Square>; 64] = [None; 64];
    let mut seen = from.bitboard();
    let mut frontier = vec![from];
    for depth in 0..MAX_HOPS {
        let hop = depth + 1;
        let mut next = Vec::new();
        for &s in &frontier {
            for n in steps(piece, s, occ) & !seen {
                // A square our own piece stands on is not a destination,
                // however reachable the path to it looks.
                if targets.has(n) && !board.colors(color).has(n) && target_ok(n) {
                    let mut via = Vec::new();
                    let mut cur = s;
                    while cur != from {
                        via.push(cur);
                        cur = prev[cur as usize].expect("bfs chain");
                    }
                    via.reverse();
                    return Some(Route { via, to: n });
                }
                if blocked.has(n) || !waypoint_ok(n, hop) {
                    continue;
                }
                prev[n as usize] = Some(s);
                seen |= n.bitboard();
                next.push(n);
            }
        }
        frontier = next;
    }
    None
}

/// Shortest safe route for the piece on `from` to any square from which
/// it would ATTACK `victim` — the "get at that defender" search.
///
/// Attack sets for knights, bishops, rooks and queens are symmetric, so
/// the squares that attack `victim` are just the squares `piece` could
/// step to *from* `victim`. Pawns and kings are excluded: a pawn's attack
/// set is not symmetric, and marching the king at a defended piece is
/// not a plan.
pub fn route_to_attack(
    board: &Board,
    color: Color,
    piece: Piece,
    from: Square,
    victim: Square,
) -> Option<Route> {
    if matches!(piece, Piece::Pawn | Piece::King) {
        return None;
    }
    let occ = board.occupied() & !from.bitboard();
    let posts = steps(piece, victim, occ) & !board.colors(color);
    if posts.has(from) {
        // Already bearing down on it: no journey needed.
        return Some(Route {
            via: Vec::new(),
            to: from,
        });
    }
    route_to(board, color, piece, from, posts, &|_| true)
}

/// Hints whose square list is an ordered route: `[origin, via.., target]`.
const ROUTE_HINTS: &[&str] = &[
    "ManeuverKnightToOutpost",
    "ManeuverBishopToSupportPoint",
    "ManeuverRookToOpenFile",
];

/// Hints that own EXACT squares and must not have them renamed or
/// merged by [`crate::plans`]'s file-level clustering.
///
/// Clustering rewrites a plan's target to a file-level vote and pools
/// every member's squares into `CompositePlan::squares`, which downstream
/// move generation then reads. For a hint whose squares are a precise
/// pair ("this guard, that square") or a precise route, that pooling is
/// not a merge but a corruption — it silently retargets other hints in
/// the same cluster.
///
/// `ManeuverKnightToOutpost` is deliberately absent: it has clustered
/// since run 5, its convergence story is the validated Sveshnikov golden,
/// and its destination survives the vote. The run-12 additions do not —
/// the Opera Game merged White's Bc4-d5 into a d-file cluster and
/// narrated it as "Black: walk the bishop round to d2". Proper
/// owner-aware convergence for these belongs to the sequencing layer,
/// which has the board; file clustering does not.
pub const EXACT_DESTINATION_HINTS: &[&str] = &[
    "ManeuverBishopToSupportPoint",
    "ManeuverRookToOpenFile",
    "UndermineDefender",
    "OverprotectStrongPoint",
];

/// Why a route's destination is worth the trip, in evidence language.
fn reason_for(hint: &str) -> &'static str {
    match hint {
        "ManeuverBishopToSupportPoint" => "support_point",
        "ManeuverRookToOpenFile" => "open_file",
        _ => "permanent_hole",
    }
}

fn piece_word(p: Piece) -> &'static str {
    match p {
        Piece::Pawn => "pawn",
        Piece::Knight => "knight",
        Piece::Bishop => "bishop",
        Piece::Rook => "rook",
        Piece::Queen => "queen",
        Piece::King => "king",
    }
}

fn parse_sq(s: &str) -> Option<Square> {
    s.parse().ok()
}

/// Promote route-bearing [`crate::record::PlanHint`]s into first-class
/// [`Maneuver`](crate::record::Maneuver) records (schema v4), shortest
/// route first. The hints keep their existing shape so every current
/// consumer is unaffected; this is the same data, no longer flattened.
pub fn extract(
    board: &Board,
    imbalances: &[crate::record::Imbalance],
) -> Vec<crate::record::Maneuver> {
    let mut out = Vec::new();
    for imb in imbalances {
        for plan in &imb.plans {
            if !ROUTE_HINTS.contains(&plan.hint.as_str()) || plan.squares.len() < 2 {
                continue;
            }
            let squares: Vec<Square> = plan.squares.iter().filter_map(|s| parse_sq(s)).collect();
            if squares.len() != plan.squares.len() {
                continue;
            }
            let (from, to) = (squares[0], squares[squares.len() - 1]);
            let Some(piece) = board.piece_on(from) else {
                continue;
            };
            let Some(color) = board.color_on(from) else {
                continue;
            };
            // What has to be true first: any enemy piece already covering
            // the destination must be traded or driven off before the
            // piece can settle there (HTRYC ex. 60 — holes are permanent,
            // piece cover is tradeable).
            let occ = board.occupied() & !from.bitboard();
            let blocked_by: Vec<String> = crate::attack::attackers_of(board, to, !color, occ)
                .into_iter()
                .map(crate::record::square_name)
                .collect();
            out.push(crate::record::Maneuver {
                piece: piece_word(piece).into(),
                from: crate::record::square_name(from),
                via: squares[1..squares.len() - 1]
                    .iter()
                    .map(|s| crate::record::square_name(*s))
                    .collect(),
                to: crate::record::square_name(to),
                moves: (squares.len() - 1) as u8,
                reason: reason_for(&plan.hint).into(),
                blocked_by,
                // A reroute belongs to whoever owns the piece, NOT to the
                // side the parent imbalance happens to favor. Inheriting
                // `imb.favors` let a Balanced imbalance hand White's
                // bishop plan to Black (Opera Game, run 12).
                favors: match color {
                    Color::White => crate::record::Favors::White,
                    Color::Black => crate::record::Favors::Black,
                },
            });
        }
    }
    // The opposition king-step: an owner-bearing record because the side
    // to move owns it, not the side the position favours.
    if let Some((color, to)) = crate::imbalance::opposition_move(board) {
        let from = board.king(color);
        out.push(crate::record::Maneuver {
            piece: "king".into(),
            from: crate::record::square_name(from),
            via: Vec::new(),
            to: crate::record::square_name(to),
            moves: 1,
            reason: "opposition".into(),
            blocked_by: Vec::new(),
            favors: match color {
                Color::White => crate::record::Favors::White,
                Color::Black => crate::record::Favors::Black,
            },
        });
    }
    out.sort_by_key(|m| m.moves);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn any(_: Square) -> bool {
        true
    }

    /// The Sveshnikov d5 complex: the c3-knight's route to d5 becomes a
    /// Maneuver that names the piece, the origin and the cost — none of
    /// which the flattened PlanHint could express.
    #[test]
    fn extract_promotes_a_route_hint_to_a_maneuver() {
        let fen = "r1bqkb1r/pp3ppp/2np1n2/1N2p3/4P3/2N5/PPP2PPP/R1BQKB1R w KQkq - 0 7";
        let board = Board::from_str(fen).expect("fen");
        let record = crate::analyze(&board);
        let m = record
            .maneuvers
            .iter()
            .find(|m| m.to == "d5")
            .expect("a maneuver toward d5");
        assert_eq!(m.piece, "knight");
        assert_eq!(m.from, "c3");
        assert_eq!(m.moves, 1);
        assert_eq!(m.favors, crate::record::Favors::White);
    }

    /// The four-hop regrouping that motivated raising the hop ceiling:
    /// the b1-knight walks to f5 via d2-f1-g3. Three hops could not find
    /// it, so the plan was invisible.
    #[test]
    fn knight_finds_the_four_hop_regrouping() {
        let board = Board::from_str("4k3/8/8/5p2/8/8/8/1N2K3 w - - 0 1").expect("fen");
        let r = route_to(
            &board,
            Color::White,
            Piece::Knight,
            Square::B1,
            Square::F5.bitboard(),
            &any,
        )
        .expect("route");
        assert_eq!(r.to, Square::F5);
        assert!(r.moves() >= 4, "{r:?}");
        assert!(r.moves() <= MAX_HOPS, "{r:?}");
    }

    /// Routing is not knight-only any more: a bishop rerouted outside its
    /// own pawn chain is one of the corpus's most common asks.
    #[test]
    fn bishop_routes_to_the_long_diagonal() {
        let board = Board::from_str("4k3/8/8/8/8/2P5/1P6/2B1K3 w - - 0 1").expect("fen");
        let r = route_to(
            &board,
            Color::White,
            Piece::Bishop,
            Square::C1,
            Square::A3.bitboard(),
            &any,
        )
        .expect("route");
        assert_eq!(r.to, Square::A3);
    }

    /// A rook lift: e1-e3-g3 is two moves, and the router reports the
    /// intermediate square rather than a bare destination.
    #[test]
    fn rook_lift_reports_its_waypoint() {
        let board = Board::from_str("6k1/8/8/8/8/8/PPP5/4R1K1 w - - 0 1").expect("fen");
        let r = route_to(
            &board,
            Color::White,
            Piece::Rook,
            Square::E1,
            Square::G3.bitboard(),
            &any,
        )
        .expect("route");
        assert_eq!(r.to, Square::G3);
        assert_eq!(r.moves(), 2, "{r:?}");
        assert_eq!(r.via.len(), 1, "the lift square must be named: {r:?}");
    }

    /// Sliders do not see through their own pawns: the e-file is shut by
    /// the e2 pawn, so reaching e5 costs a detour with a named waypoint
    /// rather than a one-move teleport up the blocked file.
    #[test]
    fn slider_route_respects_blockers() {
        let board = Board::from_str("6k1/8/8/8/8/8/PPP1P3/4R1K1 w - - 0 1").expect("fen");
        let r = route_to(
            &board,
            Color::White,
            Piece::Rook,
            Square::E1,
            Square::E5.bitboard(),
            &any,
        )
        .expect("route");
        assert_eq!(r.to, Square::E5);
        assert!(r.moves() >= 2, "the blocked e-file is not a route: {r:?}");
        assert!(!r.via.is_empty(), "the detour must be named: {r:?}");
    }

    /// A square our own piece already occupies is not a destination.
    #[test]
    fn own_piece_square_is_not_a_destination() {
        let board = Board::from_str("6k1/8/8/8/8/6N1/PPP5/4R1K1 w - - 0 1").expect("fen");
        let r = route_to(
            &board,
            Color::White,
            Piece::Rook,
            Square::E1,
            Square::G3.bitboard(),
            &any,
        );
        assert!(r.is_none(), "{r:?}");
    }

    /// `target_ok` is the caller's policy: a destination it rejects is
    /// not a route, however reachable.
    #[test]
    fn caller_owns_the_destination_policy() {
        let board = Board::from_str("4k3/8/8/8/8/8/8/1N2K3 w - - 0 1").expect("fen");
        let r = route_to(
            &board,
            Color::White,
            Piece::Knight,
            Square::B1,
            Square::D2.bitboard(),
            &|_| false,
        );
        assert!(r.is_none());
    }
}
