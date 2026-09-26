#pragma once
#include <QDate>
#include <QJsonArray>
#include <QTimeZone>
#include <QVariantList>

// Presentation only: provider identities and permission-filtered fields are unchanged.
// Clip occurrences to visible civil days and allocate overlapping event columns.

/// Lays out engine calendar rows for a grid of `days` civil days starting at
/// `first`, in `zone`.
///
/// Each input object keeps every field and gains display fields. Timed rows
/// (`start`/`end` as ISO date-times) become one segment per visible day they
/// touch, with `day_index`, `date`, `start_hour` and `duration_hours` in
/// fractional hours of that civil day (0 to 24, drawn at least 0.25 h tall),
/// plus overlap `column` and `columns`. A continuation segment's
/// `time_display` is "Continued". All-day rows (`all_day` true, ISO dates with
/// an exclusive `end`, as Google Calendar sends them) become one row with
/// `span_days` and a stacking `lane`.
///
/// Returns all-day rows first, then timed segments, each sorted by day and
/// start. Rows outside the range, with unparsable or empty intervals, are
/// dropped. Returns an empty list when `first` or `zone` is invalid or `days`
/// is outside 1 to 42. Pure function; safe on any thread.
QVariantList calendarPresentation(const QJsonArray &events, QDate first, int days, const QTimeZone &zone);

/// Recomputes `column`, `columns` and `lane` for rows that
/// calendarPresentation() already produced, for example after some were
/// filtered out. Uses only the rows' `day_index`, `start_hour`,
/// `duration_hours`, `span_days` and `all_day`; other fields are unchanged.
QVariantList reflowCalendarPresentation(const QVariantList &events);
