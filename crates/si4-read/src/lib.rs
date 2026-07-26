//! si4-read: cleanroom SCID database reader (Phase 0 spike scope).
//!
//! CLEANROOM NOTICE: this crate is written exclusively from the community
//! format documentation summarized in docs/SI4_FORMAT_NOTES.md (bkshrader/
//! asdfjkl si4spec, SCID user docs, Scidb docs). No SCID source code was
//! consulted, and none may ever be ported into this BSD-3-Clause crate.
//!
//! Spike scope: parse the .si4 index header, all 47-byte index entries, and
//! the .sn4 namebase, enough to dump every game's header line. Full .sg4
//! movetext decoding is Phase 1 work.

pub mod fixture;
mod names;
mod si4;

pub use names::{NameBase, Sn4Header};
pub use si4::{DateTriple, IndexEntry, Si4Header};

use std::io;
use std::path::Path;

#[derive(Debug, thiserror::Error)]
pub enum Si4Error {
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),
    #[error("bad magic in {file}: expected {expected:?}")]
    BadMagic {
        file: &'static str,
        expected: &'static str,
    },
    #[error("unsupported version {0} (this reader implements si4 = 400)")]
    UnsupportedVersion(u16),
    #[error("file truncated: {0}")]
    Truncated(&'static str),
    #[error("name id {id} out of range for {section} (count {count})")]
    NameIdOutOfRange {
        id: u32,
        section: &'static str,
        count: u32,
    },
}

/// A game's header fields with name IDs resolved against the namebase.
#[derive(Debug, Clone)]
pub struct GameHeader {
    pub white: String,
    pub black: String,
    pub event: String,
    pub site: String,
    pub round: String,
    pub date: DateTriple,
    pub result: &'static str,
    pub eco: String,
    pub white_elo: u16,
    pub black_elo: u16,
    pub ply_count: u16,
}

/// An opened SCID database: parsed index + namebase.
pub struct Database {
    pub header: Si4Header,
    pub entries: Vec<IndexEntry>,
    pub names: NameBase,
}

impl Database {
    /// Open `<base>.si4` + `<base>.sn4`. (`base` may also name the .si4
    /// file directly.)
    pub fn open(base: &Path) -> Result<Self, Si4Error> {
        let base = base.with_extension("");
        let si4_bytes = std::fs::read(base.with_extension("si4"))?;
        let sn4_bytes = std::fs::read(base.with_extension("sn4"))?;
        Self::from_bytes(&si4_bytes, &sn4_bytes)
    }

    pub fn from_bytes(si4: &[u8], sn4: &[u8]) -> Result<Self, Si4Error> {
        let (header, entries) = si4::parse_si4(si4)?;
        let names = names::parse_sn4(sn4)?;
        Ok(Self {
            header,
            entries,
            names,
        })
    }

    /// Resolve one index entry into a printable game header.
    pub fn game_header(&self, entry: &IndexEntry) -> Result<GameHeader, Si4Error> {
        Ok(GameHeader {
            white: self.names.player(entry.white_id)?.to_string(),
            black: self.names.player(entry.black_id)?.to_string(),
            event: self.names.event(entry.event_id)?.to_string(),
            site: self.names.site(entry.site_id)?.to_string(),
            round: self.names.round(entry.round_id)?.to_string(),
            date: entry.date,
            result: entry.result_str(),
            eco: entry.eco_str(),
            white_elo: entry.white_elo,
            black_elo: entry.black_elo,
            ply_count: entry.ply_count,
        })
    }
}
