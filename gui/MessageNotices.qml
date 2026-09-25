import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

// Notices above a message's text: the mailing-list row and the hidden-images
// row, one group between thin rules with no fill. Only rows that apply show,
// and the group hides when neither does.
ColumnLayout {
    id: notices
    objectName: "messageNotices_" + (message.id || "")
    property var message: ({})
    // The conversation's mailing_list, given only for its latest received message.
    property var mailingList: null
    // How this message unsubscribes ({method, url?, address?}), from its source.
    property var unsubscribe: null
    // The HTML reader while HTML shows; it owns the images.
    property Item htmlBody: null
    property bool unsubscribing: false
    property string unsubscribeError: ""
    property string ticket: ""
    readonly property string sender: message.sender_name || message.sender_email || ""
    readonly property var unsubscribed: mailingList ? mailingList.unsubscribed || null : null
    readonly property string method: unsubscribe ? unsubscribe.method || "" : ""
    readonly property bool showList: !!mailingList && method.length > 0
    readonly property bool showImages: !!htmlBody && htmlBody.shown && (htmlBody.hiddenCount > 0 || htmlBody.proxyFailed || htmlBody.imagesShown)
    visible: showList || showImages
    spacing: 0

    function start() {
        if (unsubscribing || unsubscribed) return
        unsubscribing = true; unsubscribeError = ""
        ticket = backend.unsubscribe({id: message.id, source_version: message.source_version, method: method})
    }
    onMessageChanged: { unsubscribing = false; unsubscribeError = ""; ticket = "" }
    Connections {
        target: backend
        function onUnsubscribeReady(ticket, result, ok) {
            if (ticket !== notices.ticket) return
            notices.ticket = ""; notices.unsubscribing = false
            if (!ok) notices.unsubscribeError = result.message || "Could not unsubscribe. Try again later."
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
        implicitWidth: Math.ceil(label.implicitWidth) + iconSpace + 16; implicitHeight: 26; radius: 4
        Layout.minimumWidth: implicitWidth; Layout.maximumWidth: implicitWidth
        color: area.containsMouse ? Theme.bg4 : "transparent"
        Accessible.role: Accessible.Button; Accessible.name: label.text; Accessible.description: tip; Accessible.onPressAction: clicked()
        FmIcon { visible: button.iconName.length > 0; x: 8; anchors.verticalCenter: parent.verticalCenter; name: button.iconName; size: 12; color: label.color }
        Text {
            id: label; x: 8 + button.iconSpace; anchors.verticalCenter: parent.verticalCenter
            width: Math.max(0, button.width - 16 - button.iconSpace); elide: Text.ElideRight; textFormat: Text.PlainText
            color: area.containsMouse ? Theme.text : Theme.inkAlpha(.85); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12.5)
        }
        MouseArea { id: area; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: button.clicked() }
        FmToolTip { shown: area.containsMouse && button.tip.length > 0; text: button.tip }
    }
    component NoticeText: Text {
        Layout.fillWidth: true; Layout.minimumWidth: 0; Layout.preferredWidth: 0; elide: Text.ElideRight; textFormat: Text.PlainText
        color: Theme.inkAlpha(.55); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12.5)
    }

    Rule {}
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
        readonly property int hidden: notices.htmlBody ? notices.htmlBody.hiddenCount : 0
        readonly property int total: notices.htmlBody ? notices.htmlBody.imageCount : 0
        readonly property string from: notices.sender || "the sender"
        function count(n) { return n === 1 ? "1 image" : n + " images" }
        FmIcon { name: imageRow.loaded ? "image" : "imageOff"; size: 13; color: Theme.inkAlpha(.45) }
        NoticeText {
            objectName: "htmlImageText_" + (notices.message.id || "")
            text: imageRow.proxyFailed ? "Proxy unavailable — images not loaded"
                : imageRow.loaded ? (imageRow.allowed ? "Images from " + imageRow.from + " load automatically" : "Images loaded for this message only")
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
                color: "#7FC4A4"; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12.5)
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
}
