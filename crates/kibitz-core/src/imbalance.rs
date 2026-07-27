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
            if own_center_pawns.len() >= 2 && mobility <= 2 {
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
            }
        }
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

    let (f, m) = favors(score, 20, 45)?;
    let mut plans = Vec::new();
    if closed && wn > bn {
        plans.push(PlanHint {
            hint: "KeepPositionClosed".into(),
            squares: vec![],
        });
    }
    if !closed && ((wb >= 2 && bb < 2) || (bb >= 2 && wb < 2)) {
        plans.push(PlanHint {
            hint: "OpenPositionForBishops".into(),
            squares: vec![],
        });
    }
    Some(Imbalance {
        kind: ImbalanceKind::MinorPieces,
        favors: f,
        magnitude: m,
        evidence,
        plans,
    })
}

/// Holes in `camp_owner`'s camp: squares the owner's pawns can never
/// defend, restricted to the ranks where an enemy outpost actually bites
/// (5th/6th from the attacker's view). Ranks nearer the back rank are
/// pawn-undefendable by construction and piece-covered in practice —
/// counting them buries the real signal in noise.
fn holes_in_camp(board: &Board, camp_owner: Color) -> BitBoard {
    let owner_span = pawn_attack_span(board, camp_owner);
    let half = match camp_owner {
        Color::White => Rank::Third.bitboard() | Rank::Fourth.bitboard(),
        Color::Black => Rank::Sixth.bitboard() | Rank::Fifth.bitboard(),
    };
    // Central-ish holes matter (files b-g).
    let files = !(File::A.bitboard() | File::H.bitboard());
    half & files & !owner_span & !board.occupied()
}

/// 2. Pawn structure.
pub fn pawn_structure(board: &Board) -> Option<Imbalance> {
    let mut evidence = BTreeMap::new();
    let mut plans = Vec::new();
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
            // Backward: stop square controlled by enemy pawns, no own pawn
            // beside/behind on adjacent files, not isolated (that's worse).
            if let Some(stop) = match color {
                Color::White => p.try_offset(0, 1),
                Color::Black => p.try_offset(0, -1),
            } {
                let enemy_controls_stop =
                    !(get_pawn_attacks(stop, !color) & BitBoard::EMPTY).is_empty() || {
                        // pawns of !color attacking stop:
                        !(get_pawn_attacks(stop, color) & enemy).is_empty()
                    };
                let support = own & adj & behind_or_beside(color, p.rank());
                if enemy_controls_stop && support.is_empty() && !(own & adj).is_empty() {
                    backward[ci] |= p.bitboard();
                }
            }
        }
    }

    // Score: passed pawns are assets; isolated/doubled/backward liabilities.
    for (ci, sign) in [(0usize, 1i32), (1, -1)] {
        score += sign
            * (passed[ci].len() as i32 * 30
                - iso[ci].len() as i32 * 15
                - doubled[ci].len() as i32 * 10
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

    // Majorities per wing (files a-c vs f-h).
    let qside = File::A.bitboard() | File::B.bitboard() | File::C.bitboard();
    let kside = File::F.bitboard() | File::G.bitboard() | File::H.bitboard();
    let wq = (board.colored_pieces(Color::White, Piece::Pawn) & qside).len() as i32;
    let bq = (board.colored_pieces(Color::Black, Piece::Pawn) & qside).len() as i32;
    let wk = (board.colored_pieces(Color::White, Piece::Pawn) & kside).len() as i32;
    let bk = (board.colored_pieces(Color::Black, Piece::Pawn) & kside).len() as i32;
    if wq > bq {
        evidence.insert("queenside_majority".into(), json!("white"));
        plans.push(PlanHint {
            hint: "AdvanceQueensideMajority".into(),
            squares: vec![],
        });
    } else if bq > wq {
        evidence.insert("queenside_majority".into(), json!("black"));
    }
    if bk > wk {
        evidence.insert("kingside_majority".into(), json!("black"));
    } else if wk > bk {
        evidence.insert("kingside_majority".into(), json!("white"));
    }

    // Backward pawns invite pressure down their file onto the stop square.
    for (ci, color) in [(0, Color::White), (1, Color::Black)] {
        for p in backward[ci] {
            if let Some(stop) = match color {
                Color::White => p.try_offset(0, 1),
                Color::Black => p.try_offset(0, -1),
            } {
                let _ = ci;
                plans.push(PlanHint {
                    hint: "PressureBackwardPawn".into(),
                    squares: vec![square_name(p), square_name(stop)],
                });
            }
        }
    }

    // Blockade hint against the strongest enemy passer.
    for (ci, color) in [(0, Color::White), (1, Color::Black)] {
        for p in passed[ci] {
            if let Some(stop) = match color {
                Color::White => p.try_offset(0, 1),
                Color::Black => p.try_offset(0, -1),
            } {
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
        }
    }

    if evidence.is_empty() {
        return None;
    }
    let (f, m) = favors(score, 15, 45)?;
    Some(Imbalance {
        kind: ImbalanceKind::PawnStructure,
        favors: f,
        magnitude: m,
        evidence,
        plans,
    })
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
    let (f, m) = favors(diff, 80, 250)?;
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
    if !w7.is_empty() {
        evidence.insert("rook_on_seventh".into(), json!("white"));
        score += 25;
    }
    if !b2.is_empty() {
        evidence.insert("rook_on_seventh".into(), json!("black"));
        score -= 25;
    }

    if evidence.is_empty() {
        return None;
    }
    let (f, m) = favors(score, 15, 40)?;
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

/// 5. Squares & outposts (with the spec's BFS knight-route plan hint).
pub fn squares_outposts(board: &Board) -> Option<Imbalance> {
    let mut evidence = BTreeMap::new();
    let mut plans = Vec::new();
    let mut score = 0i32;

    for (color, sign) in [(Color::White, 1i32), (Color::Black, -1)] {
        let enemy = !color;
        let holes = holes_in_camp(board, enemy);
        if holes.is_empty() {
            continue;
        }
        let key = if color == Color::White {
            "holes_in_black_camp"
        } else {
            "holes_in_white_camp"
        };
        evidence.insert(key.into(), sq_list(holes));
        score += sign * holes.len().min(3) as i32 * 8;

        // Established outposts: own knight on a hole, defended by own pawn.
        for n in board.colored_pieces(color, Piece::Knight) {
            if holes.has(n) {
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
                }
            } else if let Some((target, route)) = knight_route_to(board, color, n, holes) {
                plans.push(PlanHint {
                    hint: "ManeuverKnightToOutpost".into(),
                    squares: route.into_iter().chain([target]).map(square_name).collect(),
                });
                score += sign * 8;
            }
        }
    }
    if evidence.is_empty() {
        return None;
    }
    let (f, m) = favors(score, 12, 35)?;
    Some(Imbalance {
        kind: ImbalanceKind::SquaresOutposts,
        favors: f,
        magnitude: m,
        evidence,
        plans,
    })
}

/// Simple BFS (spec): shortest knight path to any hole over squares that
/// are not occupied by own pieces and not attacked by enemy pawns.
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
    let blocked = board.colors(color) | enemy_pawn_attacks | outgunned;
    let mut prev: [Option<Square>; 64] = [None; 64];
    let mut seen = from.bitboard();
    let mut frontier = vec![from];
    for _depth in 0..3 {
        let mut next_frontier = Vec::new();
        for &s in &frontier {
            for n in get_knight_moves(s) & !seen & !blocked {
                prev[n as usize] = Some(s);
                if targets.has(n) {
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
    // presence in the enemy half before reporting.
    if w.max(b) < 3 {
        return None;
    }
    let diff = (w - b) * 12;
    let mut evidence = BTreeMap::new();
    evidence.insert("white_space".into(), json!(w));
    evidence.insert("black_space".into(), json!(b));
    let (f, m) = favors(diff, 24, 60)?;
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
    let developed = |c: Color| {
        let back = match c {
            Color::White => Rank::First,
            Color::Black => Rank::Eighth,
        };
        let minors = board.colors(c) & (board.pieces(Piece::Knight) | board.pieces(Piece::Bishop));
        let out = (minors & !back.bitboard()).len() as i32;
        let castled = {
            let k = board.king(c);
            k.rank() == back && (k.file() as i8 - File::E as i8).abs() >= 2
        };
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
    let (f, m) = favors(diff, 45, 90)?;
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
