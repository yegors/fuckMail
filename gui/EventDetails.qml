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
    property var deleteTarget: ({})
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
    // Closed details do not track event lists.
    property var sourceEvents: !visible ? [] : timelineSource ? backend.todayEvents : backend.events
    CalendarOccurrenceRead {
        id: occurrenceRead; selection: details.eventData; active: details.visible; timelineSource: details.timelineSource
        scope: details.recurringScopes.length ? details.recurringScope : ""
        onResolved: function(event) {
            if (event.recurring_action === "rsvp") details.submitResponse(details.requestedResponse, event)
            else if (event.recurring_action === "delete" && (event.recurring_context||{}).scope!=="following") details.confirmDeletion(event)
            else { details.editRequested(event); details.close() }
        }
        onFailed: function(message) { details.feedback = message }
    }
    // "1 h", "1 h 30 min" or "45 min", as in the reference popover.
    function duration(minutes) { const m = Math.round(minutes); return m >= 60 ? Math.floor(m / 60) + " h" + (m % 60 ? " " + (m % 60) + " min" : "") : m + " min" }
    function relative(hours) { const m = Math.round(hours * 60); return m >= 60 ? Math.floor(m / 60) + " h " + (m % 60) + " min" : m + " min" }
    function conferenceLabel(url) {
        const value = String(url || "").toLowerCase()
        return value.indexOf("meet.google") >= 0 ? "Google Meet" : value.indexOf("teams") >= 0 ? "Teams" : value.indexOf("zoom") >= 0 ? "Zoom" : String(url || "").replace(/^https?:\/\//, "")
    }
    readonly property string sourceLabel: {
        const calendarName = eventData.calendar_name || ""
        return (eventData.account_name || "") + (calendarName && calendarName !== "Primary" && eventData.calendar_id !== "primary" ? " · " + calendarName : "")
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
    closePolicy: saving || deleteConfirmation.visible ? Popup.NoAutoClose : Popup.CloseOnEscape | Popup.CloseOnPressOutside
    background: Rectangle {
        color: Theme.surface; radius: 8; border.color: Theme.controlBorder
        layer.enabled: GraphicsInfo.api !== GraphicsInfo.Software; layer.effect: MultiEffect { shadowEnabled: true; shadowColor: Theme.shadowAlpha(.6); shadowBlur: 1; shadowVerticalOffset: 24 }
    }
    enter: Transition { NumberAnimation { property: "opacity"; from: 0; to: 1; duration: 180 } }
    onAboutToShow: { feedback = ""; pendingResponse = ({}); awaitedEtag = ""; operationState = ""; operationRevision = -1 }
    onAboutToHide: { occurrenceRead.invalidate(); deleteConfirmation.close() }
    function openAt(event, anchor) {
        if (saving || deleteConfirmation.visible || (visible && hasPendingResponse)) return
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
    function deleteEvent(scope) {
        if (!visible || !live || saving || occurrenceRead.busy || !accountReady || hasPendingResponse) return
        const allowed=(eventData.recurring_scopes||{}).delete||[]
        if(scope && allowed.indexOf(scope)>=0) {recurringScope=scope;readScoped("delete")}
        else if(!scope && eventData.can_delete===true)confirmDeletion(eventData)
    }
    function confirmDeletion(event) {
        if(event.can_delete!==true)return
        deleteTarget=Object.assign({},event)
        deleteConfirmation.open()
    }
    function submitDeletion() {
        const event=deleteTarget
        if(saving||hasPendingResponse||!backend.connected||!backend.accountCapability(event.account,"calendar_write")||!backend.googleSyncState(event.account).active||event.can_delete!==true)return
        const request={account:event.account,calendar_id:event.calendar_id,event_id:event.event_id,expected_etag:event.etag,kind:"delete",request_id:"gui-delete-"+Date.now()+"-"+Math.floor(Math.random()*1000000000)}
        if(event.recurring_context)request.recurring=event.recurring_context
        else if(event.occurrence)request.occurrence=event.occurrence
        const error = backend.queueCalendarDeletion(request, event)
        if (error.length) feedback = error; else close()
    }
    Connections {
        target: backend
        // Timeline rows arrive separately from the Calendar window.
        ignoreUnknownSignals: true
        function onEventsChanged() { if (!details.timelineSource) refresh() }
        function onTimelineChanged() { if (details.timelineSource) refresh() }
        function refresh() {
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
                if(details.pendingResponse.kind==="delete") {details.pendingResponse={};details.close();return}
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
                ScrollBar.vertical: FmScrollBar {}
        property bool blocksWorkspaceShortcuts: true
        clip: true; ScrollBar.horizontal.policy: ScrollBar.AlwaysOff
        ColumnLayout {
            id: detailContents; width: details.availableWidth; spacing: 10
            RowLayout {
                Layout.fillWidth: true; spacing: 10
                Text { text: details.eventData.title || ""; color: Theme.text; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(15); font.weight: Font.Medium; lineHeightMode: Text.FixedHeight; lineHeight: Math.round(Theme.fontSize(15) * 1.3); wrapMode: Text.Wrap; Layout.fillWidth: true; textFormat: Text.PlainText }
                Text { text: details.sourceLabel; color: Theme.inkAlpha(.5); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(11.5); Layout.minimumWidth: Math.min(120, implicitWidth); Layout.preferredWidth: Math.min(120, implicitWidth); Layout.maximumWidth: 120; elide: Text.ElideRight; Layout.alignment: Qt.AlignTop; topPadding: 2; textFormat: Text.PlainText }
            }
            Flow {
                objectName: "eventDetailsWhen"; Layout.fillWidth: true; spacing: 8
                Text {
                    width: Math.min(implicitWidth, parent.width); wrapMode: Text.Wrap
                    text: details.eventData.multi_day ? (details.eventData.start_display || "") + " – " + (details.eventData.end_display || "") : details.eventData.all_day ? "All day" : (details.eventData.time_display || "") + " – " + (details.eventData.end_display || "")
                    color: Theme.accent; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(13); font.weight: Font.Medium
                    HoverHandler { id: whenHover }
                    FmToolTip { shown: whenHover.hovered; text: backend.preferences.calendar_display_time_zone || "Time zone unavailable" }
                }
                Text {
                    width: Math.min(implicitWidth, parent.width); wrapMode: Text.Wrap
                    text: (!details.eventData.multi_day && details.eventData.date ? details.eventData.date === backend.todayDate ? "Today" : Qt.formatDate(new Date(details.eventData.date + "T12:00:00"), "ddd d MMM") : "")
                        + (!details.eventData.all_day && !details.eventData.multi_day && details.eventData.duration_hours ? " · " + details.duration(details.eventData.duration_minutes || details.eventData.duration_hours * 60) : "")
                        + (details.eventData.recurring ? details.eventData.multi_day ? "Repeats" : " · repeats" : "")
                    color: Theme.inkAlpha(.5); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(13)
                }
            }
            RowLayout {
                visible: !!details.eventData.location; Layout.fillWidth: true; spacing: 9
                FmIcon { name: "pin"; size: 13; color: Theme.inkAlpha(.45) }
                Text { text: details.eventData.location || ""; color: Theme.inkAlpha(.75); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(13); textFormat: Text.PlainText; wrapMode: Text.Wrap; Layout.fillWidth: true }
            }
            RowLayout {
                visible: /^https?:\/\/[^\/]/.test(details.eventData.conference_url || ""); Layout.fillWidth: true; spacing: 9
                FmIcon { name: "video"; size: 13; color: Theme.inkAlpha(.45) }
                FmButton {
                    objectName: "eventJoinButton"; text: details.conferenceLabel(details.eventData.conference_url); font.pointSize: Theme.fontPoints(13)
                    implicitHeight: Math.round(Theme.fontSize(13) * 1.4); leftPadding: 0; rightPadding: 0; topPadding: 0; bottomPadding: 0; Layout.maximumWidth: implicitWidth; Layout.fillWidth: true
                    textAlignment: Text.AlignLeft; foreground: hovered ? Theme.text : Theme.inkAlpha(.75); background: Item {}
                    tip: details.eventData.conference_url || ""; onClicked: backend.joinCalendarEvent(details.eventData)
                }
                Text { text: "· join"; color: Theme.inkAlpha(.45); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(13); Layout.fillWidth: true }
            }
            Text { visible: !!details.eventData.organizer; text: "Organized by " + (details.eventData.organizer || ""); color: Theme.inkAlpha(.45); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(11.5); textFormat: Text.PlainText; wrapMode: Text.Wrap; Layout.fillWidth: true }
            RowLayout {
                visible: (details.eventData.attendees || []).length > 0; Layout.fillWidth: true; spacing: 9
                FmIcon { name: "people"; size: 13; color: Theme.inkAlpha(.45); Layout.alignment: Qt.AlignTop; Layout.topMargin: 3 }
                Text { text: (details.eventData.attendees || []).map(function(a) { return (a.name || a.email) + (a.self ? " (you)" : "") }).join(", "); color: Theme.inkAlpha(.75); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(13); lineHeightMode: Text.FixedHeight; lineHeight: Math.round(Theme.fontSize(13) * 1.45); textFormat: Text.PlainText; wrapMode: Text.Wrap; Layout.fillWidth: true }
            }
            RowLayout {
                visible: details.conflicts.length > 0; Layout.fillWidth: true; spacing: 9
                Rectangle { Layout.alignment: Qt.AlignTop; Layout.topMargin: 6; implicitWidth: 7; implicitHeight: 7; radius: 3.5; color: "transparent"; border.width: 1.5; border.color: Theme.accent }
                Text {
                    text: "Overlaps " + details.conflicts.map(function(e) { return e.title + " (" + e.time_display + " – " + e.end_display + (e.account !== details.eventData.account ? ", " + e.account_name : "") + ")" }).join(", ")
                    color: Theme.accent; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(13); lineHeightMode: Text.FixedHeight; lineHeight: Math.round(Theme.fontSize(13) * 1.45); wrapMode: Text.Wrap; textFormat: Text.PlainText; Layout.fillWidth: true
                }
            }
            Text {
                visible: details.eventData.date === backend.todayDate && !details.eventData.multi_day
                property real start: details.eventData.start_hour || 0
                property real end: start + (details.eventData.duration_hours || 0)
                text: details.eventData.all_day ? "today" : backend.nowHour < start ? "in " + details.relative(start - backend.nowHour) : backend.nowHour < end ? "now · ends " + details.eventData.end_display : "ended " + details.eventData.end_display
                color: Theme.inkAlpha(.45); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(11.5); wrapMode: Text.Wrap; Layout.fillWidth: true
            }
            ScrollView {
                ScrollBar.vertical: FmScrollBar {}
                visible: !!details.eventData.description; Layout.fillWidth: true; Layout.preferredHeight: Math.min(100, description.implicitHeight + 4); clip: true
                TextArea { id: description; text: details.eventData.description || ""; color: Theme.muted; font.pointSize: Theme.fontPoints(12); textFormat: TextEdit.PlainText; wrapMode: TextEdit.Wrap; readOnly: true; selectByMouse: true; background: null; padding: 0 }
            }
            Text { visible: !!details.eventData.busy_only; text: "Availability only · details are not shared."; color: Theme.subtle; font.pointSize: Theme.fontPoints(11); wrapMode: Text.Wrap; Layout.fillWidth: true }
            RowLayout {
                visible: details.recurringScopes.length > 0; Layout.fillWidth: true; spacing: 8
                Text { text: "Apply to"; color: Theme.subtle; font.pointSize: Theme.fontPoints(11) }
                FmComboBox {
                    objectName: "eventRecurringScope"; Layout.fillWidth: true; implicitHeight: 30
                    model: details.recurringScopes.map(function(scope) { return {occurrence:"This occurrence",series:"Entire series",following:"This and following"}[scope] })
                    currentIndex: details.recurringScopes.indexOf(details.recurringScope)
                    enabled: !details.saving && !occurrenceRead.busy && !details.hasPendingResponse
                    onActivated: details.recurringScope = details.recurringScopes[currentIndex]
                }
            }
            Text { visible: !!details.eventData.occurrence_selector && !details.recurringScopes.length; text: "Changes apply to this occurrence only."; color: Theme.subtle; font.pointSize: Theme.fontPoints(11); wrapMode: Text.Wrap; Layout.fillWidth: true }
            Text { visible: !!details.eventData.read_only && !details.eventData.busy_only; text: "Read-only event"; color: Theme.subtle; font.pointSize: Theme.fontPoints(11) }
            ColumnLayout {
                visible: !!details.eventData.can_rsvp || details.scopedAllowed("rsvp"); spacing: 5; Layout.fillWidth: true
                Text { text: "Your response"; color: Theme.subtle; font.pointSize: Theme.fontPoints(11) }
                RowLayout {
                    spacing: 4
                    Repeater { model: [{label: "Accept", value: "accepted"}, {label: "Maybe", value: "tentative"}, {label: "Decline", value: "declined"}]
                        FmButton { required property var modelData; objectName: "eventRsvp-" + modelData.value; text: modelData.label; outlined: true; selected: details.eventData.self_response === modelData.value; implicitHeight: 26; font.pointSize: Theme.fontPoints(11); enabled: !details.saving && !occurrenceRead.busy && !details.awaitedEtag.length && details.accountReady && (!details.live || !details.hasPendingResponse); onClicked: details.respond(modelData.value) }
                    }
                }
            }
            Text { visible: details.live && (!!details.eventData.can_rsvp || details.scopedAllowed("rsvp")); text: "Your response is sent to Google. Google may notify the organizer; notification delivery is unverified."; color: Theme.subtle; font.pointSize: Theme.fontPoints(10); wrapMode: Text.Wrap; Layout.fillWidth: true }
            RowLayout {
                visible: details.live && details.hasPendingResponse; Layout.fillWidth: true; spacing: 6
                FmButton {
                    objectName: "eventRsvpResolve"; text: ["prepared", "ready"].indexOf(details.operationState) >= 0 ? "Resume" : details.operationState === "unknown" ? "Check outcome" : "Check status"; outlined: true
                    enabled: !details.saving && backend.connected && (["prepared", "ready", "unknown"].indexOf(details.operationState) < 0 || (details.operationState === "unknown" ? details.reconciliationReady : details.accountReady))
                    onClicked: details.resolveResponse(["prepared", "ready"].indexOf(details.operationState) >= 0 ? "resume" : details.operationState === "unknown" ? "reconcile" : "check")
                }
                FmButton { objectName: "eventRsvpAbandon"; text: "Abandon"; visible: details.operationRevision >= 0 && ["prepared", "ready", "unknown", "conflict", "rejected"].indexOf(details.operationState) >= 0; enabled: !details.saving && backend.connected; onClicked: responseAbandonConfirmation.open() }
            }
            Text { visible: details.feedback.length > 0; text: details.feedback; color: Theme.accent; font.pointSize: Theme.fontPoints(11); wrapMode: Text.Wrap; Layout.fillWidth: true; textFormat: Text.PlainText }
            Rectangle { Layout.fillWidth: true; implicitHeight: 1; color: Theme.line }
            Flow {
                Layout.fillWidth: true; Layout.minimumWidth:0; Layout.preferredWidth:1; spacing: 6
                FmButton { objectName: "eventEditButton"; visible: !!details.eventData.can_edit || details.scopedAllowed("edit") || (!details.recurringScopes.length && details.eventData.can_resolve_occurrence === true); text: occurrenceRead.busy ? "Checking…" : !details.recurringScopes.length && details.eventData.can_resolve_occurrence === true ? "Edit this occurrence" : "Edit"; prominent: true; implicitHeight: 28; leftPadding: 12; rightPadding: 12; font.pointSize: Theme.fontPoints(12); font.weight: Font.Medium; enabled: !details.saving && !occurrenceRead.busy && !details.awaitedEtag.length && details.accountReady && !details.hasPendingResponse; onClicked: { if (details.scopedAllowed("edit") || details.eventData.can_resolve_occurrence === true) details.readScoped("edit"); else { details.editRequested(details.eventData); details.close() } } }
                FmButton { objectName: "eventScopedCancel"; visible: details.live && (((details.eventData.recurring_scopes||{}).delete||[]).indexOf("occurrence")>=0 || details.scopedAllowed("delete") || (details.eventData.can_delete===true&&!details.recurringScopes.length)); text: details.recurringScope==="following"?"Delete following":details.recurringScopes.length?"Delete this":"Delete"; outlined:true;implicitHeight:28;leftPadding:12;rightPadding:12;font.pointSize:Theme.fontPoints(12);font.weight:Font.Medium;foreground:hovered?Theme.error:Theme.inkAlpha(.75);enabled:!details.saving&&!occurrenceRead.busy&&details.accountReady&&!details.hasPendingResponse;onClicked:details.deleteEvent(details.recurringScopes.length?(details.recurringScope==="following"?"following":"occurrence"):"") }
                FmButton { objectName:"eventDeleteSeries";visible:details.live&&((details.eventData.recurring_scopes||{}).delete||[]).indexOf("series")>=0;text:"Delete series";outlined:true;implicitHeight:28;leftPadding:12;rightPadding:12;font.pointSize:Theme.fontPoints(12);font.weight:Font.Medium;foreground:hovered?Theme.error:Theme.inkAlpha(.75);enabled:!details.saving&&!occurrenceRead.busy&&details.accountReady&&!details.hasPendingResponse;onClicked:details.deleteEvent("series") }
                FmButton { objectName: "eventDetailsClose"; visible:details.saving||(!details.eventData.can_edit&&!details.scopedAllowed("edit"));text: details.live && details.saving ? "Cancel request" : "Close"; implicitHeight: 28; font.pointSize: Theme.fontPoints(11); enabled: !details.saving || details.live; onClicked: { if (details.live && details.saving) backend.cancelGoogleOperation(details.pendingResponse.account, details.pendingResponse.request_id); else details.close() } }
            }
        }
    }
    FmConfirmDialog {
        id:deleteConfirmation;objectName:"eventDetailsDeleteConfirmation";actionObjectName:"confirmDetailsDelete"
        title:((details.deleteTarget.recurring_context||{}).scope==="series"?"Delete series":"Delete event")+"?"
        message:"Remove “"+(details.deleteTarget.title||"this event")+"”"+((details.deleteTarget.recurring_context||{}).scope==="series"?" and every occurrence":(details.deleteTarget.recurring_context||{}).scope==="occurrence"||details.deleteTarget.occurrence?" for this occurrence only":"")+" from Google Calendar? Google may notify guests. You can undo for a few seconds afterwards."
        actionText:"Delete";destructive:true
        onAccepted:details.submitDeletion()
    }
    FmConfirmDialog {
        id: responseAbandonConfirmation; objectName: "eventRsvpAbandonConfirmation"
        title: "Abandon recorded change?"
        message: "Abandon this recorded change. This does not undo a change already accepted by Google."
        actionText: "Abandon"
        onAccepted: details.resolveResponse("abandon")
    }
}
