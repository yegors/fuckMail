//! Historical test schemas, derived from their recorded migration SQL.
//! This module is never compiled into an application or exposed over IPC.
use rusqlite::Connection;
use std::collections::{BTreeMap, BTreeSet};

pub(crate) fn restore_schema_twelve(db: &Connection) {
    db.execute_batch("DROP INDEX IF EXISTS google_send_recovery; DROP INDEX google_drafts_due; DROP INDEX capture_execution_kind; ALTER TABLE drafts DROP COLUMN google_json; ALTER TABLE capture_operations DROP COLUMN execution_kind; PRAGMA user_version=12;").unwrap();
}

/// Remove only post-schema-8 structures from a disposable seeded test profile.
/// Merely lowering user_version leaves an invalid migration fixture behind.
pub(crate) fn restore_schema_eight(db: &Connection) {
    let historical = Connection::open_in_memory().unwrap();
    for sql in [
        include_str!("schema.sql"),
        include_str!("migration-003-drafts.sql"),
        include_str!("migration-003-google.sql"),
        include_str!("migration-004-calendar.sql"),
        include_str!("migration-004-assist.sql"),
        include_str!("migration-005.sql"),
        include_str!("migration-006.sql"),
        include_str!("migration-007.sql"),
        include_str!("migration-008.sql"),
    ] {
        historical.execute_batch(sql).unwrap();
    }
    let objects = |connection: &Connection| -> BTreeMap<String, String> {
        connection.prepare("SELECT name,type FROM sqlite_schema WHERE name NOT LIKE 'sqlite_%' AND sql IS NOT NULL")
            .unwrap().query_map([], |r| Ok((r.get(0)?,r.get(1)?))).unwrap()
            .collect::<rusqlite::Result<_>>().unwrap()
    };
    let old = objects(&historical);
    let current = objects(db);
    let quoted = |name: &str| format!("\"{}\"", name.replace('"', "\"\""));
    db.execute_batch("PRAGMA foreign_keys=OFF;").unwrap();
    for kind in ["trigger", "view", "index", "table"] {
        for (name, ty) in &current {
            if ty == kind && !old.contains_key(name) {
                db.execute_batch(&format!(
                    "DROP {} {};",
                    kind.to_ascii_uppercase(),
                    quoted(name)
                ))
                .unwrap();
            }
        }
    }
    let columns = |connection: &Connection, name: &str| -> BTreeSet<String> {
        connection
            .prepare("SELECT name FROM pragma_table_info(?1)")
            .unwrap()
            .query_map([name], |r| r.get(0))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap()
    };
    for (name, kind) in &old {
        if kind != "table" {
            continue;
        }
        let old_columns = columns(&historical, name);
        for column in columns(db, name).difference(&old_columns) {
            db.execute_batch(&format!(
                "ALTER TABLE {} DROP COLUMN {};",
                quoted(name),
                quoted(column)
            ))
            .unwrap();
        }
    }
    db.execute_batch("PRAGMA user_version=8; PRAGMA foreign_keys=ON;")
        .unwrap();
    assert_eq!(objects(db), old);
    assert!(db
        .prepare("PRAGMA foreign_key_check")
        .unwrap()
        .query([])
        .unwrap()
        .next()
        .unwrap()
        .is_none());
}
