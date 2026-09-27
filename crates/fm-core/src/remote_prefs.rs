//! Preferences for remote message content: blocked link hosts, senders whose
//! images load automatically, senders whose mail shows as HTML, sender
//! pictures in the mail list, and the optional HTTPS proxy. The proxy password lives in the OS vault, never here.
use crate::{err, Result};
use fm_mail::html_view::normalize_block_pattern;
use serde_json::Value;
use std::collections::HashSet;

pub(crate) const MAX_BLOCKED_HOSTS: usize = 1_000;
const MAX_SENDERS: usize = 2_000;

/// A per-account sender list: at most 2,000 distinct `{account, email}`
/// entries with lowercase addresses. `what` names the list in errors.
fn senders(list: &Value, what: &str) -> Result<()> {
    let entries = list
        .as_array()
        .filter(|v| v.len() <= MAX_SENDERS)
        .ok_or_else(|| {
            err(
                "invalid_request",
                &format!("{what} senders must be a list of at most 2,000"),
            )
        })?;
    let mut seen = HashSet::new();
    for entry in entries {
        crate::fields(entry, &["account", "email"])?;
        let account = crate::id(entry["account"].as_str().unwrap_or_default())?;
        let email = entry["email"].as_str().unwrap_or_default();
        if email.len() > 320
            || !email.contains('@')
            || email != email.trim().to_ascii_lowercase()
            || email.chars().any(char::is_control)
        {
            return Err(err(
                "invalid_request",
                &format!("{what} senders need a lowercase email address"),
            ));
        }
        if !seen.insert((account.to_owned(), email.to_owned())) {
            return Err(err(
                "invalid_request",
                &format!("Repeated {} sender", what.to_lowercase()),
            ));
        }
    }
    Ok(())
}

/// Validates the remote-content keys of a merged preferences document before
/// it is saved. Blocked hosts must already be in normalized form, and
/// image-sender addresses must be lowercase.
pub(crate) fn validate(preferences: &Value) -> Result<()> {
    let blocked = preferences["blocked_hosts"]
        .as_array()
        .filter(|v| v.len() <= MAX_BLOCKED_HOSTS)
        .ok_or_else(|| {
            err(
                "invalid_request",
                "Blocked domains must be a list of at most 1,000",
            )
        })?;
    let mut seen = HashSet::new();
    for entry in blocked {
        crate::fields(entry, &["pattern", "source", "added_at"])?;
        let pattern = entry["pattern"].as_str().unwrap_or_default();
        if normalize_block_pattern(pattern).as_deref() != Some(pattern) {
            return Err(err(
                "invalid_request",
                "Use a domain like tracker.io or *.tracker.io",
            ));
        }
        if !seen.insert(pattern) {
            return Err(err("invalid_request", "That domain is already blocked"));
        }
        if !matches!(entry["source"].as_str(), Some("settings" | "link"))
            || entry["added_at"].as_i64().is_none_or(|v| v < 0)
        {
            return Err(err(
                "invalid_request",
                "Blocked domains need a source and a time",
            ));
        }
    }
    senders(&preferences["remote_images_from"], "Image")?;
    senders(&preferences["html_senders"], "HTML")?;
    let proxy = &preferences["remote_proxy"];
    crate::fields(proxy, &["enabled", "host", "port", "username"])?;
    let host = proxy["host"].as_str().unwrap_or_default();
    let port = proxy["port"].as_u64().unwrap_or(0);
    let username = proxy["username"].as_str().unwrap_or_default();
    if !proxy["enabled"].is_boolean()
        || !proxy["host"].is_string()
        || !proxy["username"].is_string()
        || !(1..=65_535).contains(&port)
        || username.len() > 256
        || username.chars().any(char::is_control)
    {
        return Err(err(
            "invalid_request",
            "Proxy needs a host, a port from 1 to 65535 and a username",
        ));
    }
    if !host.is_empty() && proxy_host(host).is_none() {
        return Err(err(
            "invalid_request",
            "Enter the proxy's host name, without https:// or a path",
        ));
    }
    if proxy["enabled"] == true && host.is_empty() {
        return Err(err("invalid_request", "Enter the proxy's host name"));
    }
    if !preferences["sender_pictures"].is_boolean() {
        return Err(err(
            "invalid_request",
            "Sender pictures must be true or false",
        ));
    }
    Ok(())
}

/// What the engine's remote-content path needs from preferences.
pub struct RemoteSettings {
    /// Preferences revision these settings were read at, so callers can
    /// cache them until it changes.
    pub revision: i64,
    /// Blocked link-host patterns, such as `tracker.io` or `*.tracker.io`.
    pub blocked: Vec<String>,
    /// Enabled proxy only.
    pub proxy: Option<ProxySettings>,
    /// Brand icons and profile photos for mail list senders.
    pub sender_pictures: bool,
}

/// The user's HTTPS proxy for remote content. The password is not here; it is
/// read from the OS vault.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ProxySettings {
    /// ASCII host name, as normalized by [`proxy_host`].
    pub host: String,
    pub port: u16,
    pub username: String,
}

impl crate::Store {
    /// Reads the remote-content preferences for the engine. The proxy is
    /// `None` unless it is enabled and its stored host still parses.
    pub fn remote_content_settings(&self) -> Result<RemoteSettings> {
        let current = self.preferences()?;
        let p = &current["preferences"];
        let blocked = p["blocked_hosts"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|e| e["pattern"].as_str().map(str::to_owned))
            .collect();
        let proxy = &p["remote_proxy"];
        let proxy = (proxy["enabled"] == true)
            .then(|| {
                Some(ProxySettings {
                    host: proxy_host(proxy["host"].as_str()?)?,
                    port: u16::try_from(proxy["port"].as_u64()?).ok()?,
                    username: proxy["username"].as_str()?.to_owned(),
                })
            })
            .flatten();
        Ok(RemoteSettings {
            revision: current["revision"].as_i64().unwrap_or(0),
            blocked,
            proxy,
            sender_pictures: p["sender_pictures"].as_bool().unwrap_or(true),
        })
    }
}

/// A proxy host as ASCII, rejecting schemes, paths, ports and credentials.
pub fn proxy_host(host: &str) -> Option<String> {
    if host.len() > 253
        || host.is_empty()
        || host.contains(['/', '?', '#', '@', ' ', ':'])
        || host.chars().any(char::is_control)
    {
        return None;
    }
    let parsed = url::Url::parse(&format!("https://{host}/")).ok()?;
    parsed.host_str().map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn base() -> Value {
        json!({"blocked_hosts":[],"remote_images_from":[],"html_senders":[],"remote_proxy":{"enabled":false,"host":"","port":443,"username":""},"sender_pictures":true})
    }

    #[test]
    fn remote_preferences_are_validated() {
        assert!(validate(&base()).is_ok());
        let mut p = base();
        p["blocked_hosts"] =
            json!([{"pattern":"*.doubleclick.net","source":"settings","added_at":1}]);
        assert!(validate(&p).is_ok());
        p["blocked_hosts"] = json!([{"pattern":"Tracker.io","source":"settings","added_at":1}]);
        assert!(validate(&p).is_err(), "patterns are stored normalized");
        p["blocked_hosts"] = json!([{"pattern":"a.io","source":"x","added_at":1}]);
        assert!(validate(&p).is_err());
        let mut p = base();
        p["remote_images_from"] = json!([{"account":"acct-1","email":"news@x.test"},{"account":"acct-1","email":"news@x.test"}]);
        assert!(validate(&p).is_err());
        let mut p = base();
        p["html_senders"] = json!([{"account":"acct-1","email":"news@x.test"},{"account":"acct-2","email":"news@x.test"}]);
        assert!(
            validate(&p).is_ok(),
            "the same sender in two accounts is two choices"
        );
        p["html_senders"] = json!([{"account":"acct-1","email":"News@X.test"}]);
        assert!(validate(&p).is_err(), "addresses are stored lowercase");
        p["html_senders"] = json!([{"account":"acct-1","email":"a@x.test"},{"account":"acct-1","email":"a@x.test"}]);
        assert!(validate(&p).is_err());
        p.as_object_mut().unwrap().remove("html_senders");
        assert!(validate(&p).is_err());
        let mut p = base();
        p["remote_proxy"] = json!({"enabled":true,"host":"","port":443,"username":"u"});
        assert!(validate(&p).is_err());
        p["remote_proxy"]["host"] = "https://proxy.test".into();
        assert!(validate(&p).is_err());
        p["remote_proxy"]["host"] = "proxy.example.net".into();
        assert!(validate(&p).is_ok());
        p["remote_proxy"]["port"] = 0.into();
        assert!(validate(&p).is_err());
        let mut p = base();
        p["sender_pictures"] = false.into();
        assert!(validate(&p).is_ok());
        p["sender_pictures"] = "off".into();
        assert!(validate(&p).is_err());
        p.as_object_mut().unwrap().remove("sender_pictures");
        assert!(validate(&p).is_err());
    }
}
