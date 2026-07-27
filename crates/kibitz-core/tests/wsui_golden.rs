//! Golden-file tests for the WSUI tactical screen. Every position cites
//! its source; constructed positions are labeled as such and model a named
//! instructional pattern.

use cozy_chess::Board;
use kibitz_core::record::{AlertKind, Severity, SideColor};
use kibitz_core::wsui::{screen, WsuiConfig};

fn run(fen: &str) -> kibitz_core::record::WsuiReport {
    let board: Board = fen.parse().unwrap();
    screen(&board, &WsuiConfig::default())
}

/// Negative control: the starting position is quiet. The screen must not
/// fire and no alert may reach medium severity.
/// Source: initial chess position.
#[test]
fn startpos_is_quiet() {
    let r = run("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1");
    assert!(!r.screen_fired, "alerts: {:#?}", r.alerts);
    assert!(r.alerts.iter().all(|a| a.severity == Severity::Low));
}

/// Negative control: a genuinely quiet opening position — the Giuoco
/// Pianissimo tabiya (1.e4 e5 2.Nf3 Nc6 3.Bc4 Bc5 4.d3 Nf6). Source:
/// standard opening theory.
#[test]
fn quiet_giuoco_pianissimo_does_not_fire() {
    let r = run("r1bqk2r/pppp1ppp/2n2n2/2b1p3/2B1P3/3P1N2/PPP2PPP/RNBQK2R w KQkq - 0 5");
    assert!(!r.screen_fired, "alerts: {:#?}", r.alerts);
}

/// The CPW-position-6-style middlegame is symmetric and evaluates level,
/// but is NOT statically quiet: both g-bishops genuinely hang to the
/// opposite knights. The screen must fire on both — this is exactly the
/// class of alert the bounded engine job later refutes (mutual hangs net
/// out). Source: perft position, crates/kibitz-core/src/perft.rs.
#[test]
fn symmetric_mutual_hangs_fire_for_both_sides() {
    let r = run("r4rk1/1pp1qppp/p1np1n2/2b1p1b1/2B1P1B1/P1NP1N2/1PP1QPPP/R4RK1 w - - 0 10");
    assert!(r.screen_fired);
    let targets: Vec<_> = r
        .alerts
        .iter()
        .filter_map(|a| a.target.as_deref())
        .collect();
    assert!(targets.contains(&"g4"), "white Bg4 hangs to Nf6");
    assert!(targets.contains(&"g5"), "black Bg5 hangs to Nf3");
}

/// U — a loose (undefended) attacked knight. "Loose Pieces Drop Off"
/// (John Nunn's LPDO dictum). Constructed minimal illustration.
#[test]
fn loose_attacked_piece_fires_u() {
    let r = run("4k3/8/8/8/Qn6/8/8/4K3 b - - 0 1");
    let alert = r
        .alerts
        .iter()
        .find(|a| a.kind == AlertKind::Undefended && a.side == SideColor::Black)
        .expect("loose black knight alert");
    assert_eq!(alert.target.as_deref(), Some("b4"));
    assert_eq!(alert.severity, Severity::Medium);
    assert_eq!(alert.attackers, vec!["a4".to_string()]);
    assert!(r.screen_fired);
}

/// I — attackers outnumber defenders with a winning exchange.
/// Constructed: black Nd5 defended by Pe6, attacked by white Nc3 and Pe4;
/// the pawn captures first and the exchange favours White.
#[test]
fn inadequately_defended_piece_fires_i() {
    let r = run("4k3/8/4p3/3n4/4P3/2N5/8/4K3 b - - 0 1");
    let alert = r
        .alerts
        .iter()
        .find(|a| a.kind == AlertKind::InadequatelyDefended && a.side == SideColor::Black)
        .expect("inadequately defended d5 knight");
    assert_eq!(alert.target.as_deref(), Some("d5"));
    assert!(alert.see.unwrap() >= 220, "see = {:?}", alert.see);
    assert_eq!(alert.severity, Severity::High);
    assert!(r.screen_fired);
}

/// I/overload — one piece is the sole defender of two attacked rooks.
/// Constructed overloaded-defender pattern (a standard tactics-primer
/// motif; cf. the "overloading" chapter of any tactics manual).
#[test]
fn overloaded_sole_defender_is_flagged() {
    let r = run("2rqr1k1/5ppp/8/8/8/8/5PPP/2R1R1K1 w - - 0 1");
    let alert = r
        .alerts
        .iter()
        .find(|a| {
            a.kind == AlertKind::InadequatelyDefended
                && a.detail.as_deref() == Some("overloaded-defender")
        })
        .expect("overload alert");
    assert_eq!(
        alert.target.as_deref(),
        Some("d8"),
        "the queen is overloaded"
    );
    assert_eq!(alert.side, SideColor::Black);
    assert_eq!(alert.defenders.len(), 2, "defends both c8 and e8");
}

/// S — the Noah's Ark trap shape (Ruy Lopez instructional pattern): the
/// white bishop on b3 is buried by the a6/b5/c4 pawn chain — attacked by
/// the c4 pawn, no safe square, only losing captures.
#[test]
fn noahs_ark_trapped_bishop_fires_s() {
    let r = run("rnbqkbnr/5ppp/p2p4/1p6/2p1P3/1B6/PPPP1PPP/RNBQK1NR w KQkq - 0 6");
    let alert = r
        .alerts
        .iter()
        .find(|a| a.kind == AlertKind::TrappedPiece && a.side == SideColor::White)
        .expect("trapped bishop alert");
    assert_eq!(alert.target.as_deref(), Some("b3"));
    assert_eq!(alert.detail.as_deref(), Some("trapped-and-attacked"));
    assert!(alert.severity >= Severity::Medium);
    assert!(alert.attackers.contains(&"c4".to_string()));
    assert!(r.screen_fired);
}

/// W — back-rank weakness: king sealed behind its own pawns, no friendly
/// major on the rank, enemy majors present. Constructed back-rank pattern
/// (cf. Bernstein–Capablanca, Moscow 1914, the canonical example).
#[test]
fn back_rank_weakness_fires_w() {
    let r = run("4r1k1/5ppp/8/R7/8/2Q5/5PPP/6K1 b - - 0 1");
    let white_alert = r
        .alerts
        .iter()
        .find(|a| a.kind == AlertKind::WeakKing && a.side == SideColor::White)
        .expect("white back-rank alert");
    assert!(white_alert.detail.as_deref().unwrap().contains("back-rank"));
}

/// W — wrecked castle: missing g-pawn shield in front of the castled king
/// with an enemy rook on the open g-file. Constructed "shattered
/// kingside" pattern (standard attacking-manual material).
#[test]
fn wrecked_shield_open_file_fires_w() {
    let r = run("r4r1k/pp2pp1p/5p2/8/8/8/PPPP1P1P/2KR2R1 w - - 0 1");
    let alert = r
        .alerts
        .iter()
        .find(|a| a.kind == AlertKind::WeakKing && a.side == SideColor::Black)
        .expect("black king shield alert");
    let detail = alert.detail.as_deref().unwrap();
    assert!(detail.contains("g-file shield pawn missing"), "{detail}");
    assert!(detail.contains("open-files:g"), "{detail}");
    assert!(alert.severity >= Severity::Medium);
}

/// Pin-awareness: an absolutely pinned defender does not count.
/// Constructed: black Bf5 attacked by white Rf1; its only "defender" is
/// the e6 knight, absolutely pinned by Bc4 against Kg8.
#[test]
fn pinned_defender_does_not_count() {
    let fen = "6k1/8/4n3/5b2/2B5/8/8/4KR2 b - - 0 1";
    let board: Board = fen.parse().unwrap();
    let pinned = kibitz_core::attack::pinned_pieces(&board, cozy_chess::Color::Black);
    assert!(pinned.has(cozy_chess::Square::E6), "e6 knight pinned");
    let r = run(fen);
    let alert = r
        .alerts
        .iter()
        .find(|a| a.side == SideColor::Black && a.target.as_deref() == Some("f5"))
        .expect("f5 bishop alert");
    assert_eq!(
        alert.kind,
        AlertKind::Undefended,
        "pinned e6 knight must not count as a defender: {alert:#?}"
    );
    assert!(r.screen_fired);
}
