//! Player-name identity (run 8.5, maintainer report): the same person
//! appears under different name forms across sources — SCID-style
//! "O'Connor, Shawn" vs online "Shawn O'Connor". Profile, prep and
//! fingerprint should see ONE player.
//!
//! Matching key: lowercase, ASCII-fold common Latin diacritics, strip
//! punctuation, tokenize, SORT tokens — so comma order, apostrophes and
//! accents all wash out. Exact token-set equality only: "O'Connor, S."
//! does NOT auto-merge with "O'Connor, Shawn" (an initial could be a
//! different person); such near-forms stay separate and visible. Every
//! merge is surfaced to the UI so a false merge is never silent.

use rusqlite::Connection;

/// One name form belonging to an identity group.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NameForm {
    pub player_id: i64,
    pub name: String,
    pub games: u32,
}

/// Fold common Latin-1/Latin-Extended letters to ASCII.
fn fold_char(c: char) -> char {
    match c {
        'à'..='å' | 'ā' | 'ă' | 'ą' => 'a',
        'ç' | 'ć' | 'č' => 'c',
        'è'..='ë' | 'ē' | 'ĕ' | 'ė' | 'ę' | 'ě' => 'e',
        'ì'..='ï' | 'ī' | 'į' => 'i',
        'ñ' | 'ń' | 'ň' => 'n',
        'ò'..='ö' | 'ø' | 'ō' | 'ő' => 'o',
        'ù'..='ü' | 'ū' | 'ů' | 'ű' => 'u',
        'ý' | 'ÿ' => 'y',
        'š' | 'ś' => 's',
        'ž' | 'ź' | 'ż' => 'z',
        'ł' | 'ľ' | 'ĺ' => 'l',
        'đ' | 'ď' => 'd',
        'ř' | 'ŕ' => 'r',
        'ť' => 't',
        other => other,
    }
}

/// Normalized identity key: sorted lowercase ASCII-folded tokens.
pub fn identity_key(name: &str) -> String {
    let mut tokens: Vec<String> = name
        .to_lowercase()
        .chars()
        .map(fold_char)
        // Apostrophes and periods JOIN (O'Connor -> oconnor, "St." -> st);
        // every other separator splits.
        .filter(|c| !matches!(c, '\'' | '.'))
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .map(str::to_string)
        .collect();
    tokens.sort();
    tokens.join(" ")
}

/// All name forms sharing `name`'s identity key, with game counts,
/// largest first. Always contains at least the queried name when it
/// exists in the database.
pub fn identity_group(conn: &Connection, name: &str) -> rusqlite::Result<Vec<NameForm>> {
    let key = identity_key(name);
    let mut stmt = conn.prepare_cached(
        "SELECT p.id, p.name,
                (SELECT COUNT(*) FROM games g
                 WHERE g.white_id = p.id OR g.black_id = p.id)
         FROM players p",
    )?;
    let mut out: Vec<NameForm> = stmt
        .query_map([], |r| {
            Ok(NameForm {
                player_id: r.get(0)?,
                name: r.get(1)?,
                games: r.get::<_, i64>(2)? as u32,
            })
        })?
        .filter_map(Result::ok)
        .filter(|f| identity_key(&f.name) == key)
        .collect();
    out.sort_by(|a, b| b.games.cmp(&a.games).then(a.name.cmp(&b.name)));
    Ok(out)
}

/// The player ids of `name`'s identity group (empty when unknown).
pub fn identity_ids(conn: &Connection, name: &str) -> rusqlite::Result<Vec<i64>> {
    Ok(identity_group(conn, name)?
        .into_iter()
        .map(|f| f.player_id)
        .collect())
}

/// Declare that `a` and `b` are the same person. Joins existing groups
/// when either name is already in one (merging two groups if needed).
pub fn declare_alias(conn: &Connection, a: &str, b: &str) -> rusqlite::Result<()> {
    let find = |name: &str| -> rusqlite::Result<Option<i64>> {
        conn.query_row(
            "SELECT group_id FROM alias_members WHERE name = ?1",
            [name],
            |r| r.get(0),
        )
        .map(Some)
        .or_else(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => Ok(None),
            other => Err(other),
        })
    };
    let (ga, gb) = (find(a)?, find(b)?);
    match (ga, gb) {
        (None, None) => {
            conn.execute("INSERT INTO alias_groups (label) VALUES (?1)", [a])?;
            let gid = conn.last_insert_rowid();
            conn.execute(
                "INSERT INTO alias_members (group_id, name) VALUES (?1, ?2), (?1, ?3)",
                rusqlite::params![gid, a, b],
            )?;
        }
        (Some(g), None) => {
            conn.execute(
                "INSERT INTO alias_members (group_id, name) VALUES (?1, ?2)",
                rusqlite::params![g, b],
            )?;
        }
        (None, Some(g)) => {
            conn.execute(
                "INSERT INTO alias_members (group_id, name) VALUES (?1, ?2)",
                rusqlite::params![g, a],
            )?;
        }
        (Some(g1), Some(g2)) if g1 != g2 => {
            conn.execute(
                "UPDATE alias_members SET group_id = ?1 WHERE group_id = ?2",
                rusqlite::params![g1, g2],
            )?;
            conn.execute("DELETE FROM alias_groups WHERE id = ?1", [g2])?;
        }
        _ => {}
    }
    Ok(())
}

/// Remove one name from its declared group (the group survives for the
/// remaining members; a group of one is deleted).
pub fn remove_alias(conn: &Connection, name: &str) -> rusqlite::Result<()> {
    let gid: Option<i64> = conn
        .query_row(
            "SELECT group_id FROM alias_members WHERE name = ?1",
            [name],
            |r| r.get(0),
        )
        .map(Some)
        .or_else(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => Ok(None),
            other => Err(other),
        })?;
    if let Some(gid) = gid {
        conn.execute("DELETE FROM alias_members WHERE name = ?1", [name])?;
        let left: i64 = conn.query_row(
            "SELECT COUNT(*) FROM alias_members WHERE group_id = ?1",
            [gid],
            |r| r.get(0),
        )?;
        if left <= 1 {
            conn.execute("DELETE FROM alias_members WHERE group_id = ?1", [gid])?;
            conn.execute("DELETE FROM alias_groups WHERE id = ?1", [gid])?;
        }
    }
    Ok(())
}

/// Full identity resolution: the transitive closure of lexical variants
/// (identity_key equality) and DECLARED aliases, starting from `name`.
/// Returns every matching name form in the database plus declared names
/// that have no games yet (games = 0, player_id = -1).
pub fn resolve_identity(conn: &Connection, name: &str) -> rusqlite::Result<Vec<NameForm>> {
    use std::collections::BTreeSet;
    let mut keys: BTreeSet<String> = BTreeSet::new();
    let mut declared: BTreeSet<String> = BTreeSet::new();
    let mut frontier: Vec<String> = vec![name.to_string()];
    declared.insert(name.to_string());

    while let Some(n) = frontier.pop() {
        let key = identity_key(&n);
        if keys.insert(key.clone()) {
            // New lexical key: every declared-alias partner of any name
            // sharing this key joins the frontier.
            let mut stmt = conn.prepare_cached(
                "SELECT m2.name FROM alias_members m1
                 JOIN alias_members m2 ON m2.group_id = m1.group_id
                 WHERE m1.name = ?1",
            )?;
            // Partners of the exact queried spelling:
            let partners: Vec<String> = stmt
                .query_map([&n], |r| r.get::<_, String>(0))?
                .filter_map(Result::ok)
                .collect();
            for p in partners {
                if declared.insert(p.clone()) {
                    frontier.push(p);
                }
            }
            // And of every db name form under this lexical key:
            let mut all = conn.prepare_cached("SELECT name FROM players")?;
            let forms: Vec<String> = all
                .query_map([], |r| r.get::<_, String>(0))?
                .filter_map(Result::ok)
                .filter(|f| identity_key(f) == key)
                .collect();
            for f in forms {
                let mut stmt2 = conn.prepare_cached(
                    "SELECT m2.name FROM alias_members m1
                     JOIN alias_members m2 ON m2.group_id = m1.group_id
                     WHERE m1.name = ?1",
                )?;
                let partners: Vec<String> = stmt2
                    .query_map([&f], |r| r.get::<_, String>(0))?
                    .filter_map(Result::ok)
                    .collect();
                if declared.insert(f.clone()) {
                    frontier.push(f.clone());
                }
                for p in partners {
                    if declared.insert(p.clone()) {
                        frontier.push(p);
                    }
                }
            }
        }
    }

    // Materialize: every db player whose key ∈ keys or whose exact name
    // was declared; declared-but-absent names appended with zero games.
    let mut stmt = conn.prepare_cached(
        "SELECT p.id, p.name,
                (SELECT COUNT(*) FROM games g
                 WHERE g.white_id = p.id OR g.black_id = p.id)
         FROM players p",
    )?;
    let mut out: Vec<NameForm> = stmt
        .query_map([], |r| {
            Ok(NameForm {
                player_id: r.get(0)?,
                name: r.get(1)?,
                games: r.get::<_, i64>(2)? as u32,
            })
        })?
        .filter_map(Result::ok)
        .filter(|f| keys.contains(&identity_key(&f.name)) || declared.contains(&f.name))
        .collect();
    for d in &declared {
        if !out.iter().any(|f| &f.name == d) {
            out.push(NameForm {
                player_id: -1,
                name: d.clone(),
                games: 0,
            });
        }
    }
    out.sort_by(|a, b| b.games.cmp(&a.games).then(a.name.cmp(&b.name)));
    Ok(out)
}

/// Player ids for the resolved identity (games-bearing forms only).
pub fn resolve_identity_ids(conn: &Connection, name: &str) -> rusqlite::Result<Vec<i64>> {
    Ok(resolve_identity(conn, name)?
        .into_iter()
        .filter(|f| f.player_id >= 0)
        .map(|f| f.player_id)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comma_flip_apostrophes_and_case_wash_out() {
        assert_eq!(
            identity_key("O'Connor, Shawn"),
            identity_key("Shawn O'Connor")
        );
        assert_eq!(
            identity_key("O'Connor, Shawn"),
            identity_key("shawn oconnor")
        );
        assert_eq!(identity_key("Müller, Hans"), identity_key("Hans Muller"));
        assert_eq!(identity_key("Kráľ, Ján"), identity_key("jan kral"));
    }

    #[test]
    fn initials_and_different_people_stay_separate() {
        assert_ne!(
            identity_key("O'Connor, Shawn"),
            identity_key("O'Connor, S.")
        );
        assert_ne!(
            identity_key("O'Connor, Shawn"),
            identity_key("O'Connor, Sean")
        );
        assert_ne!(identity_key("Polgar, Judit"), identity_key("Polgar, Sofia"));
    }
}
// (identity-grouped suggestion tests live here because they exercise the
// identity_key/alias grouping that fingerprint::matching_players applies.)
#[cfg(test)]
mod suggestion_tests {
    #[test]
    fn suggestions_collapse_identity_variants_to_the_games_heaviest_form() {
        let dir = tempfile::tempdir().unwrap();
        let conn = crate::db::open(&dir.path().join("t.sqlite")).unwrap();
        conn.execute_batch(
            "INSERT INTO players (id, name) VALUES (1, 'O''Connor, Shawn');
             INSERT INTO players (id, name) VALUES (2, 'Shawn O''Connor');
             INSERT INTO players (id, name) VALUES (3, 'Connor,Stephen J');
             INSERT INTO sources (name, origin, license, kind)
               VALUES ('t', 't', 't', 'personal');
             INSERT INTO games (source_id, white_id, black_id, result, ply_count, movetext,
                                header_sig, moves_hash, encoding_version)
               VALUES (1, 1, 3, 1, 2, x'00', 'a', 1, 2),
                      (1, 3, 1, 2, 2, x'00', 'b', 2, 2),
                      (1, 2, 3, 1, 2, x'00', 'c', 3, 2);",
        )
        .unwrap();
        // Both O'Connor forms match "connor"; they collapse to the form
        // with more games. The unrelated Connor stays.
        let names = crate::fingerprint::matching_players(&conn, "Connor").unwrap();
        assert_eq!(
            names,
            vec![
                "Connor,Stephen J".to_string(),
                "O'Connor, Shawn".to_string()
            ],
            "one entry per identity, games-heaviest form wins"
        );

        // A declared alias chains transitively: declaring Stephen with
        // the comma form pulls in its lexical twin too, and the
        // games-heaviest form of the merged identity represents it.
        crate::identity::declare_alias(&conn, "Connor,Stephen J", "O'Connor, Shawn").unwrap();
        let names = crate::fingerprint::matching_players(&conn, "Connor").unwrap();
        assert_eq!(names, vec!["Connor,Stephen J".to_string()]);
    }
}
