//! Streaming, malformed-input-tolerant PGN reader.
//!
//! Yields one raw game at a time from any `BufRead` without loading the file
//! into memory. Tag pairs are parsed; movetext is tokenized into a full
//! [`PgnToken`] stream — SAN moves, nulls, NAGs (including `!?`-style
//! suffixes), brace/semicolon comments, and nested variations — in PGN text
//! order. A malformed game yields an error item and the reader
//! resynchronizes at the next tag section, so one bad game never aborts an
//! import.

use std::io::BufRead;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GameResult {
    #[default]
    Unknown,
    WhiteWins,
    BlackWins,
    Draw,
}

impl GameResult {
    /// Database encoding: 0 `*`, 1 `1-0`, 2 `0-1`, 3 `1/2-1/2`.
    pub fn as_u8(self) -> u8 {
        match self {
            GameResult::Unknown => 0,
            GameResult::WhiteWins => 1,
            GameResult::BlackWins => 2,
            GameResult::Draw => 3,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            GameResult::Unknown => "*",
            GameResult::WhiteWins => "1-0",
            GameResult::BlackWins => "0-1",
            GameResult::Draw => "1/2-1/2",
        }
    }
}

/// One movetext token, in PGN text order, variations included.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PgnToken {
    San(String),
    Null,
    Nag(u8),
    Comment(String),
    VarStart,
    VarEnd,
}

#[derive(Debug, Default)]
pub struct RawGame {
    pub tags: Vec<(String, String)>,
    /// Full movetext token stream (annotations and variations included).
    pub tokens: Vec<PgnToken>,
    pub result: GameResult,
    /// 1-based line number where this game's tag section started.
    pub start_line: u64,
}

impl RawGame {
    pub fn tag(&self, name: &str) -> Option<&str> {
        self.tags
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }

    /// Mainline SAN strings (nulls rendered as `--`), variations excluded.
    pub fn mainline_sans(&self) -> Vec<String> {
        let mut depth = 0u32;
        let mut out = Vec::new();
        for t in &self.tokens {
            match t {
                PgnToken::VarStart => depth += 1,
                PgnToken::VarEnd => depth = depth.saturating_sub(1),
                PgnToken::San(s) if depth == 0 => out.push(s.clone()),
                PgnToken::Null if depth == 0 => out.push("--".to_string()),
                _ => {}
            }
        }
        out
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PgnError {
    #[error("I/O error reading PGN: {0}")]
    Io(#[from] std::io::Error),
    #[error("line {line}: {msg}")]
    Malformed { line: u64, msg: String },
}

pub struct PgnReader<R: BufRead> {
    reader: R,
    line_no: u64,
    /// A tag line consumed while scanning for the end of the previous game.
    pending_line: Option<String>,
    eof: bool,
}

impl<R: BufRead> PgnReader<R> {
    pub fn new(reader: R) -> Self {
        Self {
            reader,
            line_no: 0,
            pending_line: None,
            eof: false,
        }
    }

    fn next_line(&mut self) -> Result<Option<String>, PgnError> {
        if let Some(l) = self.pending_line.take() {
            return Ok(Some(l));
        }
        // PGN's canonical encoding is Latin-1, and real-world files (TWIC,
        // old databases) contain it; decode UTF-8 when valid, else fall
        // back to Latin-1 so no byte sequence can fail.
        let mut buf = Vec::new();
        let n = self.reader.read_until(b'\n', &mut buf)?;
        if n == 0 {
            self.eof = true;
            return Ok(None);
        }
        self.line_no += 1;
        let line = match String::from_utf8(buf) {
            Ok(s) => s,
            Err(e) => e.into_bytes().iter().map(|&b| b as char).collect(),
        };
        Ok(Some(line))
    }

    fn parse_tag_line(line: &str, line_no: u64) -> Result<(String, String), PgnError> {
        // [Key "Value"]
        let inner = line
            .trim()
            .strip_prefix('[')
            .and_then(|s| s.strip_suffix(']'))
            .ok_or_else(|| PgnError::Malformed {
                line: line_no,
                msg: format!("bad tag line: {}", line.trim()),
            })?;
        let (key, rest) =
            inner
                .split_once(char::is_whitespace)
                .ok_or_else(|| PgnError::Malformed {
                    line: line_no,
                    msg: format!("tag without value: {}", line.trim()),
                })?;
        let value = rest
            .trim()
            .strip_prefix('"')
            .and_then(|s| s.strip_suffix('"'))
            .ok_or_else(|| PgnError::Malformed {
                line: line_no,
                msg: format!("tag value not quoted: {}", line.trim()),
            })?;
        Ok((
            key.to_string(),
            value.replace("\\\"", "\"").replace("\\\\", "\\"),
        ))
    }
}

/// Map PGN suffix annotations to their standard NAG values.
fn suffix_nag(suffix: &str) -> Option<u8> {
    match suffix {
        "!" => Some(1),
        "?" => Some(2),
        "!!" => Some(3),
        "??" => Some(4),
        "!?" => Some(5),
        "?!" => Some(6),
        _ => None,
    }
}

/// Tokenize a movetext fragment into `tokens`. `depth` tracks variation
/// nesting and `comment` an in-progress brace comment, both across line
/// boundaries. Returns the game result if a terminator was seen.
fn tokenize_movetext(
    line: &str,
    tokens: &mut Vec<PgnToken>,
    depth: &mut u32,
    comment: &mut Option<String>,
) -> Option<GameResult> {
    let mut token = String::new();
    let mut result = None;
    for c in line.chars() {
        if let Some(buf) = comment {
            if c == '}' {
                tokens.push(PgnToken::Comment(std::mem::take(buf).trim().to_string()));
                *comment = None;
            } else {
                buf.push(c);
            }
            continue;
        }
        match c {
            '{' => {
                flush_token(&mut token, tokens, &mut result);
                *comment = Some(String::new());
            }
            '(' => {
                flush_token(&mut token, tokens, &mut result);
                *depth += 1;
                tokens.push(PgnToken::VarStart);
            }
            ')' => {
                flush_token(&mut token, tokens, &mut result);
                if *depth > 0 {
                    *depth -= 1;
                    tokens.push(PgnToken::VarEnd);
                }
            }
            ';' => {
                // Rest-of-line comment.
                flush_token(&mut token, tokens, &mut result);
                let text = line.split_once(';').map(|(_, r)| r.trim()).unwrap_or("");
                if !text.is_empty() {
                    tokens.push(PgnToken::Comment(text.to_string()));
                }
                break;
            }
            c if c.is_whitespace() => flush_token(&mut token, tokens, &mut result),
            _ => token.push(c),
        }
        if result.is_some() {
            return result;
        }
    }
    flush_token(&mut token, tokens, &mut result);
    // A comment still open at line end continues on the next line.
    if let Some(buf) = comment {
        buf.push(' ');
    }
    result
}

fn flush_token(token: &mut String, tokens: &mut Vec<PgnToken>, result: &mut Option<GameResult>) {
    if token.is_empty() {
        return;
    }
    let t = std::mem::take(token);
    match t.as_str() {
        "1-0" => *result = Some(GameResult::WhiteWins),
        "0-1" => *result = Some(GameResult::BlackWins),
        "1/2-1/2" => *result = Some(GameResult::Draw),
        "*" => *result = Some(GameResult::Unknown),
        _ => {
            if let Some(rest) = t.strip_prefix('$') {
                if let Ok(n) = rest.parse::<u16>() {
                    tokens.push(PgnToken::Nag(n.min(255) as u8));
                }
                return;
            }
            // Strip a leading move number ("1." / "23..." / bare "12").
            let stripped = t.trim_start_matches(|c: char| c.is_ascii_digit());
            let stripped = stripped.trim_start_matches('.');
            if stripped.is_empty() {
                return; // pure move number token
            }
            // Peel a suffix annotation ("Ne4!?" → San("Ne4") + Nag(5)).
            let trimmed = stripped.trim_end_matches(['!', '?']);
            let suffix = &stripped[trimmed.len()..];
            if trimmed.is_empty() {
                return;
            }
            if trimmed == "--" || trimmed == "Z0" {
                tokens.push(PgnToken::Null);
            } else {
                tokens.push(PgnToken::San(trimmed.to_string()));
            }
            if let Some(nag) = suffix_nag(suffix) {
                tokens.push(PgnToken::Nag(nag));
            }
        }
    }
}

impl<R: BufRead> Iterator for PgnReader<R> {
    type Item = Result<RawGame, PgnError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.eof {
            return None;
        }
        let mut game = RawGame::default();
        let mut seen_tags = false;
        let mut seen_moves = false;
        let mut depth = 0u32;
        let mut comment: Option<String> = None;

        loop {
            let line = match self.next_line() {
                Ok(Some(l)) => l,
                Ok(None) => break,
                Err(e) => return Some(Err(e)),
            };
            let trimmed = line.trim();
            if trimmed.starts_with('%') {
                continue; // escape line
            }
            if trimmed.is_empty() {
                if seen_moves {
                    break; // blank line after movetext = end of game
                }
                continue;
            }
            if trimmed.starts_with('[') && comment.is_none() && depth == 0 && !seen_moves {
                if game.start_line == 0 {
                    game.start_line = self.line_no;
                }
                seen_tags = true;
                match Self::parse_tag_line(trimmed, self.line_no) {
                    Ok(kv) => game.tags.push(kv),
                    Err(e) => return Some(Err(e)),
                }
                continue;
            }
            if trimmed.starts_with('[') && comment.is_none() && depth == 0 && seen_moves {
                // Next game's tag section with no blank line between games.
                self.pending_line = Some(line);
                self.line_no -= 0;
                break;
            }
            // Movetext.
            if game.start_line == 0 {
                game.start_line = self.line_no;
            }
            seen_moves = true;
            if let Some(res) =
                tokenize_movetext(trimmed, &mut game.tokens, &mut depth, &mut comment)
            {
                game.result = res;
                break;
            }
        }

        if !seen_tags && !seen_moves {
            return None; // trailing whitespace at EOF
        }
        Some(Ok(game))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    const TWO_GAMES: &str = r#"[Event "Test A"]
[White "Alice"]
[Black "Bob"]
[Result "1-0"]

1. e4 e5 2. Nf3 {a comment
spanning lines} Nc6 3. Bb5 (3. Bc4 Bc5 (3... Nf6)) 3... a6 $1 1-0

[Event "Test B"]
[White "Carol"]
[Black "Dave"]
[Result "1/2-1/2"]

1. d4 d5 ; rest of line ignored
2. c4 1/2-1/2
"#;

    #[test]
    fn reads_two_games_with_comments_variations_nags() {
        let games: Vec<_> = PgnReader::new(Cursor::new(TWO_GAMES))
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(games.len(), 2);
        assert_eq!(games[0].tag("White"), Some("Alice"));
        assert_eq!(
            games[0].mainline_sans(),
            vec!["e4", "e5", "Nf3", "Nc6", "Bb5", "a6"],
            "mainline excludes variations and annotations"
        );
        assert_eq!(games[0].result, GameResult::WhiteWins);
        assert_eq!(games[1].mainline_sans(), vec!["d4", "d5", "c4"]);
        assert_eq!(games[1].result, GameResult::Draw);

        // Full token stream: the multi-line comment is captured, the nested
        // variation structure is preserved, and $1 became a NAG token.
        let toks = &games[0].tokens;
        assert!(toks
            .iter()
            .any(|t| matches!(t, PgnToken::Comment(c) if c.contains("spanning lines"))));
        assert_eq!(
            toks.iter()
                .filter(|t| matches!(t, PgnToken::VarStart))
                .count(),
            2,
            "nested variation preserved"
        );
        assert!(toks.contains(&PgnToken::Nag(1)));
        // Semicolon comment in game 2.
        assert!(games[1]
            .tokens
            .contains(&PgnToken::Comment("rest of line ignored".into())));
    }

    #[test]
    fn suffix_annotations_become_nags() {
        let text = "[White \"A\"]\n\n1. e4!? e5?? 2. Nf3! *\n";
        let games: Vec<_> = PgnReader::new(Cursor::new(text))
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(
            games[0].tokens,
            vec![
                PgnToken::San("e4".into()),
                PgnToken::Nag(5),
                PgnToken::San("e5".into()),
                PgnToken::Nag(4),
                PgnToken::San("Nf3".into()),
                PgnToken::Nag(1),
            ]
        );
    }

    #[test]
    fn resynchronizes_when_games_lack_blank_separator() {
        let text = "[White \"A\"]\n\n1. e4 * \n[White \"B\"]\n\n1. d4 *\n";
        let games: Vec<_> = PgnReader::new(Cursor::new(text))
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(games.len(), 2);
        assert_eq!(games[0].tag("White"), Some("A"));
        assert_eq!(games[1].tag("White"), Some("B"));
    }

    #[test]
    fn malformed_tag_line_is_an_error_item() {
        let text = "[White Alice]\n\n1. e4 *\n";
        let items: Vec<_> = PgnReader::new(Cursor::new(text)).collect();
        assert!(items[0].is_err());
    }
}
