//! In-memory invalidation of the GUI's independently loaded projections.

use serde_json::{json, Value};

const ACCOUNTS: u8 = 1;
const CATEGORIES: u8 = 2;
const READER: u8 = 4;
const DRAFTS: u8 = 8;
const CAPTURES: u8 = 16;
const ALL: u8 = ACCOUNTS | CATEGORIES | READER | DRAFTS | CAPTURES;

#[derive(Clone, Copy, Default)]
pub(super) struct UiChanges([u64; 5]);

impl UiChanges {
    pub(super) fn invalidate(&mut self) {
        self.advance(ALL);
    }

    pub(super) fn observe(&mut self, table: &str) {
        // Keep this dependency list conservative. New tables refresh everything
        // until their consumers have been checked. The hook already counts each
        // changed row; no extra database query or persisted counter is needed.
        let mask = match table {
            "accounts" | "preferences" => ALL,
            "account_metadata" => ACCOUNTS | DRAFTS,
            "messages" => ACCOUNTS | CATEGORIES | READER | DRAFTS,
            "conversations" => ALL,
            "message_sender_kinds" => ACCOUNTS | READER,
            "categories" | "google_category_labels" | "message_categories" => CATEGORIES | READER,
            "attachments" | "attachment_sources" => ACCOUNTS | READER | DRAFTS,
            "drafts"
            | "draft_discards"
            | "draft_transfers"
            | "google_draft_sources"
            | "google_draft_inventory" => ACCOUNTS | DRAFTS,
            "capture_operations" => CAPTURES | DRAFTS,
            "unsubscribes"
            | "assist_results"
            | "assist_policies"
            | "assist_category_intents"
            | "local_model_results"
            | "local_model_failures"
            | "local_model_settings"
            | "local_model_settings_history"
            | "local_model_label_overrides"
            | "google_message_state"
            | "google_manual_intents" => READER,
            "meta"
            | "google_accounts"
            | "google_account_clients"
            | "google_live_lanes"
            | "google_mail_coverage"
            | "google_mail_metrics"
            | "google_checkpoints"
            | "calendar_sources"
            | "calendar_event_state"
            | "calendar_publication" => ACCOUNTS,
            // Staging, receipts, view state and derived indexes do not change
            // these projections. Publication writes invalidate their consumers.
            "operations"
            | "preference_operations"
            | "views"
            | "profile_identity"
            | "mail_state_log"
            | "sqlite_sequence"
            | "archive_blob_cleanup"
            | "google_staged_messages"
            | "google_page_commits"
            | "google_live_pages"
            | "google_live_page_items"
            | "google_mail_mutations"
            | "google_label_catalogs"
            | "google_label_catalog_items"
            | "google_label_creations"
            | "calendar_batch_commits"
            | "calendar_busy_snapshots"
            | "calendar_catalog"
            | "calendar_catalog_state"
            | "calendar_event_index"
            | "calendar_reminder_receipts"
            | "calendar_sync_items"
            | "calendar_sync_pages"
            | "calendar_sync_runs"
            | "calendar_sync_state"
            | "calendar_write_operations"
            | "link_hosts"
            | "message_link_hosts"
            | "link_index_state"
            | "people"
            | "message_people" => 0,
            _ if table.starts_with("message_fts") => ACCOUNTS,
            _ => ALL,
        };
        self.advance(mask);
    }

    fn advance(&mut self, mask: u8) {
        for (index, generation) in self.0.iter_mut().enumerate() {
            if mask & (1 << index) != 0 {
                *generation += 1;
            }
        }
    }

    pub(super) fn json(self, epoch: &str) -> Value {
        let [accounts, categories, reader, drafts, captures] = self.0;
        json!({"epoch":epoch,"accounts":accounts,"categories":categories,
            "reader":reader,"drafts":drafts,"captures":captures})
    }
}

#[cfg(test)]
mod tests {
    use crate::Store;
    use serde_json::{json, Value};

    fn status(store: &mut Store) -> Value {
        store.handle("status", None, json!({})).unwrap()["ui_generations"].clone()
    }

    fn changed(before: &Value, after: &Value) -> Vec<&'static str> {
        ["accounts", "categories", "reader", "drafts", "captures"]
            .into_iter()
            .filter(|key| before[key] != after[key])
            .collect()
    }

    #[test]
    fn ui_generations_follow_published_data_not_global_revision() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::open(dir.path()).unwrap();
        let baseline = status(&mut store);
        assert_eq!(baseline, status(&mut store));
        // A view-save still advances the global revision (and account metadata
        // freshness), but never needs another reader/draft/capture projection.
        store.handle("view_save", Some("windscribe"), json!({"view":{"category":"infra"},"expected_revision":0,"request_id":"scoped-view"})).unwrap();
        let view = status(&mut store);
        assert_eq!(changed(&baseline, &view), ["accounts"]);
        store
            .db
            .execute("UPDATE calendar_publication SET revision=revision+1", [])
            .unwrap();
        let calendar = status(&mut store);
        assert_eq!(changed(&view, &calendar), ["accounts"]);
        store.db.execute("UPDATE categories SET name=name || ' updated' WHERE account='windscribe' AND id='infra'", []).unwrap();
        let categories = status(&mut store);
        assert_eq!(changed(&calendar, &categories), ["categories", "reader"]);
        let draft = store.draft("windscribe", "windscribe-thread-000").unwrap();
        store.handle("draft_save", Some("windscribe"), json!({"conversation_id":"windscribe-thread-000","body":"Generated draft text","expected_revision":draft["revision"],"request_id":"scoped-draft"})).unwrap();
        let saved = status(&mut store);
        assert_eq!(changed(&categories, &saved), ["accounts", "drafts"]);
        store.db.execute("UPDATE messages SET body=body || ' updated' WHERE account='windscribe' AND conversation_id='windscribe-thread-000'", []).unwrap();
        let mail = status(&mut store);
        assert_eq!(
            changed(&saved, &mail),
            ["accounts", "categories", "reader", "drafts"]
        );
        assert_eq!(mail, status(&mut store));
    }

    #[test]
    fn ui_generations_conservatively_handle_unknown_writes_rollback_and_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::open(dir.path()).unwrap();
        let before = status(&mut store);
        store
            .db
            .execute_batch(
                "CREATE TABLE future_projection(value); INSERT INTO future_projection VALUES(1);",
            )
            .unwrap();
        let unknown = status(&mut store);
        assert_eq!(changed(&before, &unknown).len(), 5);
        // SQLite's truncate optimization bypasses row hooks.
        store
            .db
            .execute("DELETE FROM future_projection", [])
            .unwrap();
        let missed = status(&mut store);
        assert_eq!(changed(&unknown, &missed).len(), 5);
        store
            .db
            .execute_batch(
                "BEGIN; UPDATE categories SET name='Temporary' WHERE account='windscribe';",
            )
            .unwrap();
        let inside = status(&mut store);
        assert_eq!(changed(&missed, &inside).len(), 5);
        store.db.execute_batch("ROLLBACK").unwrap();
        let rolled_back = status(&mut store);
        assert_eq!(changed(&inside, &rolled_back).len(), 5);
        assert_eq!(rolled_back, status(&mut store));
        drop(store);
        let mut reopened = Store::open(dir.path()).unwrap();
        assert_ne!(rolled_back["epoch"], status(&mut reopened)["epoch"]);
    }
}
