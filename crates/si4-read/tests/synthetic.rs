//! Parse a synthetic .si4/.sn4 pair built byte-by-byte from the documented
//! layout (docs/SI4_FORMAT_NOTES.md). Real-database verification is pending
//! access to the user's SCID files (testdata/private/ was absent this run).

use si4_read::fixture::{build_si4, build_sn4, encode_eco, FixtureGame};
use si4_read::Database;

fn fixture() -> Database {
    let players = ["Carlsen, Magnus", "Caruana, Fabiano", "Nakamura, Hikaru"];
    let events = ["Synthetic Masters 2024"];
    let sites = ["Test City"];
    let rounds = ["1", "2"];
    let games = [
        FixtureGame {
            white_id: 0,
            black_id: 1,
            event_id: 0,
            site_id: 0,
            round_id: 0,
            result: 1,
            eco: "B90",
            date: (2024, 3, 15),
            white_elo: 2830,
            black_elo: 2805,
            ply_count: 87,
        },
        FixtureGame {
            white_id: 2,
            black_id: 0,
            event_id: 0,
            site_id: 0,
            round_id: 1,
            result: 3,
            eco: "A00z4",
            date: (2024, 3, 16),
            white_elo: 2790,
            black_elo: 2830,
            // > 255 to exercise the 10-bit ply count split.
            ply_count: 300,
        },
    ];
    let si4 = build_si4("synthetic fixture", &games);
    let sn4 = build_sn4(&players, &events, &sites, &rounds);
    Database::from_bytes(&si4, &sn4).expect("fixture parses")
}

#[test]
fn header_and_entries_parse() {
    let db = fixture();
    assert_eq!(db.header.version, 400);
    assert_eq!(db.header.num_games, 2);
    assert_eq!(db.header.description, "synthetic fixture");
    assert_eq!(db.entries.len(), 2);
}

#[test]
fn game_headers_resolve_names_dates_ecos() {
    let db = fixture();
    let g0 = db.game_header(&db.entries[0]).unwrap();
    assert_eq!(g0.white, "Carlsen, Magnus");
    assert_eq!(g0.black, "Caruana, Fabiano");
    assert_eq!(g0.event, "Synthetic Masters 2024");
    assert_eq!(g0.result, "1-0");
    assert_eq!(g0.eco, "B90");
    assert_eq!(g0.date.to_string(), "2024.03.15");
    assert_eq!(g0.white_elo, 2830);
    assert_eq!(g0.ply_count, 87);

    let g1 = db.game_header(&db.entries[1]).unwrap();
    assert_eq!(g1.white, "Nakamura, Hikaru");
    assert_eq!(g1.result, "1/2-1/2");
    assert_eq!(g1.eco, "A00z4");
    assert_eq!(g1.round, "2");
    assert_eq!(g1.ply_count, 300, "10-bit ply count split survives");
}

#[test]
fn eco_enumeration_matches_documented_anchors() {
    // Anchors from docs/SI4_FORMAT_NOTES.md §4.
    assert_eq!(encode_eco("A00"), 0x0001);
    assert_eq!(encode_eco("A00z4"), 0x0083);
    assert_eq!(encode_eco("A01"), 0x0084);
    assert_eq!(encode_eco("B00"), 0x332D);
    assert_eq!(encode_eco("E99z4"), 0xFFDC);
}

#[test]
fn front_coding_with_shared_prefixes_round_trips() {
    // "Carlsen" / "Caruana" share "Car"; exercises prefix decode.
    let db = fixture();
    assert_eq!(db.names.player(0).unwrap(), "Carlsen, Magnus");
    assert_eq!(db.names.player(1).unwrap(), "Caruana, Fabiano");
    assert_eq!(db.names.player(2).unwrap(), "Nakamura, Hikaru");
}

#[test]
fn bad_magic_and_wrong_version_are_rejected() {
    let sn4 = build_sn4(&["X"], &["E"], &["S"], &["1"]);
    let junk = b"NotScid!rest".to_vec();
    assert!(Database::from_bytes(&junk, &sn4).is_err());

    let mut si4 = build_si4("x", &[]);
    si4[8] = 0x01;
    si4[9] = 0x8F; // version 399
    assert!(Database::from_bytes(&si4, &sn4).is_err());
}
