//! LLM verbalizer (feature `llm`, Phase 4): prompt construction and
//! post-validation for an LLM-backed [`Verbalizer`], with a hard fallback to
//! template mode (docs/SILMAN_ENGINE_SPEC.md, "silman-verbalize", LLM mode).
//!
//! This module has ZERO network capability. The HTTP client lives in the GPL
//! app layer and plugs in through the [`LlmTransport`] trait; everything here
//! (prompt text, JSON serialization, output validation, fallback policy) is
//! pure and fully testable offline. The transport is provider-agnostic by
//! construction: it receives a system string and a user string and returns
//! the model's text, nothing more.
//!
//! # Post-validation, precisely
//!
//! The spec's hard requirement is that LLM output may never mention a chess
//! fact absent from the record. The validator implements it as follows:
//!
//! 1. The output is split into tokens on every character outside the SAN
//!    alphabet (`a-h`, `1-8`, `K Q R B N O 0 x = + # -`). Squares and SAN
//!    moves consist solely of alphabet characters, so neither can straddle a
//!    token boundary. Leading/trailing hyphens (prose like "h3-pawn") are
//!    stripped from each token; internal hyphens (castling) are kept.
//! 2. Each token is classified as **SAN-looking** or **other**. SAN-looking
//!    means, after stripping trailing `+ # ` and mapping `0` to `O`: castling
//!    (`O-O`, `O-O-O`); a piece move `[KQRBN][a-h]?[1-8]?x?[a-h][1-8]`; a
//!    pawn capture `[a-h]x[a-h][1-8]`; any of those or a bare destination
//!    with a promotion suffix `=[QRBN]`; or a bare `x[a-h][1-8]`. A bare
//!    two-character square ("e4") is deliberately **not** SAN-looking — it is
//!    validated by the square rule below, which also covers a hallucinated
//!    bare-square move recommendation.
//! 3. **Move rule.** Every SAN-looking token must either appear verbatim in
//!    the record's serialized JSON (alert PVs, engine best/multipv, plan
//!    hints — checked both with and without its trailing `+`/`#`), or be
//!    legal in the record's FEN for the side to move. Legality is checked by
//!    generating all legal moves with cozy-chess (re-exported by
//!    silman-core) and rendering each as the set of SAN spellings this
//!    module accepts: every disambiguation variant (none, file, rank, both)
//!    is admitted, check/mate markers are ignored rather than verified, and
//!    castling is recognized from cozy-chess's king-onto-rook encoding.
//!    Moves for the side *not* to move are never generated, so an opponent
//!    reply that is not verbatim in the record is rejected. If the FEN does
//!    not parse, the legal set is empty and only verbatim matches survive.
//! 4. **Square rule.** Every `[a-h][1-8]` pair inside every non-SAN token
//!    must appear as a substring of the record's serialized JSON with the
//!    `fen` field blanked (the same set-inclusion trick as the template-mode
//!    no-invention test; the FEN is excluded so piece-placement text cannot
//!    launder an invented square). Squares inside a SAN-looking token are
//!    covered by the move rule instead — otherwise the legality alternative
//!    the spec grants to moves would be vacuous.
//! 5. Any violation — or a transport error, or empty output — falls back to
//!    the full template-mode rendering. The fallback is always total, never
//!    partial, and the outcome records which rule fired.
//!
//! By design the validator over-rejects (a prose fragment that happens to
//! parse as SAN, an unusual spelling, a legal move phrased for the wrong
//! side): every doubtful case degrades to the deterministic template prose,
//! never to unvalidated LLM prose.

use std::collections::BTreeSet;
use std::fmt;

use silman_core::record::FeatureRecord;

use crate::{verbalize_voiced, Verbalizer, Voice};

/// The system prompt sent with every request. A `const` so that prompt
/// construction is deterministic and snapshot-testable.
pub const SYSTEM_PROMPT: &str = "\
You are a chess coach explaining a single position to a student.

The user message is one JSON document (a silman FeatureRecord) describing \
everything that is known about the position: tactical alerts from a static \
screen, positional imbalances with structured evidence, plan hints, and \
optional engine data.

Hard grounding rules — follow them exactly:
1. Verbalize ONLY facts present in the supplied FeatureRecord JSON.
2. Never invent moves, squares, evaluations, piece placements, or threats \
that are not in the JSON, and never add claims from outside chess knowledge.
3. Every square you mention must appear in the JSON. Every move you mention \
must appear verbatim in the JSON (an engine line, a best move, or a plan).
4. Do not mention the JSON, its field names, or that you were given data; \
write plain coaching prose.

Output format: write up to three short coach-style paragraphs, separated by \
blank lines, in this order:
1. Tactics — the tactical alerts, most severe first. Omit this paragraph \
entirely if the JSON lists no alerts.
2. Imbalances — the positional imbalances and their evidence, strongest \
first. Omit if the JSON lists no imbalances.
3. Plans — the plan hints, phrased as advice. Omit if the JSON lists no \
plans.
Write nothing else: no headings, no lists, no preamble.";

/// Per-voice style addendum appended to [`SYSTEM_PROMPT`] (run-5 item 3).
/// The Coach voice mirrors the template overlay in `templates/coach.tmpl`;
/// both remind the model that style may never add facts.
pub fn voice_prompt(voice: Voice) -> &'static str {
    match voice {
        Voice::Coach => {
            "Style — the coaching voice: write like a Silman-school coach. \
Pieces have desires and grievances (a knight dreams of a permanent home on \
an outpost; a bad bishop bites on granite behind its own pawns). You may \
address the student directly, but sparingly. Stay vivid and concrete, never \
cute for its own sake — and style never adds facts: every square and move \
you mention must still come from the JSON."
        }
        Voice::Neutral => {
            "Style — the neutral voice: keep the tone plain and factual. Do \
not anthropomorphize the pieces; describe the position without flourish."
        }
    }
}

/// A transport-level failure (network, HTTP, provider error, bad payload).
/// The verbalizer maps any transport error to a template fallback.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransportError {
    pub message: String,
}

impl TransportError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for TransportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for TransportError {}

/// The one seam to the outside world. Implementations live in the app layer
/// (GPL) and carry the actual HTTP client and credentials; this crate never
/// gains a network dependency.
pub trait LlmTransport {
    /// Send one completion request and return the model's text output.
    fn complete(&self, system: &str, user: &str) -> Result<String, TransportError>;
}

/// Why the template fallback fired (for the UI/CLI to surface).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FallbackReason {
    /// The transport failed (network, HTTP status, provider refusal...).
    Transport(TransportError),
    /// The transport returned an empty (or whitespace-only) completion.
    EmptyOutput,
    /// The output mentions a square that is not in the record.
    UngroundedSquare(String),
    /// The output mentions a move that is neither in the record nor legal
    /// in the record's position.
    UngroundedMove(String),
}

impl fmt::Display for FallbackReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FallbackReason::Transport(e) => write!(f, "transport error: {e}"),
            FallbackReason::EmptyOutput => write!(f, "empty LLM output"),
            FallbackReason::UngroundedSquare(sq) => {
                write!(f, "output mentions square {sq} absent from the record")
            }
            FallbackReason::UngroundedMove(mv) => write!(
                f,
                "output mentions move {mv} neither in the record nor legal in the position"
            ),
        }
    }
}

/// Which mode produced the returned prose.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VerbalizeMode {
    /// The LLM output passed post-validation and was used verbatim.
    Llm,
    /// Template-mode output was substituted, for the recorded reason.
    TemplateFallback(FallbackReason),
}

/// The prose plus the mode that produced it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LlmVerbalization {
    pub text: String,
    pub mode: VerbalizeMode,
}

/// Deterministic prompt construction in the default (Coach) voice: the
/// fixed system prompt plus the record serialized as pretty JSON
/// (`BTreeMap` evidence keys keep the serialization stable).
/// Snapshot-tested.
pub fn build_prompt(record: &FeatureRecord) -> (String, String) {
    build_prompt_voiced(record, Voice::default())
}

/// [`build_prompt`] with an explicit [`Voice`]: the system prompt carries
/// the matching style addendum from [`voice_prompt`].
pub fn build_prompt_voiced(record: &FeatureRecord, voice: Voice) -> (String, String) {
    let user = serde_json::to_string_pretty(record).expect("FeatureRecord serializes to JSON");
    (format!("{SYSTEM_PROMPT}\n\n{}", voice_prompt(voice)), user)
}

/// LLM-backed [`Verbalizer`], generic over the transport. Output is used
/// only when it passes post-validation; otherwise the template rendering —
/// in the SAME voice the prompt requested — is returned in full.
pub struct LlmVerbalizer<T: LlmTransport> {
    transport: T,
    voice: Voice,
}

impl<T: LlmTransport> LlmVerbalizer<T> {
    /// A verbalizer in the default (Coach) voice.
    pub fn new(transport: T) -> Self {
        Self::with_voice(transport, Voice::default())
    }

    /// A verbalizer with an explicit [`Voice`], used for both the prompt's
    /// style addendum and the template fallback.
    pub fn with_voice(transport: T, voice: Voice) -> Self {
        Self { transport, voice }
    }

    /// Verbalize and report which mode produced the prose.
    pub fn verbalize_checked(&self, record: &FeatureRecord) -> LlmVerbalization {
        let (system, user) = build_prompt_voiced(record, self.voice);
        let fallback = |reason: FallbackReason| LlmVerbalization {
            text: verbalize_voiced(record, self.voice),
            mode: VerbalizeMode::TemplateFallback(reason),
        };
        match self.transport.complete(&system, &user) {
            Err(error) => fallback(FallbackReason::Transport(error)),
            Ok(text) => {
                let text = text.trim();
                if text.is_empty() {
                    return fallback(FallbackReason::EmptyOutput);
                }
                match validate(text, record) {
                    Ok(()) => LlmVerbalization {
                        text: text.to_string(),
                        mode: VerbalizeMode::Llm,
                    },
                    Err(reason) => fallback(reason),
                }
            }
        }
    }
}

impl<T: LlmTransport> Verbalizer for LlmVerbalizer<T> {
    fn verbalize(&self, record: &FeatureRecord) -> String {
        self.verbalize_checked(record).text
    }
}

/// Validate LLM output against the record per the module-level rules.
pub fn validate(text: &str, record: &FeatureRecord) -> Result<(), FallbackReason> {
    let grounding = grounding_json(record);
    let legal = legal_san_set(&record.fen);

    for raw in text.split(|c: char| !is_san_char(c)) {
        let token = raw.trim_matches('-');
        if token.is_empty() {
            continue;
        }
        let stripped = normalize_san(token);
        if is_san_looking(&stripped) {
            let verbatim = grounding.contains(token) || grounding.contains(stripped.as_str());
            if !verbatim && !legal.contains(stripped.as_str()) {
                return Err(FallbackReason::UngroundedMove(token.to_string()));
            }
        } else {
            for square in embedded_squares(token) {
                if !grounding.contains(&square) {
                    return Err(FallbackReason::UngroundedSquare(square));
                }
            }
        }
    }
    Ok(())
}

/// The record serialized as JSON with the FEN blanked: the haystack for the
/// set-inclusion checks. The FEN is excluded so its piece-placement text
/// cannot accidentally ground a square the record never talks about.
fn grounding_json(record: &FeatureRecord) -> String {
    let mut copy = record.clone();
    copy.fen = String::new();
    serde_json::to_string(&copy).expect("FeatureRecord serializes to JSON")
}

/// Characters a square or SAN move can consist of. Tokenization splits on
/// everything else.
fn is_san_char(c: char) -> bool {
    matches!(
        c,
        'a'..='h' | '1'..='8' | 'K' | 'Q' | 'R' | 'B' | 'N' | 'O' | '0' | 'x' | '=' | '+' | '#' | '-'
    )
}

fn is_file(c: char) -> bool {
    ('a'..='h').contains(&c)
}

fn is_rank(c: char) -> bool {
    ('1'..='8').contains(&c)
}

fn is_piece_letter(c: char) -> bool {
    matches!(c, 'K' | 'Q' | 'R' | 'B' | 'N')
}

/// Strip trailing check/mate markers and map `0`-style castling to `O`.
fn normalize_san(token: &str) -> String {
    token
        .trim_end_matches(['+', '#'])
        .chars()
        .map(|c| if c == '0' { 'O' } else { c })
        .collect()
}

/// Whether a normalized token is a SAN-looking move (see module docs). A
/// bare two-character square is NOT SAN-looking.
fn is_san_looking(token: &str) -> bool {
    if token == "O-O" || token == "O-O-O" {
        return true;
    }
    let chars: Vec<char> = token.chars().collect();
    // Optional promotion suffix "=Q".
    let (body, has_promo) = match chars.as_slice() {
        [rest @ .., '=', promo] if is_piece_letter(*promo) => (rest, true),
        _ => (chars.as_slice(), false),
    };
    // The body must end with a destination square.
    let [prefix @ .., file, rank] = body else {
        return false;
    };
    if !is_file(*file) || !is_rank(*rank) {
        return false;
    }
    match prefix {
        // Bare square: promotion makes it a pawn move ("e8=Q"); otherwise it
        // is validated by the square rule, not the move rule.
        [] => has_promo,
        // "xd5" (conservatively treated as a move; it will only pass if the
        // record or the position backs it).
        ['x'] => true,
        // Pawn capture "exd5".
        [f, 'x'] if is_file(*f) => true,
        // Piece move with optional disambiguation and capture:
        // [KQRBN][a-h]?[1-8]?x?
        [piece, rest @ ..] if is_piece_letter(*piece) => {
            let rest = match rest {
                [f, more @ ..] if is_file(*f) => more,
                other => other,
            };
            let rest = match rest {
                [r, more @ ..] if is_rank(*r) => more,
                other => other,
            };
            matches!(rest, [] | ['x'])
        }
        _ => false,
    }
}

/// Every `[a-h][1-8]` pair inside a (non-SAN) token.
fn embedded_squares(token: &str) -> Vec<String> {
    let chars: Vec<char> = token.chars().collect();
    chars
        .windows(2)
        .filter(|w| is_file(w[0]) && is_rank(w[1]))
        .map(|w| w.iter().collect())
        .collect()
}

/// All SAN spellings this validator accepts for the legal moves of the side
/// to move in `fen` (every disambiguation variant; no check markers).
/// Returns the empty set when the FEN does not parse — verbatim record
/// matches are then the only way a move can pass.
fn legal_san_set(fen: &str) -> BTreeSet<String> {
    use silman_core::cozy_chess::{Board, Piece};

    let Ok(board) = fen.parse::<Board>() else {
        return BTreeSet::new();
    };
    let stm = board.side_to_move();
    let mut moves = Vec::new();
    board.generate_moves(|piece_moves| {
        moves.extend(piece_moves);
        false
    });

    let mut set = BTreeSet::new();
    for mv in moves {
        let Some(piece) = board.piece_on(mv.from) else {
            continue;
        };
        let from = mv.from.to_string();
        let to = mv.to.to_string();
        // cozy-chess encodes castling as the king moving onto its own rook.
        if piece == Piece::King
            && board.color_on(mv.to) == Some(stm)
            && board.piece_on(mv.to) == Some(Piece::Rook)
        {
            let short = to.as_bytes()[0] > from.as_bytes()[0];
            set.insert(if short { "O-O" } else { "O-O-O" }.to_string());
            continue;
        }
        let capture = board.color_on(mv.to).is_some_and(|c| c != stm)
            || (piece == Piece::Pawn && from.as_bytes()[0] != to.as_bytes()[0]);
        if piece == Piece::Pawn {
            let mut san = if capture {
                format!("{}x{to}", &from[..1])
            } else {
                to.clone()
            };
            if let Some(promo) = mv.promotion {
                san.push('=');
                san.push(piece_letter(promo));
            }
            set.insert(san);
        } else {
            let letter = piece_letter(piece);
            let x = if capture { "x" } else { "" };
            let (file, rank) = (&from[..1], &from[1..2]);
            for disambiguation in ["".to_string(), file.into(), rank.into(), from.clone()] {
                set.insert(format!("{letter}{disambiguation}{x}{to}"));
            }
        }
    }
    set
}

fn piece_letter(piece: silman_core::cozy_chess::Piece) -> char {
    use silman_core::cozy_chess::Piece;
    match piece {
        Piece::Pawn => 'P',
        Piece::Knight => 'N',
        Piece::Bishop => 'B',
        Piece::Rook => 'R',
        Piece::Queen => 'Q',
        Piece::King => 'K',
    }
}
