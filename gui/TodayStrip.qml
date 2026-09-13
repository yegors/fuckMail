import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Rectangle {
    id: timeline
    color: Theme.bg0
    property bool headerPosition: false
    signal openCalendar()
    property var allDayEvents: backend.todayEvents.filter(function(e) { return e.all_day })
    property var timedEvents: backend.todayEvents.filter(function(e) { return !e.all_day })
    property int firstHour: backend.preferences.timeline_start_hour === undefined ? 7 : backend.preferences.timeline_start_hour
    property int lastHour: backend.preferences.timeline_end_hour === undefined ? 21 : backend.preferences.timeline_end_hour
    property real hours: Math.max(1, lastHour - firstHour)
    property real trackWidth: Math.max(1, width - 48)
    property var blocks: {
        let nowDate = backend.timelineDate === backend.todayDate, nextId = ""
        let rows = timedEvents.filter(function(e) { return e.start_hour + e.duration_hours > firstHour && e.start_hour < lastHour })
        for (let e of rows) if (nowDate && e.start_hour > backend.nowHour) { nextId = e.id; break }
        return rows.map(function(e) {
            let row = Object.assign({}, e)
            row.current = nowDate && e.start_hour <= backend.nowHour && e.start_hour + e.duration_hours > backend.nowHour
            row.next = e.id === nextId
            row.past = backend.timelineDate < backend.todayDate || (nowDate && e.start_hour + e.duration_hours <= backend.nowHour)
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
            const right = Math.max(left + row.fractionWidth * trackWidth, left + 16 + row.label.length * 6.6)
            let lane = ends.findIndex(function(end) { return left >= end + 6 })
            if (lane < 0) { lane = ends.length; ends.push(right) } else ends[lane] = right
            return lane
        })
    }
    property int lanes: Math.max(1, ...laneAssignments.map(function(lane) { return lane + 1 }))
    property int shownLanes: Math.min(3, lanes)
    property int hiddenEvents: backend.todayEvents.length - laneAssignments.filter(function(lane) { return lane < shownLanes }).length
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
    property real dragOffset: 0
    property real pressedX: 0
    property bool dragging: false
    property int navigationDelta: 0
    implicitHeight: 40 + shownLanes * 28
    onVisibleChanged: if (!visible) { details.close(); timelineMenu.close(); dayEvents.close(); slide.stop(); dragOffset = 0 }
    function showEvent(event, item) { if (!backend.timelineReady || !backend.connected) return; if (editor.opened) { editor.contentItem.forceActiveFocus(); return } details.openAt(event, item || timeline) }
    function beginDrag(x) { if (slide.running) return; pressedX = x; dragging = false }
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
        Item {
            id: panel; x: timeline.dragOffset; width: parent.width; height: parent.height
            MouseArea {
                anchors.fill: parent; cursorShape: timeline.dragging ? Qt.ClosedHandCursor : Qt.OpenHandCursor
                onPressed: function(mouse) { timeline.beginDrag(mapToItem(timeline, mouse.x, mouse.y).x) }
                onPositionChanged: function(mouse) { if (pressed) timeline.moveDrag(mapToItem(timeline, mouse.x, mouse.y).x) }
                onReleased: timeline.endDrag()
            }
            Repeater {
                model: timeline.hours + 1
                Item {
                    required property int index
                    x: 24 + index / timeline.hours * timeline.trackWidth; height: panel.height
                    Rectangle { x: Theme.snapX(parent, 0); y: Theme.snapY(parent, 24); width: Theme.ruleWidth; height: timeline.shownLanes * 28 + 8; color: Theme.bg3 }
                    Text { visible: backend.timelineDate !== backend.todayDate || Math.abs(timeline.firstHour + index - backend.nowHour) > .42; y: 8; x: index === 0 ? 0 : index === timeline.hours ? -width : -width / 2; text: (timeline.firstHour + index < 10 ? "0" : "") + (timeline.firstHour + index); color: Theme.inkAlpha((timeline.firstHour + index) % 3 === 0 ? .5 : .35); font.pixelSize: Theme.fontSize(10.5) }
                }
            }
            Repeater {
                model: timeline.overlapRanges
                Loader {
                    id: overlapHatch
                    required property var modelData
                    property color hatchColor: Theme.inkAlpha(.16)
                    x: 24 + (modelData.start - timeline.firstHour) / timeline.hours * timeline.trackWidth; y: 24
                    width: Math.max(3, (modelData.end - modelData.start) / timeline.hours * timeline.trackWidth); height: timeline.shownLanes * 28 + 8
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
                model: timeline.blocks
                Item {
                    id: eventBlock; required property var modelData; required property int index
                    readonly property int lane: timeline.laneAssignments[index] === undefined ? 0 : timeline.laneAssignments[index]
                    visible: lane < timeline.shownLanes
                    x: 24 + modelData.fraction * timeline.trackWidth; y: 28 + lane * 28
                    width: Math.min(panel.width - x - 24, Math.max(modelData.fractionWidth * timeline.trackWidth, blockLabel.implicitWidth + 16)); height: 22
                    activeFocusOnTab: visible && backend.timelineReady
                    property color eventColor: modelData.current ? Theme.error : modelData.next ? Theme.accent : modelData.past ? Theme.inkAlpha(.4) : Theme.text
                    Keys.onReturnPressed: timeline.showEvent(modelData, eventBlock)
                    Keys.onSpacePressed: timeline.showEvent(modelData, eventBlock)
                    Rectangle {
                        width: Math.max(6, eventBlock.modelData.fractionWidth * timeline.trackWidth); height: 22; radius: 3
                        color: eventBlock.modelData.current ? Theme.errorAlpha(eventMouse.containsMouse ? .3 : .2) : eventBlock.modelData.next ? Theme.accentAlpha(eventMouse.containsMouse ? .32 : .22) : eventBlock.modelData.past ? (eventMouse.containsMouse ? Theme.surface : "transparent") : eventMouse.containsMouse ? Theme.hover : Theme.selected
                        border.width: eventBlock.modelData.past || eventBlock.activeFocus ? 1 : 0; border.color: eventBlock.activeFocus ? Theme.accent : Theme.line
                        Rectangle { width: 2; height: parent.height; color: eventBlock.modelData.busy_only ? Theme.subtle : eventBlock.eventColor }
                    }
                    Text { id: blockLabel; x: 8; y: 4; width: Math.max(0, parent.width - 8); text: eventBlock.modelData.label; color: eventBlock.eventColor; font.pixelSize: Theme.fontSize(11); font.weight: eventBlock.modelData.current || eventBlock.modelData.next ? Font.Medium : Font.Normal; elide: Text.ElideRight; textFormat: Text.PlainText }
                    MouseArea {
                        id: eventMouse; anchors.fill: parent; hoverEnabled: true; cursorShape: timeline.dragging ? Qt.ClosedHandCursor : Qt.PointingHandCursor
                        onPressed: function(mouse) { timeline.beginDrag(mapToItem(timeline, mouse.x, mouse.y).x) }
                        onPositionChanged: function(mouse) { if (pressed) timeline.moveDrag(mapToItem(timeline, mouse.x, mouse.y).x) }
                        onReleased: if (!timeline.endDrag()) timeline.showEvent(eventBlock.modelData, eventBlock)
                    }
                    FmToolTip { visible: eventMouse.containsMouse && !timeline.dragging; text: modelData.title + "\n" + modelData.time_display + " – " + modelData.end_display + "\n" + modelData.account_name + " / " + modelData.calendar_name }
                    Accessible.role: Accessible.Button; Accessible.name: modelData.title + ", " + modelData.start_display
                }
            }
            Item {
                visible: backend.timelineDate === backend.todayDate && backend.nowHour >= timeline.firstHour && backend.nowHour <= timeline.lastHour
                x: 24 + (backend.nowHour - timeline.firstHour) / timeline.hours * timeline.trackWidth; height: panel.height
                Rectangle { x: Theme.snapX(parent, 0); y: Theme.snapY(parent, 22); width: Theme.ruleWidth; height: timeline.shownLanes * 28 + 12; color: Theme.accent }
                Rectangle { y: 6; x: -nowLabel.implicitWidth / 2 - 3; width: nowLabel.implicitWidth + 6; height: 16; color: Theme.bg0
                    Text { id: nowLabel; anchors.centerIn: parent; text: (Math.floor(backend.nowHour) < 10 ? "0" : "") + Math.floor(backend.nowHour) + ":" + (Math.floor(backend.nowHour * 60) % 60 < 10 ? "0" : "") + Math.floor(backend.nowHour * 60) % 60; color: Theme.accent; font.pixelSize: Theme.fontSize(10); font.weight: Font.Medium }
                }
            }
            Text { x: 24; y: 32; visible: timeline.blocks.length === 0; text: backend.timelineReady ? "Nothing scheduled in these hours" : backend.timelineCoverage; color: Theme.subtle; font.pixelSize: Theme.fontSize(11); width: panel.width - 48; elide: Text.ElideRight }
        }
    }
    Rectangle {
        anchors.right: parent.right; anchors.rightMargin: 20; y: 3; width: contextRow.implicitWidth + 6; height: 20; color: Theme.bg0
        Row {
            id: contextRow; anchors.right: parent.right; spacing: 3
            FmButton { objectName: "timelinePreviousDay"; iconName: "previous"; iconSize: 10; implicitWidth: 18; implicitHeight: 20; leftPadding: 4; rightPadding: 4; tip: "Previous day"; onClicked: backend.moveTimeline(-1) }
            FmButton { objectName: "timelineNextDay"; iconName: "next"; iconSize: 10; implicitWidth: 18; implicitHeight: 20; leftPadding: 4; rightPadding: 4; tip: "Next day"; onClicked: backend.moveTimeline(1) }
            Text { objectName: "timelineDateLabel"; text: Qt.formatDate(new Date(backend.timelineDate + "T12:00:00"), "ddd d MMM"); color: Theme.subtle; font.pixelSize: Theme.fontSize(10); height: 20; verticalAlignment: Text.AlignVCenter }
            FmButton { objectName: "timelineToday"; visible: backend.timelineDate !== backend.todayDate; text: "Back to today"; foreground: Theme.accent; implicitHeight: 20; font.pixelSize: Theme.fontSize(10); leftPadding: 5; rightPadding: 5; onClicked: backend.showTimelineToday() }
            FmButton { id: overflowButton; text: timeline.hiddenEvents > 0 ? "+" + timeline.hiddenEvents : ""; iconName: "more"; iconSize: 12; implicitHeight: 20; leftPadding: 5; rightPadding: 5; font.pixelSize: Theme.fontSize(10); tip: "All events · " + (backend.preferences.calendar_display_time_zone || "Time zone unavailable") + " · " + backend.timelineCoverage; onClicked: timelineMenu.open() }
        }
    }
    Popup {
        id: timelineMenu; parent: overflowButton; width: 265; x: parent.width - width; y: timeline.headerPosition ? parent.height + timeline.height : -height - 10; padding: 8; focus: true
        background: Rectangle { color: Theme.surface; radius: 6; border.color: Theme.controlBorder }
        contentItem: ColumnLayout {
            property bool blocksWorkspaceShortcuts: true
            spacing: 4
            FmButton { text: "All " + backend.todayEvents.length + " events"; Layout.fillWidth: true; textAlignment: Text.AlignLeft; onClicked: { timelineMenu.close(); dayEvents.open() } }
            FmButton { text: "Open calendar"; Layout.fillWidth: true; textAlignment: Text.AlignLeft; onClicked: { timelineMenu.close(); timeline.openCalendar() } }
            Text { text: (backend.preferences.calendar_display_time_zone || "Time zone unavailable") + "\n" + backend.timelineCoverage; color: Theme.subtle; font.pixelSize: Theme.fontSize(11); Layout.fillWidth: true; wrapMode: Text.Wrap }
        }
    }
    EventDetails { id: details; timelineSource: true; timelineInset: timeline.height; timelineAtTop: timeline.headerPosition; timelineEdge: timeline.mapToItem(Overlay.overlay, 0, timeline.headerPosition ? timeline.height : 0).y; onEditRequested: function(event) { editor.editEvent = event; editor.openForDate(event.start.slice(0, 10)) } }
    EventEditor { id: editor; timelineInset: timeline.visible ? timeline.height : 0; timelineAtTop: timeline.headerPosition }
    CalendarEventList { id: dayEvents; title: Qt.formatDate(new Date(backend.timelineDate + "T12:00:00"), "ddd d MMM") + " · all events"; eventRows: backend.todayEvents; available: backend.timelineReady && backend.connected; onEventSelected: function(event) { timeline.showEvent(event) } }
}
