#include "CalendarLayout.h"
#include <QDateTime>
#include <QJsonObject>
#include <algorithm>
#include <vector>

namespace {
double hour(const QTime &time) { return time.hour() + time.minute() / 60.0 + time.second() / 3600.0; }
struct Segment { QVariantMap event; int day; double start, end; };
QVariantList layoutSegments(std::vector<Segment> &timed, std::vector<Segment> &allDay) {
    auto order = [](const Segment &a, const Segment &b) {
        if (a.day != b.day) return a.day < b.day;
        if (a.start != b.start) return a.start < b.start;
        if (a.end != b.end) return a.end > b.end;
        return a.event["id"].toString() < b.event["id"].toString();
    };
    std::sort(timed.begin(), timed.end(), order);
    // A connected overlap group shares its peak column count. Touching ends do not overlap.
    for (size_t begin = 0; begin < timed.size();) {
        size_t stop = begin + 1;
        double groupEnd = timed[begin].end;
        while (stop < timed.size() && timed[stop].day == timed[begin].day && timed[stop].start < groupEnd) {
            groupEnd = std::max(groupEnd, timed[stop].end); ++stop;
        }
        std::vector<double> ends;
        for (size_t index = begin; index < stop; ++index) {
            auto free = std::find_if(ends.begin(), ends.end(), [&](double end) { return end <= timed[index].start; });
            const auto column = static_cast<int>(std::distance(ends.begin(), free));
            if (free == ends.end()) ends.push_back(timed[index].end); else *free = timed[index].end;
            timed[index].event["column"] = column;
        }
        for (size_t index = begin; index < stop; ++index) timed[index].event["columns"] = int(ends.size());
        begin = stop;
    }
    std::sort(allDay.begin(), allDay.end(), order);
    std::vector<double> laneEnds;
    for (auto &segment : allDay) {
        auto free = std::find_if(laneEnds.begin(), laneEnds.end(), [&](double end) { return end <= segment.start; });
        segment.event["lane"] = int(std::distance(laneEnds.begin(), free));
        if (free == laneEnds.end()) laneEnds.push_back(segment.end); else *free = segment.end;
    }
    QVariantList result;
    for (auto &segment : allDay) result.append(segment.event);
    for (auto &segment : timed) result.append(segment.event);
    return result;
}
}

QVariantList calendarPresentation(const QJsonArray &events, QDate first, int days, const QTimeZone &zone) {
    if (!first.isValid() || days < 1 || days > 42 || !zone.isValid()) return {};
    const auto last = first.addDays(days);
    std::vector<Segment> timed, allDay;
    for (const auto &entry : events) {
        auto event = entry.toObject().toVariantMap();
        if (event["all_day"].toBool()) {
            const auto start = QDate::fromString(event["start"].toString(), Qt::ISODate);
            const auto end = QDate::fromString(event["end"].toString(), Qt::ISODate);
            if (!start.isValid() || !end.isValid() || end <= start || start >= last || end <= first) continue;
            const auto clipped = std::max(start, first), clippedEnd = std::min(end, last);
            const int day = static_cast<int>(first.daysTo(clipped));
            event["date"] = clipped.toString(Qt::ISODate);
            event["day_index"] = day;
            event["span_days"] = clipped.daysTo(clippedEnd);
            event["start_hour"] = 0.0; event["duration_hours"] = 24.0;
            event["time_display"] = "All day";
            event["multi_day"] = start.daysTo(end) > 1;
            event["start_display"] = start.toString("ddd d MMM") + " · all day";
            event["end_display"] = end.addDays(-1).toString("ddd d MMM") + " (inclusive)";
            allDay.push_back({event, day, double(day), double(first.daysTo(clippedEnd))});
            continue;
        }
        const auto start = QDateTime::fromString(event["start"].toString(), Qt::ISODate).toTimeZone(zone);
        const auto end = QDateTime::fromString(event["end"].toString(), Qt::ISODate).toTimeZone(zone);
        if (!start.isValid() || !end.isValid() || end <= start) continue;
        const bool offsetChanges = start.offsetFromUtc() != end.offsetFromUtc();
        event["multi_day"] = start.date() != end.date();
        event["duration_minutes"] = start.secsTo(end) / 60;
        event["start_display"] = start.toString(offsetChanges ? "ddd d MMM, HH:mm t" : "ddd d MMM, HH:mm");
        event["end_display"] = end.toString(start.date() == end.date() ? (offsetChanges ? "HH:mm t" : "HH:mm") : (offsetChanges ? "ddd d MMM, HH:mm t" : "ddd d MMM, HH:mm"));
        for (int day = 0; day < days; ++day) {
            const auto date = first.addDays(day);
            const auto dayStart = date.startOfDay(zone), dayEnd = date.addDays(1).startOfDay(zone);
            if (!dayStart.isValid() || !dayEnd.isValid() || start >= dayEnd || end <= dayStart) continue;
            const auto clipped = std::max(start, dayStart), clippedEnd = std::min(end, dayEnd);
            const double fromHour = hour(clipped.time());
            // Civil day columns have 24 display hours, even across an offset change.
            const double toHour = clippedEnd == dayEnd ? 24.0 : hour(clippedEnd.time());
            const double visibleEnd = std::min(24.0, std::max(fromHour + 0.25, toHour));
            auto segment = event;
            segment["date"] = date.toString(Qt::ISODate);
            segment["day_index"] = day;
            segment["start_hour"] = fromHour;
            segment["duration_hours"] = visibleEnd - fromHour;
            segment["time_display"] = clipped == start ? start.toString("HH:mm") : QStringLiteral("Continued");
            timed.push_back({segment, day, fromHour, visibleEnd});
        }
    }
    return layoutSegments(timed, allDay);
}

QVariantList reflowCalendarPresentation(const QVariantList &events) {
    std::vector<Segment> timed, allDay;
    for (const auto &value : events) {
        const auto row = value.toMap();
        const auto day = row.value("day_index").toInt();
        if (row.value("all_day").toBool()) allDay.push_back({row, day, double(day), double(day + row.value("span_days", 1).toInt())});
        else {
            const auto start = row.value("start_hour").toDouble();
            timed.push_back({row, day, start, start + row.value("duration_hours").toDouble()});
        }
    }
    return layoutSegments(timed, allDay);
}
