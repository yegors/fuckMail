//! Private, bounded, metadata-only engine diagnostics shared by GUI and CLI sessions.
use serde_json::{json, Value};
use std::{
    fs,
    hash::{BuildHasher, Hash, Hasher, RandomState},
    io::Write,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};

const SEGMENT_BYTES: u64 = 2560 * 1024;
static LOG: OnceLock<Mutex<PathBuf>> = OnceLock::new();
static REFERENCE_KEY: OnceLock<RandomState> = OnceLock::new();

/// Stable within one engine process, with a fresh secret hash key on restart.
pub fn reference(kind: &str, value: &str) -> String {
    if value.is_empty() {
        return "none".into();
    }
    let mut hasher = REFERENCE_KEY.get_or_init(RandomState::new).build_hasher();
    kind.hash(&mut hasher);
    value.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

/// Starts logging to `diagnostics/engine.log` in the profile. Logging stays
/// off if the directory cannot be created; later calls have no effect.
pub fn initialize(profile: &Path) {
    let directory = profile.join("diagnostics");
    if fs::create_dir_all(&directory).is_ok() {
        let _ = LOG.set(Mutex::new(directory.join("engine.log")));
        event("App", "started", json!({"pid":std::process::id()}));
    }
}

/// Appends one JSON line to the engine log. The level is `ERROR` for a
/// `failed` event, `WARN` when `fields` has a string `code`, and `INFO`
/// otherwise. Lines over 4 KiB are dropped; the log rotates to one previous
/// segment at 2.5 MiB. Fields should be metadata only, with identifiers
/// passed through [`reference()`]. Does nothing before [`initialize`].
pub fn event(scope: &str, name: &str, fields: Value) {
    let Some(log) = LOG.get() else { return };
    let Ok(path) = log.lock() else { return };
    let line = json!({"at":chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis,true),
        "level":if name == "failed" {"ERROR"} else if fields["code"].is_string() {"WARN"} else {"INFO"},
        "scope":scope,"event":name,"fields":fields}).to_string() + "\n";
    if line.len() > 4096 {
        return;
    }
    append_line(&path, &line);
}

fn append_line(path: &Path, line: &str) {
    let current = fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    if current + line.len() as u64 > SEGMENT_BYTES {
        let previous = path.with_extension("log.1");
        let _ = fs::remove_file(&previous);
        if fs::rename(path, previous).is_err() {
            let _ = fs::OpenOptions::new().write(true).truncate(true).open(path);
        }
    }
    if let Ok(mut file) = fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = file.write_all(line.as_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn two_segments_remain_bounded() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("engine.log");
        let line = format!("{}\n", "word ".repeat(300));
        for _ in 0..3000 {
            append_line(&path, &line);
        }
        assert!(fs::metadata(&path).unwrap().len() <= SEGMENT_BYTES);
        assert!(fs::metadata(path.with_extension("log.1")).unwrap().len() <= SEGMENT_BYTES);
    }
    #[test]
    fn references_are_stable_without_retaining_identifiers() {
        let account = "someone@example.test";
        let a = reference("account", account);
        assert_eq!(a, reference("account", account));
        assert_ne!(a, reference("message", account));
        assert!(!a.contains(account));
    }
}
