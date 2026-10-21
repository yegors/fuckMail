import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

// Notices above a message's text: a calendar invitation, the mailing-list row
// and the hidden-images row, in that order, one group between thin rules with
// no fill. Only rows that apply show, and the group hides when none does.
ColumnLayout {
    id: notices
    objectName: "messageNotices_" + (message.id || "")
    property var message: ({})
    // The conversation's mailing_list, given only for its latest received message.
    property var mailingList: null
    // How this message unsubscribes ({method, url?, address?, subject?, body?,
    // mailto?}), from its source. One-click carries the list's email as
    // `mailto` ({address, subject, body}) for when the list refuses the POST.
    property var unsubscribe: null
    // The HTML reader while HTML shows; it owns the images.
    property Item htmlBody: null
    property bool unsubscribing: false
    property string unsubscribeError: ""
    property string ticket: ""
    readonly property string sender: message.sender_name || message.sender_email || ""
    readonly property var unsubscribed: mailingList ? mailingList.unsubscribed || null : null
    readonly property string method: unsubscribe ? unsubscribe.method || "" : ""
    // The email an unsubscribe would send, and whether it replaces a refused one-click.
    readonly property var unsubscribeEmail: !unsubscribe ? null : method === "mailto" ? unsubscribe : unsubscribe.mailto || null
    property bool emailFallback: false
    readonly property bool showList: !!mailingList && method.length > 0
    readonly property bool showImages: !!htmlBody && htmlBody.shown && (htmlBody.hiddenCount > 0 || htmlBody.proxyFailed || htmlBody.imagesShown)
    // Whether the message carries a calendar invitation (from its source).
    property bool hasInvite: false
    property var invite: null
    property string inviteTicket: ""
    // An answer given here, shown until the calendar has it.
    property string pendingAnswer: ""
    property string answerRequest: ""
    property string answerError: ""
    readonly property var inviteEvent: invite && invite.event ? invite.event : null
    readonly property bool showInvite: hasInvite && !!invite && invite.method !== "REPLY"
    // Declining waits out its Undo before reaching Google; it reads as declined meanwhile.
    readonly property bool declining: { backend.events; return !!inviteEvent && backend.calendarEventDeclining(inviteEvent) }
    readonly property var answer: declining ? "no" : pendingAnswer.length ? pendingAnswer : inviteEvent ? Theme.rsvpOf(inviteEvent) : undefined
    readonly property bool invited: !!inviteEvent && Theme.rsvpOf(inviteEvent) !== undefined && invite.method !== "CANCEL"
    // Answers show for an invitation to you, dimmed until its event has synced.
    readonly property bool answerable: showInvite && invite.method !== "CANCEL" && (!inviteEvent || invited)
    signal openInvite(var event, bool declined)
    visible: showInvite || showList || showImages
    spacing: 0

    function loadInvite() {
        if (!hasInvite || !backend.connected || !message.id || !message.source_version || inviteTicket.length) return
        inviteTicket = backend.requestMessageInvite(message.id, message.source_version)
    }
    function respond(value) {
        const event = inviteEvent
        if (!event || !event.can_rsvp || answerRequest.length || declining) return
        const response = value === "yes" ? "accepted" : value === "maybe" ? "tentative" : "declined"
        const request = {account: event.account, kind: "rsvp", calendar_id: event.calendar_id, event_id: event.event_id, expected_etag: event.etag,
            response: response, request_id: "gui-rsvp-" + Date.now() + "-" + Math.floor(Math.random() * 1000000000)}
        if (event.recurring_context) request.recurring = event.recurring_context
        answerError = ""
        // No hides the event at once with Undo, as from the event card.
        if (value === "no" && !backend.syntheticProfile) {
            const error = backend.queueCalendarDecline(request, event)
            if (error.length) answerError = error
            return
        }
        pendingAnswer = value; answerRequest = request.request_id
        backend.calendarWrite(request)
    }
    onHasInviteChanged: loadInvite()
    Timer { id: inviteReload; interval: 500; onTriggered: notices.loadInvite() }

    // An unsubscribe email goes out from the person's own account, so it waits
    // for them to see where it goes, and what it says.
    function start() {
        if (unsubscribing || unsubscribed) return
        if (method === "mailto") { emailFallback = false; mailtoConfirmation.open(); return }
        send(false)
    }
    function send(byEmail) {
        if (unsubscribing || unsubscribed) return
        unsubscribing = true; unsubscribeError = ""
        const request = {id: message.id, source_version: message.source_version, method: byEmail ? "mailto" : method}
        if (byEmail) { request.address = unsubscribeEmail.address; request.fallback = emailFallback }
        ticket = backend.unsubscribe(request)
    }
    onMessageChanged: {
        unsubscribing = false; unsubscribeError = ""; ticket = ""; emailFallback = false
        invite = null; inviteTicket = ""; pendingAnswer = ""; answerRequest = ""; answerError = ""; loadInvite()
    }
    Connections {
        target: backend
        function onUnsubscribeReady(ticket, result, ok) {
            if (ticket !== notices.ticket) return
            notices.ticket = ""; notices.unsubscribing = false
            // The list refused one-click but takes an email: ask before sending it.
            if (!ok && result.code === "unsubscribe_by_email" && notices.unsubscribeEmail && notices.unsubscribeEmail.address) {
                notices.emailFallback = true; mailtoConfirmation.open(); return
            }
            if (!ok) notices.unsubscribeError = result.message || "Could not unsubscribe. Try again later."
        }
    }
    Connections {
        target: backend
        ignoreUnknownSignals: true
        function onMessageInviteReady(ticket, result, ok) {
            if (ticket !== notices.inviteTicket) return
            notices.inviteTicket = ""
            if (!ok) return
            notices.invite = result.invite || null
            const event = notices.invite ? notices.invite.event : null
            if (notices.pendingAnswer.length && !notices.answerRequest.length && event && Theme.rsvpOf(event) === notices.pendingAnswer) notices.pendingAnswer = ""
        }
        // The calendar changed: the answer, time or event may have moved.
        function onEventsChanged() { if (notices.hasInvite && notices.visible) inviteReload.restart() }
        function onConnectedChanged() { notices.inviteTicket = ""; if (backend.connected) notices.loadInvite() }
        function onCalendarWriteFinished(ok, result, error) {
            if (result.request_id !== notices.answerRequest || !notices.answerRequest.length) return
            notices.answerRequest = ""
            if (!ok) { notices.pendingAnswer = ""; notices.answerError = error || "Your answer was not sent. Open the event to try again." }
            inviteReload.restart()
        }
    }

    component Rule: Rectangle { Layout.fillWidth: true; implicitHeight: Theme.ruleWidth; color: Theme.line }
    // A small text button that highlights on hover. Its width comes from its
    // label alone, never from the width the layout gives it, so it cannot
    // collapse; it keeps its full width unless a row sets a smaller minimum.
    component NoticeButton: Rectangle {
        id: button
        property alias text: label.text
        property string iconName: ""
        property string tip: ""
        signal clicked()
        readonly property real iconSpace: iconName.length > 0 ? 18 : 0
        implicitWidth: Math.ceil(label.implicitWidth) + iconSpace + 16; implicitHeight: 26; radius: 5
        Layout.minimumWidth: implicitWidth; Layout.maximumWidth: implicitWidth
        color: area.containsMouse ? Theme.bg4 : "transparent"
        Accessible.role: Accessible.Button; Accessible.name: label.text; Accessible.description: tip; Accessible.onPressAction: clicked()
        FmIcon { visible: button.iconName.length > 0; x: 8; anchors.verticalCenter: parent.verticalCenter; name: button.iconName; size: 12; color: label.color }
        Text {
            id: label; x: 8 + button.iconSpace; anchors.verticalCenter: parent.verticalCenter
            width: Math.max(0, button.width - 16 - button.iconSpace); elide: Text.ElideRight; textFormat: Text.PlainText
            color: area.containsMouse ? Theme.text : Theme.inkAlpha(.85); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12)
        }
        MouseArea { id: area; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: button.clicked() }
        FmToolTip { shown: area.containsMouse && button.tip.length > 0; text: button.tip }
    }
    component NoticeText: Text {
        Layout.fillWidth: true; Layout.minimumWidth: 0; Layout.preferredWidth: 0; elide: Text.ElideRight; textFormat: Text.PlainText
        color: Theme.inkAlpha(.55); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12)
    }

    Rule {}
    // The invitation: its time (and the time it moved from), your answer, and
    // Yes / Maybe / No. The time opens the event in Calendar's Day view.
    RowLayout {
        id: inviteRow
        objectName: "inviteRow_" + (notices.message.id || "")
        visible: notices.showInvite
        Layout.fillWidth: true; Layout.minimumHeight: 40; spacing: 10
        readonly property string when: !notices.invite ? "" : backend.inviteWhen(notices.inviteEvent ? notices.inviteEvent.start : notices.invite.start, notices.inviteEvent ? notices.inviteEvent.end : notices.invite.end)
        readonly property string was: notices.invite && notices.invite.was_start ? "was " + backend.inviteWhen(notices.invite.was_start, notices.invite.was_end || "", false) : ""
        FmIcon { name: "calendar"; size: 13; color: Theme.accent }
        Item {
            objectName: "inviteWhen_" + (notices.message.id || "")
            Layout.fillWidth: true; Layout.minimumWidth: 0; implicitHeight: whenRow.implicitHeight
            Row {
                id: whenRow; spacing: 8; width: parent.width
                Text {
                    id: whenText
                    text: inviteRow.when; textFormat: Text.PlainText; color: Theme.text
                    font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12); font.features: {"tnum": 1}
                }
                Text {
                    objectName: "inviteWas_" + (notices.message.id || "")
                    visible: inviteRow.was.length > 0
                    width: Math.max(0, Math.min(implicitWidth, whenRow.width - whenText.width - 8))
                    text: inviteRow.was; textFormat: Text.PlainText; elide: Text.ElideRight; font.strikeout: true
                    color: Theme.inkAlpha(.35); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12); font.features: {"tnum": 1}
                }
            }
            HoverHandler { id: whenHover; enabled: !!notices.inviteEvent; cursorShape: Qt.PointingHandCursor }
            TapHandler { enabled: !!notices.inviteEvent; onTapped: notices.openInvite(notices.inviteEvent, notices.answer === "no") }
            FmToolTip { shown: whenHover.hovered; text: "Open in Calendar" }
        }
        Text {
            objectName: "inviteState_" + (notices.message.id || "")
            visible: text.length > 0
            text: notices.invite && notices.invite.method === "CANCEL" ? "Cancelled" : notices.answerError.length ? "Not sent"
                : !notices.invited ? "" : notices.answer === "no" ? "Declined" : notices.answer === "maybe" ? "Maybe" : notices.answer === "yes" ? "Going" : ""
            color: notices.answerError.length ? Theme.error : Theme.inkAlpha(.45); textFormat: Text.PlainText
            font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(11)
            HoverHandler { id: stateHover; enabled: notices.answerError.length > 0 }
            FmToolTip { shown: stateHover.hovered; text: notices.answerError }
        }
        Rectangle {
            id: answerBox
            objectName: "inviteAnswers_" + (notices.message.id || "")
            visible: notices.answerable
            implicitWidth: answers.implicitWidth + 6; implicitHeight: answers.implicitHeight + 6; radius: 5
            color: "transparent"; border.width: 1; border.color: Theme.line2
            readonly property bool ready: !!notices.inviteEvent && notices.inviteEvent.can_rsvp === true && !notices.answerRequest.length && !notices.declining
            opacity: ready ? 1 : .5
            // Disabled buttons take no hover, so the box explains why.
            HoverHandler { id: answersHover; enabled: !answerBox.ready }
            FmToolTip { shown: answersHover.hovered; text: !notices.inviteEvent ? "Waiting for this event to reach your calendar" : notices.declining ? "Declining · you can still Undo" : notices.answerRequest.length ? "Sending your answer…" : "Answer from the event: click the time" }
            Row {
                id: answers; x: 3; y: 3; spacing: 2
                Repeater {
                    model: [{label: "Yes", value: "yes", tip: "Accept"}, {label: "Maybe", value: "maybe", tip: "Tentative"}, {label: "No", value: "no", tip: "Decline — removes it from your calendar"}]
                    FmButton {
                        required property var modelData
                        objectName: "inviteAnswer-" + modelData.value
                        readonly property bool chosen: notices.answer === modelData.value
                        text: modelData.label
                        tip: modelData.tip
                        selected: chosen; implicitHeight: 22; leftPadding: 10; rightPadding: 10; topPadding: 0; bottomPadding: 0
                        font.pointSize: Theme.fontPoints(11.5); font.weight: Font.Medium
                        foreground: chosen ? (modelData.value === "yes" ? Theme.bg : Theme.text) : hovered ? Theme.text : Theme.inkAlpha(.6)
                        background: Rectangle { radius: 5; color: parent.chosen ? (parent.modelData.value === "yes" ? Theme.text : Theme.bg6) : "transparent" }
                        enabled: answerBox.ready
                        onClicked: notices.respond(modelData.value)
                    }
                }
            }
        }
    }
    Rule { visible: notices.showInvite && (notices.showList || notices.showImages) }
    RowLayout {
        objectName: "unsubscribeRow_" + (notices.message.id || "")
        visible: notices.showList
        Layout.fillWidth: true; Layout.preferredHeight: 36; spacing: 10
        FmIcon { name: notices.unsubscribed ? "check" : "mail"; size: 13; color: notices.unsubscribed ? "#7FC4A4" : Theme.inkAlpha(.45) }
        NoticeText {
            objectName: "unsubscribeText_" + (notices.message.id || "")
            color: notices.unsubscribeError && !notices.unsubscribed ? Theme.error : Theme.inkAlpha(.55)
            text: notices.unsubscribed ? (notices.unsubscribed.method === "link" ? "Unsubscribe page opened in your browser" : "Unsubscribed from " + notices.sender + " · it can take a few days to stop")
                : notices.unsubscribeError ? notices.unsubscribeError
                : notices.mailingList ? "Mailing list · " + notices.mailingList.month + " this month, " + notices.mailingList.year + " this year" : ""
        }
        NoticeButton {
            objectName: "unsubscribeButton_" + (notices.message.id || "")
            visible: !notices.unsubscribed
            iconName: notices.method === "link" ? "external" : "mail"
            text: notices.unsubscribing ? "Unsubscribing…" : "Unsubscribe"
            tip: notices.method === "link" ? "Opens " + notices.sender + "’s unsubscribe page in your browser"
                : notices.method === "mailto" ? "Sends an unsubscribe email to " + (notices.unsubscribe.address || "the list") + " (from the List-Unsubscribe header)"
                : "Unsubscribes in the background via the List-Unsubscribe header — no page to visit"
            onClicked: notices.start()
        }
    }
    Rule { visible: notices.showList && notices.showImages }
    RowLayout {
        id: imageRow
        objectName: "htmlImageBar_" + (notices.message.id || "")
        visible: notices.showImages
        Layout.fillWidth: true; Layout.preferredHeight: 36; spacing: 10
        readonly property bool proxyFailed: !!notices.htmlBody && notices.htmlBody.proxyFailed
        readonly property bool loaded: !!notices.htmlBody && notices.htmlBody.imagesShown
        readonly property bool allowed: !!notices.htmlBody && notices.htmlBody.senderAllowed
        // Always-load is on, but Gmail could not verify this sender (or it is Spam).
        readonly property bool held: allowed && !notices.htmlBody.senderVerified
        readonly property int hidden: notices.htmlBody ? notices.htmlBody.hiddenCount : 0
        readonly property int total: notices.htmlBody ? notices.htmlBody.imageCount : 0
        readonly property string from: notices.sender || "the sender"
        function count(n) { return n === 1 ? "1 image" : n + " images" }
        FmIcon { name: imageRow.loaded ? "image" : "imageOff"; size: 13; color: Theme.inkAlpha(.45) }
        NoticeText {
            objectName: "htmlImageText_" + (notices.message.id || "")
            text: imageRow.proxyFailed ? "Proxy unavailable — images not loaded"
                : imageRow.loaded ? (imageRow.allowed ? "Images from " + imageRow.from + " load automatically" : "Images loaded for this message only")
                : imageRow.held ? imageRow.count(imageRow.hidden) + " hidden: they load automatically only when Gmail verifies the sender and it isn’t Spam"
                : imageRow.count(imageRow.hidden) + " hidden so " + imageRow.from + " can’t tell you opened this"
        }
        NoticeButton {
            objectName: "htmlLoadImages_" + (notices.message.id || "")
            visible: !imageRow.loaded
            text: imageRow.proxyFailed ? "Retry" : "Load images"; tip: "Load images in this message only"
            onClicked: notices.htmlBody.loadImages()
        }
        Row {
            objectName: "htmlImagesLoaded_" + (notices.message.id || "")
            visible: imageRow.loaded
            Layout.leftMargin: 8; Layout.rightMargin: allowImages.visible ? 8 : 0; spacing: 6
            Layout.minimumWidth: implicitWidth
            FmIcon { name: "check"; size: 11; color: "#7FC4A4"; anchors.verticalCenter: parent.verticalCenter }
            Text {
                anchors.verticalCenter: parent.verticalCenter; textFormat: Text.PlainText
                text: imageRow.count(imageRow.total) + " loaded"
                color: "#7FC4A4"; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12)
            }
        }
        NoticeButton {
            id: allowImages
            objectName: "htmlAllowImages_" + (notices.message.id || "")
            visible: !imageRow.proxyFailed && !!notices.message.sender_email
            // The long sender name may elide; the row text gives way first.
            Layout.minimumWidth: Math.min(implicitWidth, 120); Layout.preferredWidth: implicitWidth
            text: (imageRow.allowed ? "Stop always loading from " : "Always load images from ") + notices.sender
            tip: imageRow.allowed ? "Ask again before loading images from this sender" : "Load images automatically in every message from this sender"
            onClicked: imageRow.allowed ? notices.htmlBody.disallowSender() : notices.htmlBody.allowSender()
        }
    }
    Rule {}
    FmConfirmDialog {
        id: mailtoConfirmation; objectName: "unsubscribeConfirmation_" + (notices.message.id || "")
        readonly property var email: notices.unsubscribeEmail || ({})
        readonly property string body: (email.body || "").trim()
        title: "Send an unsubscribe email?"
        message: (notices.emailFallback ? "The list did not accept the one-click unsubscribe. " : "")
            + "This sends an email from your account to " + (email.address || "the list")
            + ". Gmail verified the sender's domain, and this address belongs to it."
            + "\n\nSubject: " + (email.subject || "unsubscribe")
            + "\nMessage: " + (body.length > 400 ? body.slice(0, 400) + "\u2026" : body || "unsubscribe")
        actionText: "Send"; actionObjectName: "unsubscribeConfirmSend_" + (notices.message.id || "")
        onAccepted: notices.send(true)
        onRejected: if (notices.emailFallback) notices.unsubscribeError = "The list did not accept the one-click unsubscribe. Nothing was sent."
    }
}
