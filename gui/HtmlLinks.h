#pragma once
#include <QRegularExpression>
#include <QString>
#include <QStringList>
#include <QUrl>
#include <algorithm>

/// Blocklist rules shared with fm_mail::html_view (the engine revalidates):
/// `tracker.io` blocks that host and `*.tracker.io` it and every subdomain.
/// Hosts compare as lowercase ASCII without `www.`.
///
/// The GUI uses these to show blocked links and to validate input; the
/// engine enforces the list itself before any fetch. Pure functions.
namespace HtmlLinks {
/// Lowercases `host` and strips every leading `www.`.
inline QString withoutWww(QString host) {
    host = host.toLower();
    while (host.startsWith(QLatin1String("www."))) host = host.mid(4);
    return host;
}
/// True for a DNS label of 1 to 63 ASCII letters, digits or hyphens that
/// neither starts nor ends with a hyphen.
inline bool validLabel(const QString &label) {
    if (label.isEmpty() || label.size() > 63 || label.startsWith('-') || label.endsWith('-')) return false;
    return std::all_of(label.cbegin(), label.cend(), [](QChar c) { return (c.isLetterOrNumber() && c.unicode() < 128) || c == '-'; });
}
/// A pattern as typed or pasted, or "" when it is not a domain.
///
/// Drops any scheme, path, query, fragment and `www.`, converts
/// internationalised names to ASCII (punycode), and keeps a leading `*.`.
/// Rejects ports, user info, wildcards elsewhere, single-label names and
/// numeric top-level labels (so IPv4 addresses are not patterns).
inline QString normalizePattern(const QString &input) {
    QString value = input.trimmed().toLower();
    if (const auto index = value.indexOf(QLatin1String("://")); index >= 0) value = value.mid(index + 3);
    static const QRegularExpression tail(QStringLiteral("[/?#]"));
    if (const auto index = value.indexOf(tail); index >= 0) value.truncate(index);
    const bool wildcard = value.startsWith(QLatin1String("*."));
    QString name = value;
    while (name.startsWith(QLatin1String("*."))) name = name.mid(2);
    while (name.startsWith(QLatin1String("www."))) name = name.mid(4);
    if (name.isEmpty() || name.size() > 253 || name.contains('*') || name.contains(':') || name.contains('@') || name.contains(' ')) return {};
    const auto ascii = QString::fromLatin1(QUrl::toAce(name)).toLower();
    const auto labels = ascii.split('.');
    if (ascii.isEmpty() || labels.size() < 2 || !std::all_of(labels.cbegin(), labels.cend(), validLabel)) return {};
    const auto &tld = labels.last();
    if (tld.size() < 2 || std::all_of(tld.cbegin(), tld.cend(), [](QChar c) { return c.isDigit(); })) return {};
    return wildcard ? QStringLiteral("*.") + ascii : ascii;
}
/// The first pattern that blocks `host`, or "".
/// `patterns` must already be normalised; `input` is lowercased and loses
/// its `www.` before comparing.
inline QString blockedBy(const QStringList &patterns, const QString &input) {
    const auto host = withoutWww(input);
    if (host.isEmpty()) return {};
    for (const auto &pattern : patterns) {
        if (pattern.startsWith(QLatin1String("*."))) {
            const auto base = pattern.mid(2);
            if (host == base || host.endsWith('.' + base)) return pattern;
        } else if (host == pattern) return pattern;
    }
    return {};
}
/// The host of `url` in ASCII form, lowercase and without `www.`; "" when
/// `url` does not parse strictly.
inline QString hostOf(const QString &url) {
    const QUrl parsed(url, QUrl::StrictMode);
    return parsed.isValid() ? withoutWww(parsed.host(QUrl::FullyEncoded)) : QString();
}
/// The design's plain-text label: no scheme or `www.`.
inline QString label(const QString &url) {
    static const QRegularExpression prefix(QStringLiteral("^https?://(www\\.)?"), QRegularExpression::CaseInsensitiveOption);
    return QString(url).remove(prefix);
}
}
