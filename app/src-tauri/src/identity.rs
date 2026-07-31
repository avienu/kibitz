//! Player-identity IPC (run 8.5): name forms and declared aliases.
//! `identity_group` reports every name form the given name resolves to
//! (lexical variants + declared aliases, transitively); `alias_declare`
//! and `alias_remove` manage the declared layer. Profile/prep/fingerprint
//! already resolve identities internally — these commands exist so the
//! UI can SHOW what merged and let the user correct it.

use kibitz_db::identity::{self, NameForm};
use tauri::State;

use crate::browse::{with_conn, DbState};

/// Every name form `name` resolves to, games-heavy first. Declared
/// aliases with no imported games yet appear with games = 0.
#[tauri::command]
pub async fn identity_group(
    state: State<'_, DbState>,
    name: String,
) -> Result<Vec<NameForm>, String> {
    with_conn(&state, |conn| {
        identity::resolve_identity(conn, &name).map_err(|e| e.to_string())
    })
}

/// Declare `a` and `b` to be the same person (merges groups as needed).
#[tauri::command]
pub async fn alias_declare(
    state: State<'_, DbState>,
    a: String,
    b: String,
) -> Result<Vec<NameForm>, String> {
    with_conn(&state, |conn| {
        identity::declare_alias(conn, &a, &b).map_err(|e| e.to_string())?;
        identity::resolve_identity(conn, &a).map_err(|e| e.to_string())
    })
}

/// Remove `name` from its declared alias group.
#[tauri::command]
pub async fn alias_remove(state: State<'_, DbState>, name: String) -> Result<(), String> {
    with_conn(&state, |conn| {
        identity::remove_alias(conn, &name).map_err(|e| e.to_string())
    })
}

#[cfg(test)]
mod tests {
    use kibitz_db::identity;

    #[test]
    fn declared_aliases_resolve_transitively_with_lexical_variants() {
        let dir = tempfile::tempdir().unwrap();
        let conn = kibitz_db::db::open(&dir.path().join("t.sqlite")).unwrap();
        // Two lexical variants of the same OTB name + one unrelated handle.
        conn.execute_batch(
            "INSERT INTO players (name) VALUES ('O''Connor, Shawn');
             INSERT INTO players (name) VALUES ('Shawn O''Connor');
             INSERT INTO players (name) VALUES ('avienu');
             INSERT INTO players (name) VALUES ('Somebody, Else');",
        )
        .unwrap();

        // Lexical only: the two OTB forms merge, the handle does not.
        let forms = identity::resolve_identity(&conn, "O'Connor, Shawn").unwrap();
        let names: Vec<&str> = forms.iter().map(|f| f.name.as_str()).collect();
        assert!(names.contains(&"O'Connor, Shawn") && names.contains(&"Shawn O'Connor"));
        assert!(!names.contains(&"avienu"));

        // Declare the handle once; resolution from ANY form now includes
        // all three, and never the stranger.
        identity::declare_alias(&conn, "avienu", "O'Connor, Shawn").unwrap();
        for probe in ["avienu", "Shawn O'Connor", "O'Connor, Shawn"] {
            let names: Vec<String> = identity::resolve_identity(&conn, probe)
                .unwrap()
                .into_iter()
                .map(|f| f.name)
                .collect();
            assert!(
                names.contains(&"avienu".to_string())
                    && names.contains(&"O'Connor, Shawn".to_string())
                    && names.contains(&"Shawn O'Connor".to_string()),
                "probe {probe}: {names:?}"
            );
            assert!(!names.contains(&"Somebody, Else".to_string()));
        }

        // Removal restores separation.
        identity::remove_alias(&conn, "avienu").unwrap();
        let names: Vec<String> = identity::resolve_identity(&conn, "avienu")
            .unwrap()
            .into_iter()
            .map(|f| f.name)
            .collect();
        assert!(!names.contains(&"O'Connor, Shawn".to_string()));
    }
}

/// Boot-time identity auto-link (see kibitz_db::identity::
/// auto_link_sync_handles): connect the user's linked-account handles to
/// their self identity, once each, honestly reported. No-op without a
/// self player or linked accounts.
#[tauri::command]
pub async fn auto_link_identities(state: State<'_, DbState>) -> Result<Vec<String>, String> {
    with_conn(&state, |conn| {
        let self_name: Option<String> = conn
            .query_row(
                "SELECT value FROM meta WHERE key = 'self_player'",
                [],
                |r| r.get(0),
            )
            .ok();
        let Some(self_name) = self_name.filter(|n| !n.trim().is_empty()) else {
            return Ok(Vec::new());
        };
        identity::auto_link_sync_handles(conn, self_name.trim()).map_err(|e| e.to_string())
    })
}
