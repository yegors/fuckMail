import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Effects

Popup {
    id: editor
    objectName: "eventEditor"
    parent: Overlay.overlay
    property var editEvent: ({})
    property var writableSources: backend.calendarSources.filter(function(s) { return s.writable })
    property bool saving: false
    property string feedback: ""
    property string requestId: ""
    property string submittedFields: ""
    property string targetAccount: ""
    property string targetCalendarId: ""
    property bool unknownOutcome: false
    property var submittedRequest: ({})
    property bool staleRevision: false
    property var latestEvent: ({})
    property var guestAddresses: []
    property real timelineInset: 0
    property bool timelineAtTop: false
    readonly property var preview: ({title: titleField.text || "(No title)", date: startDate.text,
        endDate: endDate.text, startHour: clockHour(startTime.text), endHour: clockHour(endTime.text),
        allDay: allDay.checked, location: locationField.text, eventId: editEvent.event_id || ""})
    width: Math.min(440, parent.width); height: Math.max(200, parent.height - 56 - timelineInset); padding: 0
    x: parent.width - width; y: 56 + (timelineAtTop ? timelineInset : 0); modal: false; dim: false; focus: true
    closePolicy: saving ? Popup.NoAutoClose : Popup.CloseOnEscape
    background: Rectangle { color: Theme.surface; layer.enabled: true; layer.effect: MultiEffect { shadowEnabled: true; shadowColor: Theme.shadowAlpha(.45); shadowBlur: .8; shadowHorizontalOffset: -12 } Rectangle { width: 1; height: parent.height; color: Theme.controlBorder } }
    enter: Transition { NumberAnimation { property: "opacity"; from: 0; to: 1; duration: 160 } }
    function clockHour(text) { let parts = text.split(":"); return parts.length === 2 ? Number(parts[0]) + Number(parts[1]) / 60 : NaN }
    function allGuests() {
        let seen = {}
        return guestAddresses.concat(attendeeField.text.split(",")).map(function(a) { return a.trim() }).filter(function(a) { if (!a.length || seen[a.toLowerCase()]) return false; seen[a.toLowerCase()] = true; return true })
    }
    function commitGuests() { guestAddresses = allGuests(); attendeeField.text = "" }
    function setSlot(date, hour) {
        let duration = Math.max(.25, clockHour(endTime.text) - clockHour(startTime.text))
        if (!isFinite(duration)) duration = (backend.preferences.default_event_duration_minutes || 60) / 60
        startDate.text = date; endDate.text = date; allDay.checked = false
        function hm(value) { let minutes = Math.round(Math.min(23.9833, value) * 60); return (Math.floor(minutes / 60) < 10 ? "0" : "") + Math.floor(minutes / 60) + ":" + (minutes % 60 < 10 ? "0" : "") + minutes % 60 }
        startTime.text = hm(hour); endTime.text = hm(hour + duration); titleField.forceActiveFocus()
    }
    Shortcut { sequence: "Ctrl+Return"; enabled: editor.opened && editor.activeFocus && !editor.saving && !editor.staleRevision && backend.connected; onActivated: editor.save() }
    Shortcut { sequence: "Meta+Return"; enabled: editor.opened && editor.activeFocus && !editor.saving && !editor.staleRevision && backend.connected; onActivated: editor.save() }
    onOpened: {
        titleField.forceActiveFocus()
        formScroll.contentItem.contentY = 0
        Qt.callLater(function() { if (editor.opened) formScroll.contentItem.contentY = 0 })
    }
    function openForDate(date) {
        titleField.text = editEvent.title || ""
        allDay.checked = !!editEvent.all_day
        startDate.text = editEvent.start ? editEvent.start.slice(0, 10) : date
        endDate.text = editEvent.end ? editEvent.end.slice(0, 10) : date
        startTime.text = editEvent.start && !editEvent.all_day ? editEvent.start.slice(11, 16) : "09:00"
        const duration = backend.preferences.default_event_duration_minutes || 60
        endTime.text = editEvent.end && !editEvent.all_day ? editEvent.end.slice(11, 16) : (duration === 60 ? "10:00" : "09:" + duration)
        zoneField.text = editEvent.time_zone || backend.preferences.calendar_display_time_zone || ""
        locationField.text = editEvent.location || ""
        conferenceField.text = editEvent.conference_url || ""
        notesField.text = editEvent.description || ""
        guestAddresses = (editEvent.attendees || []).map(function(a) { return a.email }); attendeeField.text = ""
        targetCalendar.currentIndex = Math.max(0, writableSources.findIndex(function(s) { return s.account === editor.editEvent.account && s.calendar_id === editor.editEvent.calendar_id }))
        let source = writableSources[targetCalendar.currentIndex]
        targetAccount = editEvent.account || (source ? source.account : "")
        targetCalendarId = editEvent.calendar_id || (source ? source.calendar_id : "")
        saving = false; unknownOutcome = false; staleRevision = false; latestEvent = ({}); submittedRequest = ({}); feedback = ""; requestId = ""; submittedFields = ""; open()
    }
    function save() {
        if (unknownOutcome) { saving = true; feedback = "Checking the original operation…"; backend.calendarWrite(submittedRequest); return }
        let source = writableSources.find(function(s) { return s.account === editor.targetAccount && s.calendar_id === editor.targetCalendarId })
        if (!source) { feedback = "The selected calendar is no longer writable. Your event fields remain here."; return }
        let start = allDay.checked ? startDate.text : backend.calendarDateTime(startDate.text, startTime.text, zoneField.text)
        let end = allDay.checked ? endDate.text : backend.calendarDateTime(endDate.text, endTime.text, zoneField.text)
        // Keep an untouched offset-bearing endpoint exact through repeated DST hours.
        if (!allDay.checked && !editEvent.all_day && editEvent.start && zoneField.text === editEvent.time_zone) {
            if (startDate.text === editEvent.start.slice(0, 10) && startTime.text === editEvent.start.slice(11, 16)) start = editEvent.start
            if (endDate.text === editEvent.end.slice(0, 10) && endTime.text === editEvent.end.slice(11, 16)) end = editEvent.end
        }
        if (!start || !end) { feedback = "Check the dates, times and time zone. That local time may not exist during a daylight-saving change."; return }
        let values = {title: titleField.text, start: start, end: end, all_day: allDay.checked, time_zone: zoneField.text,
            location: locationField.text, description: notesField.text, conference_url: conferenceField.text,
            attendees: allGuests()}
        let serialized = JSON.stringify({account: source.account, calendar_id: source.calendar_id, fields: values})
        if (serialized !== submittedFields || !requestId.length) requestId = "gui-event-" + Date.now() + "-" + Math.floor(Math.random() * 1000000000)
        submittedFields = serialized
        let request = {account: source.account, calendar_id: source.calendar_id, kind: editEvent.event_id ? "edit" : "create", fields: values, request_id: requestId}
        if (editEvent.event_id) { request.event_id = editEvent.event_id; request.expected_etag = editEvent.etag }
        submittedRequest = request; saving = true; feedback = "Saving locally…"; backend.calendarWrite(request)
    }
    Connections {
        target: backend
        function onEventsChanged() {
            if (!editor.opened || !editor.staleRevision || !editor.editEvent.id) return
            let latest = backend.events.concat(backend.todayEvents).find(function(e) { return e.id === editor.editEvent.id && e.can_edit && e.etag !== editor.editEvent.etag })
            if (latest) editor.latestEvent = latest
        }
        function onCalendarWriteFinished(ok, result, error) {
            if (!editor.saving) return
            editor.saving = false
            if (ok) { editor.unknownOutcome = false; editor.close() }
            else {
                editor.unknownOutcome = ["invalid_request", "invalid_event", "invalid_time", "invalid_edit", "unsupported_zone", "unsupported_operation", "permission_denied", "stale_revision", "request_id_conflict", "not_found"].indexOf(result.code || "") < 0
                editor.staleRevision = result.code === "stale_revision"
                editor.feedback = error + (editor.unknownOutcome ? " The outcome is not confirmed. Retry checks the same operation; your submitted fields are locked to prevent duplicate creation." : " Your event fields are retained here.")
                if (editor.staleRevision) backend.refresh()
            }
        }
    }
    contentItem: ColumnLayout {
        property bool blocksWorkspaceShortcuts: true
        spacing: 0
        RowLayout {
            Layout.fillWidth: true; Layout.leftMargin: 20; Layout.rightMargin: 14; Layout.preferredHeight: 56
            Text { text: editor.editEvent.event_id ? "Edit event" : "New event"; color: Theme.text; font.pixelSize: Theme.fontSize(13); font.weight: Font.Medium }
            Item { Layout.fillWidth: true }
            FmButton { iconName: "close"; implicitWidth: 28; implicitHeight: 28; enabled: !editor.saving; tip: "Close — Esc"; onClicked: editor.close() }
        }
        Rectangle { Layout.fillWidth: true; implicitHeight: 1; color: Theme.line }
        ScrollView {
            id: formScroll
            Layout.fillWidth: true; Layout.fillHeight: true; clip: true; contentWidth: availableWidth; ScrollBar.horizontal.policy: ScrollBar.AlwaysOff
            ColumnLayout {
                width: formScroll.availableWidth; spacing: 0; enabled: !editor.saving && !editor.unknownOutcome
                TextField {
                    id: titleField; objectName: "eventTitle"; Layout.fillWidth: true; implicitHeight: 65
                    leftPadding: 20; rightPadding: 20; topPadding: 18; bottomPadding: 14
                    color: Theme.text; placeholderTextColor: Theme.subtle; selectByMouse: true; placeholderText: "Title"
                    font.pixelSize: Theme.fontSize(20); font.weight: Font.Medium
                    background: Rectangle { color: "transparent"; Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 1; color: Theme.line } }
                }
                RowLayout {
                    Layout.fillWidth: true; Layout.leftMargin: 20; Layout.rightMargin: 20; Layout.minimumHeight: 44; spacing: 12
                    Text { text: "Calendar"; color: Theme.subtle; font.pixelSize: Theme.fontSize(12); Layout.preferredWidth: 64 }
                    FmComboBox {
                        id: targetCalendar; objectName: "eventTargetCalendar"; Layout.fillWidth: true; implicitHeight: 30; visible: !editor.editEvent.event_id
                        model: editor.writableSources.map(function(s) { return s.account_name + " / " + s.name })
                        onActivated: { let source = editor.writableSources[currentIndex]; if (source) { editor.targetAccount = source.account; editor.targetCalendarId = source.calendar_id } }
                    }
                    Text { visible: !!editor.editEvent.event_id; text: (editor.editEvent.account_name || "") + " / " + (editor.editEvent.calendar_name || editor.editEvent.calendar_id || ""); color: Theme.text; font.pixelSize: Theme.fontSize(12); Layout.fillWidth: true; elide: Text.ElideRight; textFormat: Text.PlainText }
                }
                Rectangle { Layout.fillWidth: true; implicitHeight: 1; color: Theme.line }
                RowLayout {
                    Layout.fillWidth: true; Layout.leftMargin: 20; Layout.rightMargin: 20; Layout.topMargin: 8; Layout.bottomMargin: 10; spacing: 12
                    Text { text: "When"; color: Theme.subtle; font.pixelSize: Theme.fontSize(12); Layout.preferredWidth: 64; Layout.alignment: Qt.AlignTop; topPadding: 8 }
                    ColumnLayout {
                        Layout.fillWidth: true; spacing: 8
                        RowLayout {
                            Layout.fillWidth: true; spacing: 8
                            TextField { id: startDate; objectName: "eventStartDate"; Layout.preferredWidth: 125; implicitHeight: 30; font.pixelSize: Theme.fontSize(12); color: Theme.text; placeholderTextColor: Theme.subtle; selectByMouse: true; placeholderText: "YYYY-MM-DD"; background: Rectangle { color: Theme.canvas; radius: 5; border.color: Theme.controlBorder } }
                            Text { text: Qt.formatDate(new Date(startDate.text + "T12:00:00"), "dddd"); color: Theme.subtle; font.pixelSize: Theme.fontSize(12); Layout.fillWidth: true; elide: Text.ElideRight }
                        }
                        RowLayout {
                            visible: !allDay.checked; spacing: 8
                            TextField { id: startTime; objectName: "eventStartTime"; Layout.preferredWidth: 75; implicitHeight: 30; font.pixelSize: Theme.fontSize(12); color: Theme.text; placeholderTextColor: Theme.subtle; selectByMouse: true; placeholderText: "HH:mm"; background: Rectangle { color: Theme.canvas; radius: 5; border.color: Theme.controlBorder } }
                            Text { text: "–"; color: Theme.subtle }
                            TextField { id: endTime; objectName: "eventEndTime"; Layout.preferredWidth: 75; implicitHeight: 30; font.pixelSize: Theme.fontSize(12); color: Theme.text; placeholderTextColor: Theme.subtle; selectByMouse: true; placeholderText: "HH:mm"; background: Rectangle { color: Theme.canvas; radius: 5; border.color: Theme.controlBorder } }
                            Text { property real minutes: Math.round((editor.clockHour(endTime.text) - editor.clockHour(startTime.text)) * 60); text: minutes > 0 ? minutes + " min" : "Check times"; color: minutes > 0 ? Theme.subtle : Theme.accent; font.pixelSize: Theme.fontSize(11) }
                        }
                        RowLayout {
                            spacing: 8
                            Text { text: allDay.checked ? "Ends before" : "End date"; color: Theme.subtle; font.pixelSize: Theme.fontSize(11) }
                            TextField { id: endDate; objectName: "eventEndDate"; Layout.preferredWidth: 125; implicitHeight: 28; font.pixelSize: Theme.fontSize(12); color: Theme.text; placeholderTextColor: Theme.subtle; selectByMouse: true; placeholderText: "YYYY-MM-DD"; background: Rectangle { color: Theme.canvas; radius: 5; border.color: Theme.controlBorder } }
                        }
                        CheckBox {
                            id: allDay; objectName: "eventAllDay"; text: "All day"; font.pixelSize: Theme.fontSize(12); palette.windowText: Theme.text
                            onToggled: if (checked && endDate.text <= startDate.text) { let date = new Date(startDate.text + "T12:00:00"); date.setDate(date.getDate() + 1); endDate.text = Qt.formatDate(date, "yyyy-MM-dd") }
                        }
                        TextField { id: zoneField; objectName: "eventTimeZone"; visible: !allDay.checked; Layout.fillWidth: true; implicitHeight: 27; font.pixelSize: Theme.fontSize(11); color: Theme.subtle; placeholderTextColor: Theme.subtle; selectByMouse: true; placeholderText: "Time zone"; background: null; leftPadding: 0 }
                        Text { visible: allDay.checked; text: "The end date is exclusive."; color: Theme.subtle; font.pixelSize: Theme.fontSize(11); Layout.fillWidth: true; wrapMode: Text.Wrap }
                    }
                }
                Rectangle { Layout.fillWidth: true; implicitHeight: 1; color: Theme.line }
                RowLayout {
                    Layout.fillWidth: true; Layout.leftMargin: 20; Layout.rightMargin: 20; Layout.minimumHeight: 44; spacing: 12
                    Text { text: "Where"; color: Theme.subtle; font.pixelSize: Theme.fontSize(12); Layout.preferredWidth: 64 }
                    TextField { id: locationField; objectName: "eventLocation"; Layout.fillWidth: true; color: Theme.text; placeholderTextColor: Theme.subtle; selectByMouse: true; placeholderText: "Add a location"; font.pixelSize: Theme.fontSize(13); background: null; leftPadding: 0 }
                }
                Rectangle { Layout.fillWidth: true; implicitHeight: 1; color: Theme.line }
                RowLayout {
                    Layout.fillWidth: true; Layout.leftMargin: 20; Layout.rightMargin: 20; Layout.minimumHeight: 44; spacing: 12
                    Text { text: "Call"; color: Theme.subtle; font.pixelSize: Theme.fontSize(12); Layout.preferredWidth: 64 }
                    TextField { id: conferenceField; objectName: "eventConference"; Layout.fillWidth: true; color: Theme.text; placeholderTextColor: Theme.subtle; selectByMouse: true; placeholderText: "Paste a meeting link"; font.pixelSize: Theme.fontSize(13); background: null; leftPadding: 0 }
                }
                Rectangle { Layout.fillWidth: true; implicitHeight: 1; color: Theme.line }
                RowLayout {
                    Layout.fillWidth: true; Layout.leftMargin: 20; Layout.rightMargin: 20; Layout.minimumHeight: 44; spacing: 12
                    Text { text: "Guests"; color: Theme.subtle; font.pixelSize: Theme.fontSize(12); Layout.preferredWidth: 64; Layout.alignment: Qt.AlignTop; topPadding: 13 }
                    ColumnLayout {
                        Layout.fillWidth: true; Layout.topMargin: 8; Layout.bottomMargin: 8; spacing: 4
                        Flow {
                            visible: editor.guestAddresses.length > 0; Layout.fillWidth: true; spacing: 4
                            Repeater { model: editor.guestAddresses
                                Rectangle {
                                    required property string modelData; required property int index
                                    width: Math.min(240, guestName.implicitWidth + 30); height: 24; radius: 12; color: "transparent"; border.color: Theme.controlBorder
                                    Text { id: guestName; x: 9; anchors.verticalCenter: parent.verticalCenter; width: parent.width - 30; text: parent.modelData; color: Theme.text; font.pixelSize: Theme.fontSize(12); elide: Text.ElideRight; textFormat: Text.PlainText }
                                    FmButton { anchors.right: parent.right; width: 22; height: 24; iconName: "close"; iconSize: 9; leftPadding: 4; rightPadding: 4; tip: "Remove " + parent.modelData; onClicked: { let index = parent.index; editor.guestAddresses = editor.guestAddresses.filter(function(a, i) { return i !== index }) } }
                                }
                            }
                        }
                        TextField { id: attendeeField; objectName: "eventAttendees"; Layout.fillWidth: true; implicitHeight: 26; color: Theme.text; placeholderTextColor: Theme.subtle; selectByMouse: true; placeholderText: "Add email addresses"; font.pixelSize: Theme.fontSize(13); background: null; leftPadding: 0; onAccepted: editor.commitGuests(); onActiveFocusChanged: if (!activeFocus && text.trim().length) editor.commitGuests() }
                    }
                }
                Rectangle { Layout.fillWidth: true; implicitHeight: 1; color: Theme.line }
                TextArea {
                    id: notesField; objectName: "eventDescription"; Layout.fillWidth: true; Layout.preferredHeight: Math.max(88, implicitHeight)
                    color: Theme.text; placeholderTextColor: Theme.subtle; placeholderText: "Notes, agenda, dial-in"; wrapMode: TextEdit.Wrap; selectByMouse: true
                    textFormat: TextEdit.PlainText; font.pixelSize: Theme.fontSize(13); leftPadding: 20; rightPadding: 20; topPadding: 12; bottomPadding: 12; background: null
                }
                Rectangle { Layout.fillWidth: true; implicitHeight: 1; color: Theme.line }
            }
        }
        Text { visible: editor.feedback.length > 0; text: editor.feedback; color: Theme.accent; font.pixelSize: Theme.fontSize(12); wrapMode: Text.Wrap; Layout.fillWidth: true; Layout.margins: 12; textFormat: Text.PlainText }
        RowLayout {
            visible: editor.staleRevision && !!editor.latestEvent.etag; Layout.fillWidth: true; Layout.leftMargin: 20; Layout.rightMargin: 20; Layout.bottomMargin: 10
            Text { text: "Current: " + (editor.latestEvent.title || ""); color: Theme.muted; font.pixelSize: Theme.fontSize(11); wrapMode: Text.Wrap; Layout.fillWidth: true; textFormat: Text.PlainText }
            FmButton { objectName: "eventUseCurrentVersion"; text: "Use current version"; outlined: true; font.pixelSize: Theme.fontSize(11); tip: "Keep your fields and update the event version"; onClicked: { let event = Object.assign({}, editor.editEvent); event.etag = editor.latestEvent.etag; editor.editEvent = event; editor.staleRevision = false; editor.requestId = ""; editor.feedback = "Current version loaded. Review your fields, then save." } }
        }
        Rectangle { Layout.fillWidth: true; implicitHeight: 1; color: Theme.line }
        RowLayout {
            Layout.fillWidth: true; Layout.leftMargin: 20; Layout.rightMargin: 20; Layout.topMargin: 12; spacing: 6
            FmButton { objectName: "eventSaveButton"; text: editor.saving ? "Saving…" : editor.unknownOutcome ? "Retry original" : editor.editEvent.event_id ? "Save changes" : "Add event"; prominent: true; font.pixelSize: Theme.fontSize(12); enabled: !editor.saving && backend.connected && !editor.staleRevision; tip: "Save locally — Ctrl+Enter"; onClicked: editor.save() }
            FmButton { objectName: "eventCancelButton"; text: editor.unknownOutcome ? "Close" : "Discard"; outlined: true; font.pixelSize: Theme.fontSize(12); enabled: !editor.saving; tip: editor.unknownOutcome ? "The operation remains in calendar activity" : "Discard unsaved event fields"; onClicked: editor.close() }
            Item { Layout.fillWidth: true }
        }
        Text { text: "Saved on this machine · no invitations sent"; color: Theme.subtle; font.pixelSize: Theme.fontSize(10); Layout.leftMargin: 20; Layout.topMargin: 7; Layout.bottomMargin: 10 }
    }
}
