import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

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
    width: Math.min(550, parent.width - 36); height: Math.min(700, parent.height - 36); padding: 24
    anchors.centerIn: parent; modal: true; focus: true
    closePolicy: saving ? Popup.NoAutoClose : Popup.CloseOnEscape
    background: Rectangle { color: Theme.surface; radius: 8; border.color: Theme.controlBorder }
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
        endTime.text = editEvent.end && !editEvent.all_day ? editEvent.end.slice(11, 16) : "10:00"
        zoneField.text = editEvent.time_zone || "America/Toronto"
        locationField.text = editEvent.location || ""
        conferenceField.text = editEvent.conference_url || ""
        notesField.text = editEvent.description || ""
        attendeeField.text = (editEvent.attendees || []).map(function(a) { return a.email }).join(", ")
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
            attendees: attendeeField.text.split(",").map(function(a) { return a.trim() }).filter(function(a) { return a.length })}
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
        spacing: 16
        RowLayout {
            Layout.fillWidth: true
            Text { text: editor.editEvent.event_id ? "Edit event" : "New event"; color: Theme.text; font.pixelSize: 21; font.weight: Font.Medium }
            Item { Layout.fillWidth: true }
            FmButton { iconName: "close"; implicitWidth: 30; enabled: !editor.saving; tip: editor.unknownOutcome ? "Close; the recorded operation remains in Local activity" : "Cancel event changes"; onClicked: editor.close() }
        }
        ScrollView {
            id: formScroll
            Layout.fillWidth: true; Layout.fillHeight: true; clip: true; contentWidth: availableWidth; ScrollBar.horizontal.policy: ScrollBar.AlwaysOff
            ColumnLayout {
                width: formScroll.availableWidth; spacing: 10; enabled: !editor.saving && !editor.unknownOutcome
                Text { text: "Title"; color: Theme.muted; font.pixelSize: 11 }
                TextField { id: titleField; objectName: "eventTitle"; Layout.fillWidth: true; color: Theme.text; placeholderTextColor: Theme.subtle; selectByMouse: true; placeholderText: "Event title"; background: Rectangle { color: Theme.canvas; radius: 5; border.color: titleField.activeFocus ? Theme.accent : Theme.line } }
                Text { text: "Calendar"; color: Theme.muted; font.pixelSize: 11 }
                FmComboBox {
                    id: targetCalendar; objectName: "eventTargetCalendar"; Layout.fillWidth: true; visible: !editor.editEvent.event_id; model: editor.writableSources.map(function(s) { return s.account_name + " / " + s.name })
                    onActivated: { let source = editor.writableSources[currentIndex]; if (source) { editor.targetAccount = source.account; editor.targetCalendarId = source.calendar_id } }
                    contentItem: Text { text: targetCalendar.displayText; color: Theme.text; verticalAlignment: Text.AlignVCenter; elide: Text.ElideRight; leftPadding: 10 }
                    background: Rectangle { color: Theme.canvas; radius: 5; border.color: Theme.line }
                }
                Text { visible: !!editor.editEvent.event_id; text: (editor.editEvent.account_name || "") + " / " + (editor.editEvent.calendar_name || editor.editEvent.calendar_id || ""); color: Theme.text; font.pixelSize: 12 }
                CheckBox { id: allDay; objectName: "eventAllDay"; text: "All day"; palette.windowText: Theme.text
                    onToggled: if (checked && endDate.text <= startDate.text) { let date = new Date(startDate.text + "T12:00:00"); date.setDate(date.getDate() + 1); endDate.text = Qt.formatDate(date, "yyyy-MM-dd") }
                }
                GridLayout {
                    Layout.fillWidth: true; columns: 3; columnSpacing: 10
                    Text { text: "Starts"; color: Theme.muted; font.pixelSize: 11 }
                    TextField { id: startDate; objectName: "eventStartDate"; Layout.fillWidth: true; color: Theme.text; placeholderTextColor: Theme.subtle; selectByMouse: true; placeholderText: "YYYY-MM-DD"; background: Rectangle { color: Theme.canvas; radius: 5; border.color: Theme.line } }
                    TextField { id: startTime; objectName: "eventStartTime"; visible: !allDay.checked; Layout.preferredWidth: 85; color: Theme.text; placeholderTextColor: Theme.subtle; selectByMouse: true; placeholderText: "HH:mm"; background: Rectangle { color: Theme.canvas; radius: 5; border.color: Theme.line } }
                    Item { visible: allDay.checked }
                    Text { text: allDay.checked ? "Ends before" : "Ends"; color: Theme.muted; font.pixelSize: 11 }
                    TextField { id: endDate; objectName: "eventEndDate"; Layout.fillWidth: true; color: Theme.text; placeholderTextColor: Theme.subtle; selectByMouse: true; placeholderText: "YYYY-MM-DD"; background: Rectangle { color: Theme.canvas; radius: 5; border.color: Theme.line } }
                    TextField { id: endTime; objectName: "eventEndTime"; visible: !allDay.checked; Layout.preferredWidth: 85; color: Theme.text; placeholderTextColor: Theme.subtle; selectByMouse: true; placeholderText: "HH:mm"; background: Rectangle { color: Theme.canvas; radius: 5; border.color: Theme.line } }
                    Item { visible: allDay.checked }
                }
                Text { visible: allDay.checked; text: "The end date is exclusive. A one-day event ends before the following day."; color: Theme.subtle; font.pixelSize: 11; Layout.fillWidth: true; wrapMode: Text.Wrap }
                Text { visible: !allDay.checked; text: "Time zone"; color: Theme.muted; font.pixelSize: 11 }
                TextField { id: zoneField; objectName: "eventTimeZone"; visible: !allDay.checked; Layout.fillWidth: true; color: Theme.text; placeholderTextColor: Theme.subtle; selectByMouse: true; background: Rectangle { color: Theme.canvas; radius: 5; border.color: Theme.line } }
                Text { text: "Location"; color: Theme.muted; font.pixelSize: 11 }
                TextField { id: locationField; objectName: "eventLocation"; Layout.fillWidth: true; color: Theme.text; placeholderTextColor: Theme.subtle; selectByMouse: true; placeholderText: "Room or physical address"; background: Rectangle { color: Theme.canvas; radius: 5; border.color: Theme.line } }
                Text { text: "Meeting link"; color: Theme.muted; font.pixelSize: 11 }
                TextField { id: conferenceField; objectName: "eventConference"; Layout.fillWidth: true; color: Theme.text; placeholderTextColor: Theme.subtle; selectByMouse: true; placeholderText: "https://"; background: Rectangle { color: Theme.canvas; radius: 5; border.color: Theme.line } }
                Text { text: "Attendees"; color: Theme.muted; font.pixelSize: 11 }
                TextField { id: attendeeField; objectName: "eventAttendees"; Layout.fillWidth: true; color: Theme.text; placeholderTextColor: Theme.subtle; selectByMouse: true; placeholderText: "Email addresses, separated by commas"; background: Rectangle { color: Theme.canvas; radius: 5; border.color: Theme.line } }
                Text { text: "Notes"; color: Theme.muted; font.pixelSize: 11 }
                TextArea { id: notesField; objectName: "eventDescription"; Layout.fillWidth: true; Layout.preferredHeight: 100; color: Theme.text; placeholderTextColor: Theme.subtle; wrapMode: TextEdit.Wrap; selectByMouse: true; textFormat: TextEdit.PlainText; background: Rectangle { color: Theme.canvas; radius: 5; border.color: Theme.line } }
            }
        }
        Text { visible: editor.feedback.length > 0; text: editor.feedback; color: Theme.accent; font.pixelSize: 12; wrapMode: Text.Wrap; Layout.fillWidth: true; textFormat: Text.PlainText }
        RowLayout {
            visible: editor.staleRevision && !!editor.latestEvent.etag; Layout.fillWidth: true
            Text { text: "Current event: " + (editor.latestEvent.title || "") + " · " + (editor.latestEvent.start_display || ""); color: Theme.muted; font.pixelSize: 11; wrapMode: Text.Wrap; Layout.fillWidth: true; textFormat: Text.PlainText }
            FmButton { objectName: "eventUseCurrentVersion"; text: "Use current version"; outlined: true; tip: "Keep your entered fields and update the event version before saving"; onClicked: { let event = Object.assign({}, editor.editEvent); event.etag = editor.latestEvent.etag; editor.editEvent = event; editor.staleRevision = false; editor.requestId = ""; editor.feedback = "Current version loaded. Review your fields, then save." } }
        }
        RowLayout {
            Layout.fillWidth: true
            Text { text: "Local fixture · no invitations sent"; color: Theme.subtle; font.pixelSize: 11; Layout.fillWidth: true }
            FmButton { objectName: "eventCancelButton"; text: editor.unknownOutcome ? "Close" : "Cancel"; enabled: !editor.saving; tip: editor.unknownOutcome ? "The original operation remains in Local activity" : "Discard unsaved event fields"; onClicked: editor.close() }
            FmButton { objectName: "eventSaveButton"; text: editor.saving ? "Saving…" : editor.unknownOutcome ? "Retry original" : "Save event"; prominent: true; enabled: !editor.saving && backend.connected && !editor.staleRevision; onClicked: editor.save() }
        }
    }
}
