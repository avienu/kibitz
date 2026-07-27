//! Small deterministic English helpers. Anything a reader actually sees
//! comes from the template store; this module only supplies mechanical
//! glue (joining, casing, key fragments, numeric formatting).

use crate::templates::lookup;
use kibitz_core::record::{Favors, Magnitude, Phase, Severity, SideColor};

pub(crate) fn side_name(side: SideColor) -> &'static str {
    match side {
        SideColor::White => lookup(&["side.white"]),
        SideColor::Black => lookup(&["side.black"]),
    }
}

pub(crate) fn favors_side(favors: Favors) -> Option<SideColor> {
    match favors {
        Favors::White => Some(SideColor::White),
        Favors::Black => Some(SideColor::Black),
        Favors::Balanced => None,
    }
}

pub(crate) fn severity_key(severity: Severity) -> &'static str {
    match severity {
        Severity::Low => "low",
        Severity::Medium => "medium",
        Severity::High => "high",
    }
}

pub(crate) fn magnitude_key(magnitude: Magnitude) -> &'static str {
    match magnitude {
        Magnitude::Minor => "minor",
        Magnitude::Clear => "clear",
        Magnitude::Winning => "winning",
    }
}

pub(crate) fn phase_key(phase: Phase) -> &'static str {
    match phase {
        Phase::Opening => "opening",
        Phase::Middlegame => "middlegame",
        Phase::Endgame => "endgame",
    }
}

/// "a", "a and b", "a, b and c" — glue tokens come from the template store.
pub(crate) fn join_and(items: &[String]) -> String {
    let sep = format!("{} ", lookup(&["glue.list_sep"]));
    let last = format!(" {} ", lookup(&["glue.list_last"]));
    match items {
        [] => String::new(),
        [only] => only.clone(),
        [head @ .., tail] => format!("{}{last}{tail}", head.join(&sep)),
    }
}

/// Centipawns -> "1.9 pawns", "2 pawns", "1 pawn".
pub(crate) fn pawns_amount(centipawns: i32) -> String {
    let abs = centipawns.abs();
    let quantity = if abs % 100 == 0 {
        format!("{}", abs / 100)
    } else {
        format!("{:.1}", f64::from(abs) / 100.0)
    };
    let unit = if abs == 100 {
        lookup(&["unit.pawn"])
    } else {
        lookup(&["unit.pawns"])
    };
    format!("{quantity} {unit}")
}

/// Template key for the SEE value band (centipawns, attacker's POV).
pub(crate) fn see_key(centipawns: i32) -> &'static str {
    match centipawns {
        i32::MIN..=49 => "see.small",
        50..=149 => "see.pawn",
        150..=249 => "see.two_pawns",
        250..=399 => "see.minor",
        400..=649 => "see.rook",
        _ => "see.queen",
    }
}

/// "BlockadeThenPressure" -> "blockade then pressure";
/// "overloaded-defender" -> "overloaded defender".
pub(crate) fn humanize(token: &str) -> String {
    let mut out = String::with_capacity(token.len() + 4);
    for (index, c) in token.chars().enumerate() {
        if c == '-' || c == '_' {
            out.push(' ');
        } else if c.is_ascii_uppercase() {
            if index > 0 {
                out.push(' ');
            }
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

pub(crate) fn capitalize_first(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

pub(crate) fn decapitalize_first(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_lowercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}
