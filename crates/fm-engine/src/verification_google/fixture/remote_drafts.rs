//! Deterministic remote-only input. These bytes are never constructed from user data.
use base64::{engine::general_purpose::STANDARD, Engine};

pub(super) const COUNT: usize = 52;

pub(super) fn index(id: &str) -> Option<usize> {
    let digits = id.strip_prefix("rd")?;
    if digits.len() != 4 || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    digits.parse::<usize>().ok().filter(|value| *value < COUNT)
}

pub(super) fn source(key: &str, index: usize, version: u8, base: u64) -> Vec<u8> {
    if index == 3 {
        return format!("Malformed remote draft {key}/0003, version {version}.\r\n").into_bytes();
    }
    let date = chrono::DateTime::from_timestamp(base as i64 - 86_400 + index as i64 * 60, 0)
        .unwrap()
        .format("%a, %d %b %Y %H:%M:%S +0000")
        .to_string();
    let sender = if index == 4 {
        "Other Author <other@example.test>"
    } else {
        "Remote Author <remote@example.test>"
    };
    let routing = if index == 4 {
        String::new()
    } else {
        "To: Reader One <one@example.test>, Reader Two <two@example.test>\r\nCc: Copy <copy@example.test>\r\nBcc: Blind <blind@example.test>\r\n".into()
    };
    let headers = format!("From: {sender}\r\n{routing}Subject: Remote {key} draft {index:04} version {version}\r\nDate: {date}\r\nMessage-ID: <remote-{key}-{index:04}-v{version}@example.test>\r\nIn-Reply-To: <prior@example.test>\r\nReferences: <first@example.test> <prior@example.test>\r\nMIME-Version: 1.0\r\n");
    let body = format!(
        "Remote cached draft for {key}/{index:04}, version {version}.\r\nSecond authored line."
    );
    if index != 0 {
        return format!("{headers}Content-Type: text/plain; charset=utf-8\r\nContent-Transfer-Encoding: 8bit\r\n\r\n{body}").into_bytes();
    }
    let mixed = format!("remote-{key}-0-v{version}");
    let alternative = format!("alternative-{key}-v{version}");
    let html = format!("<p>Remote <strong>{key}</strong> draft 0000 version {version}.</p><img src=\"cid:remote-image\">");
    let attachment = attachment(key, version);
    format!("{headers}Content-Type: multipart/mixed; boundary=\"{mixed}\"\r\n\r\n--{mixed}\r\nContent-Type: multipart/alternative; boundary=\"{alternative}\"\r\n\r\n--{alternative}\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Transfer-Encoding: 8bit\r\n\r\n{body}\r\n--{alternative}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Transfer-Encoding: 8bit\r\n\r\n{html}\r\n--{alternative}--\r\n--{mixed}\r\nContent-Type: application/octet-stream; name=\"fixture.bin\"\r\nContent-Disposition: inline; filename=\"fixture.bin\"\r\nContent-ID: <remote-image>\r\nContent-Transfer-Encoding: base64\r\n\r\n{}\r\n--{mixed}--\r\n", STANDARD.encode(attachment)).into_bytes()
}

pub(super) fn attachment(key: &str, version: u8) -> Vec<u8> {
    let mut bytes = vec![0, 1, 2, 255];
    bytes.extend_from_slice(key.as_bytes());
    bytes.push(version);
    bytes
}
