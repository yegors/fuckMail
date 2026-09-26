//! A reversible reading/index projection; callers retain original MIME separately.

/// Remove recognizable reply history while preserving new unquoted inline answers.
/// Unknown formats stay unchanged. This never edits the retained source message.
pub fn reply_contribution(body: &str) -> String {
    project(body, false)
}

/// Endings of a reply attribution such as "On Tue, Alex wrote:", in the
/// languages Gmail, Outlook and Apple Mail commonly write.
const ATTRIBUTION_ENDINGS: &[&str] = &[
    "wrote:",
    "a écrit :",
    "a écrit:",
    "schrieb:",
    "escribió:",
    "scrisse:",
    "escreveu:",
    "napisał(a):",
    "napisał:",
    "schreef:",
    "skrev:",
    "kirjoitti:",
    "írta:",
    "yazdı:",
    "пишет:",
    "написал:",
    "написала:",
    "написал(а):",
    "napsal:",
    "写道：",
    "寫道：",
    "写道:",
    "寫道:",
    "のメッセージ:",
    "님이 작성:",
];

/// Header field of a quoted message block ("From:", "Von:", "De :"...).
#[derive(PartialEq)]
enum Field {
    From,
    To,
    Subject,
    Date,
}
fn header_field(line: &str) -> Option<Field> {
    let (name, _) = line.split_once(':')?;
    let name = name.trim().to_lowercase();
    Some(match name.as_str() {
        "from" | "von" | "de" | "da" | "van" | "od" | "från" | "от" => Field::From,
        "to" | "an" | "à" | "a" | "para" | "aan" | "do" | "till" | "кому" => Field::To,
        "subject" | "betreff" | "objet" | "asunto" | "oggetto" | "assunto" | "onderwerp"
        | "temat" | "ämne" | "тема" => Field::Subject,
        "date"
        | "sent"
        | "datum"
        | "gesendet"
        | "envoyé"
        | "fecha"
        | "enviado"
        | "inviato"
        | "data"
        | "verzonden"
        | "wysłano"
        | "skickat"
        | "дата"
        | "отправлено" => Field::Date,
        _ => return None,
    })
}
/// The line index ending an attribution that starts at `i`, if one does.
fn attribution_end(lines: &[&str], i: usize) -> Option<usize> {
    let first = lines[i].trim();
    // "On …" is English; other languages start with a date or weekday, so
    // require a digit to avoid treating ordinary prose as an attribution.
    if !(first.starts_with("On ") || first.chars().any(|c| c.is_ascii_digit())) {
        return None;
    }
    let mut joined = String::new();
    for (end, line) in lines
        .iter()
        .enumerate()
        .take(lines.len().min(i + 5))
        .skip(i)
    {
        joined.push(' ');
        joined.push_str(line.trim());
        let text = joined.trim_end();
        // German puts the verb before the sender: "Am … schrieb Alex <a@b>:".
        if ATTRIBUTION_ENDINGS.iter().any(|e| text.ends_with(e))
            || (text.contains(" schrieb ") && text.ends_with(':'))
        {
            return Some(end);
        }
    }
    None
}

/// Drops `>` quotes, reply attributions and quoted header blocks from `body`.
///
/// An attribution followed by `>` lines is skipped and the answers between
/// quotes are kept. One with no quotes after it ends the new text, unless
/// `html_quotes_removed` says its quotes were HTML blockquotes that the caller
/// already dropped; then only the attribution is skipped.
pub(crate) fn project(body: &str, html_quotes_removed: bool) -> String {
    let lines: Vec<&str> = body.lines().collect();
    let mut kept = Vec::new();
    let mut changed = false;
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i].trim();
        let lower = line.to_lowercase();
        // A forward is content the sender intentionally included in this
        // message ("Forwarded message", Apple Mail's "Begin forwarded message:").
        if lower.contains("forwarded message") {
            kept.extend_from_slice(&lines[i..]);
            break;
        }
        if line.starts_with('>') {
            changed = true;
            i += 1;
            continue;
        }
        // An "-----Original Message-----" separator or a quoted header block
        // ends the new text. A block needs From followed by To, Subject and
        // Date within nine lines, so prose that starts with "From:" survives.
        let original = line.starts_with(['-', '_', '—'])
            && lower.trim_matches(['-', '_', '—', ' ', '\t']) == "original message";
        let headers = header_field(line) == Some(Field::From) && {
            let following: Vec<Option<Field>> = lines[i..lines.len().min(i + 9)]
                .iter()
                .map(|line| header_field(line.trim()))
                .collect();
            [Field::To, Field::Subject, Field::Date]
                .iter()
                .all(|field| following.iter().any(|f| f.as_ref() == Some(field)))
        };
        if original || headers {
            changed = true;
            break;
        }
        if let Some(end) = attribution_end(&lines, i) {
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
        kept.push(lines[i]);
        i += 1;
    }
    // Unchanged input comes back byte for byte, keeping any CRLF line endings.
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

    #[test]
    fn localized_attributions_and_outlook_headers_end_the_contribution() {
        for (input, expected) in [
            ("Merci !\n\nLe lun. 22 sept. 2026 à 10:00, Alex <alex@example.test> a écrit :\n> Question", "Merci !"),
            ("Danke\n\nAm Mo., 22. Sept. 2026 um 10:00 Uhr schrieb Alex\n<alex@example.test>:\n> Frage", "Danke"),
            ("Gracias\n\nEl lun, 22 sept 2026 a las 10:00, Alex escribió:\n> Pregunta", "Gracias"),
            ("Спасибо\n\n22.09.2026, 10:00, Alex пишет:\n> Вопрос", "Спасибо"),
            ("Thanks\n\nOn Mon, Sep 22, 2026 at 10:00 AM Alex <alex@example.test>\nwrote:\n", "Thanks"),
            ("Bien reçu\n\nDe : Alex\nEnvoyé : lundi 22 septembre 2026 10:00\nÀ : Sam\nObjet : Sujet\n\nAncien texte", "Bien reçu"),
            ("Passt\n\nVon: Alex\nGesendet: Montag, 22. September 2026\nAn: Sam\nBetreff: Thema\n\nAlt", "Passt"),
            ("Done\n\n________________________________\nFrom: Alex\nSent: Monday\nTo: Sam\nCc: Kim\nSubject: Topic\n\nOld", "Done\n\n________________________________"),
        ] {
            assert_eq!(reply_contribution(input), expected, "{input}");
        }
    }

    #[test]
    fn prose_that_resembles_an_attribution_is_kept() {
        for input in [
            "The reviewer wrote: this is fine\nThanks",
            "We met on Tuesday and she wrote:\nnothing else",
            "Begin forwarded message:\n\nFrom: Alex\nDate: Monday\nTo: Sam\nSubject: Topic\n\nBody",
            "De: nada\nGracias",
        ] {
            assert_eq!(reply_contribution(input), input, "{input}");
        }
    }
}
