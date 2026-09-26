// The Mail workspace's timeline strip of the day's events across accounts,
// docked above or below the mail view (preferences.timeline_position). Drag
// sideways to change day; an event opens EventDetails, and Edit opens EventEditor.
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Shapes

Rectangle {
    id: timeline
    objectName: "mailTimeline"
    color: Theme.bg0
    property bool headerPosition: false
    signal openCalendar()
    property int firstHour: backend.preferences.timeline_start_hour === undefined ? 0 : backend.preferences.timeline_start_hour
    property int lastHour: backend.preferences.timeline_end_hour === undefined ? 24 : backend.preferences.timeline_end_hour
    property real hours: Math.max(1, lastHour - firstHour)
    property real trackWidth: Math.max(1, width - 48)
    property int shownLanes: 1
    property int hiddenEvents: 0
    property real dragOffset: 0
    property real pressedX: 0
    property bool dragging: false
    property int navigationDelta: 0
    implicitHeight: 40 + shownLanes * 28
    onVisibleChanged: if (!visible) { details.close(); cancelDrag() }
    function showEvent(event, item) {
        if (!backend.timelineReady || !backend.connected) return
        if (editor.opened) { editor.contentItem.forceActiveFocus(); return }
        // Where the strip meets the page, read now: a binding keeps the position
        // from before the window laid out and opened the card at the top.
        details.timelineEdge = timeline.mapToItem(Overlay.overlay, 0, timeline.headerPosition ? timeline.height : 0).y
        details.openAt(event, item || timeline)
    }
    function beginDrag(x) { if (slide.running) return; pressedX = x; dragging = false }
    function cancelDrag() { slide.stop(); navigationDelta = 0; dragging = false; dragOffset = 0 }
    function moveDrag(x) { if (slide.running) return; let delta = x - pressedX; if (Math.abs(delta) > 4) { dragging = true; dragOffset = Math.max(-width, Math.min(width, delta)); details.close() } }
    function endDrag() {
        if (slide.running) return true
        let moved = dragging
        if (!moved) return false
        navigationDelta = Math.abs(dragOffset) > width * .18 ? (dragOffset < 0 ? 1 : -1) : 0
        slide.to = navigationDelta ? -navigationDelta * width : 0; slide.start(); dragging = false
        return moved
    }
    NumberAnimation {
        id: slide; target: timeline; property: "dragOffset"; duration: 230; easing.type: Easing.OutCubic
        onFinished: { let delta = timeline.navigationDelta; timeline.navigationDelta = 0; if (delta) backend.moveTimeline(delta); timeline.dragOffset = 0 }
    }
    Rectangle { x: Theme.snapX(parent, 0); width: parent.width; y: Theme.snapY(parent, timeline.headerPosition ? parent.height - 1 : 0); height: Theme.ruleWidth; color: Theme.line }
    Item {
        anchors.fill: parent; clip: true
        Repeater {
            id: dayPanels; model: 3
            Item {
                id: panel; required property int index
                property var snapshot: backend.timelinePanels[index]
                objectName: ["timelinePanelPrevious", "timelinePanelCurrent", "timelinePanelNext"][index]
                x: (index - 1) * width + timeline.dragOffset; width: parent.width; height: parent.height
                enabled: index === 1; clip: true
                Accessible.ignored: index !== 1
                property var timedEvents: snapshot.events.filter(function(e) { return !e.all_day })
                property var blocks: {
                    let nowDate = panel.snapshot.date === backend.todayDate, nextId = ""
                    let rows = timedEvents.filter(function(e) { return e.start_hour + e.duration_hours > firstHour && e.start_hour < lastHour })
                    // Declined events are never "next".
                    for (let e of rows) if (nowDate && e.start_hour > backend.nowHour && !Theme.eventDeclined(e)) { nextId = e.id; break }
                    return rows.map(function(e) {
                        let row = Object.assign({}, e)
                        row.current = nowDate && e.start_hour <= backend.nowHour && e.start_hour + e.duration_hours > backend.nowHour
                        row.next = e.id === nextId
                        row.past = panel.snapshot.date < backend.todayDate || (nowDate && e.start_hour + e.duration_hours <= backend.nowHour)
                        row.label = row.current ? e.title + " · until " + e.end_display : row.next ? e.time_display + " " + e.title : e.title
                        row.fraction = (Math.max(firstHour, e.start_hour) - firstHour) / hours
                        row.fractionWidth = (Math.min(lastHour, e.start_hour + e.duration_hours) - Math.max(firstHour, e.start_hour)) / hours
                        return row
                    })
                }
                // Geometry updates keep the occurrence model and its native delegates alive.
                // Only this small lane-index array changes as the available width changes.
                property var laneAssignments: {
                    let ends = []
                    return blocks.map(function(row) {
                        const left = row.fraction * trackWidth
                        // The label starts with the 14px account avatar: about 20px more.
                        const right = Math.max(left + row.fractionWidth * trackWidth, left + 36 + row.label.length * 6.6)
                        let lane = ends.findIndex(function(end) { return left >= end + 6 })
                        if (lane < 0) { lane = ends.length; ends.push(right) } else ends[lane] = right
                        return lane
                    })
                }
                property int lanes: Math.max(1, ...laneAssignments.map(function(lane) { return lane + 1 }))
                property int shownLanes: Math.min(3, lanes)
                property int hiddenEvents: panel.snapshot.events.length - laneAssignments.filter(function(lane) { return lane < shownLanes }).length
                property var overlapRanges: {
                    let points = [], ranges = [], active = 0, began = 0
                    for (let e of timedEvents) {
                        let start = Math.max(firstHour, e.start_hour), end = Math.min(lastHour, e.start_hour + e.duration_hours)
                        if (end > start) { points.push({at: start, change: 1}); points.push({at: end, change: -1}) }
                    }
                    points.sort(function(a, b) { return a.at - b.at || a.change - b.change })
                    for (let p of points) {
                        let was = active; active += p.change
                        if (was < 2 && active >= 2) began = p.at
                        if (was >= 2 && active < 2 && p.at > began) ranges.push({start: began, end: p.at})
                    }
                    return ranges
                }
                readonly property int dayOffset: Math.round((new Date(panel.snapshot.date + "T12:00:00") - new Date(backend.todayDate + "T12:00:00")) / 86400000)
                readonly property bool isToday: dayOffset === 0
                readonly property real markerEnd: dayMarker.x + dayMarker.width + 6
                Binding { target: timeline; property: "shownLanes"; value: panel.shownLanes; when: panel.index === 1 }
                Binding { target: timeline; property: "hiddenEvents"; value: panel.hiddenEvents; when: panel.index === 1 }
                MouseArea {
                    anchors.fill: parent; cursorShape: timeline.dragging ? Qt.ClosedHandCursor : Qt.OpenHandCursor
                    onPressed: function(mouse) { timeline.beginDrag(mapToItem(timeline, mouse.x, mouse.y).x) }
                    onPositionChanged: function(mouse) { if (pressed) timeline.moveDrag(mapToItem(timeline, mouse.x, mouse.y).x) }
                    onReleased: timeline.endDrag()
                    onCanceled: timeline.cancelDrag()
                }
                Repeater {
                    model: timeline.hours + 1
                    Item {
                        required property int index
                        x: 24 + index / timeline.hours * timeline.trackWidth; height: panel.height
                        // The day separator replaces the first hour's rule.
                        Rectangle { visible: index > 0; x: Theme.snapX(parent, 0); y: Theme.snapY(parent, 24); width: Theme.ruleWidth; height: panel.shownLanes * 28 + 8; color: Theme.bg3 }
                        // No label at the edges, under the day marker or beside now.
                        Text { visible: index > 0 && index < timeline.hours && parent.x - width / 2 > panel.markerEnd && (!panel.isToday || Math.abs(timeline.firstHour + index - backend.nowHour) > .42); y: 8; x: index === timeline.hours ? -width : -width / 2; text: (timeline.firstHour + index < 10 ? "0" : "") + (timeline.firstHour + index); color: Theme.inkAlpha((timeline.firstHour + index) % 3 === 0 || index === 0 || index === timeline.hours ? .45 : .28); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(10.5) }
                    }
                }
                Repeater {
                    model: panel.overlapRanges
                    Loader {
                        id: overlapHatch
                        required property var modelData
                        property color hatchColor: Theme.inkAlpha(.16)
                        x: 24 + (modelData.start - timeline.firstHour) / timeline.hours * timeline.trackWidth; y: 24
                        width: Math.max(3, (modelData.end - modelData.start) / timeline.hours * timeline.trackWidth); height: panel.shownLanes * 28 + 8
                        sourceComponent: GraphicsInfo.api === GraphicsInfo.Software ? softwareHatch : nativeHatch
                        Component { id: nativeHatch; HatchItem { color: overlapHatch.hatchColor } }
                        Component {
                            id: softwareHatch
                            Canvas {
                                property color hatchColor: overlapHatch.hatchColor
                                onHatchColorChanged: requestPaint()
                                onWidthChanged: requestPaint()
                                onHeightChanged: requestPaint()
                                onPaint: {
                                    let c = getContext("2d"); c.reset(); c.strokeStyle = hatchColor; c.lineWidth = 1
                                    for (let x = -height; x < width + height; x += 6) { c.beginPath(); c.moveTo(x, height); c.lineTo(x + height, 0); c.stroke() }
                                }
                            }
                        }
                    }
                }
                Repeater {
                    model: panel.blocks
                    Item {
                        id: eventBlock; required property var modelData; required property int index
                        objectName: (panel.index === 1 ? "timelineEvent-" : "timelineNeighbor-" + panel.snapshot.date + "-") + (modelData.event_id || modelData.id)
                        readonly property int lane: panel.laneAssignments[index] === undefined ? 0 : panel.laneAssignments[index]
                        visible: lane < panel.shownLanes
                        x: 24 + modelData.fraction * timeline.trackWidth; y: 28 + lane * 28
                        width: Math.min(panel.width - x - 24, Math.max(modelData.fractionWidth * timeline.trackWidth, blockLabel.x + blockLabel.implicitWidth + 8)); height: 22
                        activeFocusOnTab: panel.index === 1 && visible && backend.timelineReady
                        // Invitations as in Day and Week: Maybe striped, not answered
                        // outlined, declined (shown on request) faint and struck through.
                        readonly property var rsvp: Theme.rsvpOf(modelData)
                        readonly property bool marked: !modelData.current && !modelData.next && !modelData.past
                        readonly property bool maybe: marked && rsvp === "maybe"
                        readonly property bool unanswered: marked && rsvp === ""
                        readonly property bool declined: rsvp === "no"
                        property color eventColor: modelData.current ? Theme.error : modelData.next ? Theme.accent : modelData.past ? Theme.inkAlpha(.4) : declined ? Theme.inkAlpha(.35) : unanswered ? Theme.inkAlpha(.7) : maybe ? Theme.inkAlpha(.85) : Theme.text
                        Keys.onReturnPressed: timeline.showEvent(modelData, eventBlock)
                        Keys.onSpacePressed: timeline.showEvent(modelData, eventBlock)
                        Rectangle {
                            width: Math.max(6, eventBlock.modelData.fractionWidth * timeline.trackWidth); height: 22; radius: 3
                            Rectangle { width: 3; height: parent.height; color: parent.color; visible: parent.color.a > 0 }
                            color: eventBlock.modelData.current ? Theme.errorAlpha(eventMouse.containsMouse ? .3 : .2) : eventBlock.modelData.next ? Theme.accentAlpha(eventMouse.containsMouse ? .32 : .22) : eventBlock.modelData.past ? (eventMouse.containsMouse ? Theme.surface : "transparent") : eventBlock.unanswered || eventBlock.declined ? (eventMouse.containsMouse ? Theme.inkAlpha(.04) : "transparent") : eventMouse.containsMouse ? Theme.hover : Theme.selected
                            border.width: eventBlock.modelData.past || eventBlock.activeFocus ? 1 : 0; border.color: eventBlock.activeFocus ? Theme.accent : Theme.line
                            clip: true
                            Canvas {
                                anchors.fill: parent; visible: eventBlock.maybe
                                onPaint: { const c = getContext("2d"); c.reset(); c.fillStyle = Theme.inkAlpha(.045); c.rotate(Math.PI / 4); const d = width + height; for (let s = -d; s < d; s += 10) c.fillRect(s + 5, -d, 5, 2 * d) }
                                onWidthChanged: requestPaint(); onHeightChanged: requestPaint(); onVisibleChanged: if (visible) requestPaint()
                            }
                            Shape {
                                anchors.fill: parent; visible: eventBlock.unanswered && !eventBlock.activeFocus; preferredRendererType: Shape.CurveRenderer
                                ShapePath {
                                    strokeColor: Theme.accountTint(eventBlock.modelData.account, .45); strokeWidth: 1; fillColor: "transparent"; strokeStyle: ShapePath.DashLine; dashPattern: [3, 3]
                                    PathRectangle { x: .5; y: .5; width: Math.max(0, eventBlock.width - 1); height: 21; radius: 3 }
                                }
                            }
                            Rectangle { width: 2; height: parent.height; color: eventBlock.modelData.busy_only ? Theme.subtle : eventBlock.eventColor }
                        }
                        // Whose event: the account's letter in its colour, dimmed once past.
                        AccountBadge {
                            x: 7; anchors.verticalCenter: parent.verticalCenter; diameter: 14; selected: true; tipEnabled: false
                            account: eventBlock.modelData.account || ""; accountName: eventBlock.modelData.account_name || ""
                            opacity: eventBlock.modelData.past ? .55 : 1
                        }
                        Text { id: blockLabel; x: 26; anchors.verticalCenter: parent.verticalCenter; width: Math.max(0, parent.width - x); text: eventBlock.modelData.label; color: eventBlock.eventColor; font.strikeout: eventBlock.declined; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(11.5); font.weight: eventBlock.modelData.current || eventBlock.modelData.next ? Font.Medium : Font.Normal; elide: Text.ElideRight; textFormat: Text.PlainText }
                        MouseArea {
                            id: eventMouse; anchors.fill: parent; hoverEnabled: true; cursorShape: timeline.dragging ? Qt.ClosedHandCursor : Qt.PointingHandCursor
                            onPressed: function(mouse) { timeline.beginDrag(mapToItem(timeline, mouse.x, mouse.y).x) }
                            onPositionChanged: function(mouse) { if (pressed) timeline.moveDrag(mapToItem(timeline, mouse.x, mouse.y).x) }
                            onReleased: if (!timeline.endDrag()) timeline.showEvent(eventBlock.modelData, eventBlock)
                            onCanceled: timeline.cancelDrag()
                        }
                        FmToolTip { shown: eventMouse.containsMouse && !timeline.dragging; text: modelData.title + (Theme.responseLabel(modelData) ? " · " + Theme.responseLabel(modelData) : "") + "\n" + modelData.time_display + " – " + modelData.end_display + "\n" + modelData.account_name + " / " + modelData.calendar_name }
                        Accessible.ignored: panel.index !== 1
                        Accessible.role: Accessible.Button; Accessible.name: modelData.title + ", " + modelData.start_display
                        Accessible.onPressAction: timeline.showEvent(modelData, eventBlock)
                    }
                }
                // The day separator and marker: date, weekday and Today/Tomorrow/Yesterday.
                Rectangle {
                    objectName: panel.index === 1 ? "timelineDaySeparator" : ""
                    x: Theme.snapX(panel, 24); width: Theme.ruleWidth; height: panel.height
                    color: panel.isToday ? Theme.accentAlpha(.45) : Theme.inkAlpha(.2)
                }
                Row {
                    id: dayMarker
                    objectName: panel.index === 1 ? "timelineDayMarker" : ""
                    x: 30; y: 5; spacing: 5
                    readonly property string relative: panel.dayOffset === 0 ? "Today" : panel.dayOffset === 1 ? "Tomorrow" : panel.dayOffset === -1 ? "Yesterday" : ""
                    readonly property string label: dayNumber.text + " " + weekday.text + (relative.length > 0 ? " · " + relative : "")
                    Accessible.role: Accessible.StaticText; Accessible.name: label
                    Text { id: dayNumber; text: new Date(panel.snapshot.date + "T12:00:00").getDate(); color: panel.isToday ? Theme.accent : Theme.text; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12); font.weight: Font.DemiBold; font.features: {"tnum": 1} }
                    Text { id: weekday; anchors.baseline: dayNumber.baseline; text: Qt.formatDate(new Date(panel.snapshot.date + "T12:00:00"), "ddd"); color: Theme.inkAlpha(.5); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(10.5); font.capitalization: Font.AllUppercase; font.letterSpacing: Theme.fontSize(10.5) * .06 }
                    Text { visible: dayMarker.relative.length > 0; anchors.baseline: dayNumber.baseline; text: "·"; color: Theme.inkAlpha(.25); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(10.5) }
                    Text { visible: dayMarker.relative.length > 0; anchors.baseline: dayNumber.baseline; text: dayMarker.relative; color: panel.isToday ? Theme.accent : Theme.text; opacity: .85; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(10.5) }
                }
                Item {
                    objectName: panel.index === 1 ? "timelineNowLine" : ""
                    visible: panel.isToday && backend.nowHour >= timeline.firstHour && backend.nowHour <= timeline.lastHour
                    x: 24 + (backend.nowHour - timeline.firstHour) / timeline.hours * timeline.trackWidth; height: panel.height
                    Rectangle { x: Theme.snapX(parent, 0); y: Theme.snapY(parent, 22); width: Theme.ruleWidth; height: panel.shownLanes * 28 + 12; color: Theme.accent }
                    Rectangle { visible: !nowTime.visible; x: -3; y: 19; width: 7; height: 7; radius: 3.5; color: Theme.accent }
                    // The time label gives way to the day marker; the line stays.
                    Rectangle { id: nowTime; visible: parent.x - nowLabel.implicitWidth / 2 - 3 > panel.markerEnd; y: 5; x: -nowLabel.implicitWidth / 2 - 3; width: nowLabel.implicitWidth + 6; height: 16; color: Theme.bg0
                        Text { id: nowLabel; anchors.centerIn: parent; text: (Math.floor(backend.nowHour) < 10 ? "0" : "") + Math.floor(backend.nowHour) + ":" + (Math.floor(backend.nowHour * 60) % 60 < 10 ? "0" : "") + Math.floor(backend.nowHour * 60) % 60; color: Theme.accent; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(10.5); font.weight: Font.Medium }
                    }
                }
                Rectangle {
                    visible: !panel.isToday
                    anchors.right: parent.right; anchors.rightMargin: 24; y: 6
                    width: backToToday.implicitWidth + 6; height: 16; color: Theme.bg0
                    FmButton {
                        id: backToToday
                        objectName: panel.index === 1 ? "timelineToday" : ""
                        anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter
                        text: "Back to today"; implicitHeight: 16; leftPadding: 0; rightPadding: 0; topPadding: 0; bottomPadding: 0
                        font.pointSize: Theme.fontPoints(11); foreground: hovered ? Theme.text : Theme.accent
                        background: Item {}
                        onClicked: backend.showTimelineToday()
                    }
                }
                Text { x: 30; y: 32; visible: panel.blocks.length === 0; text: panel.snapshot.ready ? "Nothing scheduled" : panel.index === 1 ? backend.timelineCoverage : "Loading timeline…"; color: Theme.subtle; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(11.5); width: panel.width - 48; elide: Text.ElideRight }
            }
        }
    }

    EventDetails { id: details; timelineSource: true; timelineInset: timeline.height; timelineAtTop: timeline.headerPosition; onEditRequested: function(event) { editor.editEvent = event; editor.openForDate(event.start.slice(0, 10)) } }
    EventEditor { id: editor; timelineSource: true; timelineInset: timeline.visible ? timeline.height : 0; timelineAtTop: timeline.headerPosition }
}
