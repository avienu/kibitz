//! .sn4 namebase: 36-byte header + four front-coded name sections.
//!
//! Layout per docs/SI4_FORMAT_NOTES.md §2. The documentation is ambiguous
//! about whether the ID field width keys off the section's name count or its
//! max frequency (notes §5.11); this reader uses the name count (2 bytes if
//! < 65536, else 3 — never 1), which is the only reading consistent with
//! "SCID reports corruption if ID >= count".

use crate::Si4Error;

pub const SN4_MAGIC: &[u8; 8] = b"Scid.sn\0";

#[derive(Debug, Clone)]
pub struct Sn4Header {
    pub player_count: u32,
    pub event_count: u32,
    pub site_count: u32,
    pub round_count: u32,
    pub player_max_freq: u32,
    pub event_max_freq: u32,
    pub site_max_freq: u32,
    pub round_max_freq: u32,
}

#[derive(Debug)]
pub struct NameBase {
    pub header: Sn4Header,
    players: Vec<String>,
    events: Vec<String>,
    sites: Vec<String>,
    rounds: Vec<String>,
}

impl NameBase {
    fn get<'a>(list: &'a [String], id: u32, section: &'static str) -> Result<&'a str, Si4Error> {
        list.get(id as usize)
            .map(String::as_str)
            .ok_or(Si4Error::NameIdOutOfRange {
                id,
                section,
                count: list.len() as u32,
            })
    }

    pub fn player(&self, id: u32) -> Result<&str, Si4Error> {
        Self::get(&self.players, id, "players")
    }
    pub fn event(&self, id: u32) -> Result<&str, Si4Error> {
        Self::get(&self.events, id, "events")
    }
    pub fn site(&self, id: u32) -> Result<&str, Si4Error> {
        Self::get(&self.sites, id, "sites")
    }
    pub fn round(&self, id: u32) -> Result<&str, Si4Error> {
        Self::get(&self.rounds, id, "rounds")
    }
}

struct Cursor<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn take(&mut self, n: usize, what: &'static str) -> Result<&'a [u8], Si4Error> {
        let end = self.pos + n;
        if end > self.bytes.len() {
            return Err(Si4Error::Truncated(what));
        }
        let s = &self.bytes[self.pos..end];
        self.pos = end;
        Ok(s)
    }

    fn be(&mut self, n: usize, what: &'static str) -> Result<u32, Si4Error> {
        let b = self.take(n, what)?;
        Ok(b.iter().fold(0u32, |acc, &x| (acc << 8) | x as u32))
    }
}

/// Width in bytes for a frequency value bounded by `max_freq`.
fn freq_width(max_freq: u32) -> usize {
    if max_freq < 256 {
        1
    } else if max_freq < 65_536 {
        2
    } else {
        3
    }
}

/// Width in bytes for a name ID in a section of `count` names (never 1).
fn id_width(count: u32) -> usize {
    if count < 65_536 {
        2
    } else {
        3
    }
}

fn parse_section(
    c: &mut Cursor<'_>,
    count: u32,
    max_freq: u32,
    section: &'static str,
) -> Result<Vec<String>, Si4Error> {
    let idw = id_width(count);
    let fqw = freq_width(max_freq);
    let mut names = vec![String::new(); count as usize];
    let mut prev = String::new();
    for i in 0..count {
        let id = c.be(idw, "name id")?;
        let _freq = c.be(fqw, "name frequency")?;
        let total_len = c.be(1, "name length")? as usize;
        let prefix_len = if i == 0 {
            0
        } else {
            c.be(1, "name prefix length")? as usize
        };
        if prefix_len > prev.len() || prefix_len > total_len {
            return Err(Si4Error::Truncated("front-coded prefix inconsistent"));
        }
        let suffix = c.take(total_len - prefix_len, "name suffix")?;
        // Encoding undeclared in-file (notes §5.9); Latin-1 for the spike.
        let mut name = prev[..prefix_len].to_string();
        name.extend(suffix.iter().map(|&b| b as char));
        if id as usize >= names.len() {
            return Err(Si4Error::NameIdOutOfRange { id, section, count });
        }
        names[id as usize] = name.clone();
        prev = name;
    }
    Ok(names)
}

pub fn parse_sn4(bytes: &[u8]) -> Result<NameBase, Si4Error> {
    let mut c = Cursor { bytes, pos: 0 };
    let magic = c.take(8, "sn4 magic")?;
    if magic != SN4_MAGIC {
        return Err(Si4Error::BadMagic {
            file: "sn4",
            expected: "Scid.sn\\0",
        });
    }
    let _unused = c.take(4, "sn4 reserved")?;
    let header = Sn4Header {
        player_count: c.be(3, "player count")?,
        event_count: c.be(3, "event count")?,
        site_count: c.be(3, "site count")?,
        round_count: c.be(3, "round count")?,
        player_max_freq: c.be(3, "player max freq")?,
        event_max_freq: c.be(3, "event max freq")?,
        site_max_freq: c.be(3, "site max freq")?,
        round_max_freq: c.be(3, "round max freq")?,
    };
    let players = parse_section(
        &mut c,
        header.player_count,
        header.player_max_freq,
        "players",
    )?;
    let events = parse_section(&mut c, header.event_count, header.event_max_freq, "events")?;
    let sites = parse_section(&mut c, header.site_count, header.site_max_freq, "sites")?;
    let rounds = parse_section(&mut c, header.round_count, header.round_max_freq, "rounds")?;
    Ok(NameBase {
        header,
        players,
        events,
        sites,
        rounds,
    })
}
