//! Stage 2 — positional imbalance assessment (docs/KIBITZ_ENGINE_SPEC.md).
//!
//! Eight static detectors, each returning at most one [`Imbalance`] with
//! structured evidence and plan hints. All are cheap board scans — no
//! search, no engine.

use std::collections::BTreeMap;

use cozy_chess::{
    get_knight_moves, get_pawn_attacks, BitBoard, Board, Color, File, Piece, Rank, Square,
};
use serde_json::json;

use crate::record::{square_name, Favors, Imbalance, ImbalanceKind, Magnitude, PlanHint};
use crate::see::piece_value;

fn favors(diff: i32, minor: i32, clear: i32) -> Option<(Favors, Magnitude)> {
    let mag = diff.abs();
    if mag < minor {
        return None;
    }
    let side = if diff > 0 {
        Favors::White
    } else {
        Favors::Black
    };
    let magnitude = if mag >= clear * 2 {
        Magnitude::Winning
    } else if mag >= clear {
        Magnitude::Clear
    } else {
        Magnitude::Minor
    };
    Some((side, magnitude))
}

/// Recall-oriented variant (run 8.5): a detector that has gathered real
/// evidence should still REPORT the imbalance when the lean is too small
/// to pick a side — as a Balanced/Minor record. Narration's dominance
/// selection filters Minor noise, and a Balanced record contributes
/// nothing to any favors lean, so this stays honest.
fn favors_or_balanced(diff: i32, minor: i32, clear: i32) -> (Favors, Magnitude) {
    favors(diff, minor, clear).unwrap_or((Favors::Balanced, Magnitude::Minor))
}

/// Chebyshev distance between two squares.
fn chebyshev(a: Square, b: Square) -> i8 {
    let df = (a.file() as i8 - b.file() as i8).abs();
    let dr = (a.rank() as i8 - b.rank() as i8).abs();
    df.max(dr)
}

/// The central square (d4/d5/e4/e5) nearest to `sq`.
fn nearest_center_square(sq: Square) -> Square {
    [Square::D4, Square::D5, Square::E4, Square::E5]
        .into_iter()
        .min_by_key(|c| chebyshev(sq, *c))
        .expect("non-empty")
}

/// True if some enemy pawn can still reach a square from which it would
/// attack `sq` (a pawn LEVER remains possible against `owner`'s pawn).
fn pawn_lever_possible(board: &Board, owner: Color, sq: Square) -> bool {
    let enemy = !owner;
    let dr: i8 = match owner {
        Color::White => 1,
        Color::Black => -1,
    };
    for df in [-1i8, 1] {
        let Some(origin) = sq.try_offset(df, dr) else {
            continue;
        };
        let on_file = board.colored_pieces(enemy, Piece::Pawn) & origin.file().bitboard();
        for p in on_file {
            let can_reach = match enemy {
                Color::White => p.rank() as i8 <= origin.rank() as i8,
                Color::Black => p.rank() as i8 >= origin.rank() as i8,
            };
            if can_reach {
                return true;
            }
        }
    }
    false
}

fn sq_list(bb: BitBoard) -> serde_json::Value {
    json!(bb.into_iter().map(square_name).collect::<Vec<_>>())
}

/// Squares a side's pawns could EVER attack by advancing (pawn-attack
/// spans): used for hole detection.
fn pawn_attack_span(board: &Board, color: Color) -> BitBoard {
    let mut span = BitBoard::EMPTY;
    for p in board.colored_pieces(color, Piece::Pawn) {
        let mut sq = p;
        loop {
            span |= get_pawn_attacks(sq, color);
            let next = match color {
                Color::White => sq.try_offset(0, 1),
                Color::Black => sq.try_offset(0, -1),
            };
            match next {
                Some(n) => sq = n,
                None => break,
            }
        }
    }
    span
}

fn front_span(color: Color, sq: Square) -> BitBoard {
    // Squares strictly ahead of `sq` on its own and adjacent files.
    let mut span = BitBoard::EMPTY;
    for df in -1..=1i8 {
        let Some(mut s) = sq.try_offset(df, 0) else {
            continue;
        };
        loop {
            let next = match color {
                Color::White => s.try_offset(0, 1),
                Color::Black => s.try_offset(0, -1),
            };
            match next {
                Some(n) => {
                    span |= n.bitboard();
                    s = n;
                }
                None => break,
            }
        }
    }
    span
}

/// 1. Minor-piece imbalance: B vs N counts, bishop pair, bad bishops,
///    knight outposts, open/closed character.
pub fn minor_pieces(board: &Board) -> Option<Imbalance> {
    let mut evidence = BTreeMap::new();
    let mut score = 0i32; // + favors White
                          // Side-owned plans, filtered against the final lean (see
                          // pawn_structure for the rationale).
    let mut sided: Vec<(Color, PlanHint)> = Vec::new();

    let wb = board.colored_pieces(Color::White, Piece::Bishop).len() as i32;
    let wn = board.colored_pieces(Color::White, Piece::Knight).len() as i32;
    let bb = board.colored_pieces(Color::Black, Piece::Bishop).len() as i32;
    let bn = board.colored_pieces(Color::Black, Piece::Knight).len() as i32;
    if wb + wn + bb + bn == 0 {
        return None;
    }

    // Closed-ness: locked central pawn pairs (own pawn directly blocked by
    // an enemy pawn) on files c–f.
    let mut locked = 0;
    for p in board.colored_pieces(Color::White, Piece::Pawn) {
        if (2..=5).contains(&(p.file() as i8)) {
            if let Some(front) = p.try_offset(0, 1) {
                if board.colored_pieces(Color::Black, Piece::Pawn).has(front) {
                    locked += 1;
                }
            }
        }
    }
    let closed = locked >= 2;
    evidence.insert("locked_center_pawns".into(), json!(locked));

    // Bishop pair in an open position.
    if wb >= 2 && bb < 2 {
        evidence.insert("bishop_pair".into(), json!("white"));
        score += if closed { 10 } else { 35 };
    } else if bb >= 2 && wb < 2 {
        evidence.insert("bishop_pair".into(), json!("black"));
        score -= if closed { 10 } else { 35 };
    }

    // Hunting the pair. Alex Yermolinsky's point about the two bishops is
    // that their value is an OPTION: you get to choose the moment to
    // trade one for a knight. The corollary is the plan on this side of
    // the board — when the opponent holds the pair, go and buy one of
    // their bishops with a knight.
    //
    // "Win the pair" and "trade the pair off" are the same ACTION seen
    // from two scorelines (gain it, or deny it), so they are one hint.
    // The corpus asks for it under both names plus a third.
    for (color, theirs) in [(Color::White, bb), (Color::Black, wb)] {
        if theirs < 2 || closed {
            continue; // no pair to hunt, and in a closed position it is no prize
        }
        let enemy = !color;
        let mut found: Option<(Square, Square)> = None;
        // A bishop still sitting at home is not the one whose loss hurts,
        // and its owner would often be glad to trade it. Hunt the one
        // that is actually doing something.
        let home = match enemy {
            Color::White => Square::C1.bitboard() | Square::F1.bitboard(),
            Color::Black => Square::C8.bitboard() | Square::F8.bitboard(),
        };
        'hunt: for knight in board.colored_pieces(color, Piece::Knight) {
            for bishop in board.colored_pieces(enemy, Piece::Bishop) & !home {
                if crate::route::route_to_attack(board, color, Piece::Knight, knight, bishop)
                    .is_some()
                {
                    found = Some((knight, bishop));
                    break 'hunt;
                }
            }
        }
        if let Some((knight, bishop)) = found {
            sided.push((
                color,
                PlanHint {
                    hint: "HuntBishopPair".into(),
                    squares: vec![square_name(knight), square_name(bishop)],
                },
            ));
        }
    }

    // Bad bishops: hemmed in by own fixed central pawns on its color
    // complex (home-rank pawns excluded) AND actually immobile.
    let light = BitBoard(0x55AA_55AA_55AA_55AA);
    let advanced_ranks = |c: Color| match c {
        Color::White => {
            Rank::Third.bitboard()
                | Rank::Fourth.bitboard()
                | Rank::Fifth.bitboard()
                | Rank::Sixth.bitboard()
        }
        Color::Black => {
            Rank::Sixth.bitboard()
                | Rank::Fifth.bitboard()
                | Rank::Fourth.bitboard()
                | Rank::Third.bitboard()
        }
    };
    let mut plans = Vec::new();
    for (color, sign) in [(Color::White, 1i32), (Color::Black, -1i32)] {
        for b in board.colored_pieces(color, Piece::Bishop) {
            let complex = if light.has(b) { light } else { !light };
            let own_center_pawns = board.colored_pieces(color, Piece::Pawn)
                & complex
                & advanced_ranks(color)
                & (File::C.bitboard()
                    | File::D.bitboard()
                    | File::E.bitboard()
                    | File::F.bitboard());
            let mobility =
                (cozy_chess::get_bishop_moves(b, board.occupied()) & !board.colors(color)).len();
            // A bishop is bad behind two fixed central pawns when nearly
            // immobile, or behind a full three-pawn chain on its color
            // even with a few squares to shuffle on (Jeremy Silman, The
            // Complete Book of Chess Strategy, p. 279: the c5/d6/e5 chain
            // buries the e7-bishop).
            let bad = (own_center_pawns.len() >= 2 && mobility <= 2)
                || (own_center_pawns.len() >= 3 && mobility <= 4);
            if bad {
                evidence.insert(
                    format!(
                        "bad_bishop_{}",
                        if color == Color::White { "white" } else { "black" }
                    ),
                    json!({
                        "bishop": square_name(b),
                        "blocking_pawns": own_center_pawns.into_iter().map(square_name).collect::<Vec<_>>()
                    }),
                );
                score -= sign * 25;
                // The owner's counter-plan: trade it off or reroute it
                // outside the chain.
                sided.push((
                    color,
                    PlanHint {
                        hint: "TradeOrActivateBadBishop".into(),
                        squares: vec![square_name(b)],
                    },
                ));
            }
        }
    }

    // Opposite-colored single bishops: a named imbalance worth reporting
    // even when the ledger is level.
    if wb == 1 && bb == 1 {
        let w_light = !(board.colored_pieces(Color::White, Piece::Bishop) & light).is_empty();
        let b_light = !(board.colored_pieces(Color::Black, Piece::Bishop) & light).is_empty();
        if w_light != b_light {
            evidence.insert("opposite_bishops".into(), json!(true));
        }
    }
    // Asymmetric minor-piece mix (B vs N stories).
    if (wb, wn) != (bb, bn) {
        evidence.insert(
            "minor_mix".into(),
            json!({
                "white": format!("{wb}B+{wn}N"),
                "black": format!("{bb}B+{bn}N"),
            }),
        );
    }

    // Knights benefit from closed positions and available outposts.
    let holes_w_side = holes_in_camp(board, Color::Black); // holes in black's camp usable by white
    let holes_b_side = holes_in_camp(board, Color::White);
    if closed {
        score += (wn - bn) * 20;
    }
    if wn > 0 && !holes_w_side.is_empty() {
        score += 10;
    }
    if bn > 0 && !holes_b_side.is_empty() {
        score -= 10;
    }
    evidence.insert(
        "character".into(),
        json!(if closed { "closed" } else { "open" }),
    );

    // Outpost denial: when every enemy knight has neither an outpost nor
    // a safe route to one, the bishop side's plan is to KEEP it that way
    // (Jeremy Silman, How to Reassess Your Chess, restrict-the-knight
    // examples). The mirror of ManeuverKnightToOutpost: routes found
    // there suppress the hint here.
    for (color, sign, enemy_holes) in [
        (Color::White, 1i32, holes_in_camp(board, Color::White)),
        (Color::Black, -1i32, holes_in_camp(board, Color::Black)),
    ] {
        let enemy = !color;
        if board.colored_pieces(color, Piece::Bishop).is_empty() {
            continue;
        }
        let eknights = board.colored_pieces(enemy, Piece::Knight);
        if eknights.is_empty() {
            continue;
        }
        // An army that simply has not developed yet is not "restricted":
        // with most enemy minors still at home this is an opening, not a
        // domination story.
        let enemy_back = match enemy {
            Color::White => Rank::First,
            Color::Black => Rank::Eighth,
        };
        let enemy_minors_home = (board.colors(enemy)
            & (board.pieces(Piece::Knight) | board.pieces(Piece::Bishop))
            & enemy_back.bitboard())
        .len();
        if enemy_minors_home >= 3 {
            continue;
        }
        let mask = hole_mask(board, color); // holes in the restricting side's camp
        let all_homeless = eknights
            .into_iter()
            .all(|n| !mask.has(n) && knight_route_to(board, enemy, n, enemy_holes).is_none());
        if all_homeless {
            // Plan only — no score: "their knight has no home" is advice,
            // not yet an edge.
            let _ = sign;
            sided.push((
                color,
                PlanHint {
                    hint: "RestrictKnight".into(),
                    squares: eknights.into_iter().map(square_name).collect(),
                },
            ));
        }
    }

    let white_wants_closed = closed && (wn > bn || (wn > 0 && !holes_w_side.is_empty()));
    let black_wants_closed = closed && (bn > wn || (bn > 0 && !holes_b_side.is_empty()));
    if white_wants_closed || black_wants_closed {
        plans.push(PlanHint {
            hint: "KeepPositionClosed".into(),
            squares: vec![],
        });
    }
    // The bishop side wants lines: with the pair (whatever the current
    // character — in a closed position opening it IS the plan), or with a
    // straight bishops-vs-knights mix in an open position.
    let pair_edge = (wb >= 2 && bb < 2) || (bb >= 2 && wb < 2);
    let mix_edge = !closed && ((wb > bb && wn < bn) || (bb > wb && bn < wn));
    if pair_edge || mix_edge {
        plans.push(PlanHint {
            hint: "OpenPositionForBishops".into(),
            squares: vec![],
        });
    }

    let asymmetric = evidence.keys().any(|k| {
        k.starts_with("bad_bishop")
            || k == "bishop_pair"
            || k == "opposite_bishops"
            || k == "minor_mix"
    });
    let (f, m) = if asymmetric || !plans.is_empty() || !sided.is_empty() {
        favors_or_balanced(score, 20, 45)
    } else {
        favors(score, 20, 45)?
    };
    plans.extend(sided.into_iter().filter_map(|(side, p)| {
        let side_favors = match side {
            Color::White => Favors::White,
            Color::Black => Favors::Black,
        };
        (f == Favors::Balanced || f == side_favors).then_some(p)
    }));
    Some(Imbalance {
        kind: ImbalanceKind::MinorPieces,
        favors: f,
        magnitude: m,
        evidence,
        plans,
    })
}

/// Squares in `camp_owner`'s outpost band the owner's pawns can never
/// defend, WITHOUT the occupancy filter — used to test whether a piece
/// already standing on such a square is on an outpost.
fn hole_mask(board: &Board, camp_owner: Color) -> BitBoard {
    let owner_span = pawn_attack_span(board, camp_owner);
    let half = match camp_owner {
        Color::White => Rank::Third.bitboard() | Rank::Fourth.bitboard(),
        Color::Black => Rank::Sixth.bitboard() | Rank::Fifth.bitboard(),
    };
    let files = !(File::A.bitboard() | File::H.bitboard());
    half & files & !owner_span
}

/// Holes in `camp_owner`'s camp: squares the owner's pawns can never
/// defend, restricted to the ranks where an enemy outpost actually bites
/// (5th/6th from the attacker's view). Ranks nearer the back rank are
/// pawn-undefendable by construction and piece-covered in practice —
/// counting them buries the real signal in noise.
fn holes_in_camp(board: &Board, camp_owner: Color) -> BitBoard {
    // Central-ish holes matter (files b-g); occupied squares are not
    // DESTINATIONS (see hole_mask for the occupancy-free test).
    hole_mask(board, camp_owner) & !board.occupied()
}

/// Two kings hold the opposition when the file gap AND the rank gap
/// between them are both EVEN — direct (0,2), diagonal (2,2), distant
/// (0,4), and the off-line rectangle (4,2) are all the same statement.
/// Whoever is NOT to move holds it.
fn in_opposition(a: Square, b: Square) -> bool {
    let df = (a.file() as i8 - b.file() as i8).abs();
    let dr = (a.rank() as i8 - b.rank() as i8).abs();
    df % 2 == 0 && dr % 2 == 0 && (df != 0 || dr != 0)
}

/// The king move that seizes the opposition, if the side to move has one.
///
/// Deliberately NOT a `PlanHint`. A plan hint has no owner field, so it
/// inherits the parent imbalance's favored side — and opposition belongs
/// to whoever is to move, which is a fact of the position rather than a
/// judgement about who stands better. Routed through `Maneuver` (which
/// does carry an owner) so it can never be narrated for the wrong player.
///
/// Gated to positions with nothing but kings and pawns: opposition is a
/// real idea there and noise everywhere else, and this keeps the hint out
/// of every middlegame where two kings happen to sit on even parity.
/// (Jeremy Silman, How to Reassess Your Chess, exs. 20 and 22.)
pub(crate) fn opposition_move(board: &Board) -> Option<(Color, Square)> {
    for piece in [Piece::Knight, Piece::Bishop, Piece::Rook, Piece::Queen] {
        if !board.pieces(piece).is_empty() {
            return None;
        }
    }
    let side = board.side_to_move();
    let ours = board.king(side);
    let theirs = board.king(!side);
    // Already holding it with the opponent to move is not a plan; taking
    // it when it is OUR move is.
    for to in cozy_chess::get_king_moves(ours) & !board.colors(side) {
        // Kings may never touch, and the square must not be defended by
        // the enemy king.
        if chebyshev(to, theirs) <= 1 {
            continue;
        }
        if in_opposition(to, theirs) {
            return Some((side, to));
        }
    }
    None
}

/// 2. Pawn structure.
pub fn pawn_structure(board: &Board) -> Option<Imbalance> {
    let mut evidence = BTreeMap::new();
    // Neutral plans (blockade family: plans.rs re-attributes those by
    // name) go straight into `plans`; side-owned plans carry their owner
    // so that, once the imbalance's lean is known, plans belonging to the
    // DISFAVORED side can be dropped instead of being misattributed.
    let mut plans = Vec::new();
    let mut sided: Vec<(Color, PlanHint)> = Vec::new();
    let mut score = 0i32;

    let mut iso = [BitBoard::EMPTY; 2];
    let mut doubled = [BitBoard::EMPTY; 2];
    let mut backward = [BitBoard::EMPTY; 2];
    let mut passed = [BitBoard::EMPTY; 2];

    for (ci, color) in [(0, Color::White), (1, Color::Black)] {
        let own = board.colored_pieces(color, Piece::Pawn);
        let enemy = board.colored_pieces(!color, Piece::Pawn);
        for p in own {
            let file = p.file();
            let adj = adjacent_files(file);
            if (own & adj).is_empty() {
                iso[ci] |= p.bitboard();
            }
            if !(own & file.bitboard() & !p.bitboard()).is_empty() {
                doubled[ci] |= p.bitboard();
            }
            // Passed: no enemy pawn ahead on own/adjacent files.
            if (front_span(color, p) & enemy).is_empty()
                && (file_front(color, p) & enemy).is_empty()
            {
                passed[ci] |= p.bitboard();
            }
            // Backward: no own pawn beside/behind on adjacent files (not
            // isolated — that's worse), and the pawn cannot safely step
            // level: its stop square is enemy-pawn-controlled, or — on a
            // file the enemy has half-open — the square two ahead is,
            // so it can never rejoin its neighbors (Jeremy Silman, The
            // Amateur's Mind, p. 319 test 4: the d2-pawn under a d-file
            // grip on d4).
            if let Some(stop) = match color {
                Color::White => p.try_offset(0, 1),
                Color::Black => p.try_offset(0, -1),
            } {
                let pawn_controls = |sq: Square| !(get_pawn_attacks(sq, color) & enemy).is_empty();
                let half_open_for_enemy = (enemy & file.bitboard()).is_empty();
                let two_ahead = match color {
                    Color::White => p.try_offset(0, 2),
                    Color::Black => p.try_offset(0, -2),
                };
                // On a file the enemy has half-open, piece control of the
                // stop square holds the pawn back just as surely as pawn
                // control (Jeremy Silman, The Complete Book of Chess
                // Strategy, p. 236: the backward d6 pawn on the open
                // file).
                let occ = board.occupied();
                let enemy_piece_grip = |sq: Square| {
                    let att = crate::attack::attackers_of(board, sq, !color, occ).len();
                    let def = crate::attack::attackers_of(board, sq, color, occ).len();
                    att >= def && att > 0
                };
                let held_back = pawn_controls(stop)
                    || (half_open_for_enemy
                        && !occ.has(stop)
                        && (two_ahead.is_some_and(pawn_controls) || enemy_piece_grip(stop)));
                let support = own & adj & behind_or_beside(color, p.rank());
                if held_back && support.is_empty() && !(own & adj).is_empty() {
                    backward[ci] |= p.bitboard();
                }
            }
        }
    }

    // Score: passed pawns are assets; isolated/doubled/backward liabilities.
    // Doubled pawns are charged per EXTRA member, not per member: a
    // doubled pair is one defect, and useful doubled pawns should not be
    // double-billed (Jeremy Silman, The Complete Book of Chess Strategy,
    // p. 239).
    for (ci, sign) in [(0usize, 1i32), (1, -1)] {
        let color = if ci == 0 { Color::White } else { Color::Black };
        let own = board.colored_pieces(color, Piece::Pawn);
        let mut doubled_extras = 0i32;
        for fi in 0..8 {
            let k = (own & File::index(fi).bitboard()).len() as i32;
            doubled_extras += (k - 1).max(0);
        }
        score += sign
            * (passed[ci].len() as i32 * 30
                - iso[ci].len() as i32 * 15
                - doubled_extras * 10
                - backward[ci].len() as i32 * 15);
        // Protected passers are worth more.
        for p in passed[ci] {
            let color = if ci == 0 { Color::White } else { Color::Black };
            let protectors = get_pawn_attacks(p, !color) & board.colored_pieces(color, Piece::Pawn);
            if !protectors.is_empty() {
                score += sign * 15;
            }
        }
    }

    for (name, arr) in [
        ("isolated", &iso),
        ("doubled", &doubled),
        ("backward", &backward),
        ("passed", &passed),
    ] {
        if !arr[0].is_empty() {
            evidence.insert(format!("{name}_white"), sq_list(arr[0]));
        }
        if !arr[1].is_empty() {
            evidence.insert(format!("{name}_black"), sq_list(arr[1]));
        }
    }

    let wp = board.colored_pieces(Color::White, Piece::Pawn);
    let bp = board.colored_pieces(Color::Black, Piece::Pawn);

    // Majorities per wing (files a-c vs f-h) and in the center (d-e).
    let qside = File::A.bitboard() | File::B.bitboard() | File::C.bitboard();
    let kside = File::F.bitboard() | File::G.bitboard() | File::H.bitboard();
    let center = File::D.bitboard() | File::E.bitboard();
    let wq = (wp & qside).len() as i32;
    let bq = (bp & qside).len() as i32;
    let wk = (wp & kside).len() as i32;
    let bk = (bp & kside).len() as i32;
    let wc = (wp & center).len() as i32;
    let bc = (bp & center).len() as i32;
    let queens_on = !board.pieces(Piece::Queen).is_empty();
    // A queenside majority is a plan; but in a middlegame the CENTRAL
    // majority outranks it (Jeremy Silman, The Complete Book of Chess
    // Strategy, p. 269), so the wing hint is withheld while queens are on
    // and the opponent owns the center majority.
    if wq > bq {
        evidence.insert("queenside_majority".into(), json!("white"));
        if !(queens_on && bc > wc) {
            sided.push((
                Color::White,
                PlanHint {
                    hint: "AdvanceQueensideMajority".into(),
                    squares: vec![],
                },
            ));
        }
    } else if bq > wq {
        evidence.insert("queenside_majority".into(), json!("black"));
        if !(queens_on && wc > bc) {
            sided.push((
                Color::Black,
                PlanHint {
                    hint: "AdvanceQueensideMajority".into(),
                    squares: vec![],
                },
            ));
        }
    }
    if bk > wk {
        evidence.insert("kingside_majority".into(), json!("black"));
    } else if wk > bk {
        evidence.insert("kingside_majority".into(), json!("white"));
    }
    // Central majority: roll it forward into a passer.
    for (color, mine, theirs, own_bb) in [(Color::White, wc, bc, wp), (Color::Black, bc, wc, bp)] {
        if mine <= theirs {
            continue;
        }
        evidence.insert(
            "central_majority".into(),
            json!(if color == Color::White {
                "white"
            } else {
                "black"
            }),
        );
        // Most advanced central pawn with an empty advance square.
        let mut best: Option<(i8, Square)> = None;
        for p in own_bb & center {
            let adv = match color {
                Color::White => p.rank() as i8,
                Color::Black => 7 - p.rank() as i8,
            };
            if let Some(front) = p.try_offset(0, if color == Color::White { 1 } else { -1 }) {
                if !board.occupied().has(front) && best.map(|(a, _)| adv > a).unwrap_or(true) {
                    best = Some((adv, front));
                }
            }
        }
        if let Some((_, front)) = best {
            sided.push((
                color,
                PlanHint {
                    hint: "AdvanceCentralMajority".into(),
                    squares: vec![square_name(front)],
                },
            ));
        }
    }

    // Minority attack (Jeremy Silman, The Complete Book of Chess
    // Strategy, pp. 202-203): two pawns storm three in a Carlsbad-style
    // structure — minority side has no c-pawn, the opponent does, and the
    // d-file is locked. The b-pawn lever targets the enemy c-pawn.
    for (color, opp) in [(Color::White, Color::Black), (Color::Black, Color::White)] {
        let own = board.colored_pieces(color, Piece::Pawn);
        let their = board.colored_pieces(opp, Piece::Pawn);
        let d_locked = {
            let wd = wp & File::D.bitboard();
            let bd = bp & File::D.bitboard();
            wd.into_iter()
                .any(|p| p.try_offset(0, 1).is_some_and(|f| bd.has(f)))
        };
        if (own & qside).len() == 2
            && (their & qside).len() == 3
            && (own & File::C.bitboard()).is_empty()
            && !(their & File::C.bitboard()).is_empty()
            && !(own & File::B.bitboard()).is_empty()
            && d_locked
        {
            if let Some(target) = (their & File::C.bitboard()).into_iter().next() {
                let lever = target.try_offset(-1, if color == Color::White { -1 } else { 1 });
                let mut squares: Vec<String> = Vec::new();
                if let Some(l) = lever {
                    squares.push(square_name(l));
                }
                squares.push(square_name(target));
                sided.push((
                    color,
                    PlanHint {
                        hint: "MinorityAttack".into(),
                        squares,
                    },
                ));
            }
        }
    }

    // Wing pawn storm, gated on a truly closed center (Jeremy Silman, The
    // Amateur's Mind, pp. 322-323, tests 14/15: identical storms judged
    // solely by whether the center can still be levered open). A side may
    // storm when it owns a blocked central pawn that is either advanced
    // (across the frontier) or lever-proof, and the center as a whole is
    // closed (two locked pairs, or that anchor itself lever-proof).
    let central_files =
        File::C.bitboard() | File::D.bitboard() | File::E.bitboard() | File::F.bitboard();
    let locked_pairs = (wp & central_files)
        .into_iter()
        .filter(|p| p.try_offset(0, 1).is_some_and(|f| bp.has(f)))
        .count();
    for color in [Color::White, Color::Black] {
        let own = board.colored_pieces(color, Piece::Pawn);
        let their = board.colored_pieces(!color, Piece::Pawn);
        let fwd: i8 = if color == Color::White { 1 } else { -1 };
        let mut anchor: Option<Square> = None;
        for p in own & central_files {
            let blocked = p.try_offset(0, fwd).is_some_and(|f| their.has(f));
            if !blocked {
                continue;
            }
            let advanced = match color {
                Color::White => p.rank() >= Rank::Fifth,
                Color::Black => p.rank() <= Rank::Fourth,
            };
            let lever_proof = !pawn_lever_possible(board, color, p);
            // Lever-proof anchors qualify outright; advanced ones need
            // the center closed elsewhere too.
            if lever_proof || (advanced && locked_pairs >= 2) {
                let adv = match color {
                    Color::White => p.rank() as i8,
                    Color::Black => 7 - p.rank() as i8,
                };
                let cur = anchor.map(|a| match color {
                    Color::White => a.rank() as i8,
                    Color::Black => 7 - a.rank() as i8,
                });
                if cur.map(|c| adv > c).unwrap_or(true) {
                    anchor = Some(p);
                }
            }
        }
        if let Some(a) = anchor {
            let squares = storm_break_square(board, color, a)
                .map(|s| vec![square_name(s)])
                .unwrap_or_default();
            sided.push((
                color,
                PlanHint {
                    hint: "WingPawnStormClosedCenter".into(),
                    squares,
                },
            ));
        }
    }

    // Pressure the front member of an enemy doubled-pawn complex when its
    // own pawns can never defend it (a useful, defensible doubled pawn —
    // Jeremy Silman, The Complete Book of Chess Strategy, p. 239 — earns
    // no such plan).
    for (ci, color) in [(0usize, Color::White), (1, Color::Black)] {
        let own = board.colored_pieces(color, Piece::Pawn);
        for p in doubled[ci] {
            // Front member only: no own pawn ahead on the same file.
            if !(file_front(color, p) & own).is_empty() {
                continue;
            }
            // Only a pawn STRICTLY behind on an adjacent file can ever
            // defend it (a same-rank neighbor is already past).
            let strictly_behind = behind_or_beside(color, p.rank()) & !p.rank().bitboard();
            let indefensible = (own & adjacent_files(p.file()) & strictly_behind).is_empty();
            if indefensible {
                sided.push((
                    !color,
                    PlanHint {
                        hint: "PressureDoubledPawn".into(),
                        squares: vec![square_name(p)],
                    },
                ));
            }
        }
    }

    // Backward pawns invite pressure down their file onto the stop
    // square — unless the owner out-controls the stop square, in which
    // case the pressure has nowhere to go (Jeremy Silman, The Complete
    // Book of Chess Strategy, p. 237: the well-defended backward pawn).
    for (ci, color) in [(0, Color::White), (1, Color::Black)] {
        for p in backward[ci] {
            if let Some(stop) = match color {
                Color::White => p.try_offset(0, 1),
                Color::Black => p.try_offset(0, -1),
            } {
                let occ = board.occupied();
                let owner_grip = crate::attack::attackers_of(board, stop, color, occ).len();
                let attacker_grip = crate::attack::attackers_of(board, stop, !color, occ).len();
                if attacker_grip >= owner_grip {
                    sided.push((
                        !color,
                        PlanHint {
                            hint: "PressureBackwardPawn".into(),
                            squares: vec![square_name(p), square_name(stop)],
                        },
                    ));
                }
            }
        }
    }

    // Passer plans. Blockades are urgent only once the passer has crossed
    // the frontier (blockading a pawn still at home is premature — cf.
    // Jeremy Silman, The Complete Book of Chess Strategy, p. 298, where
    // the far-advanced passer outweighs three unadvanced ones). The
    // owner's rook belongs BEHIND the passer; a piece already sitting on
    // the stop square upgrades the defense to blockade-then-pressure.
    for (ci, color) in [(0, Color::White), (1, Color::Black)] {
        let advanced = |p: Square| match color {
            Color::White => p.rank() >= Rank::Fifth,
            Color::Black => p.rank() <= Rank::Fourth,
        };
        for p in passed[ci] {
            if let Some(stop) = match color {
                Color::White => p.try_offset(0, 1),
                Color::Black => p.try_offset(0, -1),
            } {
                if advanced(p) {
                    plans.push(PlanHint {
                        hint: if ci == 0 {
                            "BlockadeWhitePasser"
                        } else {
                            "BlockadeBlackPasser"
                        }
                        .into(),
                        squares: vec![square_name(stop)],
                    });
                }
                // Tarrasch: the rook belongs behind the passer — worth
                // saying one rank earlier than the blockade is urgent.
                let rook_worthy = match color {
                    Color::White => p.rank() >= Rank::Fourth,
                    Color::Black => p.rank() <= Rank::Fifth,
                };
                if rook_worthy && !board.colored_pieces(color, Piece::Rook).is_empty() {
                    if let Some(behind) =
                        p.try_offset(0, if color == Color::White { -1 } else { 1 })
                    {
                        sided.push((
                            color,
                            PlanHint {
                                hint: "RookBehindPasser".into(),
                                squares: vec![square_name(p), square_name(behind)],
                            },
                        ));
                    }
                }
            }
        }
        // Blockade-then-pressure against enemy passed AND isolated pawns
        // already halted by a piece on the stop square.
        for p in passed[ci] | iso[ci] {
            if let Some(stop) = match color {
                Color::White => p.try_offset(0, 1),
                Color::Black => p.try_offset(0, -1),
            } {
                if (board.colors(!color) & stop.bitboard()).is_empty() {
                    continue;
                }
                let already = plans.iter().any(|h| {
                    h.hint == "BlockadeThenPressure" && h.squares.contains(&square_name(p))
                });
                if !already {
                    plans.push(PlanHint {
                        hint: "BlockadeThenPressure".into(),
                        squares: vec![square_name(p), square_name(stop)],
                    });
                }
            }
        }
    }

    // Endgame king activity: with the heavy wood off, a king within reach
    // of the central battleground should march (Jeremy Silman, How to
    // Reassess Your Chess, the endgame-planning examples, and the same
    // author's Complete Endgame Course throughout).
    if phase(board) == crate::record::Phase::Endgame {
        for color in [Color::White, Color::Black] {
            let k = board.king(color);
            let target = nearest_center_square(k);
            // Any king not already on the central battleground has the
            // standing endgame plan of marching there.
            if chebyshev(k, target) >= 1 {
                sided.push((
                    color,
                    PlanHint {
                        hint: "ActivateKingInEndgame".into(),
                        squares: vec![square_name(target)],
                    },
                ));
            }
        }
    }

    if locked_pairs > 0 {
        let files: Vec<String> = (wp & central_files)
            .into_iter()
            .filter(|p| p.try_offset(0, 1).is_some_and(|f| bp.has(f)))
            .map(|p| ((b'a' + p.file() as u8) as char).to_string())
            .collect();
        evidence.insert("locked_files".into(), json!(files));
    }

    // A majority that can actually MAKE a passer. The existing
    // Advance*Majority hints say to push it; this names the point of
    // pushing it, which is what the corpus asks for
    // ("create-passed-pawn", HTRYC exs. 27, 126 and 183).
    // Not in the opening: a majority is an ASSET from move one, but
    // "go and make a passer" only becomes a plan once the pieces are out
    // and it can actually be executed. Announcing it on move seven of a
    // Sveshnikov is true and useless.
    for (group, files) in (phase(board) != crate::record::Phase::Opening)
        .then_some([
            ("queenside", [File::A, File::B, File::C].as_slice()),
            ("center", [File::D, File::E].as_slice()),
            ("kingside", [File::F, File::G, File::H].as_slice()),
        ])
        .into_iter()
        .flatten()
    {
        let mask = files
            .iter()
            .fold(BitBoard::EMPTY, |acc, f| acc | f.bitboard());
        for color in [Color::White, Color::Black] {
            let ours = board.colored_pieces(color, Piece::Pawn) & mask;
            let theirs = board.colored_pieces(!color, Piece::Pawn) & mask;
            if ours.len() <= theirs.len() {
                continue;
            }
            // A CRIPPLED majority makes no passer: 4-v-3 with a doubled
            // pawn is three healthy pawns against three. Count files, not
            // just pawns.
            let files_of = |bb: BitBoard| {
                bb.into_iter()
                    .map(|p| p.file() as u8)
                    .collect::<std::collections::BTreeSet<_>>()
                    .len()
            };
            if files_of(ours) < files_of(theirs) {
                continue;
            }
            // Somebody in the group has to be able to move.
            let fwd: i8 = if color == Color::White { 1 } else { -1 };
            let Some(candidate) = ours.into_iter().find(|p| {
                p.try_offset(0, fwd)
                    .is_some_and(|a| !board.occupied().has(a))
            }) else {
                continue;
            };
            evidence.insert(
                format!(
                    "healthy_majority_{}_{}",
                    group,
                    if color == Color::White {
                        "white"
                    } else {
                        "black"
                    }
                ),
                json!(ours.len()),
            );
            sided.push((
                color,
                PlanHint {
                    hint: "CreatePassedPawn".into(),
                    squares: vec![square_name(candidate)],
                },
            ));
        }
    }

    if evidence.is_empty() && plans.is_empty() && sided.is_empty() {
        return None;
    }
    let (f, m) = favors_or_balanced(score, 15, 45);
    // Keep a side-owned plan only when the imbalance is level or leans
    // toward that side: hints are attributed to the imbalance's favored
    // side downstream, so a disfavored side's plan would be narrated for
    // the wrong player.
    plans.extend(sided.into_iter().filter_map(|(side, p)| {
        let side_favors = match side {
            Color::White => Favors::White,
            Color::Black => Favors::Black,
        };
        (f == Favors::Balanced || f == side_favors).then_some(p)
    }));
    Some(Imbalance {
        kind: ImbalanceKind::PawnStructure,
        favors: f,
        magnitude: m,
        evidence,
        plans,
    })
}

/// Where a justified wing storm should strike: take the enemy pawn on the
/// storm wing nearest the anchor that a friendly pawn can still attack
/// via an empty lever square; among its lever squares prefer the wing-most
/// one (the c5 break against d6 in a King's Indian; g4 against f5 in the
/// Torre storm of Jeremy Silman, The Amateur's Mind, p. 323 test 15).
fn storm_break_square(board: &Board, color: Color, anchor: Square) -> Option<Square> {
    let own = board.colored_pieces(color, Piece::Pawn);
    let back: i8 = if color == Color::White { -1 } else { 1 };
    let support_left = anchor.try_offset(-1, back).is_some_and(|s| own.has(s));
    let support_right = anchor.try_offset(1, back).is_some_and(|s| own.has(s));
    let dir: i8 = if support_left && !support_right {
        1
    } else if support_right && !support_left {
        -1
    } else if board.king(!color).file() as i8 >= anchor.file() as i8 {
        1
    } else {
        -1
    };
    let mut candidates: Vec<(i8, Square)> = Vec::new(); // (file distance from anchor, pawn)
    for ep in board.colored_pieces(!color, Piece::Pawn) {
        let df = ep.file() as i8 - anchor.file() as i8;
        if (dir > 0 && df < 0) || (dir < 0 && df > 0) {
            continue;
        }
        candidates.push((df.abs(), ep));
    }
    candidates.sort_by_key(|(d, _)| *d);
    for (_, ep) in candidates {
        let mut best: Option<Square> = None;
        for lf in [-1i8, 1] {
            let Some(lever) = ep.try_offset(lf, back) else {
                continue;
            };
            if board.occupied().has(lever) {
                continue;
            }
            let reachable = (own & lever.file().bitboard())
                .into_iter()
                .any(|p| match color {
                    Color::White => p.rank() < lever.rank(),
                    Color::Black => p.rank() > lever.rank(),
                });
            if !reachable {
                continue;
            }
            let more_wingward = |a: Square, b: Square| {
                let center = 3.5f32;
                (a.file() as i8 as f32 - center).abs() > (b.file() as i8 as f32 - center).abs()
            };
            if best.map(|b| more_wingward(lever, b)).unwrap_or(true) {
                best = Some(lever);
            }
        }
        if best.is_some() {
            return best;
        }
    }
    None
}

fn adjacent_files(f: File) -> BitBoard {
    let mut bb = BitBoard::EMPTY;
    let i = f as i8;
    for d in [-1i8, 1] {
        let j = i + d;
        if (0..8).contains(&j) {
            bb |= File::index(j as usize).bitboard();
        }
    }
    bb
}

fn file_front(color: Color, p: Square) -> BitBoard {
    let mut span = BitBoard::EMPTY;
    let mut s = p;
    loop {
        let next = match color {
            Color::White => s.try_offset(0, 1),
            Color::Black => s.try_offset(0, -1),
        };
        match next {
            Some(n) => {
                span |= n.bitboard();
                s = n;
            }
            None => break,
        }
    }
    span
}

fn behind_or_beside(color: Color, rank: Rank) -> BitBoard {
    let mut bb = BitBoard::EMPTY;
    let r = rank as i8;
    for rr in 0..8i8 {
        let behind = match color {
            Color::White => rr <= r,
            Color::Black => rr >= r,
        };
        if behind {
            bb |= Rank::index(rr as usize).bitboard();
        }
    }
    bb
}

/// 3. Material.
pub fn material(board: &Board) -> Option<Imbalance> {
    let mut evidence = BTreeMap::new();
    let count = |c: Color, p: Piece| board.colored_pieces(c, p).len() as i32;
    let total = |c: Color| {
        count(c, Piece::Pawn) * 100
            + count(c, Piece::Knight) * 320
            + count(c, Piece::Bishop) * 330
            + count(c, Piece::Rook) * 500
            + count(c, Piece::Queen) * 900
    };
    let diff = total(Color::White) - total(Color::Black);
    evidence.insert("material_diff_cp".into(), json!(diff));
    // Per-piece surplus (white minus black) so the verbalizer can NAME
    // the imbalance ("a knight for two pawns") instead of counting pawns.
    evidence.insert(
        "piece_diff".into(),
        json!({
            "p": count(Color::White, Piece::Pawn) - count(Color::Black, Piece::Pawn),
            "n": count(Color::White, Piece::Knight) - count(Color::Black, Piece::Knight),
            "b": count(Color::White, Piece::Bishop) - count(Color::Black, Piece::Bishop),
            "r": count(Color::White, Piece::Rook) - count(Color::Black, Piece::Rook),
            "q": count(Color::White, Piece::Queen) - count(Color::Black, Piece::Queen),
        }),
    );

    // Named patterns.
    let w_minor = count(Color::White, Piece::Knight) + count(Color::White, Piece::Bishop);
    let b_minor = count(Color::Black, Piece::Knight) + count(Color::Black, Piece::Bishop);
    let w_r = count(Color::White, Piece::Rook);
    let b_r = count(Color::Black, Piece::Rook);
    if w_r > b_r && b_minor > w_minor {
        evidence.insert("pattern".into(), json!("white-exchange-up"));
    } else if b_r > w_r && w_minor > b_minor {
        evidence.insert("pattern".into(), json!("black-exchange-up"));
    }
    // A level ledger can still hide a NAMED material imbalance (rook vs
    // pieces, bishop vs knight, queen vs army): report the asymmetric mix
    // even when the point count is close.
    let mix_differs = [
        Piece::Pawn,
        Piece::Knight,
        Piece::Bishop,
        Piece::Rook,
        Piece::Queen,
    ]
    .iter()
    .any(|p| count(Color::White, *p) != count(Color::Black, *p));
    let (f, m) = if mix_differs {
        favors_or_balanced(diff, 80, 250)
    } else {
        favors(diff, 80, 250)?
    };
    Some(Imbalance {
        kind: ImbalanceKind::Material,
        favors: f,
        magnitude: m,
        evidence,
        plans: vec![],
    })
}

/// 4. Files & diagonals.
pub fn files_diagonals(board: &Board) -> Option<Imbalance> {
    let mut evidence = BTreeMap::new();
    let mut plans = Vec::new();
    let mut score = 0i32;
    let wp = board.colored_pieces(Color::White, Piece::Pawn);
    let bp = board.colored_pieces(Color::Black, Piece::Pawn);
    let mut open = Vec::new();
    let mut half_w = Vec::new();
    let mut half_b = Vec::new();

    for fi in 0..8 {
        let file = File::index(fi);
        let fb = file.bitboard();
        let w_on = !(wp & fb).is_empty();
        let b_on = !(bp & fb).is_empty();
        let fname = ((b'a' + fi as u8) as char).to_string();
        let w_majors =
            board.colors(Color::White) & (board.pieces(Piece::Rook) | board.pieces(Piece::Queen));
        let b_majors =
            board.colors(Color::Black) & (board.pieces(Piece::Rook) | board.pieces(Piece::Queen));
        match (w_on, b_on) {
            (false, false) => {
                open.push(fname.clone());
                let w_ctl = (w_majors & fb).len() as i32;
                let b_ctl = (b_majors & fb).len() as i32;
                score += (w_ctl - b_ctl) * 20;
                if w_ctl >= 2 {
                    evidence.insert(format!("doubled_majors_{fname}"), json!("white"));
                    score += 15;
                }
                if b_ctl >= 2 {
                    evidence.insert(format!("doubled_majors_{fname}"), json!("black"));
                    score -= 15;
                }
            }
            (false, true) => {
                half_w.push(fname.clone());
                score += ((w_majors & fb).len() as i32) * 8;
            }
            (true, false) => {
                half_b.push(fname.clone());
                score -= ((b_majors & fb).len() as i32) * 8;
            }
            _ => {}
        }
    }
    if !open.is_empty() {
        evidence.insert("open_files".into(), json!(open));
    }
    if !half_w.is_empty() {
        evidence.insert("half_open_files_white".into(), json!(half_w));
    }
    if !half_b.is_empty() {
        evidence.insert("half_open_files_black".into(), json!(half_b));
    }

    // 7th-rank rooks.
    let w7 = board.colored_pieces(Color::White, Piece::Rook) & Rank::Seventh.bitboard();
    let b2 = board.colored_pieces(Color::Black, Piece::Rook) & Rank::Second.bitboard();
    // Per rook: DOUBLED rooks on the seventh are a game-winning force
    // (Jeremy Silman, The Complete Book of Chess Strategy, p. 329).
    if !w7.is_empty() {
        evidence.insert("rook_on_seventh".into(), json!("white"));
        score += 25 * w7.len().min(2) as i32;
    }
    if !b2.is_empty() {
        evidence.insert("rook_on_seventh".into(), json!("black"));
        score -= 25 * b2.len().min(2) as i32;
    }

    // Rook to the seventh: a rook already there presses on; a rook on an
    // open file heads for the 7th-rank entry square — but only if that
    // square is actually enterable: an entry covered by an enemy pawn or
    // minor piece is no entry at all (Jeremy Silman, The Complete Book of
    // Chess Strategy, p. 225, the file with no penetration points).
    for (color, on7th) in [(Color::White, w7), (Color::Black, b2)] {
        for r in on7th {
            plans.push(PlanHint {
                hint: "RookToSeventh".into(),
                squares: vec![square_name(r)],
            });
        }
        if !on7th.is_empty() {
            continue;
        }
        let entry_rank = match color {
            Color::White => Rank::Seventh,
            Color::Black => Rank::Second,
        };
        let rooks = board.colored_pieces(color, Piece::Rook);
        let occ = board.occupied();
        for fname in &open {
            let file = File::index((fname.as_bytes()[0] - b'a') as usize);
            let entry = Square::new(file, entry_rank);
            let enemy_cover = crate::attack::attackers_of(board, entry, !color, occ);
            let cheap_cover = enemy_cover
                & (board.pieces(Piece::Pawn)
                    | board.pieces(Piece::Knight)
                    | board.pieces(Piece::Bishop));
            let own_cover = crate::attack::attackers_of(board, entry, color, occ);
            // An entry a pawn or minor covers is no entry at all (p. 225).
            if !cheap_cover.is_empty() || own_cover.len() < enemy_cover.len() {
                continue;
            }
            if !(rooks & file.bitboard()).is_empty() {
                plans.push(PlanHint {
                    hint: "RookToSeventh".into(),
                    squares: vec![square_name(entry)],
                });
                continue;
            }
            // No rook on the file yet: route one there. The file itself is
            // the destination — a rook lift (Re1-e3-g3) and a plain swing
            // are the same search (run 12).
            let file_targets = file.bitboard() & !board.occupied();
            let Some((rook, r)) = rooks
                .into_iter()
                .filter_map(|rk| {
                    crate::route::route_to(board, color, Piece::Rook, rk, file_targets, &|_| true)
                        .map(|r| (rk, r))
                })
                .min_by_key(|(_, r)| r.moves())
            else {
                continue;
            };
            plans.push(PlanHint {
                hint: "ManeuverRookToOpenFile".into(),
                squares: std::iter::once(rook)
                    .chain(r.via.iter().copied())
                    .chain([r.to])
                    .map(square_name)
                    .collect(),
            });
        }
    }

    // Open lines toward a weak king: only when the enemy king's shelter
    // is ALREADY thin and a usable (half-)open file sits at or beside the
    // king file — the static, no-search membrane of the direct-attack
    // family.
    for (color, halves) in [(Color::White, &half_w), (Color::Black, &half_b)] {
        let enemy = !color;
        if !shelter_is_thin(board, enemy) {
            continue;
        }
        let kf = board.king(enemy).file() as i8;
        let entry_rank = match color {
            Color::White => Rank::Seventh,
            Color::Black => Rank::Second,
        };
        let near_king = |fname: &String| {
            let f = (fname.as_bytes()[0] - b'a') as i8;
            (f - kf).abs() <= 1
        };
        if let Some(fname) = open.iter().chain(halves.iter()).find(|f| near_king(f)) {
            let file = File::index((fname.as_bytes()[0] - b'a') as usize);
            plans.push(PlanHint {
                hint: "OpenLinesTowardWeakKing".into(),
                squares: vec![square_name(Square::new(file, entry_rank))],
            });
            score += if color == Color::White { 15 } else { -15 };
        }
    }

    if evidence.is_empty() {
        return None;
    }
    let (f, m) = favors_or_balanced(score, 15, 40);
    if !open.is_empty() {
        plans.push(PlanHint {
            hint: "DoubleOnOpenFile".into(),
            squares: vec![],
        });
    }
    Some(Imbalance {
        kind: ImbalanceKind::FilesDiagonals,
        favors: f,
        magnitude: m,
        evidence,
        plans,
    })
}

/// A castled-ish king whose pawn shield has largely gone: at most one own
/// pawn remains on the three files around the king within two ranks in
/// front of it. Kings still in the center are the development story, not
/// this one.
fn shelter_is_thin(board: &Board, side: Color) -> bool {
    let k = board.king(side);
    let home_dist = match side {
        Color::White => k.rank() as u8,
        Color::Black => 7 - k.rank() as u8,
    };
    if home_dist > 1 {
        return false;
    }
    let own = board.colored_pieces(side, Piece::Pawn);
    let fwd: i8 = if side == Color::White { 1 } else { -1 };
    let mut shield = 0;
    for df in -1i8..=1 {
        for dr in 0i8..=2 {
            if df == 0 && dr == 0 {
                continue;
            }
            if let Some(s) = k.try_offset(df, dr * fwd) {
                if own.has(s) {
                    shield += 1;
                }
            }
        }
    }
    shield <= 1
}

/// Nimzowitsch's undermining: an enemy PAWN whose job is to hold a square
/// we want, which one of our own pawns can advance to attack.
///
/// The corpus asks for this twice under two names — "undermine-defender"
/// (HTRYC ex. 140: White's whole setup exists to own d5, so he thrusts
/// f4-f5 at the e6 pawn that covers it) and
/// "undermine-knight-support-points" (ex. 80: dissolve the pawns those
/// centralised knights are standing on). Both are the same rule, so both
/// are the same detector: find the pawn doing the defending, then find
/// the lever that gets at it.
///
/// The squares we want are the holes in the enemy camp plus the squares
/// their minor pieces are actually sitting on.
fn undermine_targets(board: &Board, color: Color) -> Vec<(Square, Square)> {
    let enemy = !color;
    let enemy_pawns = board.colored_pieces(enemy, Piece::Pawn);
    // How fast can OUR pawns come to attack a given square?
    let ours = crate::pawn_contact::evict_distance(board, color);
    // The window where an outpost actually bites, occupancy aside.
    let window = hole_mask_window(enemy);
    let enemy_minors =
        board.colored_pieces(enemy, Piece::Knight) | board.colored_pieces(enemy, Piece::Bishop);

    // Per-pawn forward attack spans, so we can ask what the enemy would
    // still cover if a given pawn were gone.
    let spans: Vec<(Square, BitBoard)> = enemy_pawns
        .into_iter()
        .map(|p| (p, single_pawn_attack_span(p, enemy)))
        .collect();

    // Only CENTRAL squares are worth a pawn lever on their own account.
    // Freeing b6 or g5 is technically true and strategically noise; a
    // square an enemy PIECE is actually standing on is judged on the
    // piece, not the file.
    let central = File::C.bitboard() | File::D.bitboard() | File::E.bitboard() | File::F.bitboard();

    let mut out: Vec<(Square, Square, u8)> = Vec::new();
    for (pawn, _) in &spans {
        let pawn = *pawn;
        // A defender we already hit needs no plan; one we cannot reach in
        // two pushes is a wish, not a lever.
        if crate::pawn_contact::contested_within(&ours, pawn, 0)
            || !crate::pawn_contact::contested_within(&ours, pawn, 2)
        {
            continue;
        }
        let without: BitBoard = spans
            .iter()
            .filter(|(q, _)| *q != pawn)
            .fold(BitBoard::EMPTY, |acc, (_, sp)| acc | *sp);

        // What does removing this pawn actually buy? Squares whose
        // PERMANENT cover depends on it alone. This is the real test: a
        // square that is already a hole needs no undermining, and one
        // another pawn also covers is not freed by this lever.
        // (HTRYC ex. 140: d5 is not a hole precisely BECAUSE e6 guards
        // it — which is the whole reason to play f4-f5.)
        let freed =
            window & central & !without & single_pawn_attack_span(pawn, enemy) & !board.occupied();
        // Plus the squares enemy minor pieces are standing on thanks to
        // this pawn (ex. 80: dissolve the pawns the knights rest on).
        let propped = enemy_minors & get_pawn_attacks(pawn, enemy);

        if let Some(square) = (freed | propped).into_iter().next() {
            out.push((square, pawn, ours[pawn as usize]));
        }
    }
    // Cheapest levers first, and at most two per side: a list of every
    // pawn we could theoretically poke at is not a plan.
    out.sort_by_key(|(_, _, cost)| *cost);
    out.truncate(2);
    out.into_iter().map(|(sq, pawn, _)| (sq, pawn)).collect()
}

/// The rank/file window where an outpost in `camp_owner`'s camp bites,
/// ignoring pawn cover and occupancy — the geometry half of [`hole_mask`].
fn hole_mask_window(camp_owner: Color) -> BitBoard {
    let half = match camp_owner {
        Color::White => Rank::Third.bitboard() | Rank::Fourth.bitboard(),
        Color::Black => Rank::Sixth.bitboard() | Rank::Fifth.bitboard(),
    };
    half & !(File::A.bitboard() | File::H.bitboard())
}

/// Squares one pawn attacks now or after any number of advances.
fn single_pawn_attack_span(pawn: Square, color: Color) -> BitBoard {
    let mut span = BitBoard::EMPTY;
    let mut sq = pawn;
    loop {
        span |= get_pawn_attacks(sq, color);
        let next = match color {
            Color::White => sq.try_offset(0, 1),
            Color::Black => sq.try_offset(0, -1),
        };
        match next {
            Some(n) => sq = n,
            None => break,
        }
    }
    span
}

/// Nimzowitsch's overprotection: our own fixed central spearhead, under
/// fire and worth piling extra defenders behind.
///
/// The point is prophylactic SURPLUS, so this deliberately does not wait
/// for attackers to outnumber defenders — by then it is defence, not
/// overprotection. Gated hard instead: the pawn must be an advanced
/// central one that is FIXED (an enemy pawn blocks its advance, so it is
/// a permanent strong point rather than a passing occupant) and somebody
/// must actually be shooting at it. (The Amateur's Mind test at p. 321:
/// a quiet rook move overprotecting the e5 spearhead.)
fn overprotect_squares(board: &Board, color: Color) -> Vec<Square> {
    let enemy = !color;
    let fifth = match color {
        Color::White => Rank::Fifth,
        Color::Black => Rank::Fourth,
    };
    let central = File::C.bitboard() | File::D.bitboard() | File::E.bitboard() | File::F.bitboard();
    let fwd: i8 = if color == Color::White { 1 } else { -1 };
    let mut out = Vec::new();
    for p in board.colored_pieces(color, Piece::Pawn) & fifth.bitboard() & central {
        let Some(ahead) = p.try_offset(0, fwd) else {
            continue;
        };
        if !board.colored_pieces(enemy, Piece::Pawn).has(ahead) {
            continue; // not fixed: it can still advance, so it is not a POINT
        }
        let attackers = crate::attack::attackers_of(board, p, enemy, board.occupied());
        if attackers.is_empty() {
            continue; // nothing to protect it from yet
        }
        out.push(p);
    }
    out
}

/// 5. Squares & outposts (with the spec's BFS knight-route plan hint).
pub fn squares_outposts(board: &Board) -> Option<Imbalance> {
    let mut evidence = BTreeMap::new();
    let mut plans = Vec::new();
    let mut score = 0i32;

    for (color, sign) in [(Color::White, 1i32), (Color::Black, -1)] {
        let enemy = !color;
        let holes = holes_in_camp(board, enemy);
        // Occupancy-free mask: a piece STANDING on an outpost square must
        // still be recognized as established (holes lists destinations
        // only, so testing the piece's own square against it always
        // failed — run 8.5 bug fix).
        let mask = hole_mask(board, enemy);
        let occupied_outposts = mask
            & (board.colored_pieces(color, Piece::Knight)
                | board.colored_pieces(color, Piece::Bishop));

        // Restraint plans stand on their own: they are about pawns
        // holding (or failing to hold) squares, so they must be reachable
        // even in a position with no hole and no outpost of ours.
        for (wanted, defender) in undermine_targets(board, color) {
            // No score contribution: having a lever to play is a TO-DO,
            // not an advantage. Run 11 learned this for the development
            // prior — a plan the side still has to execute poisons the
            // who-is-better vote — and the same holds here.
            plans.push(PlanHint {
                hint: "UndermineDefender".into(),
                squares: vec![square_name(defender), square_name(wanted)],
            });
        }
        for point in overprotect_squares(board, color) {
            plans.push(PlanHint {
                hint: "OverprotectStrongPoint".into(),
                squares: vec![square_name(point)],
            });
            evidence.insert(
                format!(
                    "strong_point_{}",
                    if color == Color::White {
                        "white"
                    } else {
                        "black"
                    }
                ),
                json!(square_name(point)),
            );
        }

        if holes.is_empty() && occupied_outposts.is_empty() {
            continue;
        }
        // Established outposts: a minor piece on a hole, defended by its
        // own pawn — Jeremy Silman's support points (The Complete Book of
        // Chess Strategy, pp. 276-277: knights AND bishops).
        let mut exploitable = false;
        for n in board.colored_pieces(color, Piece::Knight) {
            if mask.has(n) {
                let pawn_backup =
                    get_pawn_attacks(n, enemy) & board.colored_pieces(color, Piece::Pawn);
                if !pawn_backup.is_empty() {
                    evidence.insert(
                        format!(
                            "established_outpost_{}",
                            if color == Color::White {
                                "white"
                            } else {
                                "black"
                            }
                        ),
                        json!(square_name(n)),
                    );
                    score += sign * 30;
                    exploitable = true;
                }
            } else if let Some((target, route)) = knight_route_to(board, color, n, holes) {
                // The origin leads the square list: a reroute that does not
                // name the piece being rerouted is not a plan a human can
                // follow. (Consumers still read the DESTINATION as `.last()`.)
                let moves = route.len() as i32 + 1;
                plans.push(PlanHint {
                    hint: "ManeuverKnightToOutpost".into(),
                    squares: std::iter::once(n)
                        .chain(route)
                        .chain([target])
                        .map(square_name)
                        .collect(),
                });
                // A route is a plan, not yet an edge: half a hole's worth,
                // and worth less the longer it takes — a five-move
                // regrouping is a real idea but a weak claim about NOW.
                score += sign * if moves <= 2 { 4 } else { 6 - moves.min(5) };
                exploitable = true;
            }
        }

        // A bishop's support point is a hole its OWN pawn defends
        // (Jeremy Silman, The Complete Book of Chess Strategy, pp. 276-277 —
        // support points are not a knight's privilege).
        let own_pawns = board.colored_pieces(color, Piece::Pawn);
        let mut support_points = BitBoard::EMPTY;
        for h in holes {
            if !(get_pawn_attacks(h, enemy) & own_pawns).is_empty() {
                support_points |= h.bitboard();
            }
        }
        for b in board.colored_pieces(color, Piece::Bishop) {
            if mask.has(b) {
                let pawn_backup = get_pawn_attacks(b, enemy) & own_pawns;
                if !pawn_backup.is_empty() {
                    evidence.insert(
                        format!(
                            "bishop_outpost_{}",
                            if color == Color::White {
                                "white"
                            } else {
                                "black"
                            }
                        ),
                        json!(square_name(b)),
                    );
                    score += sign * 25;
                    continue;
                }
            }
            // Not established: can it walk to a support point? Routing is
            // not a knight's privilege either (run 12 — the corpus misses
            // were full of bishop regroupings, docs/VALIDATION.md).
            if support_points.is_empty() {
                continue;
            }
            if let Some(r) =
                crate::route::route_to(board, color, Piece::Bishop, b, support_points, &|_| true)
            {
                plans.push(PlanHint {
                    hint: "ManeuverBishopToSupportPoint".into(),
                    squares: std::iter::once(b)
                        .chain(r.via.iter().copied())
                        .chain([r.to])
                        .map(square_name)
                        .collect(),
                });
                let moves = r.moves() as i32;
                score += sign * if moves <= 2 { 4 } else { 6 - moves.min(5) };
                exploitable = true;
            }
        }

        if !holes.is_empty() {
            let key = if color == Color::White {
                "holes_in_black_camp"
            } else {
                "holes_in_white_camp"
            };
            evidence.insert(key.into(), sq_list(holes));
            // A hole is worth real points only when this side has a
            // concrete way in: an outpost held, or a knight OR bishop with
            // a route to one. Holes nobody can reach are latent, near-noise.
            // This scoring must stay BELOW the routing loops that set
            // `exploitable` — it reads their verdict.
            let per_hole = if exploitable { 8 } else { 2 };
            score += sign * holes.len().min(3) as i32 * per_hole;
        }
    }
    if evidence.is_empty() {
        return None;
    }
    let (f, m) = favors_or_balanced(score, 12, 35);
    Some(Imbalance {
        kind: ImbalanceKind::SquaresOutposts,
        favors: f,
        magnitude: m,
        evidence,
        plans,
    })
}

/// How many knight moves a route may take. Three hops covers Nc3-e2-d4
/// but not the classical regroupings the corpus keeps asking for
/// (Nb1-d2-f1-g3-f5 is four); five is the practical ceiling, since beyond
/// that a static route outlives the position it was computed in.
const MAX_ROUTE_HOPS: u8 = 5;

/// Shortest knight path to any hole over squares that are not occupied by
/// own pieces, not out-gunned by enemy pieces, and — the timing test —
/// not reachable by an enemy PAWN in the number of moves the knight needs
/// to arrive. A waypoint no pawn attacks today is worthless if the pawn
/// that evicts the knight arrives at the same time (Jeremy Silman,
/// Complete Book of Chess Strategy p. 219: an outpost you can be kicked
/// off is not an outpost). See [`crate::pawn_contact`].
fn knight_route_to(
    board: &Board,
    color: Color,
    from: Square,
    targets: BitBoard,
) -> Option<(Square, Vec<Square>)> {
    let enemy = !color;
    let enemy_pawn_attacks = {
        let mut a = BitBoard::EMPTY;
        for p in board.colored_pieces(enemy, Piece::Pawn) {
            a |= get_pawn_attacks(p, enemy);
        }
        a
    };
    // A waypoint is unsafe if enemy pieces outgun its defenders (the
    // spec's "safe squares", judged statically per square). The routing
    // knight itself cannot defend the square it stands on.
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
    // Waypoints are judged over time, not against the current attack map.
    // A waypoint is TRANSIT, not a home: being kicked off it costs a tempo,
    // not the plan — the route only dies if the pawn is already there when
    // we arrive. We reach the square on our move `hop`, so a pawn needing
    // `hop` of their moves arrives one tempo late and we simply continue.
    // Hence strictly-less, not less-or-equal. (Destinations are held to the
    // permanent hole test instead — they are squares we mean to STAY on.)
    let enemy_evict = crate::pawn_contact::evict_distance(board, enemy);
    let waypoint_ok = |sq: Square, hop: u8| {
        hop == 0 || !crate::pawn_contact::contested_within(&enemy_evict, sq, hop - 1)
    };
    // The DESTINATION hole may still be piece-contested by one unit — a
    // hole is permanent while piece cover is tradeable (Jeremy Silman,
    // How to Reassess Your Chess, ex. 60: trade away every defender of
    // d5, then settle the knight there). Waypoints stay strict; a hole
    // outgunned by two or more is a fantasy, not a plan.
    let target_ok = |n: Square| {
        if board.colors(color).has(n) || enemy_pawn_attacks.has(n) {
            return false;
        }
        let att = crate::attack::attackers_of(board, n, enemy, occ).len() as i32;
        let def =
            (crate::attack::attackers_of(board, n, color, occ) & !from.bitboard()).len() as i32;
        // Uncontested, or contested by at most one extra unit while we
        // hold at least one defender to trade behind (an attacked square
        // with NO defenders is an invasion fantasy, not a route).
        att == 0 || (def >= 1 && att - def <= 1)
    };
    let mut prev: [Option<Square>; 64] = [None; 64];
    let mut seen = from.bitboard();
    let mut frontier = vec![from];
    for depth in 0..MAX_ROUTE_HOPS {
        let hop = depth + 1;
        let mut next_frontier = Vec::new();
        for &s in &frontier {
            for n in get_knight_moves(s) & !seen {
                if targets.has(n) && target_ok(n) {
                    prev[n as usize] = Some(s);
                    // Reconstruct path (exclusive of from, inclusive of n
                    // handled by caller).
                    let mut path = Vec::new();
                    let mut cur = s;
                    while cur != from {
                        path.push(cur);
                        cur = prev[cur as usize].expect("bfs chain");
                    }
                    path.reverse();
                    return Some((n, path));
                }
                if blocked.has(n) || !waypoint_ok(n, hop) {
                    continue;
                }
                prev[n as usize] = Some(s);
                seen |= n.bitboard();
                next_frontier.push(n);
            }
        }
        frontier = next_frontier;
    }
    None
}

/// 6. Space: squares in the enemy half controlled by own pawns.
pub fn space(board: &Board) -> Option<Imbalance> {
    let enemy_half = |c: Color| match c {
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
    };
    let control = |c: Color| {
        let mut a = BitBoard::EMPTY;
        for p in board.colored_pieces(c, Piece::Pawn) {
            a |= get_pawn_attacks(p, c);
        }
        (a & enemy_half(c)).len() as i32
    };
    let w = control(Color::White);
    let b = control(Color::Black);
    // A space edge over an undeveloped position is noise; require a real
    // presence in the enemy half before reporting. And space is a story
    // about pawn FRONTS — with most pawns gone it degenerates to noise.
    if w.max(b) < 3 || board.pieces(Piece::Pawn).len() < 8 {
        return None;
    }
    let diff = (w - b) * 12;
    let mut evidence = BTreeMap::new();
    evidence.insert("white_space".into(), json!(w));
    evidence.insert("black_space".into(), json!(b));
    // A big mutual territorial presence is worth reporting even level.
    let (f, m) = if w.max(b) >= 5 {
        favors_or_balanced(diff, 24, 60)
    } else {
        favors(diff, 24, 60)?
    };
    // Plans are phrased for the favored side (the verbalizer attributes
    // them that way): keep pieces on, use the extra room.
    let plans = vec![PlanHint {
        hint: "UseSpaceAvoidExchanges".into(),
        squares: vec![],
    }];
    Some(Imbalance {
        kind: ImbalanceKind::Space,
        favors: f,
        magnitude: m,
        evidence,
        plans,
    })
}

/// 7. Development (meaningful only early or with a closed center).
pub fn development(board: &Board) -> Option<Imbalance> {
    if board.fullmove_number() > 15 {
        return None;
    }
    // Development is an opening/middlegame story; once the wood is off,
    // king "undevelopment" is usually king ACTIVITY.
    if phase(board) == crate::record::Phase::Endgame {
        return None;
    }
    let developed = |c: Color| {
        let back = match c {
            Color::White => Rank::First,
            Color::Black => Rank::Eighth,
        };
        let minors = board.colors(c) & (board.pieces(Piece::Knight) | board.pieces(Piece::Bishop));
        let out = (minors & !back.bitboard()).len() as i32;
        // Castled-ish: the king has left the central files. Judged by
        // file alone so a castled king that later stepped up a rank (as
        // in reconstructed middlegame positions) still gets credit.
        let castled = (board.king(c).file() as i8 - File::E as i8).abs() >= 2;
        out + if castled { 2 } else { 0 }
    };
    let w = developed(Color::White);
    let b = developed(Color::Black);
    let diff = (w - b) * 18;
    let mut evidence = BTreeMap::new();
    evidence.insert("white_developed".into(), json!(w));
    evidence.insert("black_developed".into(), json!(b));
    let (f, m) = favors(diff, 36, 72)?;
    let plans = vec![PlanHint {
        hint: "OpenPositionBeforeOpponentCompletes".into(),
        squares: vec![],
    }];
    Some(Imbalance {
        kind: ImbalanceKind::Development,
        favors: f,
        magnitude: m,
        evidence,
        plans,
    })
}

/// 8. Initiative: forcing options available now (checks and SEE-positive
///    captures), interacting with the development lead.
pub fn initiative(board: &Board) -> Option<Imbalance> {
    let count_forcing = |b: &Board| {
        let mut n = 0i32;
        let enemy = !b.side_to_move();
        b.generate_moves(|pm| {
            for mv in pm {
                let is_capture = b.colors(enemy).has(mv.to);
                if is_capture && crate::see::see(b, mv.to, b.side_to_move()) > 0 {
                    n += 1;
                    continue;
                }
                let mut b2 = b.clone();
                b2.play_unchecked(mv);
                if !b2.checkers().is_empty() {
                    n += 1;
                }
            }
            false
        });
        n
    };
    let stm = board.side_to_move();
    let ours = count_forcing(board);
    let theirs = board.null_move().map(|nb| count_forcing(&nb)).unwrap_or(0);
    let (w, b) = if stm == Color::White {
        (ours, theirs)
    } else {
        (theirs, ours)
    };
    let diff = (w - b) * 15;
    let mut evidence = BTreeMap::new();
    evidence.insert("white_forcing_moves".into(), json!(w));
    evidence.insert("black_forcing_moves".into(), json!(b));
    // A two-forcing-move edge is worth NAMING but not worth a side-lean:
    // report it as Balanced/Minor; a three-move edge picks a side as
    // before (recall tuning against the book corpus, run 8.5).
    let (f, m) = if diff.abs() >= 30 {
        favors_or_balanced(diff, 45, 90)
    } else {
        favors(diff, 45, 90)?
    };
    Some(Imbalance {
        kind: ImbalanceKind::Initiative,
        favors: f,
        magnitude: m,
        evidence,
        plans: vec![],
    })
}

/// Run all eight detectors, dominant (highest magnitude) first.
pub fn assess(board: &Board) -> Vec<Imbalance> {
    let mut out: Vec<Imbalance> = [
        minor_pieces(board),
        pawn_structure(board),
        material(board),
        files_diagonals(board),
        squares_outposts(board),
        space(board),
        development(board),
        initiative(board),
    ]
    .into_iter()
    .flatten()
    .collect();
    out.sort_by_key(|i| std::cmp::Reverse(i.magnitude));
    out
}

/// Game phase (material + move based, per spec).
pub fn phase(board: &Board) -> crate::record::Phase {
    let nonpawn = |c: Color| {
        board
            .colors(c)
            .into_iter()
            .filter_map(|s| board.piece_on(s))
            .filter(|p| *p != Piece::Pawn && *p != Piece::King)
            .map(piece_value)
            .sum::<i32>()
    };
    let total = nonpawn(Color::White) + nonpawn(Color::Black);
    let queens = board.pieces(Piece::Queen).len();
    if total <= 2600 || (queens == 0 && total <= 3300) {
        crate::record::Phase::Endgame
    } else if board.fullmove_number() <= 10 {
        crate::record::Phase::Opening
    } else {
        crate::record::Phase::Middlegame
    }
}

#[cfg(test)]
mod tests {
    //! Cited unit tests for the run-8.5 plan hints. Each position is a
    //! book diagram given as FEN + citation only (no book prose).

    use super::*;

    fn board(fen: &str) -> Board {
        fen.parse().unwrap()
    }

    fn hints(imb: Option<Imbalance>) -> Vec<PlanHint> {
        imb.map(|i| i.plans).unwrap_or_default()
    }

    fn has(plans: &[PlanHint], hint: &str) -> bool {
        plans.iter().any(|p| p.hint == hint)
    }

    /// Jeremy Silman, The Amateur's Mind, p. 323, test 15: wing storm
    /// justified — the blocked e5 pawn can never be levered.
    #[test]
    fn wing_storm_fires_when_center_is_locked() {
        let b = board("r1bq1rk1/pp1nb1pp/4p3/2ppPp2/5B2/2PBP3/PP1N1PPP/R2QK2R w KQ - 0 1");
        let plans = hints(pawn_structure(&b));
        assert!(has(&plans, "WingPawnStormClosedCenter"), "{plans:?}");
    }

    /// Jeremy Silman, The Amateur's Mind, p. 322, test 14: the SAME storm
    /// idea is wrong here — the center can still be levered open. The
    /// discriminating partner of test 15.
    #[test]
    fn wing_storm_silent_when_center_is_fluid() {
        let b = board("r3nrk1/pppq1pbp/2np2p1/4p3/4P3/2NP1NP1/PPP2PKP/R1BQ1R2 w - - 0 1");
        let plans = hints(pawn_structure(&b));
        assert!(!has(&plans, "WingPawnStormClosedCenter"), "{plans:?}");
    }

    /// Jeremy Silman, The Complete Book of Chess Strategy, p. 202, entry
    /// 'Minority Attack': Carlsbad structure, White's b-pawn lever
    /// targets c6.
    #[test]
    fn minority_attack_white_carlsbad() {
        let b = board("r1bqrnk1/pp2bppp/2p2n2/3p2B1/3P4/2NBPN2/PPQ2PPP/R4RK1 w - - 0 1");
        let plans = hints(pawn_structure(&b));
        assert!(plans
            .iter()
            .any(|p| p.hint == "MinorityAttack" && p.squares == vec!["b5", "c6"]));
    }

    /// Jeremy Silman, The Complete Book of Chess Strategy, p. 203, entry
    /// 'Minority Attack': the mirrored structure — BLACK owns the
    /// minority attack against c3.
    #[test]
    fn minority_attack_black_mirror() {
        let b = board("r2qkbnr/pp3ppp/2n1p3/3p4/3P4/2PQ1N2/PP3PPP/RNB1K2R w KQkq - 0 1");
        let plans = hints(pawn_structure(&b));
        assert!(plans
            .iter()
            .any(|p| p.hint == "MinorityAttack" && p.squares == vec!["b4", "c3"]));
    }

    /// Jeremy Silman, The Complete Book of Chess Strategy, p. 329, entry
    /// 'Two Hogs on the Seventh': rooks already on the seventh press on.
    #[test]
    fn rook_to_seventh_two_hogs() {
        let b = board("r3k3/pRR5/8/5p2/6p1/6P1/r4PK1/8 w - - 0 1");
        let imb = files_diagonals(&b).expect("files imbalance");
        assert!(has(&imb.plans, "RookToSeventh"));
        assert_eq!(imb.favors, Favors::White);
        assert_eq!(imb.magnitude, Magnitude::Winning);
    }

    /// Jeremy Silman, The Complete Book of Chess Strategy, p. 225, entry
    /// 'No Entrance!': doubled rooks on the open a-file, but every entry
    /// square is covered — RookToSeventh must NOT fire.
    #[test]
    fn rook_to_seventh_denied_without_entry_squares() {
        let b = board("r5k1/rbqn1pb1/3p1npp/2pPp3/1pP1P3/1P4NP/1B1Q1PPN/1B2RRK1 b - - 0 1");
        let plans = hints(files_diagonals(&b));
        assert!(!has(&plans, "RookToSeventh"), "{plans:?}");
    }

    /// Jeremy Silman, The Complete Book of Chess Strategy, p. 323, entry
    /// 'Rooks Behind Passed Pawns': the rook belongs behind the a4
    /// passer.
    #[test]
    fn rook_behind_passer_tarrasch() {
        let b = board("8/5pk1/6p1/7p/P7/6P1/2r2PKP/1R6 w - - 0 1");
        let plans = hints(pawn_structure(&b));
        assert!(plans
            .iter()
            .any(|p| p.hint == "RookBehindPasser" && p.squares.contains(&"a4".to_string())));
    }

    /// Jeremy Silman, The Complete Book of Chess Strategy, p. 240, entry
    /// 'Pawn Structure - Doubled Pawns': the front doubled c4 pawn can
    /// never be defended by a pawn — pile up on it.
    #[test]
    fn pressure_doubled_pawn_saemisch_c4() {
        let b = board("rnbq1rk1/p1pp1ppp/1p2pn2/8/2PPP3/P1P2P2/6PP/R1BQKBNR b KQ - 0 1");
        let plans = hints(pawn_structure(&b));
        assert!(plans
            .iter()
            .any(|p| p.hint == "PressureDoubledPawn" && p.squares == vec!["c4"]));
    }

    /// Jeremy Silman, The Complete Book of Chess Strategy, p. 239, entry
    /// 'Pawn Structure - Doubled Pawns': USEFUL doubled e3/e4 pawns — the
    /// pressure plan must NOT fire against them.
    #[test]
    fn pressure_doubled_pawn_silent_on_useful_doubled_pawns() {
        let b = board("r1bq1rk1/ppp2pp1/2np1n1p/4p3/2B1P3/2NPPN2/PPP3PP/R2Q1RK1 w - - 0 1");
        let plans = hints(pawn_structure(&b));
        assert!(!has(&plans, "PressureDoubledPawn"), "{plans:?}");
    }

    /// Jeremy Silman, The Complete Book of Chess Strategy, p. 279, entry
    /// 'Trading Pieces': the e7 bishop buried behind the c5/d6/e5 chain
    /// wants to be traded or freed.
    /// Jeremy Silman, How to Reassess Your Chess, ex. 140: White's whole
    /// setup exists to own d5, and d5 is not a hole only because the e6
    /// pawn guards it — so the plan is f4-f5, striking the guard.
    #[test]
    fn undermine_names_the_pawn_that_guards_the_square_we_want() {
        let fen = "r2qkb1r/5ppp/p1bppn2/1p6/4PP2/1BN5/PPP3PP/R1BQ1RK1 w kq - 0 1";
        let plans = squares_outposts(&board(fen)).expect("imbalance").plans;
        assert!(
            plans
                .iter()
                .any(|p| p.hint == "UndermineDefender" && p.squares == vec!["e6", "d5"]),
            "{plans:?}"
        );
    }

    /// Ex. 80: the centralised knights rest on loose foundations, so the
    /// plan names the PAWNS propping them up, not the knights.
    #[test]
    fn undermine_names_the_pawn_propping_an_enemy_piece() {
        let fen = "r1r5/ppq2k2/2pp2pp/4nn2/1PPBB2P/6P1/P2Q1P2/1RR3K1 w - - 0 1";
        let plans = squares_outposts(&board(fen)).expect("imbalance").plans;
        assert!(
            plans
                .iter()
                .any(|p| p.hint == "UndermineDefender" && p.squares == vec!["d6", "e5"]),
            "{plans:?}"
        );
        // Levering a rook's-file pawn to "free" a wing square is true and
        // useless; only central squares justify a lever on their own.
        assert!(
            !plans
                .iter()
                .any(|p| p.hint == "UndermineDefender" && p.squares == vec!["a7", "b6"]),
            "{plans:?}"
        );
    }

    /// Jeremy Silman, The Amateur's Mind, p. 321 test 1: a quiet move
    /// overprotecting the e5 spearhead. The pawn is fixed (e6 blocks it)
    /// and under fire from the g7 bishop, which is the whole case for
    /// piling more defenders behind it (Nimzowitsch).
    #[test]
    fn overprotect_names_the_fixed_central_spearhead() {
        let fen = "r2q1rk1/p1p2pbp/b1p1p1p1/3pP3/3P1B2/2P2N2/PP1Q1PPP/R4RK1 w - - 0 1";
        let plans = squares_outposts(&board(fen)).expect("imbalance").plans;
        assert!(
            plans
                .iter()
                .any(|p| p.hint == "OverprotectStrongPoint" && p.squares == vec!["e5"]),
            "{plans:?}"
        );
    }

    /// A central pawn that can still advance is an occupant, not a strong
    /// POINT — overprotection is for squares you mean to keep forever.
    #[test]
    fn overprotect_is_silent_when_the_pawn_can_still_advance() {
        let fen = "rnbqkbnr/pppp1ppp/8/4p3/3PP3/8/PPP2PPP/RNBQKBNR w KQkq - 0 3";
        let plans = squares_outposts(&board(fen))
            .map(|i| i.plans)
            .unwrap_or_default();
        assert!(
            !plans.iter().any(|p| p.hint == "OverprotectStrongPoint"),
            "{plans:?}"
        );
    }

    /// Jeremy Silman, How to Reassess Your Chess, ex. 20: Black draws
    /// only by grabbing the DISTANT opposition (Ke7), not by walking at
    /// the pawn. The owner is the side to move, which is why this rides
    /// on Maneuver rather than a PlanHint.
    #[test]
    fn opposition_finds_the_distant_king_step() {
        let b = board("4k3/8/8/8/8/8/4P3/4K3 b - - 0 1");
        let (color, to) = opposition_move(&b).expect("an opposition move");
        assert_eq!(color, Color::Black);
        assert_eq!(square_name(to), "e7");
    }

    /// Ex. 22: the kings share no file, rank or diagonal, and Kg1 is
    /// still the move — the parity rule covers the rectangle case that a
    /// line-based test cannot see.
    #[test]
    fn opposition_handles_the_off_line_rectangle() {
        let b = board("8/8/8/k7/8/8/8/7K w - - 0 1");
        let (color, to) = opposition_move(&b).expect("an opposition move");
        assert_eq!(color, Color::White);
        assert_eq!(square_name(to), "g1");
    }

    /// Opposition is the whole game in a bare king ending and noise
    /// anywhere else, so the detector stays silent while pieces remain.
    #[test]
    fn opposition_is_silent_while_pieces_remain() {
        let b = board("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1");
        assert!(opposition_move(&b).is_none());
    }

    /// Jeremy Silman, How to Reassess Your Chess, ex. 27: the queenside
    /// majority is the point of the squeeze.
    #[test]
    fn healthy_majority_promises_a_passer() {
        let fen = "r4rk1/pb1p1ppp/1p2pn2/8/1PPP4/P1N2P2/3K2PP/R4B1R b - - 0 1";
        let plans = pawn_structure(&board(fen)).expect("imbalance").plans;
        assert!(has(&plans, "CreatePassedPawn"), "{plans:?}");
    }

    /// A crippled majority makes no passer: four against three with a
    /// doubled pawn is three healthy pawns against three, so the hint
    /// counts FILES, not just bodies.
    #[test]
    fn crippled_majority_promises_nothing() {
        // White f2/f3/g2/h2 (four pawns, three files) vs black f7/g7/h7.
        let fen = "4k3/5ppp/8/8/8/5P2/5PPP/4K3 w - - 0 1";
        let plans = pawn_structure(&board(fen))
            .map(|i| i.plans)
            .unwrap_or_default();
        assert!(
            !plans
                .iter()
                .any(|p| p.hint == "CreatePassedPawn" && p.squares == vec!["f3"]),
            "{plans:?}"
        );
    }

    /// Jeremy Silman, The Amateur's Mind, p. 326 test 1: decide which
    /// imbalance you are playing for, and hunt the bishop pair. The
    /// knight goes after the DEVELOPED bishop on e6.
    #[test]
    fn hunt_bishop_pair_goes_after_the_working_bishop() {
        let fen = "r2qkbnr/pppn1ppp/3pb3/4p3/3PP3/2N2N2/PPP2PPP/R1BQKB1R w KQkq - 0 1";
        let plans = minor_pieces(&board(fen)).expect("imbalance").plans;
        assert!(
            plans
                .iter()
                .any(|p| p.hint == "HuntBishopPair" && p.squares == vec!["c3", "e6"]),
            "{plans:?}"
        );
    }

    /// A bishop still on its home square is not the one whose loss hurts
    /// — its owner is often glad to trade it — so hunting it is a
    /// fantasy. On move seven of a Sveshnikov both black bishops are
    /// home and the hint must stay silent.
    #[test]
    fn hunt_bishop_pair_ignores_bishops_still_at_home() {
        let fen = "r1bqkb1r/pp3ppp/2np1n2/1N2p3/4P3/2N5/PPP2PPP/R1BQKB1R w KQkq - 0 7";
        let plans = minor_pieces(&board(fen))
            .map(|i| i.plans)
            .unwrap_or_default();
        assert!(
            !plans.iter().any(|p| p.hint == "HuntBishopPair"),
            "{plans:?}"
        );
    }

    #[test]
    fn trade_or_activate_bad_bishop() {
        let b = board("rnbq1rk1/pp2bpnp/3p2pB/2pPp3/2P1P1P1/2N2N1P/PP1Q1P2/R3R1K1 b - - 0 1");
        let plans = hints(minor_pieces(&b));
        assert!(plans
            .iter()
            .any(|p| p.hint == "TradeOrActivateBadBishop" && p.squares == vec!["e7"]));
    }

    /// Jeremy Silman, How to Reassess Your Chess, 3rd ed., p. 367,
    /// problem 27: queens off — the king marches toward the center.
    #[test]
    fn activate_king_in_endgame() {
        let b = board("r4rk1/pb1p1ppp/1p2pn2/8/1PPP4/P1N2P2/3K2PP/R4B1R b - - 0 1");
        let plans = hints(pawn_structure(&b));
        assert!(has(&plans, "ActivateKingInEndgame"), "{plans:?}");
    }

    /// Jeremy Silman, How to Reassess Your Chess, 3rd ed., p. 371,
    /// problem 82: the b8 knight has no stable square anywhere — keep it
    /// that way, and open the position for the bishop.
    #[test]
    fn restrict_knight_with_no_home() {
        let b = board("1n1rr1k1/p1p2ppp/1p1p4/4q3/2P5/P3PB2/1PQR1PPP/5RK1 w - - 0 1");
        let plans = hints(minor_pieces(&b));
        assert!(plans
            .iter()
            .any(|p| p.hint == "RestrictKnight" && p.squares == vec!["b8"]));
        assert!(has(&plans, "OpenPositionForBishops"));
    }

    /// Jeremy Silman, The Complete Book of Chess Strategy, p. 269, entry
    /// 'Queenside Pawn Majority': in a middlegame the CENTRAL majority
    /// outranks the queenside one — the central hint fires, the wing
    /// hint is withheld.
    #[test]
    fn central_majority_outranks_queenside_in_middlegame() {
        let b = board("2rr2k1/p1qn1ppb/1p2p2p/8/2P5/1P2BN2/P3QPPP/3RR1K1 b - - 0 1");
        let plans = hints(pawn_structure(&b));
        assert!(has(&plans, "AdvanceCentralMajority"), "{plans:?}");
        assert!(!has(&plans, "AdvanceQueensideMajority"), "{plans:?}");
    }

    /// Jeremy Silman, The Amateur's Mind, p. 316, test 2: the black king
    /// is airy and White owns the half-open f-file beside it.
    #[test]
    fn open_lines_toward_weak_king() {
        let b = board("r3r1k1/pb3p2/4pR2/1p1p2p1/3P1n2/B1P5/PP1N2PP/R5K1 w - - 0 1");
        let imb = files_diagonals(&b).expect("files imbalance");
        assert_eq!(imb.favors, Favors::White);
        assert!(imb
            .plans
            .iter()
            .any(|p| p.hint == "OpenLinesTowardWeakKing" && p.squares == vec!["f7"]));
    }

    /// Jeremy Silman, The Complete Book of Chess Strategy, p. 236, entry
    /// 'Pawn Structure - Backward Pawns': the backward d6 pawn on
    /// White's half-open file, with White in charge of the stop square.
    #[test]
    fn pressure_backward_pawn_on_half_open_file() {
        let b = board("r1q2rk1/1p2bppp/pBnp4/4p3/P7/2NB1QP1/1PP2P1P/R2R2K1 w - - 0 1");
        let plans = hints(pawn_structure(&b));
        assert!(plans
            .iter()
            .any(|p| p.hint == "PressureBackwardPawn" && p.squares == vec!["d6", "d5"]));
    }

    /// Jeremy Silman, The Complete Book of Chess Strategy, p. 237, entry
    /// 'Pawn Structure - Backward Pawns': d6 is backward but WELL
    /// DEFENDED — the pressure plan goes nowhere and must stay silent.
    #[test]
    fn pressure_backward_pawn_silent_when_well_defended() {
        let b = board("r2r1bk1/1pq1ppp1/p1npbn1p/8/4P3/1NN1BP2/PPPQB1PP/R2R2K1 w - - 0 1");
        let plans = hints(pawn_structure(&b));
        assert!(!has(&plans, "PressureBackwardPawn"), "{plans:?}");
    }
}
