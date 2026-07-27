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
            // Human text before the engine name starts lowercase; the
            // token adjacent to depth:eval (e.g. "64bit:") may too.
            if !name_tokens.is_empty()
                && (t.chars().next().is_some_and(|c| c.is_ascii_lowercase())
                    || t.contains(':')
                    || t.starts_with('+')
                    || t.starts_with('-'))
            {
                break;
            }
            name_tokens.push(t);
        }
    }
    // Engine names start with a capitalized word (Stockfish, Toga,
    // Rybka...): trim leading junk (SCID blunder markers like
    // "****D9 2.9->9.7") off the collected window.
    name_tokens.reverse();
    while let Some(first) = name_tokens.first() {
        if first.chars().next().is_some_and(|c| c.is_ascii_uppercase()) {
            break;
        }
        name_tokens.remove(0);
    }
    let engine = if name_tokens.is_empty() {
        "unknown (imported)".to_string()
    } else {
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
            Token::Comment(text) if depth == 0 && ply > 0 => {
                // Comments may contain SEVERAL stacked engine evals
                // (re-annotated games); peel them off right to left.
                let mut remainder = text.clone();
                let mut found = Vec::new();
                while let Some((rest, engine, d, eval_cp)) = parse_engine_comment(&remainder) {
                    found.push(LegacyEval {
                        ply,
                        engine,
                        depth: d,
                        eval_cp,
                    });
                    remainder = rest;
                }
                if found.is_empty() {
                    out.push(t);
                } else {
                    evals.extend(found.into_iter().rev());
                    let rest = remainder.trim();
                    if !rest.is_empty() {
                        out.push(Token::Comment(rest.to_string()));
                    }
                }
            }
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

        // Multi-word capitalized engine names survive intact (observed:
        // "Toga II 1.2.1a" in the maintainer's database).
        let (rest, engine, _, _) = parse_engine_comment("Toga II 1.2.1a: 12:+0.15").unwrap();
        assert_eq!(engine, "Toga II 1.2.1a");
        assert_eq!(rest, "");
        let (rest, engine, _, _) =
            parse_engine_comment("Last book move Stockfish 2.1.1 64bit: 20:+0.84").unwrap();
        assert_eq!(engine, "Stockfish 2.1.1 64bit");
        assert_eq!(rest, "Last book move");

        // Stacked double annotations peel correctly (observed in real
        // re-annotated games).
        let (rest, engine, depth, cp) =
            parse_engine_comment("Stockfish 2.0.1: 25:+1.53 Stockfish 2.0.1: 24:-5.73").unwrap();
        assert_eq!(engine, "Stockfish 2.0.1");
        assert_eq!((depth, cp), (24, -573));
        assert_eq!(rest, "Stockfish 2.0.1: 25:+1.53");
        let (rest2, engine2, depth2, cp2) = parse_engine_comment(&rest).unwrap();
        assert_eq!(engine2, "Stockfish 2.0.1");
        assert_eq!((depth2, cp2), (25, 153));
        assert_eq!(rest2, "");

        // Plain human comments pass through.
        assert!(parse_engine_comment("Last book move").is_none());
        assert!(parse_engine_comment("wins the queen: 20 points").is_none());
        assert!(parse_engine_comment("time 5:30").is_none());
    }
}
