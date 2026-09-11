use crate::*;
use rusqlite::params;
impl Store {
    pub(crate) fn accounts(&self) -> Result<Value> {
        let mut stmt = self
            .db
            .prepare("SELECT id,name,email,provider FROM accounts ORDER BY rowid")?;
        let accounts = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                ))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let mut out = Vec::new();
        for (account, name, email, provider) in accounts {
            let (unread,starred,messages):(i64,i64,i64)=self.db.query_row("SELECT COALESCE(SUM(unread),0),COALESCE(SUM(starred),0),COUNT(*) FROM messages WHERE account=?1",[&account],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?;
            let count: i64 = self.db.query_row(
                "SELECT COUNT(*) FROM conversations WHERE account=?1",
                [&account],
                |r| r.get(0),
            )?;
            out.push(json!({"id":account,"name":name,"email":email,"provider":provider,"signature_text":format!("—\n{name}"),"unread_count":unread,"starred_count":starred,"conversation_count":count,"message_count":messages,"coverage":self.coverage(&account)?}));
        }
        Ok(json!({"accounts":out,"fixture_revision":FIXTURE_REVISION}))
    }
    fn coverage(&self, account: &str) -> Result<Value> {
        let (messages, source_records): (i64, i64) = self.db.query_row(
            "SELECT COUNT(*),COALESCE(SUM(source_hash<>''),0) FROM messages WHERE account=?1",
            [account],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        let indexed_messages: i64 = self.db.query_row(
            "SELECT COUNT(*) FROM messages m JOIN message_fts f ON f.rowid=m.rowid WHERE m.account=?1",
            [account], |r| r.get(0),
        )?;
        let (attachments, recorded_available): (i64, i64) = self.db.query_row(
            "SELECT COUNT(*),COALESCE(SUM(state='available'),0) FROM attachments WHERE account=?1",
            [account],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        // A database record is not proof that its file remains present/intact.
        // List queries count actual indexed rows but never hash the whole corpus.
        // Individual attachment use verifies its bytes; there is no aggregate
        // full-profile verification pass in this API yet.
        Ok(
            json!({"synthetic":true,"inventory_complete":true,"messages":messages,
            "indexed_messages":indexed_messages,"text_index_complete":indexed_messages==messages,
            "retained_source_records":source_records,"retained_sources":null,"source_verification":"not_run",
            "attachments":attachments,"recorded_available_attachments":recorded_available,
            "complete_attachments":null,"verified_attachments":null,"attachment_verification":"not_run",
            "offline_complete":false,
            "text":format!("Synthetic text index: {indexed_messages}/{messages} messages. Full offline content verification has not run; missing/partial attachment fixtures are included.")}),
        )
    }
    pub(crate) fn conversations(&self, account: &str, p: &Value) -> Result<Value> {
        fields(
            p,
            &["query", "field", "category", "folder", "limit", "offset"],
        )?;
        let limit = number(p, "limit", 50, 1, 100)?;
        let offset = number(p, "offset", 0, 0, 10_000_000)?;
        let query = optional_string(p, "query", "")?;
        if query.len() > 512 {
            return Err(err("invalid_request", "Search is limited to 512 bytes"));
        }
        let field = optional_string(p, "field", "all")?;
        if !["all", "sender", "subject", "body"].contains(&field) {
            return Err(err("invalid_request", "Unknown search field"));
        }
        let folder = optional_string(p, "folder", "inbox")?;
        if !["all", "inbox", "sent"].contains(&folder) {
            return Err(err("invalid_request", "Unknown folder"));
        }
        let category = optional_string(p, "category", "")?;
        let tokens: Vec<_> = query
            .split(|c: char| !c.is_alphanumeric())
            .filter(|s| !s.is_empty())
            .collect();
        if tokens.len() > 32 || (!query.trim().is_empty() && tokens.is_empty()) {
            return Err(err("invalid_request", "Search needs 1–32 words"));
        }
        let expression = tokens
            .iter()
            .map(|s| format!("\"{s}\"*"))
            .collect::<Vec<_>>()
            .join(" AND ");
        let expression = if field == "all" || expression.is_empty() {
            expression
        } else {
            format!("{field}:({expression})")
        };
        // Evaluate FTS once, retaining exact matching IDs with each grouped page.
        // List hydration reads only small metadata and one selected body per row;
        // it never loads every thread member's body, attachment or source blob.
        let sql = search_page_sql(!expression.is_empty());
        let mut stmt = self.db.prepare_cached(&sql)?;
        let mut page = stmt
            .query_map(
                params![account, folder, category, expression, limit + 1, offset],
                |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
            )?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let more = page.len() > limit as usize;
        page.truncate(limit as usize);
        let mut out = Vec::with_capacity(page.len());
        for (cid, matching_json) in page {
            let matched: std::collections::HashSet<String> = serde_json::from_str(&matching_json)?;
            let (subject, revision): (String, i64) = self.db.query_row(
                "SELECT subject,revision FROM conversations WHERE account=?1 AND id=?2",
                params![account, cid],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?;
            let mut members = self.db.prepare_cached("SELECT id,sent_at,unread,starred FROM messages WHERE account=?1 AND conversation_id=?2 ORDER BY sent_at,id LIMIT 201")?;
            let members = members
                .query_map(params![account, cid], |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, bool>(2)?,
                        r.get::<_, bool>(3)?,
                    ))
                })?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            if members.is_empty() || members.len() > 200 {
                return Err(err(
                    "limit_exceeded",
                    "This experiment supports 1–200 members per conversation",
                ));
            }
            let matching: Vec<_> = members
                .iter()
                .rev()
                .filter(|m| matched.contains(&m.0))
                .map(|m| m.0.clone())
                .collect();
            let selected = matching
                .first()
                .ok_or_else(|| err("storage_error", "Search result has no matching member"))?;
            let (sender_name, sender_email, body): (String, String, String) = self.db.query_row(
                "SELECT sender_name,sender_email,body FROM messages WHERE account=?1 AND id=?2",
                params![account, selected],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )?;
            let mut categories = self.db.prepare_cached("SELECT DISTINCT c.id,c.name FROM message_categories mc JOIN messages m ON m.account=mc.account AND m.id=mc.message_id JOIN categories c ON c.account=mc.account AND c.id=mc.category_id WHERE m.account=?1 AND m.conversation_id=?2 ORDER BY c.id")?;
            let categories = categories
                .query_map(params![account, cid], |r| {
                    Ok(json!({"id":r.get::<_,String>(0)?,"name":r.get::<_,String>(1)?}))
                })?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            out.push(json!({"id":cid,"subject":subject,"sender_name":sender_name,"sender_email":sender_email,"snippet":search_snippet(&body,&tokens),"latest_at":members.last().unwrap().1,"message_count":members.len(),"unread":members.iter().any(|m|m.2),"starred":members.iter().any(|m|m.3),"revision":revision,"categories":categories,"message_ids":members.iter().map(|m|m.0.clone()).collect::<Vec<_>>(),"matching_message_ids":if expression.is_empty(){Vec::<String>::new()}else{matching}}));
        }
        Ok(
            json!({"conversations":out,"next_offset":if more{Some(offset+limit)}else{None},"coverage":self.coverage(account)?}),
        )
    }
    pub(crate) fn conversation(&self, account: &str, cid: &str) -> Result<Value> {
        id(cid)?;
        let (subject, revision): (String, i64) = self
            .db
            .query_row(
                "SELECT subject,revision FROM conversations WHERE account=?1 AND id=?2",
                params![account, cid],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?
            .ok_or_else(|| err("not_found", "Conversation not found in this account"))?;
        let mut stmt=self.db.prepare("SELECT id,sender_name,sender_email,recipient,subject,sent_at,body,unread,starred,revision,source_hash FROM messages WHERE account=?1 AND conversation_id=?2 ORDER BY sent_at,id LIMIT 201")?;
        let mut messages=stmt.query_map(params![account,cid],|r|Ok(json!({"id":r.get::<_,String>(0)?,"sender_name":r.get::<_,String>(1)?,"sender_email":r.get::<_,String>(2)?,"to":r.get::<_,String>(3)?,"subject":r.get::<_,String>(4)?,"sent_at":r.get::<_,String>(5)?,"text":r.get::<_,String>(6)?,"unread":r.get::<_,bool>(7)?,"starred":r.get::<_,bool>(8)?,"revision":r.get::<_,i64>(9)?,"source_hash":r.get::<_,String>(10)?})))?.collect::<std::result::Result<Vec<_>,_>>()?;
        if messages.is_empty() || messages.len() > 200 {
            return Err(err(
                "limit_exceeded",
                "This experiment supports 1–200 members per conversation",
            ));
        }
        let mut categories = std::collections::BTreeSet::new();
        for message in &mut messages {
            let mid = message["id"].as_str().unwrap().to_owned();
            let hash = message["source_hash"].as_str().unwrap();
            message["source_available"] = self.blob_present(hash).into();
            message.as_object_mut().unwrap().remove("source_hash");
            let mut stmt=self.db.prepare("SELECT id,name,mime_type,size,state FROM attachments WHERE account=?1 AND message_id=?2 ORDER BY id")?;
            message["attachments"]=stmt.query_map(params![account,mid],|r|Ok(json!({"id":r.get::<_,String>(0)?,"name":r.get::<_,String>(1)?,"mime_type":r.get::<_,String>(2)?,"size":r.get::<_,i64>(3)?,"state":r.get::<_,String>(4)?})))?.collect::<std::result::Result<Vec<_>,_>>()?.into();
            let mut stmt=self.db.prepare("SELECT category_id FROM message_categories WHERE account=?1 AND message_id=?2 ORDER BY category_id")?;
            let labels = stmt
                .query_map(params![account, mid], |r| r.get::<_, String>(0))?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            categories.extend(labels.clone());
            message["categories"] = labels.into();
        }
        let categories = categories
            .into_iter()
            .map(|cid| {
                let name: String = self.db.query_row(
                    "SELECT name FROM categories WHERE account=?1 AND id=?2",
                    params![account, cid],
                    |r| r.get(0),
                )?;
                Ok(json!({"id":cid,"name":name}))
            })
            .collect::<Result<Vec<_>>>()?;
        let ids: Vec<_> = messages.iter().map(|m| m["id"].clone()).collect();
        Ok(
            json!({"id":cid,"subject":subject,"revision":revision,"message_ids":ids,"categories":categories,"messages":messages}),
        )
    }
    pub(crate) fn categories(&self, account: &str) -> Result<Value> {
        let mut stmt=self.db.prepare("SELECT c.id,c.name,COUNT(DISTINCT m.conversation_id) FROM categories c LEFT JOIN message_categories mc ON mc.account=c.account AND mc.category_id=c.id LEFT JOIN messages m ON m.account=mc.account AND m.id=mc.message_id WHERE c.account=?1 GROUP BY c.id,c.name ORDER BY c.name")?;
        Ok(
            json!({"categories":stmt.query_map([account],|r|Ok(json!({"id":r.get::<_,String>(0)?,"name":r.get::<_,String>(1)?,"conversation_count":r.get::<_,i64>(2)?})))?.collect::<std::result::Result<Vec<_>,_>>()?}),
        )
    }
    pub(crate) fn draft(&self, account: &str, cid: &str) -> Result<Value> {
        let conversation = self.conversation(account, cid)?;
        let (body, revision, body_html, attachments_json) = self
            .db
            .query_row(
                "SELECT body,revision,body_html,attachments_json FROM drafts WHERE account=?1 AND conversation_id=?2",
                params![account, cid],
                |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?,r.get::<_,Option<String>>(2)?,r.get::<_,String>(3)?)),
            )
            .optional()?
            .unwrap_or((String::new(),0,None,"[]".into()));
        let attachment_ids: Vec<String> = serde_json::from_str(&attachments_json)?;
        if attachment_ids.len() > 50 {
            return Err(err(
                "storage_error",
                "Draft attachment references exceed their bound",
            ));
        }
        let attachments = attachment_ids.iter().map(|id| {
            self.db.query_row("SELECT name,mime_type,size,state FROM attachments WHERE account=?1 AND id=?2",params![account,id],|row|Ok(json!({"id":id,"name":row.get::<_,String>(0)?,"mime_type":row.get::<_,String>(1)?,"size":row.get::<_,i64>(2)?,"state":row.get::<_,String>(3)?})))
                .optional().map(|row|row.unwrap_or_else(||json!({"id":id,"state":"missing"})))
        }).collect::<std::result::Result<Vec<_>,_>>()?;
        let from: String =
            self.db
                .query_row("SELECT email FROM accounts WHERE id=?1", [account], |r| {
                    r.get(0)
                })?;
        let to =
            conversation["messages"].as_array().unwrap().last().unwrap()["sender_email"].clone();
        Ok(
            json!({"conversation_id":cid,"body":body,"body_html":body_html,"attachment_ids":attachment_ids,"to":to,"from":from,"subject":format!("Re: {}",conversation["subject"].as_str().unwrap()),"revision":revision,"attachments":attachments}),
        )
    }
    pub(crate) fn view(&self, account: &str) -> Result<Value> {
        let found = self
            .db
            .query_row(
                "SELECT revision,json FROM views WHERE account=?1",
                [account],
                |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)),
            )
            .optional()?;
        let (revision, view) = match found {
            Some((r, j)) => (r, serde_json::from_str::<Value>(&j)?),
            None => (0, json!({})),
        };
        Ok(json!({"account":account,"revision":revision,"view":view}))
    }
    pub(crate) fn calendar(&self, account: &str, p: &Value) -> Result<Value> {
        fields(p, &["from", "to"])?;
        let from = optional_string(p, "from", "2026-09-07")?;
        let to = optional_string(p, "to", "2026-09-14")?;
        let parse = |s: &str| {
            chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d")
                .map_err(|_| err("invalid_request", "Calendar needs YYYY-MM-DD dates"))
        };
        let (start, end) = (parse(from)?, parse(to)?);
        if end <= start || (end - start).num_days() > 42 {
            return Err(err("invalid_request", "Calendar range must be 1–42 days"));
        }
        let mut stmt = self.db.prepare(
            "SELECT json FROM calendar_sources WHERE account=?1 ORDER BY event_id LIMIT 5001",
        )?;
        let raw = stmt
            .query_map([account], |r| r.get::<_, String>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let sources = raw
            .iter()
            .map(|j| serde_json::from_str(j))
            .collect::<std::result::Result<Vec<Value>, _>>()?;
        let events = fm_calendar::project_google_events(
            account,
            "primary",
            "America/Toronto",
            &sources,
            start,
            end,
            fm_calendar::Access::Reader,
        )
        .map_err(|e| err(e.code, &e.message))?;
        Ok(
            json!({"events":events,"from":from,"to":to,"time_zone":"America/Toronto","fixture_today":FIXTURE_TODAY}),
        )
    }
}
fn optional_string<'a>(p: &'a Value, key: &str, default: &'a str) -> Result<&'a str> {
    match p.get(key) {
        None => Ok(default),
        Some(v) => v
            .as_str()
            .ok_or_else(|| err("invalid_request", "Expected a string")),
    }
}
fn number(p: &Value, key: &str, default: i64, min: i64, max: i64) -> Result<i64> {
    let n = match p.get(key) {
        Some(v) => v
            .as_i64()
            .ok_or_else(|| err("invalid_request", "Expected an integer"))?,
        None => default,
    };
    if n < min || n > max {
        return Err(err("invalid_request", "Number is outside permitted bounds"));
    }
    Ok(n)
}

// Shared with the opt-in benchmark so EXPLAIN covers the exact production query.
pub(crate) fn search_page_sql(search: bool) -> String {
    let predicate = "m.account=?1 AND (?2='all' OR (?2='inbox' AND m.inbox=1) OR (?2='sent' AND m.sent=1)) AND (?3='' OR EXISTS(SELECT 1 FROM message_categories mc WHERE mc.account=m.account AND mc.message_id=m.id AND mc.category_id=?3))";
    let search = if search {
        " AND m.rowid IN (SELECT rowid FROM message_fts WHERE message_fts MATCH ?4)"
    } else {
        " AND ?4=''"
    };
    format!("SELECT m.conversation_id,json_group_array(m.id) FROM messages m WHERE {predicate}{search} GROUP BY m.conversation_id ORDER BY MAX(m.sent_at) DESC,m.conversation_id LIMIT ?5 OFFSET ?6")
}

fn search_snippet(body: &str, tokens: &[&str]) -> String {
    let lower = body.to_lowercase();
    let position = tokens
        .iter()
        .filter_map(|token| lower.find(&token.to_lowercase()))
        .min();
    let start = position
        .map(|p| lower[..p].chars().count().saturating_sub(60))
        .unwrap_or(0);
    let mut chars = body.chars().skip(start);
    let text: String = chars.by_ref().take(180).collect();
    format!(
        "{}{}{}",
        if start > 0 { "… " } else { "" },
        text,
        if chars.next().is_some() { " …" } else { "" }
    )
}
