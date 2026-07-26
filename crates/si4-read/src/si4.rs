//! .si4 index file: 182-byte header + 47-byte per-game entries.
//!
//! Layout per docs/SI4_FORMAT_NOTES.md §1 (community si4spec; big-endian).

use crate::Si4Error;

pub const SI4_MAGIC: &[u8; 8] = b"Scid.si\0";
pub const HEADER_LEN: usize = 182;
pub const ENTRY_LEN: usize = 47;

#[derive(Debug, Clone)]
pub struct Si4Header {
    pub version: u16,
    pub db_type: u32,
    pub num_games: u32,
    pub auto_load: u32,
    pub description: String,
    pub custom_flag_names: [String; 6],
}

/// (year, month, day); zero components mean "unknown".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DateTriple {
    pub year: u16,
    pub month: u8,
    pub day: u8,
}

impl std::fmt::Display for DateTriple {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let y = if self.year == 0 {
            "????".to_string()
        } else {
            format!("{:04}", self.year)
        };
        let m = if self.month == 0 {
            "??".to_string()
        } else {
            format!("{:02}", self.month)
        };
        let d = if self.day == 0 {
            "??".to_string()
        } else {
            format!("{:02}", self.day)
        };
        write!(f, "{y}.{m}.{d}")
    }
}

#[derive(Debug, Clone)]
pub struct IndexEntry {
    /// Byte offset of the game record in the .sg4 file.
    pub game_offset: u32,
    /// Game record length (17 bits).
    pub game_length: u32,
    /// User-defined flag bits 5–0 from the length-high byte.
    pub custom_flags: u8,
    /// Standard flag word (delete-mark, promotion, ... per notes §1).
    pub flags: u16,
    pub white_id: u32,
    pub black_id: u32,
    pub event_id: u32,
    pub site_id: u32,
    pub round_id: u32,
    /// 0 `*`, 1 `1-0`, 2 `0-1`, 3 `1/2-1/2`.
    pub result: u8,
    pub nag_count_code: u8,
    pub comment_count_code: u8,
    pub variation_count_code: u8,
    pub eco: u16,
    pub date: DateTriple,
    pub event_date: Option<DateTriple>,
    pub white_elo: u16,
    pub black_elo: u16,
    pub stored_line_code: u8,
    pub final_material: u32,
    pub ply_count: u16,
    pub home_pawn_len: u8,
    pub home_pawn_nibbles: u64,
}

impl IndexEntry {
    pub fn result_str(&self) -> &'static str {
        match self.result {
            1 => "1-0",
            2 => "0-1",
            3 => "1/2-1/2",
            _ => "*",
        }
    }

    /// Decode the dense ECO enumeration (notes §4): `1 + base*131 + sub`.
    pub fn eco_str(&self) -> String {
        if self.eco == 0 || self.eco > 0xFFDC {
            return String::new();
        }
        let v = (self.eco - 1) as u32;
        let base = v / 131;
        let sub = v % 131;
        let letter = (b'A' + (base / 100) as u8) as char;
        let number = base % 100;
        let mut out = format!("{letter}{number:02}");
        if sub > 0 {
            let s = sub - 1;
            out.push((b'a' + (s / 5) as u8) as char);
            let digit = s % 5;
            if digit > 0 {
                out.push((b'0' + digit as u8) as char);
            }
        }
        out
    }
}

struct Cursor<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, pos: 0 }
    }

    fn take(&mut self, n: usize, what: &'static str) -> Result<&'a [u8], Si4Error> {
        let end = self.pos + n;
        if end > self.bytes.len() {
            return Err(Si4Error::Truncated(what));
        }
        let s = &self.bytes[self.pos..end];
        self.pos = end;
        Ok(s)
    }

    fn u8(&mut self, what: &'static str) -> Result<u8, Si4Error> {
        Ok(self.take(1, what)?[0])
    }

    fn u16(&mut self, what: &'static str) -> Result<u16, Si4Error> {
        let b = self.take(2, what)?;
        Ok(u16::from_be_bytes([b[0], b[1]]))
    }

    fn u24(&mut self, what: &'static str) -> Result<u32, Si4Error> {
        let b = self.take(3, what)?;
        Ok(u32::from_be_bytes([0, b[0], b[1], b[2]]))
    }

    fn u32(&mut self, what: &'static str) -> Result<u32, Si4Error> {
        let b = self.take(4, what)?;
        Ok(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }
}

fn nul_terminated(bytes: &[u8]) -> String {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    // Encoding is undeclared in-file (notes §5.9); Latin-1 is the safe
    // superset for a spike (every byte maps to a char).
    bytes[..end].iter().map(|&b| b as char).collect()
}

pub fn parse_si4(bytes: &[u8]) -> Result<(Si4Header, Vec<IndexEntry>), Si4Error> {
    let mut c = Cursor::new(bytes);
    let magic = c.take(8, "si4 magic")?;
    if magic != SI4_MAGIC {
        return Err(Si4Error::BadMagic {
            file: "si4",
            expected: "Scid.si\\0",
        });
    }
    let version = c.u16("si4 version")?;
    if version != 400 {
        return Err(Si4Error::UnsupportedVersion(version));
    }
    let db_type = c.u32("db type")?;
    let num_games = c.u24("num games")?;
    let auto_load = c.u24("auto load")?;
    let description = nul_terminated(c.take(108, "description")?);
    let mut custom_flag_names: [String; 6] = Default::default();
    for slot in custom_flag_names.iter_mut() {
        *slot = nul_terminated(c.take(9, "custom flag name")?);
    }
    debug_assert_eq!(c.pos, HEADER_LEN);
    if bytes.len() < HEADER_LEN + num_games as usize * ENTRY_LEN {
        return Err(Si4Error::Truncated("index entries"));
    }

    let mut entries = Vec::with_capacity(num_games as usize);
    for _ in 0..num_games {
        entries.push(parse_entry(&mut c)?);
    }
    Ok((
        Si4Header {
            version,
            db_type,
            num_games,
            auto_load,
            description,
            custom_flag_names,
        },
        entries,
    ))
}

fn decode_dates(v: u32) -> (DateTriple, Option<DateTriple>) {
    let game = DateTriple {
        year: ((v >> 9) & 0x7FF) as u16,
        month: ((v >> 5) & 0xF) as u8,
        day: (v & 0x1F) as u8,
    };
    let event_bits = v >> 20;
    let year_mod = (event_bits >> 9) & 0x7;
    let event = if year_mod == 0 {
        None
    } else {
        Some(DateTriple {
            year: (game.year as i32 + year_mod as i32 - 4).max(0) as u16,
            month: ((event_bits >> 5) & 0xF) as u8,
            day: (event_bits & 0x1F) as u8,
        })
    };
    (game, event)
}

fn parse_entry(c: &mut Cursor<'_>) -> Result<IndexEntry, Si4Error> {
    let game_offset = c.u32("entry offset")?;
    let length_low = c.u16("entry length")?;
    let length_high = c.u8("entry length-high")?;
    let flags = c.u16("entry flags")?;
    let wb_high = c.u8("white/black high")?;
    let white_low = c.u16("white id")?;
    let black_low = c.u16("black id")?;
    let esr_high = c.u8("event/site/round high")?;
    let event_low = c.u16("event id")?;
    let site_low = c.u16("site id")?;
    let round_low = c.u16("round id")?;
    let result_counts = c.u16("result/counts")?;
    let eco = c.u16("eco")?;
    let dates = c.u32("dates")?;
    let white_elo_raw = c.u16("white elo")?;
    let black_elo_raw = c.u16("black elo")?;
    let stored_line_code = c.u8("stored line")?;
    let final_material = c.u24("final material")?;
    let ply_low = c.u8("ply count low")?;
    let hp = c.take(9, "home pawn block")?;

    let (date, event_date) = decode_dates(dates);
    // Home-pawn block: bits 71–70 ply-count high, 69–64 nibble count,
    // 63–0 nibbles (we keep them raw for the spike).
    let ply_count = ((hp[0] as u16 >> 6) << 8) | ply_low as u16;
    let home_pawn_len = hp[0] & 0x3F;
    let mut nibbles = 0u64;
    for &b in &hp[1..9] {
        nibbles = (nibbles << 8) | b as u64;
    }

    Ok(IndexEntry {
        game_offset,
        game_length: length_low as u32 | (((length_high >> 7) as u32) << 16),
        custom_flags: length_high & 0x3F,
        flags,
        white_id: ((wb_high as u32 >> 4) << 16) | white_low as u32,
        black_id: ((wb_high as u32 & 0xF) << 16) | black_low as u32,
        event_id: ((esr_high as u32 >> 5) << 16) | event_low as u32,
        site_id: (((esr_high as u32 >> 2) & 0x7) << 16) | site_low as u32,
        round_id: ((esr_high as u32 & 0x3) << 16) | round_low as u32,
        result: ((result_counts >> 12) & 0x3) as u8,
        nag_count_code: ((result_counts >> 8) & 0xF) as u8,
        comment_count_code: ((result_counts >> 4) & 0xF) as u8,
        variation_count_code: (result_counts & 0xF) as u8,
        eco,
        date,
        event_date,
        // Elo is documented as 12 bits; the top nibble's meaning is an open
        // documentation gap (notes §5.2), so mask it off.
        white_elo: white_elo_raw & 0xFFF,
        black_elo: black_elo_raw & 0xFFF,
        stored_line_code,
        final_material,
        ply_count,
        home_pawn_len,
        home_pawn_nibbles: nibbles,
    })
}
