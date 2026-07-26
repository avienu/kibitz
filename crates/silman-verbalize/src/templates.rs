//! Embedded template store: `key = template` data files parsed once at
//! first use. All user-visible English lives in `templates/*.tmpl`
//! (CLAUDE.md convention: UI text and explanation templates are data,
//! not string literals).
//!
//! Voices (run-5 item 3) are a template OVERLAY, not a code fork: the
//! coach voice's lines live in `templates/coach.tmpl` under `coach.<key>`
//! namespaced keys, and the voiced lookups below consult `coach.<key>`
//! before falling back to the base `<key>`. The Neutral voice reads only
//! base keys, so a key without a coach override renders identically in
//! both voices.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use crate::Voice;

const SOURCES: &[&str] = &[
    include_str!("../templates/common.tmpl"),
    include_str!("../templates/alerts.tmpl"),
    include_str!("../templates/imbalances.tmpl"),
    include_str!("../templates/evidence.tmpl"),
    include_str!("../templates/plans.tmpl"),
    include_str!("../templates/coach.tmpl"),
];

static STORE: OnceLock<BTreeMap<String, String>> = OnceLock::new();

fn store() -> &'static BTreeMap<String, String> {
    STORE.get_or_init(|| {
        let mut map = BTreeMap::new();
        for source in SOURCES {
            for line in source.lines() {
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }
                if let Some((key, value)) = line.split_once('=') {
                    map.insert(key.trim().to_string(), value.trim().to_string());
                }
            }
        }
        map
    })
}

/// The template stored under `key` in `voice`: the `coach.`-prefixed
/// override when the Coach voice is selected and one exists, else the base
/// key. Neutral never sees coach lines.
fn resolve(voice: Voice, key: &str) -> Option<&'static str> {
    if voice == Voice::Coach {
        if let Some(value) = store().get(&format!("coach.{key}")) {
            return Some(value);
        }
    }
    store().get(key).map(String::as_str)
}

/// The first BASE template found among `keys`, or `""` when none exists.
/// Voice-invariant vocabulary (side names, piece names, glue) and machine
/// configuration (`evidence.order`) go through here.
pub(crate) fn lookup(keys: &[&str]) -> &'static str {
    lookup_voiced(Voice::Neutral, keys)
}

/// The first template found among `keys` in `voice`, or `""` when none
/// exists. Callers always end their chains with a generic key that the
/// base data files are guaranteed to define.
pub(crate) fn lookup_voiced(voice: Voice, keys: &[&str]) -> &'static str {
    for key in keys {
        if let Some(value) = resolve(voice, key) {
            return value;
        }
    }
    ""
}

/// A template by exact key in `voice`, or None — for optional per-token
/// clauses.
pub(crate) fn try_lookup_voiced(voice: Voice, key: &str) -> Option<&'static str> {
    resolve(voice, key)
}

/// Replace each `{name}` placeholder with its value. Unknown placeholders
/// are left in place so the snapshot tests can catch template drift.
pub(crate) fn fill(template: &str, slots: &[(&str, &str)]) -> String {
    let mut out = template.to_string();
    for (name, value) in slots {
        out = out.replace(&format!("{{{name}}}"), value);
    }
    out
}

/// Like [`lookup_voiced`], but with deterministic phrasing variety: when
/// `seed` is odd and the first matching key has an `.alt` sibling, the
/// sibling is used. Seeded by the alert's target square so the same story
/// on the same square always reads the same, while neighbouring alerts of
/// the same kind on different squares phrase differently (run-5 item 2).
///
/// A key with a coach override draws its variety only from its own
/// `coach.<key>.alt` sibling — base `.alt` lines never mix into coach
/// prose, and vice versa.
pub(crate) fn lookup_var(voice: Voice, keys: &[&str], seed: usize) -> &'static str {
    for key in keys {
        if voice == Voice::Coach {
            if let Some(value) = store().get(&format!("coach.{key}")) {
                if seed % 2 == 1 {
                    if let Some(alt) = store().get(&format!("coach.{key}.alt")) {
                        return alt;
                    }
                }
                return value;
            }
        }
        if let Some(value) = store().get(*key) {
            if seed % 2 == 1 {
                if let Some(alt) = store().get(&format!("{key}.alt")) {
                    return alt;
                }
            }
            return value;
        }
    }
    ""
}

/// Deterministic sentence-start rotation: entry `index % n` of the
/// `|`-separated list stored under `key` (first entry is empty).
pub(crate) fn rotation(voice: Voice, key: &str, index: usize) -> &'static str {
    let raw = lookup_voiced(voice, &[key]);
    if raw.is_empty() {
        return "";
    }
    let parts: Vec<&'static str> = raw.split('|').collect();
    parts[index % parts.len()]
}
