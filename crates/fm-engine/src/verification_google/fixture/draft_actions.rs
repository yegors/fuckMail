//! Six generated write-enabled targets, separate from the read-only inventory.
use super::remote_drafts;
pub(super) const COUNT: usize = 6;
pub(super) fn index(id: &str) -> Option<usize> {
    let digits = id.strip_prefix("ra")?;
    if digits.len() != 4 || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    digits.parse::<usize>().ok().filter(|n| *n < COUNT)
}
pub(super) fn source(key: &str, index: usize, version: u8, base: u64) -> Vec<u8> {
    let raw = remote_drafts::source(key, [10, 0, 11, 12, 13, 14][index], version, base);
    if index == 2 {
        return raw;
    }
    let text = String::from_utf8(raw).expect("generated UTF-8 source");
    let (_, rest) = text.split_once("\r\n").expect("generated From header");
    format!("From: Primary Author <{key}@example.test>\r\n{rest}").into_bytes()
}
