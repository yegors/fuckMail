//! Account-local label editing. Historical receipts remain immutable.
use crate::{err, fields, id, string, Result, Store};
use fm_assist::ollama::Settings;
use rusqlite::{params, OptionalExtension};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

fn description<'a>(p: &'a Value, key: &str) -> Result<&'a str> {
    p[key]
        .as_str()
        .filter(|text| text.len() <= 512 && !text.contains('\0'))
        .ok_or_else(|| {
            err(
                "invalid_request",
                "Label instructions must be at most 512 UTF-8 bytes",
            )
        })
}

impl Store {
    pub fn label_catalog(&self, account: &str) -> Result<Value> {
        self.require_active_account(account)?;
        let state = self.local_model_settings()?;
        let settings: Settings = serde_json::from_value(state["settings"].clone())?;
        let descriptions = settings.accounts.get(account).map(|a| &a.descriptions);
        let auto_read = settings.accounts.get(account).map(|a| &a.auto_read);
        let auto_spam = settings.accounts.get(account).map(|a| &a.auto_spam);
        let mut catalog = self.categories(account)?;
        for row in catalog["categories"].as_array_mut().unwrap() {
            row["description"] = descriptions
                .and_then(|d| d.get(row["id"].as_str().unwrap()))
                .cloned()
                .unwrap_or_default()
                .into();
            let default_auto_read = row["name"]
                .as_str()
                .is_some_and(|name| name.eq_ignore_ascii_case("Receipts"));
            row["auto_read"] = auto_read
                .and_then(|choices| choices.get(row["id"].as_str().unwrap()))
                .copied()
                .unwrap_or(default_auto_read)
                .into();
            row["auto_spam"] = auto_spam
                .and_then(|choices| choices.get(row["id"].as_str().unwrap()))
                .copied()
                .unwrap_or(false)
                .into();
            row["creation_state"] = self
                .db
                .query_row(
                    "SELECT state FROM google_label_creations WHERE account=?1 AND category_id=?2",
                    params![account, row["id"].as_str().unwrap()],
                    |r| r.get::<_, String>(0),
                )
                .optional()?
                .into();
        }
        Ok(catalog)
    }
    pub fn label_edit(&mut self, account: &str, method: &str, p: &Value) -> Result<Value> {
        fields(
            p,
            match method {
                "label_create" => &["request_id", "id", "name"],
                "label_describe" => &["request_id", "id", "expected_description", "description"],
                "label_auto_read" => &["request_id", "id", "expected_auto_read", "auto_read"],
                "label_auto_spam" => &["request_id", "id", "expected_auto_spam", "auto_spam"],
                "label_remove" => &[
                    "request_id",
                    "id",
                    "expected_name",
                    "expected_provider_label_id",
                ],
                _ => return Err(err("unknown_method", "Unknown label action")),
            },
        )?;
        let request = string(p, "request_id")?;
        id(request)?;
        let category = string(p, "id")?;
        id(category)?;
        let fingerprint = format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&json!([method, p]))?)
        );
        let tx = self.db.unchecked_transaction()?;
        if let Some((prior, result)) = tx
            .query_row(
                "SELECT fingerprint,result_json FROM operations WHERE account=?1 AND request_id=?2",
                params![account, request],
                |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
            )
            .optional()?
        {
            if prior != fingerprint {
                return Err(err(
                    "request_id_conflict",
                    "This request ID belongs to another change",
                ));
            }
            return Ok(serde_json::from_str(&result)?);
        }
        self.require_active_account(account)?;
        let name: Option<String> = tx
            .query_row(
                "SELECT name FROM categories WHERE account=?1 AND id=?2",
                params![account, category],
                |r| r.get(0),
            )
            .optional()?;
        let state = self.local_model_settings()?;
        let mut settings: Settings = serde_json::from_value(state["settings"].clone())?;
        let mut settings_changed = false;
        // Label descriptions feed model classification; Auto-read and Spam do not.
        let mut descriptions_changed = false;
        match method {
            "label_create" => {
                if name.is_some() || tx.query_row("SELECT EXISTS(SELECT 1 FROM google_label_creations WHERE account=?1 AND category_id=?2)",params![account,category],|r|r.get::<_,bool>(0))? {
                    return Err(err("already_exists", "This label already exists"));
                }
                let name = string(p, "name")?.trim();
                if name.is_empty() || name.len() > 256 || name.chars().any(char::is_control) {
                    return Err(err("invalid_request", "Use a label name of 1–256 bytes"));
                }
                let names = tx
                    .prepare("SELECT name FROM categories WHERE account=?1")?
                    .query_map([account], |r| r.get::<_, String>(0))?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                if names.len() >= 64 {
                    return Err(err(
                        "limit_exceeded",
                        "This account supports up to 64 labels",
                    ));
                }
                if names
                    .iter()
                    .any(|n| n.to_lowercase() == name.to_lowercase())
                {
                    return Err(err(
                        "already_exists",
                        "A label with this name already exists",
                    ));
                }
                tx.execute(
                    "INSERT INTO categories(account,id,name) VALUES(?1,?2,?3)",
                    params![account, category, name],
                )?;
            }
            "label_describe" => {
                if name.is_none() {
                    return Err(err("not_found", "Label no longer exists"));
                }
                let description = description(p, "description")?;
                let expected = self::description(p, "expected_description")?;
                let policy = settings.accounts.entry(account.into()).or_default();
                if policy
                    .descriptions
                    .get(category)
                    .map(String::as_str)
                    .unwrap_or_default()
                    != expected
                {
                    return Err(err(
                        "stale_revision",
                        "The label instructions changed; reload before saving",
                    ));
                }
                if description.is_empty() {
                    policy.descriptions.remove(category);
                } else {
                    policy
                        .descriptions
                        .insert(category.into(), description.into());
                }
                settings
                    .validate()
                    .map_err(|message| err("invalid_request", &message))?;
                settings_changed = true;
                descriptions_changed = description != expected;
            }
            "label_auto_read" | "label_auto_spam" => {
                let Some(name) = name.as_deref() else {
                    return Err(err("not_found", "Label no longer exists"));
                };
                let spam = method == "label_auto_spam";
                let (field, title) = if spam {
                    ("auto_spam", "Spam")
                } else {
                    ("auto_read", "Auto-read")
                };
                let enabled = p[field]
                    .as_bool()
                    .ok_or_else(|| err("invalid_request", &format!("{title} must be on or off")))?;
                let expected = p[format!("expected_{field}").as_str()]
                    .as_bool()
                    .ok_or_else(|| {
                        err(
                            "invalid_request",
                            &format!("Supply the previous {title} value"),
                        )
                    })?;
                let policy = settings.accounts.entry(account.into()).or_default();
                let (choices, default) = if spam {
                    (&mut policy.auto_spam, false)
                } else {
                    (&mut policy.auto_read, name.eq_ignore_ascii_case("Receipts"))
                };
                let current = choices.get(category).copied().unwrap_or(default);
                if current != expected {
                    return Err(err(
                        "stale_revision",
                        &format!("{title} changed; reload before saving"),
                    ));
                }
                // Spam is off unless chosen, so only chosen labels are kept.
                if spam && !enabled {
                    choices.remove(category);
                } else {
                    choices.insert(category.into(), enabled);
                }
                settings
                    .validate()
                    .map_err(|message| err("invalid_request", &message))?;
                settings_changed = true;
            }
            "label_remove" => {
                if name.as_deref() != Some(string(p, "expected_name")?) {
                    return Err(err("stale_revision", "The label changed; review it again"));
                }
                let mapping:Option<String>=tx.query_row("SELECT provider_label_id FROM google_category_labels WHERE account=?1 AND category_id=?2",params![account,category],|r|r.get(0)).optional()?;
                let expected = p
                    .get("expected_provider_label_id")
                    .ok_or_else(|| err("invalid_request", "Supply the previous label link"))?;
                if json!(mapping) != *expected {
                    return Err(err(
                        "stale_revision",
                        "The Google label link changed; review it again",
                    ));
                }
                tx.execute("UPDATE conversations SET revision=revision+1 WHERE account=?1 AND id IN (SELECT m.conversation_id FROM messages m JOIN message_categories c ON c.account=m.account AND c.message_id=m.id WHERE c.account=?1 AND c.category_id=?2)",params![account,category])?;
                tx.execute("UPDATE messages SET revision=revision+1 WHERE account=?1 AND id IN (SELECT message_id FROM message_categories WHERE account=?1 AND category_id=?2)",params![account,category])?;
                tx.execute("INSERT OR IGNORE INTO local_model_label_overrides SELECT account,message_id FROM message_categories WHERE account=?1 AND category_id=?2",params![account,category])?;
                tx.execute(
                    "DELETE FROM message_categories WHERE account=?1 AND category_id=?2",
                    params![account, category],
                )?;
                tx.execute("DELETE FROM google_manual_intents WHERE account=?1 AND field='category' AND category_id=?2",params![account,category])?;
                tx.execute(
                    "DELETE FROM assist_category_intents WHERE account=?1 AND category_id=?2",
                    params![account, category],
                )?;
                tx.execute(
                    "DELETE FROM google_category_labels WHERE account=?1 AND category_id=?2",
                    params![account, category],
                )?;
                tx.execute(
                    "DELETE FROM categories WHERE account=?1 AND id=?2",
                    params![account, category],
                )?;
                tx.execute("UPDATE views SET json=json_set(json,'$.category',''),revision=revision+1 WHERE account=?1 AND json_extract(json,'$.category')=?2",params![account,category])?;
                if let Some(policy) = settings.accounts.get_mut(account) {
                    descriptions_changed = policy.descriptions.remove(category).is_some();
                    settings_changed = descriptions_changed
                        | policy.auto_read.remove(category).is_some()
                        | policy.auto_spam.remove(category).is_some();
                }
            }
            _ => unreachable!(),
        }
        if settings_changed {
            tx.execute("INSERT INTO local_model_settings VALUES(1,?1,?2) ON CONFLICT(id) DO UPDATE SET revision=excluded.revision,settings_json=excluded.settings_json",params![state["revision"].as_i64().unwrap()+1,serde_json::to_string(&settings)?])?;
        }
        if descriptions_changed {
            tx.execute("DELETE FROM message_sender_kinds WHERE account=?1 AND provenance IN ('ollama','openai_compatible')",[account])?;
        }
        let result = json!({"request_id":request,"id":category,"local_only":true});
        tx.execute("INSERT INTO operations(account,request_id,fingerprint,result_json) VALUES(?1,?2,?3,?4)",params![account,request,fingerprint,serde_json::to_string(&result)?])?;
        tx.execute("UPDATE meta SET value=value+1 WHERE key='revision'", [])?;
        tx.commit()?;
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn labels_edit_remove_replay_and_account_isolation() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::open_synthetic(dir.path()).unwrap();
        let a = "windscribe";
        let other = "personal";
        let c = "windscribe-thread-001";
        let m = "windscribe-message-001-000";
        let create = json!({"request_id":"create-label","id":"research","name":"Research"});
        let result = store.label_edit(a, "label_create", &create).unwrap();
        assert_eq!(
            store.label_edit(a, "label_create", &create).unwrap(),
            result
        );
        let duplicate = json!({"request_id":"duplicate-label","id":"other-id","name":"research"});
        assert_eq!(
            store
                .label_edit(a, "label_create", &duplicate)
                .unwrap_err()
                .code,
            "already_exists"
        );
        let before_settings = store.local_model_settings().unwrap();
        let describe = json!({"request_id":"describe-label","id":"research","expected_description":"","description":"Research papers and project findings."});
        store.label_edit(a, "label_describe", &describe).unwrap();
        let settings = store.local_model_settings().unwrap();
        assert_eq!(
            settings["settings"]["prompt"],
            before_settings["settings"]["prompt"]
        );
        assert_eq!(
            settings["settings"]["model"],
            before_settings["settings"]["model"]
        );
        assert_eq!(settings["settings"]["accounts"][a]["classification"], true);
        let mut stale = describe.clone();
        stale["request_id"] = json!("stale-description");
        stale["description"] = json!("Overwrites another edit");
        assert_eq!(
            store
                .label_edit(a, "label_describe", &stale)
                .unwrap_err()
                .code,
            "stale_revision"
        );
        assert!(store.label_catalog(other).unwrap()["categories"]
            .as_array()
            .unwrap()
            .iter()
            .all(|r| r["id"] != "research"));
        let before = store.conversation(a, c).unwrap();
        store.handle("set_category",Some(a),json!({"id":c,"message_ids":[m],"category_id":"research","present":true,"expected_revision":before["revision"],"request_id":"apply-research"})).unwrap();
        let original_receipt:String=store.db.query_row("SELECT result_json FROM operations WHERE account=?1 AND request_id='apply-research'",[a],|r|r.get(0)).unwrap();
        let remove = json!({"request_id":"remove-research","id":"research","expected_name":"Research","expected_provider_label_id":null});
        store.label_edit(a, "label_remove", &remove).unwrap();
        assert!(store.label_catalog(a).unwrap()["categories"]
            .as_array()
            .unwrap()
            .iter()
            .all(|r| r["id"] != "research"));
        assert_eq!(store.db.query_row("SELECT count(*) FROM message_categories WHERE account=?1 AND category_id='research'",[a],|r|r.get::<_,i64>(0)).unwrap(),0);
        assert_eq!(store.db.query_row("SELECT result_json FROM operations WHERE account=?1 AND request_id='apply-research'",[a],|r|r.get::<_,String>(0)).unwrap(),original_receipt);
        assert_eq!(
            store.label_edit(a, "label_remove", &remove).unwrap()["local_only"],
            true
        );
        assert_eq!(
            store.label_edit(a, "label_create", &create).unwrap(),
            result
        );
        assert!(
            store.local_model_settings().unwrap()["settings"]["accounts"][a]["descriptions"]
                .get("research")
                .is_none()
        );
        let after = store.conversation(a, c).unwrap();
        for old in before["messages"].as_array().unwrap() {
            let new = after["messages"]
                .as_array()
                .unwrap()
                .iter()
                .find(|new| new["id"] == old["id"])
                .unwrap();
            assert_eq!(old["unread"], new["unread"]);
            assert_eq!(old["starred"], new["starred"]);
        }
        drop(store);
        let store = Store::open_synthetic(dir.path()).unwrap();
        assert!(store.label_catalog(a).unwrap()["categories"]
            .as_array()
            .unwrap()
            .iter()
            .all(|r| r["id"] != "research"));
    }
    #[test]
    fn label_spam_is_per_account_replays_and_goes_with_the_label() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::open_synthetic(dir.path()).unwrap();
        let (a, other) = ("windscribe", "personal");
        for account in [a, other] {
            let create = json!({"request_id":format!("create-{account}"),"id":"promotions","name":"Promotions"});
            store.label_edit(account, "label_create", &create).unwrap();
        }
        let spam = |store: &Store, account: &str| {
            store.label_catalog(account).unwrap()["categories"]
                .as_array()
                .unwrap()
                .iter()
                .find(|row| row["id"] == "promotions")
                .unwrap()["auto_spam"]
                .clone()
        };
        assert_eq!(spam(&store, a), false, "Spam is off by default");
        let on = json!({"request_id":"spam-on","id":"promotions","expected_auto_spam":false,"auto_spam":true});
        let result = store.label_edit(a, "label_auto_spam", &on).unwrap();
        assert_eq!(store.label_edit(a, "label_auto_spam", &on).unwrap(), result);
        assert_eq!(spam(&store, a), true);
        assert_eq!(
            spam(&store, other),
            false,
            "another account keeps its own choice"
        );
        let mut stale = on.clone();
        stale["request_id"] = json!("spam-stale");
        assert_eq!(
            store
                .label_edit(a, "label_auto_spam", &stale)
                .unwrap_err()
                .code,
            "stale_revision"
        );
        let mut reused = on.clone();
        reused["auto_spam"] = json!(false);
        assert_eq!(
            store
                .label_edit(a, "label_auto_spam", &reused)
                .unwrap_err()
                .code,
            "request_id_conflict"
        );
        let missing = json!({"request_id":"spam-missing","id":"promotions","auto_spam":true});
        assert_eq!(
            store
                .label_edit(a, "label_auto_spam", &missing)
                .unwrap_err()
                .code,
            "invalid_request"
        );
        let remove = json!({"request_id":"remove-promotions","id":"promotions","expected_name":"Promotions","expected_provider_label_id":null});
        store.label_edit(a, "label_remove", &remove).unwrap();
        let settings = store.local_model_settings().unwrap();
        assert!(settings["settings"]["accounts"][a]
            .get("auto_spam")
            .is_none_or(|choices| choices.get("promotions").is_none()));
        drop(store);
        let store = Store::open_synthetic(dir.path()).unwrap();
        assert_eq!(spam(&store, other), false);
    }
}
