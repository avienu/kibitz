//! Endgame trainer (ROADMAP Phase 5): a tiered curriculum of classic
//! theoretical positions (bundled data file), a drill session state machine
//! where the user plays the goal side against an automatic opponent, and
//! attempt/mastery bookkeeping (migration 0011).
//!
//! Engine-off principle (CLAUDE.md #6): NOTHING in this module spawns an
//! engine. The opponent is either
//!   1. **Tablebase** — when a [`silman_tb::Tablebase`] is supplied and the
//!      position has at most `largest()` pieces, the reply is Fathom's
//!      DTZ-informed, WDL-preserving root move (provably result-optimal).
//!      With only the 3-man test set most curriculum drills exceed the
//!      limit and fall back to the heuristic; a 3-4-5 set covers all of
//!      them.
//!   2. **Heuristic** — a deterministic 2-ply material minimax with small
//!      positional nudges (pawn advancement, king proximity to the action,
//!      direct opposition). It is a plausible sparring partner for these
//!      curated positions — it defends king-and-pawn endings sensibly and
//!      never hangs material to a one-move refutation — but it is NOT an
//!      oracle and will not find study-like resources. Its limits are
//!      accepted: drills are graded on the *user's* play.
//!
//! Outcome policing (documented, honest about its limits):
//! - **Success** is terminal: checkmate for win drills; stalemate, bare/
//!   insufficient material, the 50-move rule or threefold repetition for
//!   draw drills (delivering mate in a draw drill also counts — it is
//!   strictly better than the goal).
//! - **Failure** is (a) terminal: getting mated, or reaching a drawn
//!   terminal in a win drill; or (b) **tablebase-verified result flip**:
//!   when tables cover the position, every user move is probed before and
//!   after, and a move that forfeits the theoretical goal fails the drill
//!   immediately. Without tablebase files only terminal detection applies —
//!   a theoretical mistake then surfaces later (or not at all if the
//!   heuristic fails to punish it), which is recorded per attempt in the
//!   `verification` column.

use std::collections::HashMap;
use std::sync::OnceLock;

use cozy_chess::{Board, Color, Move, Piece, Rank, Square};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use silman_tb::{RootProbe, Tablebase};

use crate::hash::position_hash;
use crate::tactics::parse_uci;

// ---------------------------------------------------------------------------
// Curriculum (bundled data file, per the data-not-string-literals convention)
// ---------------------------------------------------------------------------

/// The curriculum source. Structure (rating-banded tiers, essentials first)
/// follows the classic endgame-course format; all content is original or
/// public-domain theory. See the file's own `comment` field.
const CURRICULUM_JSON: &str = include_str!("../data/endgame_curriculum.json");

/// Consecutive clean (solved) attempts required for mastery.
pub const MASTERY_STREAK: i64 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Goal {
    Win,
    Draw,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Tier {
    pub id: String,
    pub name: String,
    pub rating_band: String,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Drill {
    /// Stable string id; attempt history is keyed by it.
    pub id: String,
    pub tier: String,
    pub title: String,
    /// Public-domain concept name (e.g. "lucena", "square_of_pawn").
    pub concept: String,
    /// White-vs-black piece letters in KQRBNP order (e.g. "KRPvKR");
    /// asserted against the FEN by the curriculum tests.
    pub material: String,
    /// The side to move is the side the user plays.
    pub fen: String,
    pub goal: Goal,
    pub instruction: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Curriculum {
    pub version: u32,
    pub tiers: Vec<Tier>,
    pub drills: Vec<Drill>,
}

/// The parsed bundled curriculum. Panics only if the bundled file is
/// malformed, which the curriculum tests rule out.
pub fn curriculum() -> &'static Curriculum {
    static CURRICULUM: OnceLock<Curriculum> = OnceLock::new();
    CURRICULUM.get_or_init(|| {
        serde_json::from_str(CURRICULUM_JSON).expect("bundled endgame curriculum parses")
    })
}

pub fn drill(id: &str) -> Option<&'static Drill> {
    curriculum().drills.iter().find(|d| d.id == id)
}

// ---------------------------------------------------------------------------
// Drill session state machine
// ---------------------------------------------------------------------------

/// How the drill ended.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Outcome {
    pub solved: bool,
    pub detail: String,
}

/// Where one opponent reply came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum OpponentSource {
    Tablebase,
    Heuristic,
}

/// One opponent reply.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpponentMove {
    pub uci: String,
    pub source: OpponentSource,
}

/// Grading of one move in the feedback aside. User moves are graded ONLY
/// from tablebase probes — never engine scores; opponent replies carry the
/// `engine` label (the design's name for the scripted defender), ungraded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Verdict {
    /// Kept the theoretical result on the fastest (DTZ-optimal) path.
    Winning,
    /// Kept the result but the DTZ worsened — `dtz_cost` states the cost.
    Slower,
    /// Flipped the theoretical result.
    Throws,
    /// No tablebase coverage for this move (graded on terminals only).
    Unverified,
    /// The scripted defender's reply.
    Engine,
}

/// One row of the endgame feedback aside: `no | SAN | verdict | note`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VerdictRow {
    /// 1-based row number over the whole session (user moves and replies).
    pub index: u32,
    pub san: String,
    pub verdict: Verdict,
    /// Only for [`Verdict::Slower`]: how many plies longer the tablebase
    /// path became compared to the fastest move.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dtz_cost: Option<u32>,
    /// Short human note; empty when the verdict speaks for itself.
    pub note: String,
}

/// What one user move produced.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StepReport {
    /// Position after the user's move.
    pub fen_after_user: String,
    /// The opponent's reply, when the drill continued.
    pub opponent: Option<OpponentMove>,
    /// Position after the opponent's reply (when there was one).
    pub fen_after_opponent: Option<String>,
    /// Feedback rows ADDED by this step, in order: the user move's graded
    /// row, then the opponent reply's `engine` row when there was one.
    /// (The whole session's list is on [`DrillSession::verdict_rows`].)
    pub rows: Vec<VerdictRow>,
    /// Set when the drill ended on this step.
    pub outcome: Option<Outcome>,
}

/// A running drill: the user plays the side to move of the drill FEN
/// against the tablebase/heuristic opponent. Pure state machine — no IO,
/// no clock; persistence is the caller's job via [`record_attempt`].
pub struct DrillSession {
    drill: Drill,
    board: Board,
    user: Color,
    user_moves: u32,
    /// User moves whose before/after theoretical result was tablebase-
    /// checked (drives the attempt's `verification` column).
    tb_checked_moves: u32,
    tb_replies: u32,
    heuristic_replies: u32,
    /// Position-hash occurrence counts for threefold detection (uses the
    /// ep-normalized hash — the same one the position index uses).
    reps: HashMap<u64, u32>,
    /// Feedback rows for the whole session (user moves + replies).
    rows: Vec<VerdictRow>,
    outcome: Option<Outcome>,
}

/// Terminal states of a position within a session.
enum Terminal {
    /// The side to move is checkmated.
    Mate,
    Stalemate,
    InsufficientMaterial,
    FiftyMoveRule,
    ThreefoldRepetition,
}

impl DrillSession {
    pub fn new(drill: &Drill) -> anyhow::Result<Self> {
        let board = Board::from_fen(&drill.fen, false)
            .map_err(|e| anyhow::anyhow!("drill {}: bad FEN {:?}: {e}", drill.id, drill.fen))?;
        let mut reps = HashMap::new();
        reps.insert(position_hash(&board), 1);
        Ok(DrillSession {
            user: board.side_to_move(),
            board,
            drill: drill.clone(),
            user_moves: 0,
            tb_checked_moves: 0,
            tb_replies: 0,
            heuristic_replies: 0,
            reps,
            rows: Vec::new(),
            outcome: None,
        })
    }

    /// All feedback rows so far (user moves graded by tablebase truth,
    /// opponent replies labeled `engine`).
    pub fn verdict_rows(&self) -> &[VerdictRow] {
        &self.rows
    }

    fn push_row(
        &mut self,
        san: String,
        verdict: Verdict,
        dtz_cost: Option<u32>,
        note: String,
    ) -> VerdictRow {
        let row = VerdictRow {
            index: self.rows.len() as u32 + 1,
            san,
            verdict,
            dtz_cost,
            note,
        };
        self.rows.push(row.clone());
        row
    }

    pub fn drill(&self) -> &Drill {
        &self.drill
    }

    pub fn board(&self) -> &Board {
        &self.board
    }

    pub fn fen(&self) -> String {
        format!("{}", self.board)
    }

    pub fn user_color(&self) -> Color {
        self.user
    }

    pub fn user_moves(&self) -> u32 {
        self.user_moves
    }

    pub fn outcome(&self) -> Option<&Outcome> {
        self.outcome.as_ref()
    }

    /// `opponent` column value for the finished attempt.
    pub fn opponent_kind(&self) -> &'static str {
        match (self.tb_replies > 0, self.heuristic_replies > 0) {
            (true, true) => "mixed",
            (true, false) => "tablebase",
            (false, true) => "heuristic",
            (false, false) => "none",
        }
    }

    /// `verification` column value: "tablebase" only when EVERY user move
    /// was probed for a result flip.
    pub fn verification_kind(&self) -> &'static str {
        if self.user_moves > 0 && self.tb_checked_moves == self.user_moves {
            "tablebase"
        } else {
            "terminal"
        }
    }

    /// Whether an opponent reply in the CURRENT position would come from
    /// the tablebase (for UI display before any move is made).
    pub fn opponent_would_use_tb(&self, tb: Option<&Tablebase>) -> bool {
        tb.is_some_and(|tb| self.board.occupied().len() <= tb.largest())
    }

    /// Give up: fails the drill (recorded like any other failed attempt).
    pub fn resign(&mut self) {
        if self.outcome.is_none() {
            self.outcome = Some(Outcome {
                solved: false,
                detail: "Gave up.".to_string(),
            });
        }
    }

    /// Play one user move (UCI), then — if the drill continues — the
    /// opponent's reply. `tb` enables tablebase result-flip policing,
    /// tablebase move grading (the feedback rows) and tablebase opponent
    /// replies where the piece count allows.
    ///
    /// Errors on an unparseable/illegal move or a finished drill; those are
    /// caller mistakes, not drill failures.
    pub fn user_move(
        &mut self,
        uci: &str,
        mut tb: Option<&mut Tablebase>,
    ) -> anyhow::Result<StepReport> {
        anyhow::ensure!(self.outcome.is_none(), "drill already finished");
        anyhow::ensure!(
            self.board.side_to_move() == self.user,
            "internal error: not the user's turn"
        );
        let mv = parse_uci(&self.board, uci).map_err(|e| anyhow::anyhow!(e))?;
        let user_san = crate::san::format_san(&self.board, mv);

        // Tablebase probes before and after the move: WDL for result-flip
        // policing, DTZ for pace grading. NEVER an engine score.
        let pre = tb.as_deref_mut().and_then(|tb| tb_probe(tb, &self.board));
        let mut after = self.board.clone();
        after
            .try_play(mv)
            .map_err(|e| anyhow::anyhow!("legal move failed to play: {e}"))?;
        // Opponent-to-move probe -> user perspective for the score.
        let post_user = tb
            .as_deref_mut()
            .and_then(|tb| tb_probe(tb, &after))
            .map(|(s, d)| (-s, d));
        let checked = pre.is_some() && post_user.is_some();
        // A zeroing move (pawn move or capture) restarts the DTZ count, so
        // pace comparison across it is meaningless — and a result-keeping
        // zeroing move IS the progress DTZ measures.
        let move_zeroed = after.halfmove_clock() == 0;

        self.board = after;
        self.user_moves += 1;
        if checked {
            self.tb_checked_moves += 1;
        }
        *self.reps.entry(position_hash(&self.board)).or_insert(0) += 1;
        let fen_after_user = self.fen();

        if let (Some((pre_s, _)), Some((post_s, _))) = (pre, post_user) {
            if meets_goal(pre_s, self.drill.goal) && !meets_goal(post_s, self.drill.goal) {
                self.outcome = Some(Outcome {
                    solved: false,
                    detail: format!(
                        "That move throws away the {} — the tablebase says the position is now {}.",
                        goal_word(self.drill.goal),
                        score_word(post_s),
                    ),
                });
                let row = self.push_row(
                    user_san,
                    Verdict::Throws,
                    None,
                    format!(
                        "Throws away the {}: the position is now {}.",
                        goal_word(self.drill.goal),
                        score_word(post_s)
                    ),
                );
                return Ok(StepReport {
                    fen_after_user,
                    opponent: None,
                    fen_after_opponent: None,
                    rows: vec![row],
                    outcome: self.outcome.clone(),
                });
            }
        }

        // Terminal after the user's move (the opponent is now to move).
        if let Some(t) = self.terminal() {
            self.outcome = Some(match t {
                Terminal::Mate => Outcome {
                    solved: true,
                    detail: "Checkmate!".to_string(),
                },
                other => self.draw_outcome(&other),
            });
            let outcome = self.outcome.clone().expect("just set");
            // Terminals are ground truth: a solved ending kept the result,
            // a failed one (drawn terminal in a win drill) threw it.
            let verdict = if outcome.solved {
                Verdict::Winning
            } else {
                Verdict::Throws
            };
            let row = self.push_row(user_san, verdict, None, outcome.detail.clone());
            return Ok(StepReport {
                fen_after_user,
                opponent: None,
                fen_after_opponent: None,
                rows: vec![row],
                outcome: self.outcome.clone(),
            });
        }

        // Grade the (non-terminal, non-flipping) user move.
        let user_row = match (pre, post_user) {
            (Some((pre_s, pre_d)), Some((post_s, post_d))) => {
                // -1..=1 class: 0 loss, 1 draw band, 2 win — for positions
                // whose goal already slipped during an uncovered stretch.
                let class = |s: i8| -> u8 {
                    if s >= 2 {
                        2
                    } else if s >= -1 {
                        1
                    } else {
                        0
                    }
                };
                if class(post_s) < class(pre_s) {
                    // The goal was already gone, but this move loses even
                    // the remaining theoretical result.
                    self.push_row(
                        user_san,
                        Verdict::Throws,
                        None,
                        format!("The position is now {}.", score_word(post_s)),
                    )
                } else if self.drill.goal == Goal::Win && pre_s >= 2 && !move_zeroed {
                    // Winning position kept: grade the pace. Optimal play
                    // shortens the DTZ by one ply per move.
                    let cost = post_d as i64 + 1 - pre_d as i64;
                    if cost > 0 {
                        self.push_row(
                            user_san,
                            Verdict::Slower,
                            Some(cost as u32),
                            format!(
                                "Still winning, but the tablebase path is {cost} pl{} longer.",
                                if cost == 1 { "y" } else { "ies" }
                            ),
                        )
                    } else {
                        self.push_row(user_san, Verdict::Winning, None, String::new())
                    }
                } else {
                    self.push_row(user_san, Verdict::Winning, None, String::new())
                }
            }
            _ => self.push_row(
                user_san,
                Verdict::Unverified,
                None,
                "No tablebase coverage for this position.".to_string(),
            ),
        };

        // Opponent reply.
        let (reply, source) = self.opponent_reply(tb);
        let reply_san = crate::san::format_san(&self.board, reply);
        self.board
            .try_play(reply)
            .map_err(|e| anyhow::anyhow!("opponent move failed to play: {e}"))?;
        match source {
            OpponentSource::Tablebase => self.tb_replies += 1,
            OpponentSource::Heuristic => self.heuristic_replies += 1,
        }
        *self.reps.entry(position_hash(&self.board)).or_insert(0) += 1;
        let fen_after_opponent = self.fen();
        let reply_row = self.push_row(reply_san, Verdict::Engine, None, String::new());

        // Terminal after the opponent's reply (the user is now to move).
        if let Some(t) = self.terminal() {
            self.outcome = Some(match t {
                Terminal::Mate => Outcome {
                    solved: false,
                    detail: "You were checkmated.".to_string(),
                },
                other => self.draw_outcome(&other),
            });
        }

        Ok(StepReport {
            fen_after_user,
            opponent: Some(OpponentMove {
                uci: reply.to_string(),
                source,
            }),
            fen_after_opponent: Some(fen_after_opponent),
            rows: vec![user_row, reply_row],
            outcome: self.outcome.clone(),
        })
    }

    /// A drawn terminal graded against the drill goal.
    fn draw_outcome(&self, t: &Terminal) -> Outcome {
        let how = match t {
            Terminal::Stalemate => "stalemate",
            Terminal::InsufficientMaterial => "insufficient material",
            Terminal::FiftyMoveRule => "the 50-move rule",
            Terminal::ThreefoldRepetition => "threefold repetition",
            Terminal::Mate => unreachable!("mate is not a draw"),
        };
        match self.drill.goal {
            Goal::Draw => Outcome {
                solved: true,
                detail: format!("Draw held ({how})."),
            },
            Goal::Win => Outcome {
                solved: false,
                detail: format!("Only a draw ({how}) — the position was winning."),
            },
        }
    }

    /// Terminal state of the current position, if any.
    fn terminal(&self) -> Option<Terminal> {
        if legal_moves(&self.board).is_empty() {
            return Some(if self.board.checkers().is_empty() {
                Terminal::Stalemate
            } else {
                Terminal::Mate
            });
        }
        if insufficient_material(&self.board) {
            return Some(Terminal::InsufficientMaterial);
        }
        if self.board.halfmove_clock() >= 100 {
            return Some(Terminal::FiftyMoveRule);
        }
        if self
            .reps
            .get(&position_hash(&self.board))
            .is_some_and(|&n| n >= 3)
        {
            return Some(Terminal::ThreefoldRepetition);
        }
        None
    }

    /// Pick the opponent's reply: tablebase when covered, else heuristic.
    fn opponent_reply(&self, tb: Option<&mut Tablebase>) -> (Move, OpponentSource) {
        if let Some(tb) = tb {
            if self.board.occupied().len() <= tb.largest() {
                if let Ok(RootProbe::Move(m)) = tb.probe_root_board(&self.board) {
                    let mv = Move {
                        from: m.from,
                        to: m.to,
                        promotion: m.promotion,
                    };
                    if self.board.is_legal(mv) {
                        return (mv, OpponentSource::Tablebase);
                    }
                }
            }
        }
        (self.heuristic_reply(), OpponentSource::Heuristic)
    }

    /// Deterministic 2-ply minimax reply (see module docs for the policy
    /// and its limits). Ties break on the lexicographically smallest UCI.
    fn heuristic_reply(&self) -> Move {
        let mut best: Option<(i64, String, Move)> = None;
        for mv in legal_moves(&self.board) {
            let mut b = self.board.clone();
            b.play_unchecked(mv);
            let score = -self.negamax(&b, 1);
            let uci = mv.to_string();
            let better = match &best {
                None => true,
                Some((s, u, _)) => score > *s || (score == *s && uci < *u),
            };
            if better {
                best = Some((score, uci, mv));
            }
        }
        best.expect("opponent_reply called on a non-terminal position")
            .2
    }

    /// Negamax from the perspective of `board`'s side to move. Repetition
    /// is deliberately ignored inside the search (the session-level
    /// threefold check governs the game itself).
    fn negamax(&self, board: &Board, depth: u32) -> i64 {
        let moves = legal_moves(board);
        if moves.is_empty() {
            return if board.checkers().is_empty() {
                self.draw_score(board.side_to_move())
            } else {
                // Mated; prefer later mates (depth raises the score).
                -MATE_SCORE - depth as i64
            };
        }
        if insufficient_material(board) || board.halfmove_clock() >= 100 {
            return self.draw_score(board.side_to_move());
        }
        if depth == 0 {
            return self.eval(board);
        }
        let mut best = i64::MIN;
        for mv in moves {
            let mut b = board.clone();
            b.play_unchecked(mv);
            best = best.max(-self.negamax(&b, depth - 1));
        }
        best
    }

    /// Whether `color` is trying to draw this drill (the opponent of a
    /// win-goal user defends; the opponent of a draw-goal user attacks).
    fn wants_draw(&self, color: Color) -> bool {
        (color == self.user) == (self.drill.goal == Goal::Draw)
    }

    fn draw_score(&self, side_to_move: Color) -> i64 {
        if self.wants_draw(side_to_move) {
            DRAW_SCORE
        } else {
            -DRAW_SCORE
        }
    }

    /// Static evaluation from the side to move's perspective: material,
    /// pawn advancement, king proximity to the most advanced pawn's
    /// promotion square (or the enemy king), and a direct-opposition nudge.
    fn eval(&self, board: &Board) -> i64 {
        let stm = board.side_to_move();
        let opp = !stm;
        let mut s = material(board, stm) - material(board, opp);
        for color in [stm, opp] {
            let sign = if color == stm { 1 } else { -1 };
            for sq in board.pieces(Piece::Pawn) & board.colors(color) {
                s += sign * 12 * pawn_progress(sq, color);
            }
        }
        let focus = focus_square(board);
        let (my_king, their_king) = (board.king(stm), board.king(opp));
        s += 3 * (chebyshev(their_king, focus) - chebyshev(my_king, focus));
        // Standing in direct opposition with the move is the bad side of it.
        let df = (my_king.file() as i8 - their_king.file() as i8).abs();
        let dr = (my_king.rank() as i8 - their_king.rank() as i8).abs();
        if (df == 0 && dr == 2) || (dr == 0 && df == 2) {
            s -= 8;
        }
        s
    }
}

const MATE_SCORE: i64 = 1_000_000;
/// A draw outranks any material for the side that wants one.
const DRAW_SCORE: i64 = 200_000;

fn legal_moves(board: &Board) -> Vec<Move> {
    let mut v = Vec::with_capacity(64);
    board.generate_moves(|ml| {
        v.extend(ml);
        false
    });
    v
}

fn material(board: &Board, color: Color) -> i64 {
    let side = board.colors(color);
    let count = |p: Piece| (board.pieces(p) & side).len() as i64;
    900 * count(Piece::Queen)
        + 500 * count(Piece::Rook)
        + 330 * count(Piece::Bishop)
        + 320 * count(Piece::Knight)
        + 100 * count(Piece::Pawn)
}

/// Ranks a pawn has advanced from its starting rank (0..=5).
fn pawn_progress(sq: Square, color: Color) -> i64 {
    match color {
        Color::White => sq.rank() as i64 - 1,
        Color::Black => 6 - sq.rank() as i64,
    }
}

/// The square the position revolves around: the promotion square of the
/// most advanced pawn on the board, or the white king's square if no pawns
/// remain (any fixed point keeps kings engaged).
fn focus_square(board: &Board) -> Square {
    let mut best: Option<(i64, Square)> = None;
    for color in [Color::White, Color::Black] {
        for sq in board.pieces(Piece::Pawn) & board.colors(color) {
            let progress = pawn_progress(sq, color);
            if best.is_none_or(|(p, _)| progress > p) {
                let promo_rank = match color {
                    Color::White => Rank::Eighth,
                    Color::Black => Rank::First,
                };
                best = Some((progress, Square::new(sq.file(), promo_rank)));
            }
        }
    }
    best.map(|(_, sq)| sq)
        .unwrap_or_else(|| board.king(Color::White))
}

fn chebyshev(a: Square, b: Square) -> i64 {
    let df = (a.file() as i8 - b.file() as i8).abs() as i64;
    let dr = (a.rank() as i8 - b.rank() as i8).abs() as i64;
    df.max(dr)
}

/// Dead draws detectable by material alone: bare kings, or a lone minor.
fn insufficient_material(board: &Board) -> bool {
    let non_kings = board.occupied() & !board.pieces(Piece::King);
    match non_kings.len() {
        0 => true,
        1 => {
            let sq = non_kings.next_square().expect("len is 1");
            matches!(board.piece_on(sq), Some(Piece::Bishop | Piece::Knight))
        }
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// Tablebase scoring
// ---------------------------------------------------------------------------

/// Position value and DTZ from the side to move's perspective: score is
/// -2 loss, -1 blessed loss (50-move-rule draw), 0 draw, 1 cursed win,
/// 2 win; DTZ is the distance to zeroing under optimal play (0 at
/// terminals). `None` when the tables do not cover the position (too many
/// pieces, missing file, ...). Uses the root probe because — unlike the
/// WDL probe — it accepts a nonzero 50-move counter, which mid-drill
/// positions routinely have.
fn tb_probe(tb: &mut Tablebase, board: &Board) -> Option<(i8, u32)> {
    if board.occupied().len() > tb.largest() {
        return None;
    }
    match tb.probe_root_board(board) {
        Ok(RootProbe::Checkmate) => Some((-2, 0)),
        Ok(RootProbe::Stalemate) => Some((0, 0)),
        Ok(RootProbe::Move(m)) => Some((
            match m.wdl {
                silman_tb::Wdl::Loss => -2,
                silman_tb::Wdl::BlessedLoss => -1,
                silman_tb::Wdl::Draw => 0,
                silman_tb::Wdl::CursedWin => 1,
                silman_tb::Wdl::Win => 2,
            },
            m.dtz,
        )),
        Err(_) => None,
    }
}

/// Whether a user-perspective tablebase score still satisfies the goal.
/// A blessed loss (-1) is a draw under the rules, so it holds a draw goal;
/// a cursed win (1) is NOT a win — the 50-move rule spoils it.
fn meets_goal(score: i8, goal: Goal) -> bool {
    match goal {
        Goal::Win => score >= 2,
        Goal::Draw => score >= -1,
    }
}

fn goal_word(goal: Goal) -> &'static str {
    match goal {
        Goal::Win => "win",
        Goal::Draw => "draw",
    }
}

fn score_word(score: i8) -> &'static str {
    match score {
        2 => "winning for you",
        1 => "only a 50-move-rule draw",
        0 => "drawn",
        -1 => "a 50-move-rule draw at best",
        _ => "lost for you",
    }
}

// ---------------------------------------------------------------------------
// Attempts and mastery (migration 0011)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DrillProgress {
    pub drill_id: String,
    pub attempts: i64,
    pub solved: i64,
    pub clean_streak: i64,
    pub mastered: bool,
}

/// Record one finished attempt and update the drill's mastery row. A solved
/// attempt extends the clean streak (mastery at [`MASTERY_STREAK`]); a
/// failed one resets it. `mastered_at`, once set, persists.
pub fn record_attempt(
    conn: &Connection,
    drill_id: &str,
    solved: bool,
    user_moves: u32,
    time_ms: i64,
    opponent: &str,
    verification: &str,
) -> anyhow::Result<DrillProgress> {
    anyhow::ensure!(
        drill(drill_id).is_some(),
        "unknown endgame drill {drill_id:?}"
    );
    conn.execute(
        "INSERT INTO endgame_attempts
             (drill_id, solved, user_moves, time_ms, opponent, verification)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            drill_id,
            solved as i64,
            user_moves as i64,
            time_ms,
            opponent,
            verification
        ],
    )?;
    let row: Option<(i64, i64, i64, Option<String>)> = conn
        .query_row(
            "SELECT attempts, solved, clean_streak, mastered_at
             FROM endgame_mastery WHERE drill_id = ?1",
            [drill_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .optional()?;
    let (attempts, solved_n, streak, mastered_at) = row.unwrap_or((0, 0, 0, None));
    let attempts = attempts + 1;
    let solved_n = solved_n + i64::from(solved);
    let streak = if solved { streak + 1 } else { 0 };
    let mastered = mastered_at.is_some() || streak >= MASTERY_STREAK;
    conn.execute(
        "INSERT INTO endgame_mastery (drill_id, attempts, solved, clean_streak, mastered_at)
         VALUES (?1, ?2, ?3, ?4, CASE WHEN ?5 THEN datetime('now') ELSE NULL END)
         ON CONFLICT(drill_id) DO UPDATE SET
             attempts = ?2, solved = ?3, clean_streak = ?4,
             mastered_at = COALESCE(mastered_at,
                                    CASE WHEN ?5 THEN datetime('now') ELSE NULL END)",
        params![drill_id, attempts, solved_n, streak, mastered],
    )?;
    Ok(DrillProgress {
        drill_id: drill_id.to_string(),
        attempts,
        solved: solved_n,
        clean_streak: streak,
        mastered,
    })
}

/// Progress rows for every drill the user has attempted.
pub fn progress_all(conn: &Connection) -> anyhow::Result<Vec<DrillProgress>> {
    let mut stmt = conn.prepare(
        "SELECT drill_id, attempts, solved, clean_streak, mastered_at IS NOT NULL
         FROM endgame_mastery ORDER BY drill_id",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(DrillProgress {
            drill_id: r.get(0)?,
            attempts: r.get(1)?,
            solved: r.get(2)?,
            clean_streak: r.get(3)?,
            mastered: r.get(4)?,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}
