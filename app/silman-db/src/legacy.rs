//! Legacy engine-analysis extraction (run-4 maintainer verdict 3b).
//!
//! Imported SCID/Fritz-annotated games carry engine evaluations inside
//! comments, in the shape `EngineName Version: depth:eval` (e.g.
//! `Stockfish 2.1.1 64bit: 20:+0.44`) or a bare `depth:eval`. Mainline
//! occurrences are lifted into structured `analyses` rows tagged
//! 'legacy-import'; the comment keeps any surrounding human text and is
//! dropped entirely when it was pure engine output. Variation-comment
//! evals are left as text (their ply is ambiguous). Unparseable comments
//! pass through untouched.

use crate::movebin::Token;

#[derive(Debug, Clone, PartialEq)]
pub struct LegacyEval {
    /// Mainline ply the eval refers to (the comment follows this move).
    pub ply: u32,
    /// Engine identity as written; "unknown (imported)" for bare evals.
    pub engine: String,
    pub depth: u32,
    pub eval_cp: i32,
}

/// Parse `depth:eval` (eval in pawns, signed decimal) from one token.
fn parse_depth_eval(token: &str) -> Option<(u32, i32)> {
    let (d, e) = token.split_once(':')?;
    let depth: u32 = d.parse().ok()?;
    if !(1..=99).contains(&depth) {
        return None;
    }
    let e = e.trim();
    if !(e.starts_with('+') || e.starts_with('-')) {
        return None;
    }
    let pawns: f64 = e.parse().ok()?;
    if pawns.abs() > 300.0 {
        return None;
    }
    Some((depth, (pawns * 100.0).round() as i32))
}

/// Split a comment into (remaining human text, engine, depth, eval) if it
/// ends with an engine-eval pattern.
fn parse_engine_comment(text: &str) -> Option<(String, String, u32, i32)> {
    let mut tokens: Vec<&str> = text.split_whitespace().collect();
    let last = tokens.pop()?;
    let (depth, eval_cp) = parse_depth_eval(last)?;

    // Engine name: the longest suffix of preceding tokens ending with ':'
    // that looks like an identity string — stop at tokens that are
    // percentages, SAN-ish, or after 6 tokens.
    let mut name_tokens: Vec<&str> = Vec::new();
    if tokens.last().is_some_and(|t| t.ends_with(':')) {
        for t in tokens.iter().rev() {
            if name_tokens.len() >= 6 || t.ends_with('%') || t.chars().all(|c| c.is_ascii_digit()) {
                break;
            }
            name_tokens.push(t);
            // An engine name plausibly starts at a capitalized word.
            if t.chars().next().is_some_and(|c| c.is_ascii_uppercase()) && name_tokens.len() >= 2 {
                break;
            }
        }
    }
    let engine = if name_tokens.is_empty() {
        "unknown (imported)".to_string()
    } else {
        name_tokens.reverse();
        tokens.truncate(tokens.len() - name_tokens.len());
        name_tokens.join(" ").trim_end_matches(':').to_string()
    };
    Some((tokens.join(" "), engine, depth, eval_cp))
}

/// Walk a token stream, extracting mainline engine-comment evals.
/// Returns the cleaned stream and the structured records.
pub fn extract_legacy_evals(tokens: Vec<Token>) -> (Vec<Token>, Vec<LegacyEval>) {
    let mut out = Vec::with_capacity(tokens.len());
    let mut evals = Vec::new();
    let mut depth = 0u32;
    let mut ply = 0u32;
    for t in tokens {
        match &t {
            Token::VarStart => {
                depth += 1;
                out.push(t);
            }
            Token::VarEnd => {
                depth = depth.saturating_sub(1);
                out.push(t);
            }
            Token::Move(_) | Token::Null if depth == 0 => {
                ply += 1;
                out.push(t);
            }
            Token::Comment(text) if depth == 0 && ply > 0 => match parse_engine_comment(text) {
                Some((rest, engine, d, eval_cp)) => {
                    evals.push(LegacyEval {
                        ply,
                        engine,
                        depth: d,
                        eval_cp,
                    });
                    let rest = rest.trim();
                    if !rest.is_empty() {
                        out.push(Token::Comment(rest.to_string()));
                    }
                }
                None => out.push(t),
            },
            _ => out.push(t),
        }
    }
    (out, evals)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_maintainers_exact_comments() {
        // Pure engine comment.
        let (rest, engine, depth, cp) =
            parse_engine_comment("Stockfish 2.1.1 64bit: 20:+0.44").unwrap();
        assert_eq!(rest, "");
        assert_eq!(engine, "Stockfish 2.1.1 64bit");
        assert_eq!((depth, cp), (20, 44));

        // Mixed book-exit + engine tail.
        let (rest, engine, depth, cp) =
            parse_engine_comment("Move out of book Nf6 82% g6 18% Stockfish 2.1.1 64bit: 20:+0.84")
                .unwrap();
        assert_eq!(rest, "Move out of book Nf6 82% g6 18%");
        assert_eq!(engine, "Stockfish 2.1.1 64bit");
        assert_eq!((depth, cp), (20, 84));

        // Bare depth:eval (variation style).
        let (rest, engine, depth, cp) = parse_engine_comment("19:+4.04").unwrap();
        assert_eq!(rest, "");
        assert_eq!(engine, "unknown (imported)");
        assert_eq!((depth, cp), (19, 404));

        // Negative eval.
        let (_, _, _, cp) = parse_engine_comment("Rybka 3: 14:-1.20").unwrap();
        assert_eq!(cp, -120);

        // Plain human comments pass through.
        assert!(parse_engine_comment("Last book move").is_none());
        assert!(parse_engine_comment("wins the queen: 20 points").is_none());
        assert!(parse_engine_comment("time 5:30").is_none());
    }
}
