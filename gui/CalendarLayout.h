#pragma once
#include <QDate>
#include <QJsonArray>
#include <QTimeZone>
#include <QVariantList>

// Presentation only: provider identities and permission-filtered fields are unchanged.
// Clip occurrences to visible civil days and allocate overlapping event columns.
QVariantList calendarPresentation(const QJsonArray &events, QDate first, int days, const QTimeZone &zone);
QVariantList reflowCalendarPresentation(const QVariantList &events);
