use crate::{err, Result, Store};
use serde_json::Value;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

impl Store {
    /// The token belongs to one request. It is never reused for another owner.
    /// Reads can stop inside SQLite; a mutation that has started finishes its
    /// short transaction so reconnecting clients can recover its journal entry.
    pub fn handle_cancellable(
        &mut self,
        method: &str,
        account: Option<&str>,
        params: Value,
        cancelled: Arc<AtomicBool>,
    ) -> Result<Value> {
        if cancelled.load(Ordering::Acquire) {
            return Err(err("cancelled", "Request owner disconnected"));
        }
        if matches!(
            method,
            "accounts"
                | "status"
                | "conversations"
                | "conversation"
                | "draft_get"
                | "calendar"
                | "categories"
                | "view_get"
        ) {
            self.with_cancellable_read(cancelled, |store| store.handle(method, account, params))
        } else {
            self.handle(method, account, params)
        }
    }

    fn with_cancellable_read<T>(
        &mut self,
        cancelled: Arc<AtomicBool>,
        query: impl FnOnce(&mut Self) -> Result<T>,
    ) -> Result<T> {
        let token = Arc::clone(&cancelled);
        self.db
            .progress_handler(1000, Some(move || token.load(Ordering::Acquire)));
        let result = query(self);
        // Remove the old owner's callback before the single writer accepts any
        // next request. A late cancellation cannot interrupt that next owner.
        self.db.progress_handler(0, None::<fn() -> bool>);
        if cancelled.load(Ordering::Acquire) {
            Err(err("cancelled", "Request owner disconnected"))
        } else {
            result
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn active_sqlite_read_stops_without_cancelling_the_next_owner() {
        let temp = tempfile::tempdir().unwrap();
        let mut store = Store::open(temp.path()).unwrap();
        let cancelled = Arc::new(AtomicBool::new(false));
        let token = Arc::clone(&cancelled);
        let cancel = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(10));
            token.store(true, Ordering::Release);
        });
        let result = store.with_cancellable_read(cancelled, |store| {
            let value: i64 = store.db.query_row(
                "WITH RECURSIVE n(x) AS (VALUES(1) UNION ALL SELECT x+1 FROM n WHERE x<1000000000) SELECT sum(x) FROM n", [], |r| r.get(0))?;
            Ok(value)
        });
        cancel.join().unwrap();
        assert_eq!(result.unwrap_err().code, "cancelled");
        let status = store
            .handle_cancellable("status", None, json!({}), Arc::new(AtomicBool::new(false)))
            .unwrap();
        assert_eq!(status["schema_version"], crate::SCHEMA_VERSION);
        assert!(store
            .handle(
                "conversations",
                Some("windscribe"),
                json!({"query":"review"})
            )
            .is_ok());
    }

    #[test]
    fn cancelled_queued_mutation_has_no_journal_or_draft_effect() {
        let temp = tempfile::tempdir().unwrap();
        let mut store = Store::open(temp.path()).unwrap();
        let params = json!({"conversation_id":"windscribe-thread-000", "body":"Do not save", "expected_revision":0, "request_id":"cancelled-before-work"});
        let result = store.handle_cancellable(
            "draft_save",
            Some("windscribe"),
            params,
            Arc::new(AtomicBool::new(true)),
        );
        assert_eq!(result.unwrap_err().code, "cancelled");
        let count: i64 = store
            .db
            .query_row("SELECT COUNT(*) FROM operations", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0);
        assert_eq!(
            store
                .handle(
                    "draft_get",
                    Some("windscribe"),
                    json!({"conversation_id":"windscribe-thread-000"})
                )
                .unwrap()["revision"],
            0
        );
    }
}
