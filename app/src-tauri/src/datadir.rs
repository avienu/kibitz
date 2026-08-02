//! One-time adoption of the pre-release data directory.
//!
//! The bundle identifier became `org.kibitzchess.kibitz` (2026-07-31, for
//! Developer ID signing). Every platform derives the app data and config
//! directories from that identifier, so an install made under the old
//! `org.kibitzchess.app` would come back up looking at empty directories
//! with its database — 1.1 GB of it, in the maintainer's case — stranded
//! at the old path. That is data loss as the user experiences it, even
//! though every byte is still on disk.
//!
//! So the app adopts the old directory once, at startup, before anything
//! reads either location. Entries move (a rename inside the same
//! directory tree is instant, whatever the file's size); anything already
//! present at the destination is left alone, so a fresh install that
//! happens to sit beside an old one is never overwritten.

use std::path::{Path, PathBuf};

/// The identifier every build before 2026-07-31 shipped with.
pub const LEGACY_IDENTIFIER: &str = "org.kibitzchess.app";

/// What an adoption pass did, for the startup log.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Adopted {
    /// Entries moved out of the legacy directory.
    pub moved: Vec<String>,
    /// Entries left behind because the destination already had one.
    pub kept: Vec<String>,
}

impl Adopted {
    pub fn is_empty(&self) -> bool {
        self.moved.is_empty() && self.kept.is_empty()
    }
}

/// The legacy sibling of `dir` — same parent, old identifier. `None` when
/// `dir` has no parent, or is itself the legacy directory (identifier
/// unchanged, nothing to do).
fn legacy_sibling(dir: &Path) -> Option<PathBuf> {
    if dir.file_name()? == LEGACY_IDENTIFIER {
        return None;
    }
    let legacy = dir.parent()?.join(LEGACY_IDENTIFIER);
    legacy.is_dir().then_some(legacy)
}

/// Move the contents of the legacy directory into `dir`. Best-effort by
/// design: an entry that cannot be moved is reported and skipped, never
/// fatal — failing to adopt an old database must not stop the app from
/// starting with a new one.
pub fn adopt_legacy(dir: &Path) -> Adopted {
    let mut out = Adopted::default();
    let Some(legacy) = legacy_sibling(dir) else {
        return out;
    };
    let Ok(entries) = std::fs::read_dir(&legacy) else {
        return out;
    };
    if std::fs::create_dir_all(dir).is_err() {
        return out;
    }
    for entry in entries.flatten() {
        let name = entry.file_name();
        let dest = dir.join(&name);
        let label = name.to_string_lossy().to_string();
        if dest.exists() {
            out.kept.push(label);
            continue;
        }
        match std::fs::rename(entry.path(), &dest) {
            Ok(()) => out.moved.push(label),
            Err(e) => {
                eprintln!("data dir: could not adopt {label:?} from {legacy:?}: {e}");
                out.kept.push(label);
            }
        }
    }
    // Only ever removes it once empty — never recursive, so anything left
    // behind stays exactly where it is.
    let _ = std::fs::remove_dir(&legacy);
    out
}

/// Adopt the legacy data AND config directories (the same directory on
/// macOS, different ones on Linux), then repoint a remembered database
/// path that still names the old location. Call once at startup.
pub fn adopt_all(data_dir: &Path, config_dir: &Path) -> Adopted {
    let mut out = adopt_legacy(data_dir);
    if config_dir != data_dir {
        let more = adopt_legacy(config_dir);
        out.moved.extend(more.moved);
        out.kept.extend(more.kept);
    }
    // The remembered path is absolute and was written before the move.
    for dir in [data_dir, config_dir] {
        if let Some(legacy) = dir.parent().map(|p| p.join(LEGACY_IDENTIFIER)) {
            crate::session::repoint_remembered_db(config_dir, &legacy, dir);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A parent holding the legacy directory and the new one beside it.
    fn dirs() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let root = tempfile::tempdir().unwrap();
        let legacy = root.path().join(LEGACY_IDENTIFIER);
        let new = root.path().join("org.kibitzchess.kibitz");
        std::fs::create_dir_all(&legacy).unwrap();
        (root, legacy, new)
    }

    #[test]
    fn adopts_the_old_database_into_the_new_directory() {
        let (_root, legacy, new) = dirs();
        std::fs::write(legacy.join("kibitz.sqlite"), b"db").unwrap();
        std::fs::write(legacy.join("kibitz.sqlite-wal"), b"wal").unwrap();

        let got = adopt_legacy(&new);
        assert_eq!(got.moved.len(), 2, "{got:?}");
        assert!(got.kept.is_empty());
        assert_eq!(std::fs::read(new.join("kibitz.sqlite")).unwrap(), b"db");
        assert_eq!(
            std::fs::read(new.join("kibitz.sqlite-wal")).unwrap(),
            b"wal"
        );
        assert!(
            !legacy.exists(),
            "the emptied legacy directory is tidied away"
        );
    }

    #[test]
    fn never_overwrites_a_file_the_new_install_already_has() {
        let (_root, legacy, new) = dirs();
        std::fs::create_dir_all(&new).unwrap();
        std::fs::write(legacy.join("kibitz.sqlite"), b"old").unwrap();
        std::fs::write(new.join("kibitz.sqlite"), b"new").unwrap();

        let got = adopt_legacy(&new);
        assert_eq!(got.kept, vec!["kibitz.sqlite".to_string()]);
        assert_eq!(std::fs::read(new.join("kibitz.sqlite")).unwrap(), b"new");
        assert_eq!(
            std::fs::read(legacy.join("kibitz.sqlite")).unwrap(),
            b"old",
            "the old file stays put rather than being destroyed"
        );
    }

    #[test]
    fn does_nothing_without_a_legacy_directory_or_when_it_is_the_target() {
        let root = tempfile::tempdir().unwrap();
        let new = root.path().join("org.kibitzchess.kibitz");
        assert!(adopt_legacy(&new).is_empty());
        assert!(
            !new.exists(),
            "no legacy data means no directory is created"
        );

        // Running under the old identifier: the target IS the legacy dir.
        let legacy = root.path().join(LEGACY_IDENTIFIER);
        std::fs::create_dir_all(&legacy).unwrap();
        std::fs::write(legacy.join("kibitz.sqlite"), b"db").unwrap();
        assert!(adopt_legacy(&legacy).is_empty());
        assert!(legacy.join("kibitz.sqlite").exists());
    }

    #[test]
    fn repoints_a_remembered_database_path_at_the_new_directory() {
        let (_root, legacy, new) = dirs();
        let db = legacy.join("kibitz.sqlite");
        std::fs::write(&db, b"db").unwrap();
        crate::session::remember_db_path(&legacy, &db.to_string_lossy()).unwrap();

        let got = adopt_all(&new, &new);
        assert_eq!(got.moved.len(), 2, "database and session file: {got:?}");
        assert_eq!(
            crate::session::recall_db_path(&new).unwrap(),
            new.join("kibitz.sqlite").to_string_lossy(),
            "the app reopens the database at its new home, not the old path"
        );
    }
}
