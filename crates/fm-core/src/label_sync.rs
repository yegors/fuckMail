//! Explicit Google label-name merge and durable create admission.
use crate::{err, id, Result, Store};
use fm_google::auth::ScopeSet;
use rusqlite::{params, OptionalExtension};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

pub(crate) fn initial_import_key(account: &str) -> String {
    format!("labels_initial_import:{account}")
}

impl Store {
    pub fn google_label_memberships_queue(&self, account: &str) -> Result<usize> {
        self.label_sync_authority(account)?;
        let tx = self.db.unchecked_transaction()?;
        let queued = crate::google_live_mutations::enqueue_bound_categories(&tx, account)?;
        tx.commit()?;
        Ok(queued)
    }
    fn label_sync_authority(&self, account: &str) -> Result<()> {
        self.require_active_account(account)?;
        if !crate::preferences::google_write_enabled(&self.db, account, "mail")?
            || !self.google_account_bindings()?.iter().any(|binding| {
                binding.account == account && ScopeSet::new(&binding.scopes).mail_write()
            })
        {
            return Err(err(
                "capability_unavailable",
                "Google mail editing permission is required to sync labels",
            ));
        }
        Ok(())
    }
    pub fn google_labels_merge(&mut self, account: &str, request: &str) -> Result<Value> {
        self.label_sync_authority(account)?;
        id(request)?;
        let state = self.google_labels(account, &json!({"limit":1}))?;
        if state["available"] != true || state["stale_connection"] == true {
            return Err(err(
                "label_catalog_required",
                "Refresh Google labels before syncing names",
            ));
        }
        let remote=self.db.prepare("SELECT label_id,name FROM google_label_catalog_items WHERE account=?1 AND generation=(SELECT generation FROM google_label_catalogs WHERE account=?1) AND label_type='user' ORDER BY label_id")?.query_map([account],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
        let mut pulled = 0;
        let mut renamed = 0;
        let mut skipped = 0;
        for (label, name) in remote {
            if name.len() > 256 {
                skipped += 1;
                continue;
            }
            let linked:Option<(String,String)>=self.db.query_row("SELECT c.id,c.name FROM categories c JOIN google_category_labels g ON g.account=c.account AND g.category_id=c.id WHERE c.account=?1 AND g.provider_label_id=?2",params![account,label],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
            if let Some((category, previous)) = linked {
                if previous != name {
                    let conflict:bool=self.db.query_row("SELECT EXISTS(SELECT 1 FROM categories WHERE account=?1 AND id!=?2 AND lower(name)=lower(?3))",params![account,category,name],|r|r.get(0))?;
                    if conflict {
                        skipped += 1;
                        continue;
                    }
                    self.db.execute(
                        "UPDATE categories SET name=?3 WHERE account=?1 AND id=?2",
                        params![account, category, name],
                    )?;
                    self.db
                        .execute("UPDATE meta SET value=value+1 WHERE key='revision'", [])?;
                    renamed += 1;
                }
                continue;
            }
            let candidates=self.db.prepare("SELECT c.id,c.name FROM categories c WHERE c.account=?1 AND NOT EXISTS(SELECT 1 FROM google_category_labels g WHERE g.account=c.account AND g.category_id=c.id)")?.query_map([account],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
            let mut matches = candidates
                .iter()
                .filter(|(_, n)| n.to_lowercase() == name.to_lowercase());
            let existing = matches.next().map(|(id, _)| id.clone());
            if matches.next().is_some() {
                skipped += 1;
                continue;
            }
            let category = if let Some(category) = existing {
                category
            } else {
                let suffix = format!(
                    "{:x}",
                    Sha256::digest(format!("{request}\n{label}").as_bytes())
                );
                let category = format!("google-{}", &suffix[..32]);
                let result = self.label_edit(
                    account,
                    "label_create",
                    &json!({"request_id":format!("import-{suffix}"),"id":category,"name":name}),
                );
                match result {
                    Ok(_) => {
                        pulled += 1;
                        category
                    }
                    Err(e) if e.code == "already_exists" || e.code == "limit_exceeded" => {
                        skipped += 1;
                        continue;
                    }
                    Err(e) => return Err(e),
                }
            };
            let bind_request = format!(
                "bind-{:x}",
                Sha256::digest(format!("{request}\n{category}\n{label}").as_bytes())
            );
            self.google_category_bind(account,&json!({"request_id":bind_request,"category_id":category,"provider_label_id":label,"expected_provider_label_id":null,"expected_catalog_revision":state["catalog_revision"]}))?;
            self.db.execute(
                "UPDATE categories SET name=?3 WHERE account=?1 AND id=?2",
                params![account, category, name],
            )?;
            self.db.execute("UPDATE google_label_creations SET state='created',provider_label_id=?3,error_code=NULL WHERE account=?1 AND category_id=?2 AND state='unknown'",params![account,category,label])?;
        }
        let pending=self.db.prepare("SELECT c.id,c.name,g.state FROM categories c LEFT JOIN google_label_creations g ON g.account=c.account AND g.category_id=c.id WHERE c.account=?1 AND NOT EXISTS(SELECT 1 FROM google_category_labels m WHERE m.account=c.account AND m.category_id=c.id) ORDER BY c.id")?.query_map([account],|r|Ok(json!({"id":r.get::<_,String>(0)?,"name":r.get::<_,String>(1)?,"state":r.get::<_,Option<String>>(2)?})))?.collect::<rusqlite::Result<Vec<_>>>()?;
        let missing:i64=self.db.query_row("SELECT count(*) FROM google_category_labels g WHERE g.account=?1 AND NOT EXISTS(SELECT 1 FROM google_label_catalog_items i WHERE i.account=g.account AND i.generation=(SELECT generation FROM google_label_catalogs WHERE account=g.account) AND i.label_id=g.provider_label_id AND i.label_type='user')",[account],|r|r.get(0))?;
        // Any completed merge satisfies a new account's pending first import.
        self.db.execute(
            "DELETE FROM meta WHERE key=?1",
            [initial_import_key(account)],
        )?;
        Ok(
            json!({"pulled":pulled,"renamed":renamed,"skipped":skipped,"pending":pending,"missing":missing}),
        )
    }
    /// Import a newly added account's Gmail labels once, after its first catalog
    /// refresh. This only links existing Gmail labels; it never creates labels at
    /// Google. Returns None when no import is pending or mail editing is unavailable.
    pub fn google_labels_initial_import(&mut self, account: &str) -> Result<Option<Value>> {
        let key = initial_import_key(account);
        let pending: bool = self.db.query_row(
            "SELECT EXISTS(SELECT 1 FROM meta WHERE key=?1)",
            [&key],
            |r| r.get(0),
        )?;
        if !pending {
            return Ok(None);
        }
        match self.google_labels_merge(account, "initial-import") {
            Ok(result) => {
                if result["pulled"].as_u64().is_some_and(|pulled| pulled > 0) {
                    // Mail classified before the import saw no labels; queue it again.
                    self.local_model_requeue_account(account)?;
                }
                Ok(Some(result))
            }
            // Linking labels needs mail editing; read-only accounts keep the manual path.
            Err(e) if e.code == "capability_unavailable" => {
                self.db.execute("DELETE FROM meta WHERE key=?1", [&key])?;
                Ok(None)
            }
            Err(e) => Err(e),
        }
    }
    pub fn google_label_create_begin(
        &self,
        account: &str,
        category: &str,
        name: &str,
    ) -> Result<bool> {
        self.label_sync_authority(account)?;
        id(category)?;
        id(name)?;
        let tx = self.db.unchecked_transaction()?;
        let current:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM categories c WHERE account=?1 AND id=?2 AND name=?3 AND NOT EXISTS(SELECT 1 FROM google_category_labels g WHERE g.account=c.account AND g.category_id=c.id))",params![account,category,name],|r|r.get(0))?;
        if !current {
            return Ok(false);
        }
        let state: Option<(String, String)> = tx
            .query_row(
                "SELECT name,state FROM google_label_creations WHERE account=?1 AND category_id=?2",
                params![account, category],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        if state
            .as_ref()
            .is_some_and(|(old, state)| old != name || state != "rejected")
        {
            return Ok(false);
        }
        tx.execute("INSERT INTO google_label_creations(account,category_id,name,state) VALUES(?1,?2,?3,'unknown') ON CONFLICT(account,category_id) DO UPDATE SET state='unknown',error_code=NULL",params![account,category,name])?;
        tx.commit()?;
        Ok(true)
    }
    pub fn google_label_create_finish(
        &self,
        account: &str,
        category: &str,
        name: &str,
        provider: Option<&str>,
        rejected: bool,
        error: Option<&str>,
    ) -> Result<()> {
        // Recording an accepted outcome survives owner cancellation. It cannot restore a removed category.
        let tx = self.db.unchecked_transaction()?;
        let changed = tx.execute("UPDATE google_label_creations SET state=?4,provider_label_id=?5,error_code=?6 WHERE account=?1 AND category_id=?2 AND name=?3 AND state='unknown'",params![account,category,name,if provider.is_some(){"created"}else if rejected{"rejected"}else{"unknown"},provider,error])?;
        if changed == 0 {
            return Ok(());
        }
        if let Some(provider) = provider {
            id(provider)?;
            let current:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM categories c JOIN accounts a ON a.id=c.account WHERE c.account=?1 AND c.id=?2 AND c.name=?3 AND a.lifecycle_state='active' AND NOT EXISTS(SELECT 1 FROM google_category_labels g WHERE g.account=c.account AND g.category_id=c.id))",params![account,category,name],|r|r.get(0))?;
            if current {
                self.google_map_category(account, category, provider)?;
                self.project_google_category(account, category, provider)?;
            }
        }
        tx.execute("UPDATE meta SET value=value+1 WHERE key='revision'", [])?;
        tx.commit()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn accepted_creation_cannot_restore_removed_or_retarget_reused_identity() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::open_live(dir.path()).unwrap();
        let account = store
            .bind_google_account(
                "label-create-owner",
                "owner@example.test",
                &std::collections::BTreeSet::from([
                    "https://www.googleapis.com/auth/gmail.modify".to_owned()
                ]),
                None,
            )
            .unwrap();
        let create = json!({"id":"original","name":"Research","request_id":"create-original"});
        store.label_edit(&account, "label_create", &create).unwrap();
        assert!(store
            .google_label_create_begin(&account, "original", "Research")
            .unwrap());
        assert!(!store
            .google_label_create_begin(&account, "original", "Research")
            .unwrap());
        store.label_edit(&account,"label_remove",&json!({"id":"original","expected_name":"Research","expected_provider_label_id":null,"request_id":"remove-original"})).unwrap();
        let mut reuse = create.clone();
        reuse["request_id"] = json!("reuse-id");
        assert_eq!(
            store
                .label_edit(&account, "label_create", &reuse)
                .unwrap_err()
                .code,
            "already_exists"
        );
        store
            .google_label_create_finish(
                &account,
                "original",
                "Research",
                Some("Label_original"),
                false,
                None,
            )
            .unwrap();
        assert!(store.label_catalog(&account).unwrap()["categories"]
            .as_array()
            .unwrap()
            .is_empty());
        assert_eq!(
            store
                .db
                .query_row(
                    "SELECT state FROM google_label_creations WHERE account=?1",
                    [&account],
                    |r| r.get::<_, String>(0)
                )
                .unwrap(),
            "created"
        );
        store
            .google_label_create_finish(
                &account,
                "original",
                "Research",
                Some("Label_wrong"),
                false,
                None,
            )
            .unwrap();
        assert_eq!(
            store
                .db
                .query_row(
                    "SELECT provider_label_id FROM google_label_creations WHERE account=?1",
                    [&account],
                    |r| r.get::<_, String>(0)
                )
                .unwrap(),
            "Label_original"
        );
    }
}
