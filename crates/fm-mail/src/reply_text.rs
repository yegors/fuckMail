//! A reversible reading/index projection; callers retain original MIME separately.

/// Remove recognizable reply history while preserving new unquoted inline answers.
/// Unknown formats stay unchanged. This never edits the retained source message.
pub fn reply_contribution(body: &str) -> String {
    project(body, false)
}

pub(crate) fn project(body: &str, html_quotes_removed: bool) -> String {
    let lines: Vec<&str> = body.lines().collect();
    let mut kept = Vec::new();
    let mut changed = false;
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i].trim();
        let lower = line.to_ascii_lowercase();
        // A forward is content the sender intentionally included in this message.
        if lower.contains("forwarded message") {
            kept.extend_from_slice(&lines[i..]);
            break;
        }
        if line.starts_with('>') {
            changed = true;
            i += 1;
            continue;
        }
        let original = line.starts_with(['-', '_', '—'])
            && lower.trim_matches(['-', '_', '—', ' ', '\t']) == "original message";
        let headers = lower.starts_with("from:") && {
            let following: Vec<String> = lines[i..lines.len().min(i + 9)]
                .iter()
                .map(|line| line.trim().to_ascii_lowercase())
                .collect();
            following.iter().any(|line| line.starts_with("to:"))
                && following.iter().any(|line| line.starts_with("subject:"))
                && following
                    .iter()
                    .any(|line| line.starts_with("date:") || line.starts_with("sent:"))
        };
        if original || headers {
            changed = true;
            break;
        }
        if line.starts_with("On ") {
            let end =
                (i..lines.len().min(i + 5)).find(|&end| lines[end].trim_end().ends_with("wrote:"));
            if let Some(end) = end {
                changed = true;
                i = end + 1;
                if html_quotes_removed
                    || lines[i..]
                        .iter()
                        .any(|line| line.trim_start().starts_with('>'))
                {
                    // Skip the attribution; the loop keeps answers between quotes.
                    continue;
                }
                break;
            }
        }
        kept.push(lines[i]);
        i += 1;
    }
    if changed {
        kept.join("\n").trim().to_owned()
    } else {
        body.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contribution_omits_history_but_keeps_new_inline_answers() {
        assert_eq!(reply_contribution("New answer\n\nOn Tuesday, Alex wrote:\n> Earlier question\nInline answer\n>> Older answer\nAnother answer"), "New answer\n\nInline answer\nAnother answer");
        assert_eq!(reply_contribution("New answer\n\n------ Original Message ------\nFrom Alex <alex@example.test>\nTo Sam\nOld answer"), "New answer");
        assert_eq!(
            reply_contribution(
                "New answer\n\nFrom: Alex\nTo: Sam\nDate: Tuesday\nSubject: Topic\n\nOld answer"
            ),
            "New answer"
        );
    }

    #[test]
    fn ordinary_text_unicode_and_forwards_are_preserved() {
        for input in ["From: a business perspective\r\nKeep these exact words.\r\n", "Hi Zoë — 東京 👋\n\nThanks\n", "Please review\n\n---------- Forwarded message ---------\nFrom: Alex\nTo: Sam\nDate: Tuesday\nSubject: Topic\n\n> Forwarded quotation"] {
            assert_eq!(reply_contribution(input), input);
        }
        assert_eq!(reply_contribution("> All quoted\n>> Older content"), "");
    }
}
