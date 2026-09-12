use crate::*;
use rusqlite::params;

pub(crate) fn seed_sender_kinds(tx: &rusqlite::Transaction<'_>) -> Result<()> {
    // Author-specified synthetic roles; never infer a role from sender text.
    for account in ["windscribe", "personal", "sideproject"] {
        for n in 0..110 {
            let members = if n == 0 {
                3
            } else if n == 4 {
                32
            } else {
                1
            };
            for member in 0..members {
                let mid = format!("{account}-message-{n:03}-{member:03}");
                tx.execute("INSERT INTO message_sender_kinds(account,message_id,source_hash,kind,provenance) SELECT account,id,source_hash,?3,'fixture' FROM messages WHERE account=?1 AND id=?2",params![account,mid,if n==5 {"automated"} else {"people"}])?;
            }
        }
    }
    Ok(())
}

struct Message {
    account: String,
    cid: String,
    id: String,
    remote: String,
    sender: String,
    email: String,
    recipient: String,
    subject: String,
    body: String,
    at: String,
    unread: bool,
    starred: bool,
    inbox: bool,
    sent: bool,
    hash: String,
    category: String,
}
impl Store {
    pub(crate) fn seed(&mut self) -> Result<()> {
        // Finalize immutable bytes before any rows reference them. An interrupted
        // seed may leave harmless orphan blobs; the database transaction is all-or-none.
        let schema: u32 = self.db.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        let fixture_accounts: &[&str] = if schema >= 5 {
            &["windscribe", "personal", "sideproject"]
        } else {
            &["windscribe", "personal"]
        };
        let mut messages = Vec::new();
        for &account in fixture_accounts {
            for n in 0..110 {
                let members = if n == 0 {
                    3
                } else if n == 4 {
                    32
                } else {
                    1
                };
                for member in 0..members {
                    let cid = format!("{account}-thread-{n:03}");
                    let mid = format!("{account}-message-{n:03}-{member:03}");
                    let (subject,mut sender,mut email,body,category)=match n {
                        0=>("Q3 infra review: v4 attached".to_owned(),"Maya Lindqvist".to_owned(),format!("maya@{account}.example"),"v4 attached. Two open calls: renew the Frankfurt bare-metal contract at the new rate, or move 40% of that pool to the second provider?\n\nNeed your call before the 14:00 review.\n\nhttps://example.com/review\n\n> Earlier question: please review both options.\nΚαλημέρα — こんにちは".to_owned(),"infra"),
                        1=>("App Store review — 3.1.2 rejected".to_owned(),"Tomasz Nowak".to_owned(),format!("tomasz@{account}.example"),"Revised paywall copy is ready; resubmitting today keeps Monday.".to_owned(),"apps"),
                        2=>("Head of Support — final loop".to_owned(),"Aisha Rahman".to_owned(),format!("aisha@{account}.example"),"3 of 4 recommend hire. Please send your read before Monday.".to_owned(),"hiring"),
                        3=>("WireGuard relay rollout".to_owned(),"Sam Okoro".to_owned(),format!("sam@{account}.example"),"Toronto is clean since Thursday. Can we use Saturday 06:00–10:00 UTC?".to_owned(),"infra"),
                        4=>("Long project discussion".to_owned(),"Maya Lindqvist".to_owned(),format!("maya@{account}.example"),format!("Member {} of 32. The exact human exchange is retained, including this member's distinctive longthread{member} keyword.",member+1),"planning"),
                        5 if schema>=5 =>("Weekly service availability".to_owned(),"Status Robot".to_owned(),"status@service.example".to_owned(),String::new(),"planning"),
                        _=>(format!("Project correspondence {n:03}"),format!("Colleague {}",n%20),format!("colleague+{}@{account}.example",n%20),format!("This is synthetic human message {n:03}. We discussed the invoice, project delivery, and quarterly planning. Searchable keyword archive{n:03}."),if n%2==0{"finance"}else{"planning"}),
                    };
                    let outgoing = schema >= 5 && (n == 0 && member == 1 || n > 0 && n % 17 == 0);
                    if outgoing {
                        sender = "You".into();
                        email = format!("you@{account}.example");
                    }
                    let recipient = if n == 5 && schema >= 5 {
                        "alex@windscribe.example".into()
                    } else if outgoing {
                        format!("maya@{account}.example")
                    } else {
                        format!("you@{account}.example")
                    };
                    let at = if n < 4 {
                        format!("2026-09-11T{:02}:{:02}:00-04:00", 9 - n, 10 + member)
                    } else {
                        format!("2026-08-{:02}T10:{:02}:00-04:00", 28 - n % 28, member)
                    };
                    let raw = if n == 5 {
                        include_bytes!("../../../fixtures/messages/html-only.eml").to_vec()
                    } else {
                        format!("From: {sender} <{email}>\r\nTo: {recipient}\r\nSubject: {subject}\r\nMessage-ID: <{mid}@fixture.example>\r\nMIME-Version: 1.0\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Transfer-Encoding: 8bit\r\n\r\n{body}").into_bytes()
                    };
                    let text = fm_mail::extract_incoming_text(&raw)
                        .map_err(|_| err("invalid_fixture", "Cannot extract fixture MIME text"))?;
                    let hash = self.stage_blob(&raw)?;
                    messages.push(Message {
                        account: account.into(),
                        cid,
                        id: mid,
                        remote: format!("remote-{n:03}-{member:03}"),
                        sender,
                        email,
                        recipient,
                        subject,
                        body: text,
                        at,
                        unread: n < 4 || n % 5 == 0,
                        starred: n == 4 && member % 2 == 0 || n % 11 == 0,
                        inbox: n < 90,
                        sent: if schema >= 5 { outgoing } else { n % 17 == 0 },
                        hash,
                        category: category.into(),
                    });
                }
            }
        }
        let attachment_bytes=b"Q3 infrastructure review, revision 4\n\nOption A: renew Frankfurt capacity.\nOption B: shift 40% to the second provider.\nSynthetic offline attachment.\n";
        let attachment_hash = self.stage_blob(attachment_bytes)?;
        let tx = self.db.transaction()?;
        for &account in fixture_accounts {
            tx.execute(
                "INSERT INTO accounts(id,name,email,provider) VALUES(?1,?2,?3,?4)",
                params![
                    account,
                    if account == "windscribe" {
                        "Windscribe"
                    } else if account == "personal" {
                        "Personal"
                    } else {
                        "Side Project"
                    },
                    format!("you@{account}.example"),
                    if account == "windscribe" {
                        "Google Workspace (synthetic)"
                    } else {
                        "Gmail (synthetic)"
                    }
                ],
            )?;
            for (id, name) in [
                ("infra", "Infra"),
                ("finance", "Finance"),
                ("apps", "Apps"),
                ("hiring", "Hiring"),
                ("planning", "Planning"),
                ("noise", "Noise"),
            ] {
                tx.execute(
                    "INSERT INTO categories VALUES(?1,?2,?3)",
                    params![account, id, name],
                )?;
            }
        }
        for m in messages {
            tx.execute(
                "INSERT OR IGNORE INTO conversations(account,id,subject) VALUES(?1,?2,?3)",
                params![m.account, m.cid, m.subject],
            )?;
            tx.execute("INSERT INTO messages(account,id,conversation_id,remote_id,sender_name,sender_email,recipient,subject,body,sent_at,unread,starred,inbox,sent,source_hash) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15)",params![m.account,m.id,m.cid,m.remote,m.sender,m.email,m.recipient,m.subject,m.body,m.at,m.unread,m.starred,m.inbox,m.sent,m.hash])?;
            tx.execute(
                "INSERT INTO message_fts(rowid,sender,subject,body) VALUES(?1,?2,?3,?4)",
                params![
                    tx.last_insert_rowid(),
                    format!("{} {}", m.sender, m.email),
                    m.subject,
                    m.body
                ],
            )?;
            tx.execute(
                "INSERT INTO message_categories VALUES(?1,?2,?3)",
                params![m.account, m.id, m.category],
            )?;
        }
        for &account in fixture_accounts {
            for (n, state) in [(0, "available"), (1, "missing"), (2, "partial")] {
                tx.execute(
                    "INSERT INTO attachments VALUES(?1,?2,?3,?4,'text/plain',?5,?6,?7)",
                    params![
                        account,
                        format!("{account}-attachment-{n}"),
                        format!("{account}-message-{n:03}-{:03}", if n == 0 { 2 } else { 0 }),
                        format!("Q3-infra-v4-{n}.txt"),
                        attachment_bytes.len() as i64,
                        state,
                        attachment_hash
                    ],
                )?;
            }
            let sources = [
                json!({"id":"standup","etag":"\"v1\"","summary":"Team standup","start":{"dateTime":"2026-09-07T09:30:00-04:00","timeZone":"America/Toronto"},"end":{"dateTime":"2026-09-07T10:00:00-04:00","timeZone":"America/Toronto"},"recurrence":["RRULE:FREQ=WEEKLY;BYDAY=MO,WE,FR;COUNT=3"]}),
                json!({"id":"review","etag":"\"v1\"","summary":"Q3 infra review","location":"Review room","start":{"dateTime":"2026-09-11T14:00:00-04:00","timeZone":"America/Toronto"},"end":{"dateTime":"2026-09-11T15:30:00-04:00","timeZone":"America/Toronto"}}),
                json!({"id":"lunch","etag":"\"v1\"","summary":"Lunch — Maya","start":{"dateTime":"2026-09-11T13:00:00-04:00","timeZone":"America/Toronto"},"end":{"dateTime":"2026-09-11T14:00:00-04:00","timeZone":"America/Toronto"}}),
                json!({"id":"overlap","etag":"\"v1\"","summary":"Project handoff","start":{"dateTime":"2026-09-11T14:30:00-04:00","timeZone":"America/Toronto"},"end":{"dateTime":"2026-09-11T15:00:00-04:00","timeZone":"America/Toronto"}}),
                json!({"id":"away","etag":"\"v1\"","summary":"Weekend away","start":{"date":"2026-09-12"},"end":{"date":"2026-09-14"}}),
            ];
            for event in sources {
                tx.execute(
                    "INSERT INTO calendar_sources VALUES(?1,'primary',?2,?3)",
                    params![
                        account,
                        event["id"].as_str().unwrap(),
                        serde_json::to_string(&event)?
                    ],
                )?;
            }
        }
        // Upgrade tests deliberately seed the historical v2 schema. Its calendar
        // metadata is populated by migration 4, after the old profile is reopened.
        let version: u32 = tx.query_row("PRAGMA user_version", [], |row| row.get(0))?;
        if version >= 4 {
            tx.execute("INSERT OR IGNORE INTO calendar_sync_state(account,calendar_id,query_fingerprint,time_zone,access,sync_token,revision) SELECT DISTINCT account,calendar_id,'fixture-calendar-v1','America/Toronto','reader',NULL,0 FROM calendar_sources",[])?;
            tx.execute("INSERT OR IGNORE INTO calendar_event_state(account,calendar_id,event_id,visible,observed_json) SELECT account,calendar_id,event_id,1,json FROM calendar_sources",[])?;
        }
        if schema >= 5 {
            tx.execute("INSERT INTO account_metadata(account,signature_text) SELECT id,'—'||char(10)||name FROM accounts",[])?;
            tx.execute("INSERT INTO drafts(account,conversation_id,body,revision,signature_text,from_name,identity_revision) SELECT a.id,a.id||'-thread-003','I will review the rollout window and confirm the timing.',1,m.signature_text,a.name,m.revision FROM accounts a JOIN account_metadata m ON m.account=a.id",[])?;
            crate::phase1_calendar::seed_phase1_calendars(&tx)?;
        }
        tx.execute(
            "INSERT INTO meta VALUES('fixture_revision',?1)",
            [if schema >= 5 { FIXTURE_REVISION } else { 1 }],
        )?;
        tx.commit()?;
        Ok(())
    }
}
