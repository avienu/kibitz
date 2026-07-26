//! Synthetic .si4/.sn4 byte builders for tests and the demo dump example.
//!
//! Written from the same community documentation as the parser (an
//! independent inverse of the read path, so tests exercise the documented
//! layout rather than a parser round-trip of itself). No real SCID files
//! were available when this was written; see RUN_REPORT.md.

/// Encode an ECO string like "B90" or "A00z4" into the dense enumeration.
pub fn encode_eco(eco: &str) -> u16 {
    let bytes = eco.as_bytes();
    if bytes.len() < 3 {
        return 0;
    }
    let base =
        (bytes[0] - b'A') as u32 * 100 + (bytes[1] - b'0') as u32 * 10 + (bytes[2] - b'0') as u32;
    let sub = match bytes.len() {
        3 => 0,
        4 => 1 + (bytes[3] - b'a') as u32 * 5,
        _ => 1 + (bytes[3] - b'a') as u32 * 5 + (bytes[4] - b'0') as u32,
    };
    (1 + base * 131 + sub) as u16
}

pub fn encode_date(year: u16, month: u8, day: u8) -> u32 {
    ((year as u32) << 9) | ((month as u32) << 5) | day as u32
}

/// One synthetic game for the fixture index.
pub struct FixtureGame {
    pub white_id: u32,
    pub black_id: u32,
    pub event_id: u32,
    pub site_id: u32,
    pub round_id: u32,
    /// 0 `*`, 1 `1-0`, 2 `0-1`, 3 `1/2-1/2`.
    pub result: u8,
    pub eco: &'static str,
    pub date: (u16, u8, u8),
    pub white_elo: u16,
    pub black_elo: u16,
    pub ply_count: u16,
}

fn push_u16(out: &mut Vec<u8>, v: u16) {
    out.extend_from_slice(&v.to_be_bytes());
}

fn push_u24(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_be_bytes()[1..4]);
}

fn push_u32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_be_bytes());
}

/// Build a .si4 file image (182-byte header + 47-byte entries).
pub fn build_si4(description: &str, games: &[FixtureGame]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(b"Scid.si\0");
    push_u16(&mut out, 400); // version
    push_u32(&mut out, 0); // db type
    push_u24(&mut out, games.len() as u32);
    push_u24(&mut out, 0); // auto-load
    let mut desc = [0u8; 108];
    for (i, &b) in description.as_bytes().iter().take(107).enumerate() {
        desc[i] = b;
    }
    out.extend_from_slice(&desc);
    out.extend_from_slice(&[0u8; 54]); // six empty custom flag names
    assert_eq!(out.len(), 182);

    for (i, g) in games.iter().enumerate() {
        push_u32(&mut out, (i as u32) * 100); // sg4 offset (synthetic)
        push_u16(&mut out, 80); // length low
        out.push(0); // length high + custom flags
        push_u16(&mut out, 0); // flags
        out.push((((g.white_id >> 16) as u8) << 4) | ((g.black_id >> 16) as u8 & 0xF));
        push_u16(&mut out, g.white_id as u16);
        push_u16(&mut out, g.black_id as u16);
        out.push(
            (((g.event_id >> 16) as u8) << 5)
                | (((g.site_id >> 16) as u8 & 0x7) << 2)
                | ((g.round_id >> 16) as u8 & 0x3),
        );
        push_u16(&mut out, g.event_id as u16);
        push_u16(&mut out, g.site_id as u16);
        push_u16(&mut out, g.round_id as u16);
        push_u16(&mut out, (g.result as u16) << 12); // counts all zero
        push_u16(&mut out, encode_eco(g.eco));
        push_u32(&mut out, encode_date(g.date.0, g.date.1, g.date.2));
        push_u16(&mut out, g.white_elo);
        push_u16(&mut out, g.black_elo);
        out.push(0); // stored line code
        push_u24(&mut out, 0); // final material (empty for fixture)
        out.push(g.ply_count as u8); // ply low
        let mut hp = [0u8; 9];
        hp[0] = ((g.ply_count >> 8) as u8 & 0x3) << 6;
        out.extend_from_slice(&hp);
    }
    out
}

/// Build a .sn4 file image. Each section is a list of names; the name's
/// index in its (alphabetically sorted) list is its ID.
pub fn build_sn4(players: &[&str], events: &[&str], sites: &[&str], rounds: &[&str]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(b"Scid.sn\0");
    push_u32(&mut out, 0); // reserved
    for list in [players, events, sites, rounds] {
        push_u24(&mut out, list.len() as u32);
    }
    for _ in 0..4 {
        push_u24(&mut out, 1); // max frequency: 1 byte wide is enough
    }
    for list in [players, events, sites, rounds] {
        let mut sorted: Vec<(usize, &&str)> = list.iter().enumerate().collect();
        sorted.sort_by_key(|(_, name)| **name);
        let mut prev = "";
        for (row, (id, name)) in sorted.iter().enumerate() {
            push_u16(&mut out, *id as u16); // ID (2 bytes: count < 65536)
            out.push(1); // frequency
            out.push(name.len() as u8);
            let prefix = if row == 0 {
                0
            } else {
                let p = common_prefix(prev, name);
                out.push(p as u8);
                p
            };
            out.extend_from_slice(&name.as_bytes()[prefix..]);
            prev = name;
        }
    }
    out
}

fn common_prefix(a: &str, b: &str) -> usize {
    a.bytes().zip(b.bytes()).take_while(|(x, y)| x == y).count()
}
