#include "CalendarLayout.h"
#include <QCoreApplication>
#include <QJsonObject>
#include <QDebug>
#include <cstdlib>

static void require(bool condition, const char *message) {
    if (!condition) { qCritical() << message; std::exit(1); }
}
static QJsonObject timed(const char *id, const char *start, const char *end) {
    return {{"id", id}, {"start", start}, {"end", end}, {"all_day", false}, {"title", id}};
}
int main(int argc, char **argv) {
    QCoreApplication app(argc, argv);
    const QTimeZone zone("America/Toronto");
    auto rows = calendarPresentation({
        timed("long", "2026-09-11T09:00:00-04:00", "2026-09-11T12:00:00-04:00"),
        timed("early", "2026-09-11T10:00:00-04:00", "2026-09-11T11:00:00-04:00"),
        timed("third", "2026-09-11T10:30:00-04:00", "2026-09-11T11:30:00-04:00"),
        timed("touching", "2026-09-11T12:00:00-04:00", "2026-09-11T13:00:00-04:00")
    }, QDate(2026,9,7), 7, zone);
    require(rows.size() == 4, "All overlapping identities remain visible");
    require(rows[0].toMap()["columns"].toInt() == 3 && rows[2].toMap()["column"].toInt() == 2, "Three overlaps need three columns");
    require(rows[3].toMap()["columns"].toInt() == 1, "A touching event starts a full-width group");
    rows = calendarPresentation({timed("overnight", "2026-09-10T23:00:00-04:00", "2026-09-11T01:30:00-04:00")}, QDate(2026,9,7), 7, zone);
    require(rows.size() == 2, "An overnight occurrence has two visible segments");
    require(rows[0].toMap()["id"] == rows[1].toMap()["id"], "Clipping preserves the provider occurrence identity");
    require(rows[0].toMap()["duration_hours"].toDouble() == 1.0 && rows[1].toMap()["duration_hours"].toDouble() == 1.5, "Segments clip at civil midnight");
    require(rows[0].toMap()["multi_day"].toBool() && rows[1].toMap()["duration_minutes"].toInt() == 150, "Details retain the full multi-day duration across clipped segments");
    auto today = calendarPresentation({timed("overnight", "2026-09-10T23:00:00-04:00", "2026-09-11T01:30:00-04:00")}, QDate(2026,9,11), 1, zone);
    require(today.size() == 1 && today[0].toMap()["start_hour"].toDouble() == 0.0, "Footer receives only the part in today");
    rows = calendarPresentation({
        QJsonObject{{"id", "trip"}, {"start", "2026-09-06"}, {"end", "2026-09-10"}, {"all_day", true}},
        QJsonObject{{"id", "overlap"}, {"start", "2026-09-09"}, {"end", "2026-09-11"}, {"all_day", true}},
        QJsonObject{{"id", "after"}, {"start", "2026-09-11"}, {"end", "2026-09-12"}, {"all_day", true}}
    }, QDate(2026,9,7), 7, zone);
    require(rows.size() == 3, "All-day dates remain visible");
    require(rows[0].toMap()["day_index"].toInt() == 0 && rows[0].toMap()["span_days"].toInt() == 3, "All-day interval clips its start and preserves exclusive end");
    require(rows[1].toMap()["lane"].toInt() == 1 && rows[2].toMap()["lane"].toInt() == 0, "All-day overlaps use lanes and reuse available space");
    rows = calendarPresentation({
        timed("fall-back", "2026-11-01T01:30:00-04:00", "2026-11-01T01:45:00-05:00"),
        timed("spring-forward", "2026-03-08T01:30:00-05:00", "2026-03-08T03:30:00-04:00")
    }, QDate(2026,11,1), 1, zone);
    require(rows.size() == 1 && rows[0].toMap()["duration_hours"].toDouble() > 0, "Repeated-hour events remain visible and preserve identity");
    auto source = timed("cross-zone", "2026-09-12T02:00:00+09:00", "2026-09-12T03:00:00+09:00");
    source["account"] = "sideproject";
    source["calendar_id"] = "primary";
    source["busy_only"] = true;
    rows = calendarPresentation({source}, QDate(2026,9,11), 1, zone);
    require(rows.size() == 1 && rows[0].toMap()["start_hour"].toDouble() == 13, "Display dates use the common zone rather than source timestamp date");
    require(rows[0].toMap()["account"].toString() == "sideproject" && rows[0].toMap()["busy_only"].toBool(), "Layout retains scoped identity and restricted projection");
    require(calendarPresentation({source}, QDate(2026,10,1), 42, zone).empty(), "A displayed month excluding today cannot inherit today's occurrences");
    qInfo() << "Calendar layout passed: overlaps, midnight clipping, all-day overflow lanes, DST, cross-zone identities, independent ranges.";
}
