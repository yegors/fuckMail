//! Historical test schemas, derived from their recorded migration SQL.
//! This module is never compiled into an application or exposed over IPC.
use rusqlite::Connection;
use std::collections::{BTreeMap, BTreeSet};

pub(crate) fn restore_schema_twelve(db: &Connection) {
    restore_schema_fourteen(db);
    db.execute_batch("DROP INDEX IF EXISTS google_send_recovery; DROP INDEX google_drafts_due; DROP INDEX capture_execution_kind; ALTER TABLE drafts DROP COLUMN google_json; ALTER TABLE capture_operations DROP COLUMN execution_kind; PRAGMA user_version=12;").unwrap();
}

pub(crate) fn restore_schema_fourteen(db: &Connection) {
    restore_schema_fifteen(db);
    db.execute_batch("DROP TABLE google_draft_sources; DROP TABLE google_draft_inventory; DROP INDEX google_draft_known_target; DROP INDEX google_draft_known_ack; DROP INDEX google_send_known_target; DROP INDEX google_draft_baseline_target; DROP INDEX google_draft_unknown_create; DROP INDEX drafts_count; DROP INDEX google_drafts_due; ALTER TABLE drafts DROP COLUMN origin;").unwrap();
    let old_index = include_str!("migration-014-google-send.sql")
        .split_once("CREATE INDEX google_drafts_due")
        .unwrap()
        .1
        .split_once("CREATE INDEX google_send_recovery")
        .unwrap()
        .0;
    db.execute_batch(&format!(
        "CREATE INDEX google_drafts_due{old_index}\nPRAGMA user_version=14;"
    ))
    .unwrap();
}

pub(crate) fn restore_schema_fifteen(db: &Connection) {
    restore_schema_sixteen(db);
    db.execute_batch("DROP INDEX IF EXISTS google_draft_adoption_target; DROP INDEX IF EXISTS google_delete_target; DROP INDEX IF EXISTS google_delete_recovery; DROP INDEX IF EXISTS google_delete_history; DROP INDEX google_drafts_due;").unwrap();
    let old_index = include_str!("migration-015-google-draft-inventory.sql")
        .split_once("CREATE INDEX google_drafts_due")
        .unwrap()
        .1
        .split_once("CREATE INDEX google_draft_source_duplicate")
        .unwrap()
        .0;
    db.execute_batch(&format!(
        "CREATE INDEX google_drafts_due{old_index}\nPRAGMA user_version=15;"
    ))
    .unwrap();
}

pub(crate) fn restore_schema_seventeen(db: &Connection) {
    db.execute_batch("DROP INDEX accounts_active; ALTER TABLE accounts DROP COLUMN cleanup_json; ALTER TABLE accounts DROP COLUMN lifecycle_revision; ALTER TABLE accounts DROP COLUMN lifecycle_state; PRAGMA user_version=17;").unwrap();
}
pub(crate) fn restore_schema_sixteen(db: &Connection) {
    restore_schema_seventeen(db);
    db.pragma_update(None, "user_version", 16).unwrap();
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
