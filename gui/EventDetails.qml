import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Effects

Popup {
    id: details
    objectName: "eventDetails"
    parent: Overlay.overlay
    property var eventData: ({})
    property bool saving: false
    property string feedback: ""
    property var pendingResponse: ({})
    readonly property bool live: !backend.syntheticProfile
    property string operationState: ""
    property double operationRevision: -1
    readonly property bool hasPendingResponse: !!pendingResponse.request_id && operationState.length > 0
    readonly property bool accountReady: backend.connected && (!live || (backend.accounts.length >= 0 && backend.googleSync
        && backend.accountCapability(eventData.account || "", "calendar_write") && backend.googleSyncState(eventData.account || "").active === true))
    readonly property bool reconciliationReady: backend.connected && backend.accounts.length >= 0 && backend.googleSync
        && backend.accountCapability(eventData.account || "", "calendar_read") && backend.googleSyncState(eventData.account || "").active === true
    property bool timelineSource: false
    property string awaitedEtag: ""
    property string recurringScope: "occurrence"
    property string requestedResponse: ""
    readonly property var recurringScopes: ["occurrence", "series", "following"].filter(function(scope) {
        return ["edit", "delete", "rsvp"].some(function(action) { return ((details.eventData.recurring_scopes || {})[action] || []).indexOf(scope) >= 0 })
    })
    function scopedAllowed(action) { return ((eventData.recurring_scopes || {})[action] || []).indexOf(recurringScope) >= 0 }
    function readScoped(action) {
        occurrenceRead.action = action
        feedback = "Checking the selected recurring scope on Google…"
        occurrenceRead.read()
    }
    signal editRequested(var event)
    property real originX: parent.width / 2
    property real originY: 80
    property real timelineInset: 0
    property bool timelineAtTop: false
    property real timelineEdge: parent.height - timelineInset
    property var sourceEvents: timelineSource ? backend.todayEvents : backend.events
    CalendarOccurrenceRead {
        id: occurrenceRead; selection: details.eventData; active: details.visible; timelineSource: details.timelineSource
        scope: details.recurringScopes.length ? details.recurringScope : ""
        onResolved: function(event) {
            if (event.recurring_action === "rsvp") details.submitResponse(details.requestedResponse, event)
            else { details.editRequested(event); details.close() }
        }
        onFailed: function(message) { details.feedback = message }
    }
    property var conflicts: {
        let seen = {}
        return sourceEvents.filter(function(e) {
            if (e.id === eventData.id || e.all_day || eventData.all_day || e.date !== eventData.date || seen[e.id]) return false
            if (e.start_hour < eventData.start_hour + eventData.duration_hours && e.start_hour + e.duration_hours > eventData.start_hour) { seen[e.id] = true; return true }
            return false
        })
    }
    width: Math.min(300, parent.width - 32); padding: 16; topPadding: 14; bottomPadding: 14
    height: Math.min(parent.height - 80 - timelineInset, detailContents.implicitHeight + topPadding + bottomPadding)
    x: Math.max(16, Math.min(originX - width / 2, parent.width - width - 16))
    y: timelineSource ? Math.max(64, Math.min(parent.height - height - 16, timelineAtTop ? timelineEdge + 12 : timelineEdge - height - 12)) : Math.max(64, originY + 14 + height <= parent.height - 16 ? originY + 14 : originY - height - 14)
    modal: false; dim: false; focus: true
    closePolicy: saving ? Popup.NoAutoClose : Popup.CloseOnEscape | Popup.CloseOnPressOutside
    background: Rectangle {
        color: Theme.surface; radius: 8; border.color: Theme.controlBorder
        layer.enabled: true; layer.effect: MultiEffect { shadowEnabled: true; shadowColor: Theme.shadowAlpha(.6); shadowBlur: .8; shadowVerticalOffset: 12 }
    }
    enter: Transition { NumberAnimation { property: "opacity"; from: 0; to: 1; duration: 180 } }
    onAboutToShow: { feedback = ""; pendingResponse = ({}); awaitedEtag = ""; operationState = ""; operationRevision = -1 }
    onAboutToHide: occurrenceRead.invalidate()
    function openAt(event, anchor) {
        if (saving || (visible && hasPendingResponse)) return
        occurrenceRead.invalidate()
        eventData = event
        recurringScope = recurringScopes.indexOf("occurrence") >= 0 ? "occurrence" : recurringScopes[0] || "occurrence"
        requestedResponse = ""
        if (anchor) { let point = anchor.mapToItem(parent, anchor.width / 2, anchor.height); originX = point.x; originY = point.y }
        open()
    }
    function respond(response) {
        if (saving || occurrenceRead.busy || !accountReady || (live && hasPendingResponse)) return
        if (scopedAllowed("rsvp")) { requestedResponse = response; readScoped("rsvp"); return }
        if (!eventData.can_rsvp) return
        submitResponse(response, eventData)
    }
    function submitResponse(response, event) {
        if (saving || !accountReady || !event.can_rsvp || (live && hasPendingResponse)) return
        saving = true; feedback = live ? "Saving your response to Google Calendar…" : "Saving your response locally…"
        if (pendingResponse.response !== response || pendingResponse.expected_etag !== event.etag) {
            let request = {account: event.account, kind: "rsvp", calendar_id: event.calendar_id,
                event_id: event.event_id, expected_etag: event.etag, response: response,
                request_id: "gui-rsvp-" + Date.now() + "-" + Math.floor(Math.random() * 1000000000)}
            if (event.recurring_context) request.recurring = event.recurring_context
            pendingResponse = request
        }
        backend.calendarWrite(pendingResponse)
    }
    function resolveResponse(action) {
        if (saving || !backend.connected || !pendingResponse.request_id) return
        saving = true; feedback = "Checking the recorded response…"
        backend.calendarOperationAction(pendingResponse.account, pendingResponse.request_id, action, operationRevision)
    }
    Connections {
        target: backend
        function onEventsChanged() {
            if (!details.opened || details.saving || !details.eventData.id) return
            if (!(details.timelineSource ? backend.timelineReady : backend.calendarReady)) { details.eventData = ({}); details.close(); return }
            let candidates = (details.timelineSource ? backend.todayEvents : backend.events).filter(function(e) { return e.id === details.eventData.id })
            if (!candidates.length) { details.eventData = ({}); details.close(); return }
            let candidate = candidates.find(function(e) { return e.busy_only }) || candidates[0]
            if (details.awaitedEtag.length && !candidate.busy_only && candidate.etag !== details.awaitedEtag) return
            details.awaitedEtag = ""; details.eventData = candidate
        }
        function onConnectedChanged() { if (!backend.connected && details.opened && !details.saving) details.close() }
        function onCalendarWriteFinished(ok, result, error) {
            if (!details.saving || result.account !== details.pendingResponse.account || result.request_id !== details.pendingResponse.request_id) return
            details.saving = false
            if (ok) {
                details.operationState = ""; details.operationRevision = -1
                if (details.live) { details.feedback = result.state === "reconciled" ? "Your response was observed on Google. The original response and notification delivery remain unconfirmed." : "Google accepted your response. Notification delivery is unverified."; details.pendingResponse = ({}) }
                else { details.feedback = "Response saved locally. No invitation was sent."; details.awaitedEtag = result.etag || ""; let updated = Object.assign({}, details.eventData); if (result.etag) updated.etag = result.etag; updated.self_response = details.pendingResponse.response; details.eventData = updated }
            } else if (details.live) {
                details.feedback = error
                if (result.state === "abandoned" || (result.local_preflight === true && result.operation_action === "write") || (result.operation_action === "check" && result.code === "not_found")) {
                    details.operationState = ""; details.operationRevision = -1; details.pendingResponse = ({})
                } else {
                    details.operationState = result.state || details.operationState || "unconfirmed"
                    if (result.revision !== undefined) details.operationRevision = result.revision
                }
            } else details.feedback = error + " Reopen the event to refresh its details; retry checks this same operation."
        }
    }
    contentItem: ScrollView {
        property bool blocksWorkspaceShortcuts: true
        clip: true; ScrollBar.horizontal.policy: ScrollBar.AlwaysOff
        ColumnLayout {
            id: detailContents; width: details.availableWidth; spacing: 10
            RowLayout {
                Layout.fillWidth: true; spacing: 10
                Text { text: details.eventData.title || ""; color: Theme.text; font.pixelSize: Theme.fontSize(15); font.weight: Font.Medium; wrapMode: Text.Wrap; Layout.fillWidth: true; textFormat: Text.PlainText }
                Text { text: details.eventData.account_name || ""; color: Theme.subtle; font.pixelSize: Theme.fontSize(10); Layout.maximumWidth: 95; elide: Text.ElideRight; Layout.alignment: Qt.AlignTop; topPadding: 2 }
            }
            Flow {
            Layout.fillWidth: true; spacing: 8
            Text { text: details.eventData.all_day ? "All day" : (details.eventData.time_display || "") + " – " + (details.eventData.end_display || ""); color: Theme.accent; font.pixelSize: Theme.fontSize(13); font.weight: Font.Medium }
            Text {
                text: (details.eventData.date ? Qt.formatDate(new Date(details.eventData.date + "T12:00:00"), "ddd d MMM") : "")
                    + (!details.eventData.all_day && details.eventData.duration_hours ? " · " + Math.round(details.eventData.duration_hours * 60) + " min" : "")
                    + (details.eventData.recurring ? " · repeats" : "")
                color: Theme.subtle; font.pixelSize: Theme.fontSize(12)
            }
            }
            Text { text: (details.eventData.calendar_name || details.eventData.calendar_id || "") + " · " + (backend.preferences.calendar_display_time_zone || "Time zone unavailable"); color: Theme.subtle; font.pixelSize: Theme.fontSize(10); textFormat: Text.PlainText; wrapMode: Text.Wrap; Layout.fillWidth: true }
            RowLayout {
                visible: !!details.eventData.location; Layout.fillWidth: true; spacing: 9
                FmIcon { name: "info"; size: 13; color: Theme.subtle }
                Text { text: details.eventData.location || ""; color: Theme.muted; font.pixelSize: Theme.fontSize(13); textFormat: Text.PlainText; wrapMode: Text.Wrap; Layout.fillWidth: true }
            }
            RowLayout {
                visible: /^https?:\/\/[^\/]/.test(details.eventData.conference_url || ""); Layout.fillWidth: true; spacing: 9
                FmIcon { name: "calendar"; size: 13; color: Theme.subtle }
                FmButton { text: (details.eventData.conference_url || "").replace(/^https?:\/\//, "") + " · join"; font.pixelSize: Theme.fontSize(12); implicitHeight: 24; leftPadding: 0; rightPadding: 0; Layout.fillWidth: true; textAlignment: Text.AlignLeft; tip: details.eventData.conference_url || ""; onClicked: backend.joinCalendarEvent(details.eventData) }
            }
            Text { visible: !!details.eventData.organizer; text: "Organized by " + (details.eventData.organizer || ""); color: Theme.subtle; font.pixelSize: Theme.fontSize(11); textFormat: Text.PlainText; wrapMode: Text.Wrap; Layout.fillWidth: true }
            RowLayout {
                visible: (details.eventData.attendees || []).length > 0; Layout.fillWidth: true; spacing: 9
                FmIcon { name: "person"; size: 13; color: Theme.subtle; Layout.alignment: Qt.AlignTop }
                Text { text: (details.eventData.attendees || []).map(function(a) { return a.email + (a.self ? " (you)" : "") }).join(", "); color: Theme.muted; font.pixelSize: Theme.fontSize(12); textFormat: Text.PlainText; wrapMode: Text.Wrap; Layout.fillWidth: true }
            }
            Text { visible: details.conflicts.length > 0; text: "Overlaps " + details.conflicts.map(function(e) { return e.title + " (" + e.time_display + " – " + e.end_display + ", " + e.account_name + " / " + e.calendar_name + ")" }).join(", "); color: Theme.accent; font.pixelSize: Theme.fontSize(12); wrapMode: Text.Wrap; textFormat: Text.PlainText; Layout.fillWidth: true }
            Text {
                visible: details.eventData.date === backend.todayDate && !details.eventData.all_day
                property real start: details.eventData.start_hour || 0
                property real end: start + (details.eventData.duration_hours || 0)
                text: backend.nowHour < start ? "Starts in " + Math.ceil((start - backend.nowHour) * 60) + " min" : backend.nowHour < end ? "Now · ends " + details.eventData.end_display : "Ended " + details.eventData.end_display
                color: Theme.subtle; font.pixelSize: Theme.fontSize(11); wrapMode: Text.Wrap; Layout.fillWidth: true
            }
            ScrollView {
                visible: !!details.eventData.description; Layout.fillWidth: true; Layout.preferredHeight: Math.min(100, description.implicitHeight + 4); clip: true
                TextArea { id: description; text: details.eventData.description || ""; color: Theme.muted; font.pixelSize: Theme.fontSize(12); textFormat: TextEdit.PlainText; wrapMode: TextEdit.Wrap; readOnly: true; selectByMouse: true; background: null; padding: 0 }
            }
            Text { visible: !!details.eventData.busy_only; text: "Availability only · details are not shared."; color: Theme.subtle; font.pixelSize: Theme.fontSize(11); wrapMode: Text.Wrap; Layout.fillWidth: true }
            RowLayout {
                visible: details.recurringScopes.length > 0; Layout.fillWidth: true; spacing: 8
                Text { text: "Apply to"; color: Theme.subtle; font.pixelSize: Theme.fontSize(11) }
                FmComboBox {
                    objectName: "eventRecurringScope"; Layout.fillWidth: true; implicitHeight: 30
                    model: details.recurringScopes.map(function(scope) { return {occurrence:"This occurrence",series:"Entire series",following:"This and following"}[scope] })
                    currentIndex: details.recurringScopes.indexOf(details.recurringScope)
                    enabled: !details.saving && !occurrenceRead.busy && !details.hasPendingResponse
                    onActivated: details.recurringScope = details.recurringScopes[currentIndex]
                }
            }
            Text { visible: !!details.eventData.occurrence_selector && !details.recurringScopes.length; text: "Changes apply to this occurrence only."; color: Theme.subtle; font.pixelSize: Theme.fontSize(11); wrapMode: Text.Wrap; Layout.fillWidth: true }
            Text { visible: !!details.eventData.read_only && !details.eventData.busy_only; text: "Read-only event"; color: Theme.subtle; font.pixelSize: Theme.fontSize(11) }
            ColumnLayout {
                visible: !!details.eventData.can_rsvp || details.scopedAllowed("rsvp"); spacing: 5; Layout.fillWidth: true
                Text { text: "Your response"; color: Theme.subtle; font.pixelSize: Theme.fontSize(11) }
                RowLayout {
                    spacing: 4
                    Repeater { model: [{label: "Accept", value: "accepted"}, {label: "Maybe", value: "tentative"}, {label: "Decline", value: "declined"}]
                        FmButton { required property var modelData; objectName: "eventRsvp-" + modelData.value; text: modelData.label; outlined: true; selected: details.eventData.self_response === modelData.value; implicitHeight: 26; font.pixelSize: Theme.fontSize(11); enabled: !details.saving && !occurrenceRead.busy && !details.awaitedEtag.length && details.accountReady && (!details.live || !details.hasPendingResponse); onClicked: details.respond(modelData.value) }
                    }
                }
            }
            Text { visible: details.live && (!!details.eventData.can_rsvp || details.scopedAllowed("rsvp")); text: "Your response is sent to Google. Google may notify the organizer; notification delivery is unverified."; color: Theme.subtle; font.pixelSize: Theme.fontSize(10); wrapMode: Text.Wrap; Layout.fillWidth: true }
            RowLayout {
                visible: details.live && details.hasPendingResponse; Layout.fillWidth: true; spacing: 6
                FmButton {
                    objectName: "eventRsvpResolve"; text: ["prepared", "ready"].indexOf(details.operationState) >= 0 ? "Resume" : details.operationState === "unknown" ? "Check outcome" : "Check status"; outlined: true
                    enabled: !details.saving && backend.connected && (["prepared", "ready", "unknown"].indexOf(details.operationState) < 0 || (details.operationState === "unknown" ? details.reconciliationReady : details.accountReady))
                    onClicked: details.resolveResponse(["prepared", "ready"].indexOf(details.operationState) >= 0 ? "resume" : details.operationState === "unknown" ? "reconcile" : "check")
                }
                FmButton { objectName: "eventRsvpAbandon"; text: "Abandon"; visible: details.operationRevision >= 0 && ["prepared", "ready", "unknown", "conflict", "rejected"].indexOf(details.operationState) >= 0; enabled: !details.saving && backend.connected; onClicked: responseAbandonConfirmation.open() }
            }
            Text { visible: details.feedback.length > 0; text: details.feedback; color: Theme.accent; font.pixelSize: Theme.fontSize(11); wrapMode: Text.Wrap; Layout.fillWidth: true; textFormat: Text.PlainText }
            Rectangle { Layout.fillWidth: true; implicitHeight: 1; color: Theme.line }
            RowLayout {
                Layout.fillWidth: true; spacing: 6
                FmButton { objectName: "eventEditButton"; visible: !!details.eventData.can_edit || details.scopedAllowed("edit") || (!details.recurringScopes.length && details.eventData.can_resolve_occurrence === true); text: occurrenceRead.busy ? "Checking…" : !details.recurringScopes.length && details.eventData.can_resolve_occurrence === true ? "Edit this occurrence" : "Edit"; prominent: true; implicitHeight: 28; font.pixelSize: Theme.fontSize(12); enabled: !details.saving && !occurrenceRead.busy && !details.awaitedEtag.length && details.accountReady && !details.hasPendingResponse; onClicked: { if (details.scopedAllowed("edit") || details.eventData.can_resolve_occurrence === true) details.readScoped("edit"); else { details.editRequested(details.eventData); details.close() } } }
                FmButton { objectName: "eventScopedCancel"; visible: details.scopedAllowed("delete"); text: "Cancel"; outlined: true; implicitHeight: 28; font.pixelSize: Theme.fontSize(12); enabled: !details.saving && !occurrenceRead.busy && details.accountReady && !details.hasPendingResponse; onClicked: details.readScoped("delete") }
                Item { Layout.fillWidth: true }
                FmButton { objectName: "eventDetailsClose"; text: details.live && details.saving ? "Cancel request" : "Close"; implicitHeight: 28; font.pixelSize: Theme.fontSize(11); enabled: !details.saving || details.live; onClicked: { if (details.live && details.saving) backend.cancelGoogleOperation(details.pendingResponse.account, details.pendingResponse.request_id); else details.close() } }
            }
        }
    }
    Dialog {
        id: responseAbandonConfirmation; objectName: "eventRsvpAbandonConfirmation"; parent: Overlay.overlay; anchors.centerIn: parent; modal: true
        implicitWidth: 304; width: Math.min(implicitWidth, parent.width - 32)
        implicitHeight: topPadding + bottomPadding + responseBody.implicitHeight + (header ? header.implicitHeight + spacing : 0) + (footer ? footer.implicitHeight + spacing : 0)
        title: "Abandon recorded response?"; standardButtons: Dialog.Ok | Dialog.Cancel
        background: Rectangle { color: Theme.surface; border.color: Theme.controlBorder; radius: 8 }
        palette.windowText: Theme.text; palette.buttonText: Theme.text
        onOpened: standardButton(Dialog.Ok).text = "Abandon"
        contentItem: Item {
            implicitHeight: responseBody.implicitHeight
            Text { id: responseBody; width: responseAbandonConfirmation.availableWidth; text: "Abandon this recorded response. This does not undo a response already accepted by Google."; color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(13); wrapMode: Text.Wrap; textFormat: Text.PlainText }
        }
        onAccepted: details.resolveResponse("abandon")
    }
}
