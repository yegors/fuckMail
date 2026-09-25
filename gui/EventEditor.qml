import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Effects
import QtQuick.Shapes

Popup {
    id: editor
    objectName: "eventEditor"
    parent: Overlay.overlay
    property var editEvent: ({})
    readonly property bool live: !backend.syntheticProfile
    property var writableSources: backend.calendarSources.filter(function(s) { return s.writable && (!editor.live || s.can_create === true) })
    property bool saving: false
    property string feedback: ""
    property string requestId: ""
    property string submittedFields: ""
    property string targetAccount: ""
    property string targetCalendarId: ""
    function refreshTargetCalendar() {
        if (!visible || editEvent.event_id || saving || unknownOutcome) return
        // A first sync can discover calendars after the user starts typing.
        // Once chosen, retain the exact target even if it becomes unavailable.
        if (!targetAccount && !targetCalendarId && writableSources.length) {
            targetAccount = writableSources[0].account
            targetCalendarId = writableSources[0].calendar_id
        }
        targetCalendar.currentIndex = writableSources.findIndex(function(s) {
            return s.account === editor.targetAccount && s.calendar_id === editor.targetCalendarId
        })
    }
    onWritableSourcesChanged: Qt.callLater(refreshTargetCalendar)
    property bool unknownOutcome: false
    property string operationState: ""
    property double operationRevision: -1
    property bool operationCanResume: false
    property bool recurrenceTouched: false
    readonly property int repeatRuleIndex: repeatFrequency.currentIndex - (editEvent.event_id ? 1 : 0)
    property var repeatDays: []
    readonly property var weekDays: ["SU", "MO", "TU", "WE", "TH", "FR", "SA"]
    readonly property var weekDayNames: ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"]
    readonly property var dayOrder: backend.preferences.week_start === "sunday" ? [0,1,2,3,4,5,6] : [1,2,3,4,5,6,0]
    function shiftDate(value, amount) {
        const date = startDate.parseDate(value)
        if (!date) return ""
        date.setDate(date.getDate() + amount)
        return Qt.formatDate(date, "yyyy-MM-dd")
    }
    function recurrenceRule() {
        const kind = repeatRuleIndex
        if (kind <= 0) return []
        const interval = Number(repeatInterval.text), count = repeatCount.text.trim().length ? Number(repeatCount.text) : 0
        if (!Number.isInteger(interval) || interval < 1 || interval > 99 || (repeatCount.text.trim().length && (!Number.isInteger(count) || count < 1 || count > 1000))) {
            feedback = "Use an interval from 1 to 99 and a count from 1 to 1000."; return null
        }
        const day = new Date(startDate.text + "T12:00:00").getDay()
        let rule = "RRULE:FREQ=" + (kind === 1 ? "DAILY" : kind === 4 ? "MONTHLY" : "WEEKLY")
        if (interval !== 1) rule += ";INTERVAL=" + interval
        if (kind === 2) rule += ";BYDAY=MO,TU,WE,TH,FR"
        else if (kind === 3 || kind === 5) {
            const days = kind === 3 ? [day] : repeatDays
            if (!days.length) { feedback = "Choose at least one repeat day."; return null }
            rule += ";BYDAY=" + dayOrder.filter(function(d) { return days.indexOf(d) >= 0 }).map(function(d) { return editor.weekDays[d] }).join(",")
        } else if (kind === 4) rule += ";BYMONTHDAY=" + Number(startDate.text.slice(8, 10))
        if (repeatEnd.currentIndex === 1) {
            if (!repeatUntil.validDate || repeatUntil.text < startDate.text) { feedback = "The repeat end date must be on or after the start."; return null }
            if (allDay.checked) rule += ";UNTIL=" + repeatUntil.text.replace(/-/g, "")
            else {
                const last = backend.calendarDateTime(repeatUntil.text, "23:59", zoneField.text)
                if (!last || isNaN(new Date(last).getTime())) { feedback = "Check the repeat end date and time zone."; return null }
                rule += ";UNTIL=" + new Date(new Date(last).getTime() + 59000).toISOString().slice(0,19).replace(/[-:]/g, "") + "Z"
            }
        } else if (count) rule += ";COUNT=" + count
        return [rule]
    }
    property var initialTextFields: ({})
    // Notes start 176px tall; the corner handle makes them taller.
    property real notesHeight: 176
    property string conferenceChange: "keep"
    property var initialReminder: undefined
    property string initialAvailability: "busy"
    readonly property var reminderChoices: [{text:"Keep existing",value:-2},{text:"Use default",value:null},{text:"At the start",value:0},{text:"5 min before",value:5},{text:"10 min before",value:10},{text:"15 min before",value:15},{text:"30 min before",value:30},{text:"1 h before",value:60},{text:"1 day before",value:1440}].concat(initialReminder !== undefined && initialReminder !== null && [0,5,10,15,30,60,1440].indexOf(initialReminder) < 0 ? [{text:initialReminder + " min before",value:initialReminder}] : [])
    readonly property bool conferencePresent: conferenceChange === "google_meet" || (conferenceChange === "keep" && (editEvent.has_conference || !!editEvent.conference_url))
    property var initialTimeFields: ({})
    property var submittedRequest: ({})
    property bool staleRevision: false
    property var latestEvent: ({})
    property bool timelineSource: false
    readonly property string recurringScope: (editEvent.recurring_context || {}).scope || ""
    readonly property bool occurrence: !!editEvent.occurrence || recurringScope.length > 0
    readonly property bool following: recurringScope === "following"
    function scopeLabel() { return {occurrence:"This occurrence",series:"Entire series",following:"This and following"}[recurringScope] || "This occurrence" }
    function recoveryAction() { return ["prepared", "ready"].indexOf(operationState) >= 0 || (operationState === "partial" && operationCanResume) ? "resume" : operationState === "unknown" ? "reconcile" : "check" }
    function followingText() { return editEvent.following_notice || "Google resets exceptions from this occurrence onward. Earlier exceptions stay unchanged. If this event uses Google Meet, the replacement series gets a new meeting." }
    function stepSummary(steps) {
        return (steps || []).map(function(step) {
            return (step.step === "trim" ? "Old series trim: " : "Replacement: ") + ({acknowledged:"accepted by Google",accepted:"accepted by Google, refreshing",reconciled:"observed; response unconfirmed",not_attempted:"not attempted",unknown:"result unconfirmed",in_flight:"in progress",prepared:"not sent",ready:"not sent",rejected:"not accepted",abandoned:"abandoned"}[step.state] || "needs attention")
        }).join(". ")
    }
    function recurringRequest(confirmed) { return Object.assign({}, editEvent.recurring_context, following && confirmed ? {reset_future_exceptions:true} : {}) }
    readonly property bool occurrenceReady: !occurrence || occurrenceRead.current(editEvent.occurrence_ui_context)
    CalendarOccurrenceRead {
        id: occurrenceRead; selection: editor.editEvent; active: editor.visible; timelineSource: editor.timelineSource
        scope: editor.recurringScope; action: editor.editEvent.recurring_action || "edit"
        onResolved: function(event) {
            editor.editEvent = event; editor.staleRevision = false; editor.requestId = ""; editor.submittedFields = ""
            editor.feedback = "Current occurrence checked. Your fields are retained; review them before saving."
        }
        onFailed: function(message) { editor.feedback = message + " Your event fields remain here." }
    }
    property var guestAddresses: []
    property var initialGuestAddresses: []
    readonly property bool guestsEditable: !live || (!editEvent.event_id ? formReady : editEvent.can_edit_attendees === true)
    readonly property var protectedGuests: live ? (editEvent.protected_attendees || []) : []
    property real timelineInset: 0
    property bool timelineAtTop: false
    readonly property bool accountReady: backend.connected && (!live || (backend.accounts.length >= 0 && backend.googleSync
        && backend.accountCapability(targetAccount, "calendar_write") && backend.googleSyncState(targetAccount).active === true))
    readonly property bool sourceReady: backend.calendarSources.some(function(s) { return s.account === editor.targetAccount && s.calendar_id === editor.targetCalendarId && s.writable && (!editor.live || !!editor.editEvent.event_id || s.can_create === true) })
    readonly property bool formReady: accountReady && sourceReady && occurrenceReady && !occurrenceRead.busy && (!live || !editEvent.event_id || editEvent.can_edit === true)
    readonly property bool reconciliationReady: backend.connected && backend.accounts.length >= 0 && backend.googleSync
        && backend.accountCapability(targetAccount, "calendar_read") && backend.googleSyncState(targetAccount).active === true
    readonly property bool actionReady: backend.connected && (!unknownOutcome || !live || recoveryAction() === "check" || (recoveryAction() === "reconcile" ? reconciliationReady : accountReady))
    readonly property bool canAbandon: live && unknownOutcome && operationRevision >= 0
        && ["prepared", "ready", "unknown", "conflict", "rejected", "partial"].indexOf(operationState) >= 0
    readonly property var preview: ({title: titleField.text || "(No title)", date: startDate.text,
        endDate: endDate.text, startHour: clockHour(startTime.text), endHour: clockHour(endTime.text),
        allDay: allDay.checked, location: locationField.text, eventId: editEvent.event_id || "",
        account: targetAccount, calendar_id: targetCalendarId, id: "editor-preview", all_day: allDay.checked,
        start: allDay.checked ? startDate.text : backend.calendarDateTime(startDate.text, startTime.text, zoneField.text),
        end: allDay.checked ? shiftDate(endDate.text, 1) : backend.calendarDateTime(endDate.text, endTime.text, zoneField.text)})
    width: Math.min(440, parent.width); height: Math.max(200, parent.height - 56 - timelineInset); padding: 0
    // The panel slides in by position; the overlay never scrolls, so focusing
    // the title while it moves cannot shift the rest of the window.
    property real slideOffset: 0
    x: parent.width - width + slideOffset; y: 56 + (timelineAtTop ? timelineInset : 0); modal: false; dim: false; focus: true
    closePolicy: saving ? Popup.NoAutoClose : Popup.CloseOnEscape
    background: Rectangle { color: Theme.surface; layer.enabled: GraphicsInfo.api !== GraphicsInfo.Software; layer.effect: MultiEffect { shadowEnabled: true; shadowColor: Theme.shadowAlpha(.45); shadowBlur: .8; shadowHorizontalOffset: -12 } Rectangle { width: 1; height: parent.height; color: Theme.controlBorder } }
    enter: Transition {
        NumberAnimation { property: "opacity"; from: 0; to: 1; duration: 200; easing.type: Easing.OutCubic }
        NumberAnimation { property: "slideOffset"; from: 24; to: 0; duration: 200; easing.type: Easing.OutCubic }
    }
    function clockHour(text) { let parts = text.split(":"); return parts.length === 2 ? Number(parts[0]) + Number(parts[1]) / 60 : NaN }
    function allGuests() {
        const addresses = guestAddresses.concat(attendeeField.text.split(",")).map(function(a) { return a.trim() }).filter(function(a) { return a.length > 0 })
        // Live validation belongs to the shared boundary. Preserve duplicate or
        // malformed authored input so a refused edit can be corrected in place.
        if (live) return addresses
        let seen = {}
        return addresses.filter(function(a) { if (seen[a.toLowerCase()]) return false; seen[a.toLowerCase()] = true; return true })
    }
    function commitGuests() { if (guestsEditable && !saving && !unknownOutcome) { guestAddresses = allGuests(); attendeeField.text = "" } }
    function guestSet(addresses) { return JSON.stringify(addresses.map(function(a) { return a.trim().toLowerCase() }).sort()) }
    function guestEditExplanation() {
        const reason = editEvent.attendee_edit_reason || ""
        if (reason === "incomplete_attendees") return "Google has not supplied a complete guest list. Other event fields can still be edited."
        if (reason === "too_many_attendees") return "This guest list exceeds the supported limit of 50. Other event fields can still be edited."
        return "This guest list contains details that cannot be safely edited here. Other event fields can still be edited."
    }
    function setSlot(date, hour) { setSlotRange(date, hour, undefined) }
    function setSlotRange(date, hour, endHour) {
        if (saving || unknownOutcome) return
        let duration = Math.max(.25, clockHour(endTime.text) - clockHour(startTime.text))
        if (!isFinite(duration)) duration = (backend.preferences.default_event_duration_minutes || 60) / 60
        startDate.text = date; endDate.text = date; allDay.checked = false; sameDayEnd = true
        function hm(value) { let minutes = Math.round(Math.min(23.9833, value) * 60); return (Math.floor(minutes / 60) < 10 ? "0" : "") + Math.floor(minutes / 60) + ":" + (minutes % 60 < 10 ? "0" : "") + minutes % 60 }
        const end = endHour === undefined ? Math.min(23.9833, hour + duration) : endHour
        startTime.text = hm(hour)
        if (end >= 24) { const next = new Date(date + "T12:00:00"); next.setDate(next.getDate() + 1); endDate.text = Qt.formatDate(next, "yyyy-MM-dd"); endTime.text = "00:00" }
        else endTime.text = hm(end)
        titleField.forceActiveFocus()
    }
    Shortcut { sequence: "Ctrl+Return"; enabled: editor.opened && editor.activeFocus && !editor.saving && editor.actionReady && (editor.unknownOutcome || (!editor.staleRevision && editor.formReady)); onActivated: editor.save() }
    Shortcut { sequence: "Meta+Return"; enabled: editor.opened && editor.activeFocus && !editor.saving && editor.actionReady && (editor.unknownOutcome || (!editor.staleRevision && editor.formReady)); onActivated: editor.save() }
    onOpened: {
        refreshTargetCalendar()
        titleField.forceActiveFocus()
        formScroll.contentItem.contentY = 0
        Qt.callLater(function() { if (editor.opened) { formScroll.contentItem.contentY = 0; if (editor.editEvent.recurring_action === "delete") deleteConfirmation.open() } })
    }
    onAboutToHide: { occurrenceRead.invalidate(); followingConfirmation.close(); deleteConfirmation.close() }
    function openForDate(date) {
        occurrenceRead.invalidate()
        titleField.text = editEvent.title || ""
        allDay.checked = !!editEvent.all_day
        startDate.text = editEvent.start ? editEvent.start.slice(0, 10) : date
        endDate.text = editEvent.end ? (editEvent.all_day ? shiftDate(editEvent.end.slice(0, 10), -1) : editEvent.end.slice(0, 10)) : date
        startTime.text = editEvent.start && !editEvent.all_day ? editEvent.start.slice(11, 16) : "09:00"
        const duration = backend.preferences.default_event_duration_minutes || 60
        endTime.text = editEvent.end && !editEvent.all_day ? editEvent.end.slice(11, 16) : (duration === 60 ? "10:00" : "09:" + duration)
        sameDayEnd = endDate.text === startDate.text
        zoneField.text = editEvent.time_zone || backend.preferences.calendar_display_time_zone || ""
        locationField.text = editEvent.location || ""
        conferenceField.text = editEvent.conference_url || ""
        conferenceChange = "keep"
        initialReminder = editEvent.event_id ? editEvent.reminder_minutes : Number(backend.preferences.default_reminder_minutes || 15)
        reminderField.currentIndex = reminderChoices.findIndex(function(choice) { return choice.value === (editor.initialReminder === undefined ? -2 : editor.initialReminder) })
        initialAvailability = editEvent.availability || "busy"
        availabilityField.currentIndex = initialAvailability === "free" ? 1 : 0
        notesField.text = editEvent.description || ""
        notesHeight = 176
        guestAddresses = live ? (editEvent.guest_addresses || []).slice() : (editEvent.attendees || []).map(function(a) { return a.email })
        initialGuestAddresses = guestAddresses.slice(); attendeeField.text = ""
        targetCalendar.currentIndex = Math.max(0, writableSources.findIndex(function(s) { return s.account === editor.editEvent.account && s.calendar_id === editor.editEvent.calendar_id }))
        let source = writableSources[targetCalendar.currentIndex]
        targetAccount = editEvent.account || (source ? source.account : "")
        targetCalendarId = editEvent.calendar_id || (source ? source.calendar_id : "")
        initialTextFields = {title: titleField.text, location: locationField.text, description: notesField.text}
        initialTimeFields = timeFields()
        recurrenceTouched = false; repeatFrequency.currentIndex = 0; repeatInterval.text = "1"; repeatCount.text = ""
        repeatDays = [new Date(startDate.text + "T12:00:00").getDay()]; repeatEnd.currentIndex = 0; repeatUntil.text = shiftDate(startDate.text, 84)
        operationCanResume = false
        saving = false; unknownOutcome = false; operationState = ""; operationRevision = -1; staleRevision = false; latestEvent = ({}); submittedRequest = ({}); feedback = ""; requestId = ""; submittedFields = ""; open()
    }
    property bool sameDayEnd: true
    readonly property bool showSameDay: sameDayEnd && !allDay.checked && endDate.text === startDate.text
    function durationText() {
        const start = new Date(startDate.text + "T" + (allDay.checked ? "00:00" : startTime.text) + ":00"), end = new Date(endDate.text + "T" + (allDay.checked ? "00:00" : endTime.text) + ":00")
        if (isNaN(start.getTime()) || isNaN(end.getTime())) return ""
        const minutes = Math.round((end - start) / 60000)
        if (minutes <= 0) return "Ends before it starts"
        return minutes >= 60 ? Math.floor(minutes / 60) + " h" + (minutes % 60 ? " " + minutes % 60 + " min" : "") : minutes + " min"
    }
    function weekday(value) { const date = new Date(value + "T12:00:00"); return isNaN(date.getTime()) ? "" : Qt.formatDate(date, "dddd") }
    function timeFields() { return {all_day: allDay.checked, start_date: startDate.text, end_date: endDate.text, start_time: startTime.text, end_time: endTime.text, time_zone: zoneField.text} }
    function changedFields() {
        let fields = {}
        const editing = !!editEvent.event_id
        const text = {title: titleField.text, location: locationField.text, description: notesField.text}
        for (const name of ["title", "location", "description"])
            if (!live || !editing || text[name] !== initialTextFields[name]) fields[name] = text[name]
        if (!live || !editing || JSON.stringify(timeFields()) !== JSON.stringify(initialTimeFields)) {
            if (!startDate.validDate || !endDate.validDate || endDate.text < startDate.text) { feedback = "Choose valid start and end dates, with the end on or after the start."; return null }
            let start = allDay.checked ? startDate.text : backend.calendarDateTime(startDate.text, startTime.text, zoneField.text)
            let end = allDay.checked ? shiftDate(endDate.text, 1) : backend.calendarDateTime(endDate.text, endTime.text, zoneField.text)
            // Keep untouched offset-bearing endpoints exact through repeated DST hours.
            if (!allDay.checked && !editEvent.all_day && editEvent.start && zoneField.text === editEvent.time_zone) {
                if (startDate.text === editEvent.start.slice(0, 10) && startTime.text === editEvent.start.slice(11, 16)) start = editEvent.start
                if (endDate.text === editEvent.end.slice(0, 10) && endTime.text === editEvent.end.slice(11, 16)) end = editEvent.end
            }
            if (!start || !end) { feedback = "Check the dates, times and time zone. That local time may not exist during a daylight-saving change."; return null }
            fields.start = start; fields.end = end; fields.all_day = allDay.checked; fields.time_zone = zoneField.text
            if (live && editing && allDay.checked !== !!editEvent.all_day) fields.convert_time_kind = true
        }
        const reminder = reminderChoices[reminderField.currentIndex]
        if (reminder && reminder.value !== -2 && (!editing || reminder.value !== initialReminder)) fields.reminder_minutes = reminder.value
        const availability = availabilityField.currentIndex === 1 ? "free" : "busy"
        if (!editing || availability !== initialAvailability) fields.availability = availability
        if (live && conferenceChange !== "keep") fields.conference = conferenceChange
        if (!live) { fields.conference_url = conferenceField.text; fields.attendees = allGuests() }
        else if (guestSet(allGuests()) !== guestSet(initialGuestAddresses)) {
            if (!guestsEditable) { feedback = "Guest editing is no longer available. Your guest changes remain here; reopen the current event before continuing."; return null }
            fields.attendees = allGuests()
        }
        if (live && recurrenceTouched && (!editing || recurringScope === "series")) {
            const recurrence = recurrenceRule()
            if (recurrence === null) return null
            if (editing || recurrence.length) fields.recurrence = recurrence
        }

        return fields
    }
    function resolveOperation(action) {
        if (saving || !backend.connected || !submittedRequest.request_id) return
        saving = true; feedback = action === "resume" ? "Resuming the recorded change…" : "Checking the recorded change…"
        backend.calendarOperationAction(submittedRequest.account, submittedRequest.request_id, action, operationRevision)
    }
    function save() { saveConfirmed(false) }
    function saveConfirmed(followingConfirmed) {
        if (saving) return
        if (unknownOutcome) {
            if (live) resolveOperation(recoveryAction())
            else { saving = true; feedback = "Checking the original operation…"; backend.calendarWrite(submittedRequest) }
            return
        }
        if (following && followingConfirmed !== true) { followingConfirmation.open(); return }
        let source = backend.calendarSources.find(function(s) { return s.account === editor.targetAccount && s.calendar_id === editor.targetCalendarId && s.writable })
        if (!source || !formReady) { feedback = "Start sync for a supported writable calendar. Your event fields remain here."; return }
        let values = changedFields()
        if (!values) return
        if (!Object.keys(values).length) { feedback = "No event fields have changed."; return }
        let serialized = JSON.stringify({account: source.account, calendar_id: source.calendar_id, fields: values})
        if (serialized !== submittedFields || !requestId.length) requestId = "gui-event-" + Date.now() + "-" + Math.floor(Math.random() * 1000000000)
        submittedFields = serialized
        let request = {account: source.account, calendar_id: source.calendar_id, kind: editEvent.event_id ? "edit" : "create", fields: values, request_id: requestId}
        if (editEvent.event_id) { request.event_id = editEvent.event_id; request.expected_etag = editEvent.etag }
        if (recurringScope.length) request.recurring = recurringRequest(followingConfirmed === true)
        else if (occurrence) request.occurrence = editEvent.occurrence
        submittedRequest = request; saving = true; feedback = live ? "Saving to Google Calendar…" : "Saving locally…"; backend.calendarWrite(request)
    }
    function deleteEvent() { deleteEventConfirmed(false) }
    function deleteEventConfirmed(confirmed, capturedEvent) {
        if (capturedEvent && JSON.stringify(capturedEvent) !== JSON.stringify(editEvent)) { feedback = "The event changed. Review it before deleting."; return }
        if (saving || unknownOutcome || occurrenceRead.busy || !occurrenceReady || !live || !accountReady || !sourceReady || editEvent.can_delete !== true || (following && confirmed !== true)) return
        requestId = "gui-delete-" + Date.now() + "-" + Math.floor(Math.random() * 1000000000)
        submittedRequest = {account: editEvent.account, calendar_id: editEvent.calendar_id, event_id: editEvent.event_id,
            expected_etag: editEvent.etag, kind: "delete", request_id: requestId}
        if (recurringScope.length) submittedRequest.recurring = recurringRequest(confirmed === true)
        else if (occurrence) submittedRequest.occurrence = editEvent.occurrence
        const error = backend.queueCalendarDeletion(submittedRequest, editEvent)
        if (error.length) feedback = error; else close()
    }
    Connections {
        target: backend
        ignoreUnknownSignals: true
        function onTimelineChanged() { onEventsChanged() }
        function onEventsChanged() {
            if (editor.opened && editor.live && editor.editEvent.event_id && !editor.occurrence) {
                let current = backend.events.concat(backend.todayEvents).find(function(e) { return e.account === editor.editEvent.account && e.calendar_id === editor.editEvent.calendar_id && e.event_id === editor.editEvent.event_id })
                if (current) editor.editEvent = Object.assign({}, editor.editEvent, {can_edit: current.can_edit === true, can_delete: current.can_delete === true,
                    can_edit_attendees: current.can_edit_attendees === true, attendee_edit_reason: current.attendee_edit_reason})
            }
            if (!editor.opened || editor.occurrence || !editor.staleRevision || !editor.editEvent.id) return
            let latest = backend.events.concat(backend.todayEvents).find(function(e) { return e.id === editor.editEvent.id && e.can_edit && e.etag !== editor.editEvent.etag })
            if (latest) editor.latestEvent = latest
        }
        function onCalendarWriteFinished(ok, result, error) {
            if (!editor.saving || result.account !== editor.submittedRequest.account || result.request_id !== editor.submittedRequest.request_id) return
            editor.saving = false
            if (ok) { editor.unknownOutcome = false; editor.close() }
            else if (editor.live) {
                editor.feedback = error + (result.steps && result.steps.length ? " " + editor.stepSummary(result.steps) + "." : "") + " Your event fields are retained here."
                if (result.state === "abandoned" || (result.local_preflight === true && result.operation_action === "write") || (result.operation_action === "check" && result.code === "not_found")) {
                    editor.unknownOutcome = false; editor.operationState = ""; editor.operationRevision = -1; editor.requestId = ""
                } else {
                    editor.operationState = result.state || editor.operationState || "unconfirmed"
                    editor.operationCanResume = result.can_resume === true
                    if (result.revision !== undefined) editor.operationRevision = result.revision
                    editor.unknownOutcome = true
                }
                if (result.state === "conflict" || result.code === "stale_revision") { editor.staleRevision = true; backend.refresh() }
            }
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
            Text { text: editor.occurrence ? editor.scopeLabel() : editor.editEvent.event_id ? "Edit event" : "New event"; color: Theme.text; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(13); font.weight: Font.Medium }
            Item { Layout.fillWidth: true }
            FmButton { objectName: "eventDeleteButton"; visible: editor.live && editor.editEvent.can_delete === true && !editor.unknownOutcome; text: editor.occurrence ? "Cancel" : "Delete"; implicitHeight: 28; font.pointSize: Theme.fontPoints(12); enabled: !editor.saving && !occurrenceRead.busy && editor.occurrenceReady && editor.accountReady && editor.sourceReady; tip: editor.occurrence ? "Cancel: " + editor.scopeLabel() : "Delete this event from Google Calendar"; onClicked: deleteConfirmation.open() }
            FmButton { objectName: "eventAbandonButton"; visible: editor.canAbandon; text: "Abandon"; implicitHeight: 28; font.pointSize: Theme.fontPoints(12); enabled: !editor.saving; tip: "Abandon the recorded change and retain these form fields"; onClicked: abandonConfirmation.open() }
            FmButton { iconName: "close"; iconSize: 12; implicitWidth: 28; implicitHeight: 28; leftPadding: 0; rightPadding: 0; foreground: hovered ? Theme.text : Theme.inkAlpha(.6); enabled: !editor.saving; tip: "Close — Esc"; onClicked: editor.close() }
        }
        Rectangle { Layout.fillWidth: true; implicitHeight: 1; color: Theme.line }
        ScrollView {
                ScrollBar.vertical: FmScrollBar {}
            id: formScroll; objectName: "eventFormScroll"
            Layout.fillWidth: true; Layout.fillHeight: true; clip: true; contentWidth: availableWidth; ScrollBar.horizontal.policy: ScrollBar.AlwaysOff
            ColumnLayout {
                width: formScroll.availableWidth; spacing: 0; enabled: !editor.saving && !editor.unknownOutcome && (!editor.live || !editor.editEvent.event_id || editor.editEvent.can_edit === true)
                TextField {
                    id: titleField; objectName: "eventTitle"; Layout.fillWidth: true; implicitHeight: 65
                    leftPadding: 20; rightPadding: 20; topPadding: 18; bottomPadding: 14
                    color: Theme.text; placeholderTextColor: Theme.subtle; selectByMouse: true; placeholderText: "Title"
                    font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(20); font.weight: Font.Medium; font.letterSpacing: -0.4
                    background: Rectangle { color: "transparent"; Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 1; color: Theme.line } }
                }
                RowLayout {
                    Layout.fillWidth: true; Layout.leftMargin: 20; Layout.rightMargin: 20; Layout.minimumHeight: 44; spacing: 12
                    Text { text: "Calendar"; color: Theme.inkAlpha(0.45); font.pointSize: Theme.fontPoints(12.5); Layout.preferredWidth: 64 }
                    FmComboBox {
                        id: targetCalendar; objectName: "eventTargetCalendar"; Layout.fillWidth: true; implicitHeight: 30; fill: Theme.canvas; visible: !editor.editEvent.event_id
                        model: editor.writableSources.map(function(s) { return s.account_name + " / " + s.name })
                        displayText: currentIndex < 0 ? "Choose a writable calendar" : currentText
                        onActivated: { let source = editor.writableSources[currentIndex]; if (source) { editor.targetAccount = source.account; editor.targetCalendarId = source.calendar_id } }
                    }
                    Text { visible: !!editor.editEvent.event_id; text: (editor.editEvent.account_name || "") + " / " + (editor.editEvent.calendar_name || editor.editEvent.calendar_id || ""); color: Theme.text; font.pointSize: Theme.fontPoints(12); Layout.fillWidth: true; elide: Text.ElideRight; textFormat: Text.PlainText }
                }
                Rectangle { Layout.fillWidth: true; implicitHeight: 1; color: Theme.line }
                RowLayout {
                    Layout.fillWidth: true; Layout.leftMargin: 20; Layout.rightMargin: 20; Layout.minimumHeight: 44; spacing: 12
                    Text { text: "All day"; color: Theme.inkAlpha(0.45); font.pointSize: Theme.fontPoints(12.5); Layout.preferredWidth: 64 }
                    FmSwitch { id: allDay; objectName: "eventAllDay"; Accessible.name: "All day"; onToggled: if (endDate.text < startDate.text) endDate.text = startDate.text }
                    Text {
                        readonly property int days: Math.round((new Date(endDate.text + "T12:00:00") - new Date(startDate.text + "T12:00:00")) / 86400000) + 1
                        text: allDay.checked ? (days > 1 ? days + " days" : "Whole day") : ""
                        color: Theme.inkAlpha(0.45); font.pointSize: Theme.fontPoints(12); Layout.fillWidth: true
                    }
                }
                Rectangle { Layout.fillWidth: true; implicitHeight: 1; color: Theme.line }
                RowLayout {
                    Layout.fillWidth: true; Layout.leftMargin: 20; Layout.rightMargin: 20; Layout.minimumHeight: 44; spacing: 12
                    Text { text: "Starts"; color: Theme.inkAlpha(0.45); font.pointSize: Theme.fontPoints(12.5); Layout.preferredWidth: 64 }
                    FmDateField { weekStartsMonday: backend.preferences.week_start !== "sunday"; id: startDate; objectName: "eventStartDate"; Layout.preferredWidth: 138; Accessible.name: "Event start date"; onTextEdited: if (validDate && (endDate.text < text || (editor.sameDayEnd && !allDay.checked))) endDate.text = text }
                    FmTextField { id: startTime; objectName: "eventStartTime"; visible: !allDay.checked; Layout.preferredWidth: 80; implicitHeight: Math.max(30, contentHeight + 8); padding: 4; leftPadding: 8; rightPadding: 8; placeholderText: "HH:mm"; Accessible.name: "Event start time" }
                    Text { text: editor.weekday(startDate.text); color: Theme.inkAlpha(.45); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12); Layout.leftMargin: -6; Layout.fillWidth: true; elide: Text.ElideRight }
                }
                Rectangle { Layout.fillWidth: true; implicitHeight: 1; color: Theme.line }
                RowLayout {
                    Layout.fillWidth: true; Layout.leftMargin: 20; Layout.rightMargin: 20; Layout.minimumHeight: 44; spacing: 12
                    Text { text: "Ends"; color: Theme.inkAlpha(0.45); font.pointSize: Theme.fontPoints(12.5); Layout.preferredWidth: 64 }
                    FmDateField { weekStartsMonday: backend.preferences.week_start !== "sunday"; id: endDate; objectName: "eventEndDate"; visible: !editor.showSameDay; Layout.preferredWidth: 138; Accessible.name: "Event end date"; onTextEdited: editor.sameDayEnd = false }
                    FmButton {
                        objectName: "eventEndSameDay"; visible: editor.showSameDay; text: "Same day"; Layout.preferredWidth: 138; implicitHeight: 30; leftPadding: 9; rightPadding: 9
                        textAlignment: Text.AlignLeft; foreground: hovered ? Theme.text : Theme.inkAlpha(.45); tip: "End on another day"
                        onClicked: { editor.sameDayEnd = false; endDate.forceActiveFocus() }
                    }
                    FmTextField { id: endTime; objectName: "eventEndTime"; visible: !allDay.checked; Layout.preferredWidth: 80; implicitHeight: Math.max(30, contentHeight + 8); padding: 4; leftPadding: 8; rightPadding: 8; placeholderText: "HH:mm"; Accessible.name: "Event end time" }
                    Text {
                        readonly property string duration: editor.durationText()
                        text: allDay.checked ? editor.weekday(endDate.text) : duration; Layout.leftMargin: -6; Layout.fillWidth: true; elide: Text.ElideRight
                        color: duration === "Ends before it starts" && !allDay.checked ? Theme.error : Theme.inkAlpha(.45); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12)
                    }
                }
                FmTextField { id: zoneField; objectName: "eventTimeZone"; visible: !allDay.checked; Layout.fillWidth: true; Layout.leftMargin: 96; Layout.rightMargin: 20; implicitHeight: 27; padding: 4; font.pointSize: Theme.fontPoints(11); color: Theme.subtle; placeholderText: "Time zone"; background: null; Accessible.name: "Event time zone" }
                Rectangle { Layout.fillWidth: true; implicitHeight: 1; color: Theme.line }
                ColumnLayout {
                    id: repeatSection
                    visible: editor.live && (!editor.editEvent.event_id || editor.recurringScope === "series" || editor.following)
                    Layout.fillWidth: true; Layout.leftMargin: 20; Layout.rightMargin: 20; Layout.topMargin: 10; Layout.bottomMargin: 10; spacing: 8
                    RowLayout {
                        visible: !editor.following; Layout.fillWidth: true; spacing: 12
                        Text { text: "Repeat"; color: Theme.inkAlpha(0.45); font.pointSize: Theme.fontPoints(12.5); Layout.preferredWidth: 64 }
                        FmComboBox {
                            id: repeatFrequency; objectName: "eventRepeat"; Layout.fillWidth: true; implicitHeight: 30; fill: Theme.canvas
                            model: (editor.editEvent.event_id ? ["Keep current rule"] : []).concat(["Does not repeat", "Every day", "Every weekday", "Weekly on " + editor.weekDayNames[new Date(startDate.text + "T12:00:00").getDay()], "Monthly on day " + Number(startDate.text.slice(8,10)), "Custom…"])
                            onActivated: editor.recurrenceTouched = !editor.editEvent.event_id || currentIndex > 0
                        }
                    }
                    ColumnLayout {
                        visible: !editor.following && editor.repeatRuleIndex === 5; Layout.fillWidth: true; Layout.leftMargin: 76; spacing: 10
                        Row {
                            spacing: 5
                            Repeater {
                                model: editor.dayOrder
                                FmButton {
                                    required property int modelData
                                    objectName: "eventRepeatDay_" + editor.weekDays[modelData]
                                    implicitWidth: 30; implicitHeight: 30; leftPadding: 0; rightPadding: 0
                                    text: editor.weekDayNames[modelData].slice(0,1); tip: editor.weekDayNames[modelData]
                                    prominent: editor.repeatDays.indexOf(modelData) >= 0; outlined: !prominent
                                    Accessible.role: Accessible.CheckBox; Accessible.checked: prominent
                                    onClicked: {
                                        if (prominent && editor.repeatDays.length === 1) return
                                        editor.repeatDays = prominent ? editor.repeatDays.filter(function(d) { return d !== modelData }) : editor.repeatDays.concat([modelData])
                                        editor.recurrenceTouched = true
                                    }
                                }
                            }
                        }
                        RowLayout {
                            Layout.fillWidth: true; spacing: 8
                            Text { text: "Every"; color: Theme.subtle; font.pointSize: Theme.fontPoints(12) }
                            FmTextField { id: repeatInterval; objectName: "eventRepeatInterval"; text: "1"; Layout.preferredWidth: 48; implicitHeight: Math.max(30, contentHeight + 8); padding: 4; leftPadding: 6; rightPadding: 6; validator: IntValidator { bottom: 1; top: 99 } onTextEdited: editor.recurrenceTouched = true; Accessible.name: "Repeat interval in weeks" }
                            Text { text: "weeks"; color: Theme.subtle; font.pointSize: Theme.fontPoints(12) }
                            Item { Layout.fillWidth: true }
                        }
                    }
                    RowLayout {
                        visible: !editor.following && editor.repeatRuleIndex > 0; Layout.fillWidth: true; Layout.leftMargin: 76; spacing: 8
                        Text { text: "Ends"; color: Theme.subtle; font.pointSize: Theme.fontPoints(12) }
                        FmComboBox { id: repeatEnd; objectName: "eventRepeatEnd"; model: ["Never", "On date", "After count"]; Layout.preferredWidth: 112; implicitHeight: 30; onActivated: { editor.recurrenceTouched = true; if (currentIndex !== 2) repeatCount.text = "" } }
                        FmDateField { weekStartsMonday: backend.preferences.week_start !== "sunday"; id: repeatUntil; objectName: "eventRepeatUntil"; visible: repeatEnd.currentIndex === 1; Layout.fillWidth: true; onTextEdited: editor.recurrenceTouched = true; Accessible.name: "Repeat end date" }
                        FmTextField { id: repeatCount; objectName: "eventRepeatCount"; visible: repeatEnd.currentIndex === 2; placeholderText: "Count"; Layout.fillWidth: true; implicitHeight: Math.max(30, contentHeight + 8); padding: 4; leftPadding: 6; rightPadding: 6; onTextEdited: editor.recurrenceTouched = true; Accessible.name: "Number of occurrences" }
                        Item { visible: repeatEnd.currentIndex === 0; Layout.fillWidth: true }
                    }
                    Text {
                        visible: !editor.following && editor.repeatRuleIndex === 5; Layout.fillWidth: true; Layout.leftMargin: 76
                        text: "Every " + (Number(repeatInterval.text) > 1 ? repeatInterval.text + " weeks" : "week") + " on " + editor.dayOrder.filter(function(d) { return editor.repeatDays.indexOf(d) >= 0 }).map(function(d) { return editor.weekDayNames[d] }).join(", ") + (repeatEnd.currentIndex === 1 ? ", until " + repeatUntil.text : "") + "."
                        color: Theme.subtle; font.pointSize: Theme.fontPoints(12); wrapMode: Text.Wrap
                    }
                    Text { objectName: "eventRetainedRecurrence"; visible: editor.following || (!!editor.editEvent.event_id && !editor.recurrenceTouched); text: (editor.following ? "Retained repeat rule: " : "") + (editor.editEvent.recurrence || []).join(" · "); textFormat: Text.PlainText; color: Theme.subtle; font.pointSize: Theme.fontPoints(10); wrapMode: Text.Wrap; Layout.fillWidth: true }
                }
                Rectangle { visible: repeatSection.visible; Layout.fillWidth: true; implicitHeight: 1; color: Theme.line }
                RowLayout {
                    Layout.fillWidth: true; Layout.leftMargin: 20; Layout.rightMargin: 20; Layout.minimumHeight: 44; spacing: 12
                    Text { text: "Where"; color: Theme.inkAlpha(0.45); font.pointSize: Theme.fontPoints(12.5); Layout.preferredWidth: 64 }
                    TextField { id: locationField; objectName: "eventLocation"; Layout.fillWidth: true; color: Theme.text; placeholderTextColor: Theme.subtle; selectByMouse: true; placeholderText: "Add a location"; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(13.5); background: null; leftPadding: 0 }
                }
                Rectangle { Layout.fillWidth: true; implicitHeight: 1; color: Theme.line }
                RowLayout {
                    Layout.fillWidth: true; Layout.leftMargin: 20; Layout.rightMargin: 20; Layout.minimumHeight: 44; spacing: 12
                    Text { text: "Call"; color: Theme.inkAlpha(0.45); font.pointSize: Theme.fontPoints(12.5); Layout.preferredWidth: 64 }
                    ColumnLayout {
                        Layout.fillWidth: true; spacing: 2
                        FmButton { objectName: "eventAddMeet"; visible: editor.live && !editor.conferencePresent; text: "Add Google Meet"; iconName: "plus"; leftPadding: 0; enabled: editor.formReady; onClicked: editor.conferenceChange = "google_meet" }
                        TextField { id: conferenceField; objectName: "eventConference"; visible: !editor.live || editor.conferencePresent; Layout.fillWidth: true; color: Theme.text; placeholderTextColor: Theme.subtle; selectByMouse: true; readOnly: editor.live; placeholderText: editor.live ? (editor.conferenceChange === "google_meet" ? "Google Meet · link added when saved" : editor.editEvent.conference_status === "failure" ? "Google Meet could not be created" : "Google Meet · preparing link…") : "Paste a meeting link"; font.pointSize: Theme.fontPoints(13); background: null; leftPadding: 0 }
                    }
                    FmButton { objectName: "eventRemoveMeet"; visible: editor.live && editor.conferencePresent; iconName: "close"; tip: "Remove meeting"; implicitWidth: 26; implicitHeight: 26; leftPadding: 5; rightPadding: 5; enabled: editor.formReady; onClicked: { editor.conferenceChange = editor.editEvent.has_conference || !!editor.editEvent.conference_url ? "none" : "keep"; conferenceField.text = "" } }
                }
                Rectangle { Layout.fillWidth: true; implicitHeight: 1; color: Theme.line }
                RowLayout {
                    Layout.fillWidth: true; Layout.leftMargin: 20; Layout.rightMargin: 20; Layout.minimumHeight: 44; spacing: 12
                    Text { text: "Guests"; color: Theme.inkAlpha(0.45); font.pointSize: Theme.fontPoints(12.5); Layout.preferredWidth: 64; Layout.alignment: Qt.AlignTop; topPadding: 13 }
                    ColumnLayout {
                        Layout.fillWidth: true; Layout.topMargin: 8; Layout.bottomMargin: 8; spacing: 4
                        Flow {
                            visible: editor.guestAddresses.length > 0; Layout.fillWidth: true; spacing: 4
                            Repeater { model: editor.guestAddresses
                                Rectangle {
                                    required property string modelData; required property int index
                                    width: Math.min(240, guestName.implicitWidth + 30); height: 24; radius: 12; color: "transparent"; border.color: Theme.controlBorder
                                    Text { id: guestName; x: 9; anchors.verticalCenter: parent.verticalCenter; width: parent.width - 30; text: parent.modelData; color: Theme.text; font.pointSize: Theme.fontPoints(12); elide: Text.ElideRight; textFormat: Text.PlainText }
                                    FmButton { objectName: "eventRemoveGuest-" + parent.index; anchors.right: parent.right; width: 22; height: 24; visible: editor.guestsEditable; iconName: "close"; iconSize: 9; leftPadding: 4; rightPadding: 4; tip: "Remove " + parent.modelData; onClicked: { if (!editor.guestsEditable || editor.saving || editor.unknownOutcome) return; let index = parent.index; editor.guestAddresses = editor.guestAddresses.filter(function(a, i) { return i !== index }) } }
                                }
                            }
                        }
                        Text { objectName: "eventProtectedGuests"; visible: editor.protectedGuests.length > 0; text: editor.protectedGuests.map(function(a) { return a.email + (a.self ? " (you)" : " (organizer)") }).join(", ") + " · retained"; color: Theme.subtle; font.pointSize: Theme.fontPoints(11); wrapMode: Text.Wrap; textFormat: Text.PlainText; Layout.fillWidth: true }
                        Text { visible: editor.live && !editor.guestsEditable; text: (editor.editEvent.attendees || []).map(function(a) { return a.email || "Address unavailable" }).join(", "); color: Theme.muted; font.pointSize: Theme.fontPoints(12); wrapMode: Text.Wrap; textFormat: Text.PlainText; Layout.fillWidth: true }
                        TextField { id: attendeeField; objectName: "eventAttendees"; Layout.fillWidth: true; implicitHeight: 26; color: Theme.text; placeholderTextColor: Theme.subtle; selectByMouse: true; readOnly: !editor.guestsEditable; placeholderText: editor.guestsEditable ? "Add email addresses" : "Guest changes unavailable"; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(13.5); background: null; leftPadding: 0; onAccepted: editor.commitGuests(); onActiveFocusChanged: if (!activeFocus && text.trim().length) editor.commitGuests() }
                        Text { objectName: "eventGuestEditExplanation"; visible: editor.live && !editor.guestsEditable; text: editor.guestEditExplanation(); color: Theme.subtle; font.pointSize: Theme.fontPoints(11); wrapMode: Text.Wrap; Layout.fillWidth: true }
                    }
                }
                Rectangle { Layout.fillWidth: true; implicitHeight: 1; color: Theme.line }
                RowLayout {
                    Layout.fillWidth: true; Layout.leftMargin: 20; Layout.rightMargin: 20; Layout.topMargin: 12; Layout.bottomMargin: 12; spacing: 12
                    Text { text: "Notes"; color: Theme.inkAlpha(0.45); font.pointSize: Theme.fontPoints(12.5); Layout.preferredWidth: 64; Layout.alignment: Qt.AlignTop; Layout.topMargin: 2 }
                    TextArea {
                        id: notesField; objectName: "eventDescription"; Layout.fillWidth: true; Layout.alignment: Qt.AlignTop; Layout.preferredHeight: Math.max(editor.notesHeight, implicitHeight)
                        color: Theme.text; placeholderTextColor: Theme.subtle; placeholderText: "Agenda, dial-in — anything guests should see"; wrapMode: TextEdit.Wrap; selectByMouse: true
                        textFormat: TextEdit.PlainText; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(13.5); leftPadding: 0; rightPadding: 0; topPadding: 1; bottomPadding: 0; background: null
                        Accessible.name: "Event notes"
                        Shape {
                            anchors.right: parent.right; anchors.bottom: parent.bottom; width: 10; height: 10
                            ShapePath { strokeColor: Theme.inkAlpha(.3); strokeWidth: 1; fillColor: "transparent"; PathSvg { path: "M 9.5 3 L 3 9.5 M 9.5 6.5 L 6.5 9.5" } }
                        }
                        MouseArea {
                            objectName: "eventDescriptionResize"
                            anchors.right: parent.right; anchors.bottom: parent.bottom; width: 16; height: 16; cursorShape: Qt.SizeVerCursor; preventStealing: true
                            property real startY: 0
                            property real startHeight: 0
                            onPressed: function(mouse) { startY = mapToItem(null, mouse.x, mouse.y).y; startHeight = notesField.height }
                            onPositionChanged: function(mouse) { if (pressed) editor.notesHeight = Math.max(176, startHeight + mapToItem(null, mouse.x, mouse.y).y - startY) }
                        }
                    }
                }
                Rectangle { Layout.fillWidth: true; implicitHeight: 1; color: Theme.line }
                RowLayout {
                    Layout.fillWidth: true; Layout.leftMargin: 20; Layout.rightMargin: 20; Layout.minimumHeight: 44; spacing: 12
                    Text { text: "Remind"; color: Theme.inkAlpha(0.45); font.pointSize: Theme.fontPoints(12.5); Layout.preferredWidth: 64 }
                    FmComboBox { id: reminderField; objectName: "eventReminder"; model: editor.reminderChoices; textRole: "text"; fill: Theme.canvas; Layout.preferredWidth: Math.max(140, reminderMetrics.advanceWidth(displayText) + 44); implicitHeight: 30; Accessible.name: "Event reminder" }
                    FontMetrics { id: reminderMetrics; font: reminderField.font }
                    Item { Layout.fillWidth: true }
                }
                Rectangle { Layout.fillWidth: true; implicitHeight: 1; color: Theme.line }
                RowLayout {
                    Layout.fillWidth: true; Layout.leftMargin: 20; Layout.rightMargin: 20; Layout.minimumHeight: 44; spacing: 12
                    Text { text: "Show as"; color: Theme.inkAlpha(0.45); font.pointSize: Theme.fontPoints(12.5); Layout.preferredWidth: 64 }
                    FmSegmented { id: availabilityField; objectName: "eventAvailability"; model: ["Busy", "Free"]; optionNames: ["eventAvailabilityBusy", "eventAvailabilityFree"]; fill: Theme.canvas; Accessible.name: "Calendar availability" }
                    Item { Layout.fillWidth: true }
                }
            }
        }
        Text { visible: editor.feedback.length > 0; text: editor.feedback; color: Theme.accent; font.pointSize: Theme.fontPoints(12); wrapMode: Text.Wrap; Layout.fillWidth: true; Layout.margins: 12; textFormat: Text.PlainText }
        RowLayout {
            visible: editor.occurrence && !editor.unknownOutcome; Layout.fillWidth: true; Layout.leftMargin: 20; Layout.rightMargin: 20; Layout.bottomMargin: 10
            Text { text: editor.occurrenceReady ? editor.scopeLabel() : "This recurring scope needs a fresh check."; color: Theme.muted; font.pointSize: Theme.fontPoints(11); wrapMode: Text.Wrap; Layout.fillWidth: true }
            FmButton { objectName: "eventRefreshOccurrence"; text: occurrenceRead.busy ? "Checking…" : "Refresh version"; outlined: true; font.pointSize: Theme.fontPoints(11); enabled: !editor.saving && !occurrenceRead.busy && editor.accountReady && editor.sourceReady; onClicked: occurrenceRead.read() }
        }
        RowLayout {
            visible: !editor.occurrence && editor.staleRevision && !editor.unknownOutcome && !!editor.latestEvent.etag; Layout.fillWidth: true; Layout.leftMargin: 20; Layout.rightMargin: 20; Layout.bottomMargin: 10
            Text { text: "Current: " + (editor.latestEvent.title || ""); color: Theme.muted; font.pointSize: Theme.fontPoints(11); wrapMode: Text.Wrap; Layout.fillWidth: true; textFormat: Text.PlainText }
            FmButton { objectName: "eventUseCurrentVersion"; text: "Use current version"; outlined: true; font.pointSize: Theme.fontPoints(11); tip: "Keep your fields and update the event version"; onClicked: { let event = Object.assign({}, editor.editEvent); event.etag = editor.latestEvent.etag; editor.editEvent = event; editor.staleRevision = false; editor.requestId = ""; editor.feedback = "Current version loaded. Review your fields, then save." } }
        }
        Rectangle { Layout.fillWidth: true; implicitHeight: 1; color: Theme.line }
        RowLayout {
            Layout.fillWidth: true; Layout.leftMargin: 20; Layout.rightMargin: 20; Layout.topMargin: 12; spacing: 6
            FmButton { objectName: "eventSaveButton"; text: editor.saving ? "Saving…" : editor.unknownOutcome ? !editor.live ? "Retry original" : editor.recoveryAction() === "resume" ? "Resume" : editor.recoveryAction() === "reconcile" ? "Check outcome" : "Check status" : editor.editEvent.event_id ? "Save changes" : "Add event"; prominent: true; implicitHeight: 32; leftPadding: 16; rightPadding: 16; font.pointSize: Theme.fontPoints(12.5); shortcutHint: Qt.platform.os === "osx" ? "⌘↵" : "Ctrl+↵"; enabled: !editor.saving && editor.actionReady && (editor.unknownOutcome || (!editor.staleRevision && editor.formReady)); tip: editor.live ? "Save to Google Calendar — Ctrl+Enter" : "Save locally — Ctrl+Enter"; onClicked: editor.save() }
            FmButton { objectName: "eventCancelButton"; text: editor.live && editor.saving ? "Cancel request" : editor.unknownOutcome ? "Close" : "Discard"; outlined: true; implicitHeight: 32; leftPadding: 14; rightPadding: 14; font.pointSize: Theme.fontPoints(12.5); font.weight: Font.Medium; enabled: !editor.saving || editor.live; tip: editor.saving ? "Cancel this request and retain your fields" : editor.unknownOutcome ? "The operation remains in calendar activity" : "Discard unsaved event fields"; onClicked: { if (editor.live && editor.saving) backend.cancelGoogleOperation(editor.submittedRequest.account, editor.submittedRequest.request_id); else editor.close() } }
            Item { Layout.fillWidth: true }
        }
        Text { objectName: "eventNotificationExplanation"; text: editor.live ? "Google may notify guests when you save or delete. Notification delivery is not confirmed here." : "Saved on this machine · no invitations sent"; color: Theme.subtle; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(11); wrapMode: Text.Wrap; Layout.fillWidth: true; Layout.leftMargin: 20; Layout.rightMargin: 20; Layout.topMargin: 8; Layout.bottomMargin: 12 }
    }
    FmConfirmDialog {
        id: deleteConfirmation; objectName: "eventDeleteConfirmation"
        property var capturedEvent: ({})
        onAboutToShow: capturedEvent = Object.assign({}, editor.editEvent)
        title: editor.occurrence ? "Cancel " + editor.scopeLabel().toLowerCase() + "?" : "Delete event?"
        message: (editor.recurringScope.length ? "Cancel “" + (editor.editEvent.title || "this event") + "” for " + editor.scopeLabel().toLowerCase() + "? " + (editor.following ? editor.followingText() : "") : editor.occurrence ? "Cancel only the occurrence of “" + (editor.editEvent.title || "this event") + "” originally scheduled for " + (((editor.editEvent.occurrence || {}).original_start || {}).date || ((editor.editEvent.occurrence || {}).original_start || {}).instant || "") + "? Other occurrences remain unchanged." : "Remove “" + (editor.editEvent.title || "this event") + "” from Google Calendar?") + " Google may notify guests. You can undo for a few seconds afterwards."
        destructive: true
        actionText: editor.recurringScope === "series" ? "Cancel series" : editor.following ? "Cancel following" : editor.occurrence ? "Cancel occurrence" : "Delete"
        actionObjectName: "eventConfirmDelete"
        onAccepted: editor.deleteEventConfirmed(true, capturedEvent)
    }
    FmConfirmDialog {
        id: followingConfirmation; objectName: "eventFollowingConfirmation"
        title: "Change this and following events?"
        message: editor.followingText() + " Google may notify guests. If only part of the change succeeds, you can review and recover it here."
        actionText: "Apply changes"; actionObjectName: "eventConfirmFollowing"
        onAccepted: editor.saveConfirmed(true)
    }
    FmConfirmDialog {
        id: abandonConfirmation; objectName: "eventAbandonConfirmation"
        title: "Abandon recorded change?"
        message: "Keep these form fields and abandon the recorded change. This does not undo a change already accepted by Google."
        actionText: "Abandon"
        onAccepted: editor.resolveOperation("abandon")
    }
}
