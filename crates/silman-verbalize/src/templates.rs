//! Embedded template store: `key = template` data files parsed once at
//! first use. All user-visible English lives in `templates/*.tmpl`
//! (CLAUDE.md convention: UI text and explanation templates are data,
//! not string literals).

use std::collections::BTreeMap;
use std::sync::OnceLock;

const SOURCES: &[&str] = &[
    include_str!("../templates/common.tmpl"),
    include_str!("../templates/alerts.tmpl"),
    include_str!("../templates/imbalances.tmpl"),
    include_str!("../templates/evidence.tmpl"),
    include_str!("../templates/plans.tmpl"),
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

/// The first template found among `keys`, or `""` when none exists.
/// Callers always end their chains with a generic key that the data
/// files are guaranteed to define.
pub(crate) fn lookup(keys: &[&str]) -> &'static str {
    for key in keys {
        if let Some(value) = store().get(*key) {
            return value;
        }
    }
    ""
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

/// Deterministic sentence-start rotation: entry `index % n` of the
/// `|`-separated list stored under `key` (first entry is empty).
pub(crate) fn rotation(key: &str, index: usize) -> &'static str {
    let raw = lookup(&[key]);
    if raw.is_empty() {
        return "";
    }
    let parts: Vec<&'static str> = raw.split('|').collect();
    parts[index % parts.len()]
}
