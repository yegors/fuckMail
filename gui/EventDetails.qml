import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Popup {
    id: details
    objectName: "eventDetails"
    parent: Overlay.overlay
    property var eventData: ({})
    property bool saving: false
    property string feedback: ""
    property var pendingResponse: ({})
    property bool timelineSource: false
    property string awaitedEtag: ""
    signal editRequested(var event)
    width: Math.min(470, parent.width - 36); padding: 24
    height: Math.min(parent.height - 36, detailContents.implicitHeight + topPadding + bottomPadding)
    anchors.centerIn: parent; modal: true; focus: true
    closePolicy: saving ? Popup.NoAutoClose : Popup.CloseOnEscape | Popup.CloseOnPressOutside
    background: Rectangle { color: Theme.surface; radius: 8; border.color: Theme.controlBorder }
    onOpened: { feedback = ""; pendingResponse = ({}); awaitedEtag = "" }
    function respond(response) {
        saving = true; feedback = "Saving your response locally…"
        if (pendingResponse.response !== response || pendingResponse.expected_etag !== eventData.etag)
            pendingResponse = {account: eventData.account, kind: "rsvp", calendar_id: eventData.calendar_id,
                event_id: eventData.event_id, expected_etag: eventData.etag, response: response,
                request_id: "gui-rsvp-" + Date.now() + "-" + Math.floor(Math.random() * 1000000000)}
        backend.calendarWrite(pendingResponse)
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
            if (!details.saving) return
            details.saving = false
            if (ok) { details.feedback = "Response saved locally. No invitation was sent."; details.awaitedEtag = result.etag || ""; let updated = Object.assign({}, details.eventData); updated.etag = result.etag; updated.self_response = details.pendingResponse.response; details.eventData = updated }
            else details.feedback = error + " Reopen the event to refresh its details; retry checks this same operation."
        }
    }
    contentItem: ScrollView {
        property bool blocksWorkspaceShortcuts: true
        clip: true; ScrollBar.horizontal.policy: ScrollBar.AlwaysOff
        ColumnLayout {
        id: detailContents
        width: details.availableWidth
        spacing: 15
        RowLayout {
            Layout.fillWidth: true
            Text { text: details.eventData.title || ""; color: Theme.text; font.pixelSize: 21; font.weight: Font.Medium; wrapMode: Text.Wrap; Layout.fillWidth: true; textFormat: Text.PlainText }
            FmButton { iconName: "close"; implicitWidth: 30; enabled: !details.saving; tip: "Close event"; onClicked: details.close() }
        }
        Text { text: (details.eventData.start_display || "") + " — " + (details.eventData.end_display || "") + "\nAmerica/Toronto"; color: Theme.muted; font.pixelSize: 12; lineHeight: 1.5; wrapMode: Text.Wrap; Layout.fillWidth: true }
        Text { text: (details.eventData.account_name || "") + " / " + (details.eventData.calendar_name || details.eventData.calendar_id || ""); color: Theme.subtle; font.pixelSize: 11; textFormat: Text.PlainText; wrapMode: Text.Wrap; Layout.fillWidth: true }
        Rectangle { Layout.fillWidth: true; implicitHeight: 1; color: Theme.line }
        Text { visible: !!details.eventData.location; text: details.eventData.location || ""; color: Theme.muted; font.pixelSize: 12; textFormat: Text.PlainText; wrapMode: Text.Wrap; Layout.fillWidth: true }
        Text { visible: !!details.eventData.organizer; text: "Organized by " + (details.eventData.organizer || ""); color: Theme.muted; font.pixelSize: 12; textFormat: Text.PlainText; wrapMode: Text.Wrap; Layout.fillWidth: true }
        ScrollView {
            visible: !!details.eventData.description; Layout.fillWidth: true; Layout.preferredHeight: Math.min(140, description.implicitHeight + 4); clip: true
            TextArea { id: description; text: details.eventData.description || ""; color: Theme.muted; font.pixelSize: 12; textFormat: TextEdit.PlainText; wrapMode: TextEdit.Wrap; readOnly: true; selectByMouse: true; background: null; padding: 0 }
        }
        ScrollView {
            visible: (details.eventData.attendees || []).length > 0; Layout.fillWidth: true; Layout.preferredHeight: Math.min(110, attendees.implicitHeight); clip: true
            Column { id: attendees; width: parent.width; spacing: 5
                Repeater { model: details.eventData.attendees || []
                    Text { required property var modelData; text: modelData.email + (modelData.self ? " (you)" : "") + " · " + (modelData.responseStatus || "needsAction"); color: Theme.subtle; font.pixelSize: 11; textFormat: Text.PlainText; width: attendees.width; wrapMode: Text.Wrap }
                }
            }
        }
        Text { visible: !!details.eventData.busy_only; text: "This calendar shares availability only. Event details and changes are unavailable."; color: Theme.subtle; font.pixelSize: 12; wrapMode: Text.Wrap; Layout.fillWidth: true }
        Text { visible: !!details.eventData.recurring; text: "Recurring event. Series changes are available in a later phase."; color: Theme.subtle; font.pixelSize: 11; wrapMode: Text.Wrap; Layout.fillWidth: true }
        Text { visible: !!details.eventData.read_only && !details.eventData.busy_only; text: "Read-only event details"; color: Theme.subtle; font.pixelSize: 11 }
        RowLayout {
            visible: !!details.eventData.can_rsvp; spacing: 5
            Text { text: "Your RSVP"; color: Theme.muted; font.pixelSize: 11 }
            Repeater { model: [{label: "Accept", value: "accepted"}, {label: "Maybe", value: "tentative"}, {label: "Decline", value: "declined"}]
                FmButton { required property var modelData; objectName: "eventRsvp-" + modelData.value; text: modelData.label; outlined: true; selected: details.eventData.self_response === modelData.value; enabled: !details.saving && !details.awaitedEtag.length && backend.connected; onClicked: details.respond(modelData.value) }
            }
        }
        Text { visible: details.feedback.length > 0; text: details.feedback; color: Theme.accent; font.pixelSize: 12; wrapMode: Text.Wrap; Layout.fillWidth: true; textFormat: Text.PlainText }
        RowLayout {
            Layout.fillWidth: true
            FmButton { visible: /^https?:\/\/[^\/]/.test(details.eventData.conference_url || ""); text: "Join meeting"; prominent: true; onClicked: Qt.openUrlExternally(details.eventData.conference_url) }
            Item { Layout.fillWidth: true }
            FmButton { objectName: "eventEditButton"; visible: !!details.eventData.can_edit; text: "Edit event"; outlined: true; enabled: !details.saving && !details.awaitedEtag.length && backend.connected; onClicked: { details.editRequested(details.eventData); details.close() } }
        }
        }
    }
}
