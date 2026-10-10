import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Effects

// The hover card for a link in a message: its domain, warnings, the full URL
// with the domain emphasised so lookalikes stand out, how often this account
// has seen the domain, and Copy / Block. Matching and blocking belong to the
// HTML reader's content service.
Popup {
    id: card
    objectName: "linkCard"
    parent: Overlay.overlay
    property string url: ""
    property string conversation: ""
    property bool copied: false
    readonly property var service: backend.htmlContent || null
    readonly property int revision: service ? service.revision : 0
    readonly property string host: service ? service.hostOf(url) : ""
    readonly property string blockedBy: service && revision >= 0 && host ? service.blockedBy(host) : ""
    readonly property bool insecure: /^http:/i.test(url)
    readonly property var parts: {
        const match = /^(https?:\/\/)(www\.)?([^\/?#]+)(.*)$/i.exec(url)
        return match ? {pre: match[1] + (match[2] || ""), host: match[3], rest: match[4]} : {pre: "", host: url, rest: ""}
    }
    readonly property var seen: service && revision >= 0 && host && opened ? service.linkSummary(host, conversation) : ({loading: true})
    width: Math.min(360, parent ? parent.width - 24 : 360)
    padding: 0; topPadding: 12; bottomPadding: 12; leftPadding: 14; rightPadding: 14
    closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
    // Shown for a link at `box` (overlay coordinates): below it, or above it
    // near the bottom of the window, never past either side.
    function show(link, box, thread) {
        lingerTimer.stop()
        url = link; conversation = thread || ""; copied = false
        const below = box.y + box.height + 200 < parent.height
        x = Math.max(12, Math.min(box.x, parent.width - width - 12))
        y = below ? box.y + box.height + 6 : Math.max(12, box.y - 6 - implicitHeight)
        open()
    }
    // Hover opens the card after 200 ms, cancelled if the pointer leaves first;
    // coming back to the link whose card is open keeps it open.
    property var pendingLink: null
    function hover(link, box, thread) {
        lingerTimer.stop()
        if (opened && url === link) { openTimer.stop(); pendingLink = null; return }
        pendingLink = {link: link, box: box, thread: thread}
        openTimer.restart()
    }
    Timer { id: openTimer; interval: 200; onTriggered: { const next = card.pendingLink; card.pendingLink = null; if (next) card.show(next.link, next.box, next.thread) } }
    // A 200 ms linger lets the pointer travel onto the card.
    function scheduleHide() { openTimer.stop(); pendingLink = null; lingerTimer.restart() }
    Timer { id: lingerTimer; interval: 200; onTriggered: if (!cardHover.hovered) card.close() }
    enter: Transition { NumberAnimation { property: "opacity"; from: 0; to: 1; duration: Theme.ms(120); easing.type: Easing.OutCubic } }
    background: Rectangle {
        color: Theme.bg2; radius: 8; border.color: Theme.line2
        layer.enabled: GraphicsInfo.api !== GraphicsInfo.Software
        layer.effect: MultiEffect { shadowEnabled: true; shadowColor: Theme.shadowAlpha(.55); shadowBlur: .8; shadowVerticalOffset: 18 }
    }
    contentItem: ColumnLayout {
        spacing: 8
        HoverHandler { id: cardHover; onHoveredChanged: if (!hovered) card.scheduleHide() }
        RowLayout {
            Layout.fillWidth: true; spacing: 8
            FmIcon { name: "globe"; size: 13; color: Theme.inkAlpha(.5) }
            Text { Layout.fillWidth: true; Layout.minimumWidth: 0; text: card.host; elide: Text.ElideRight; color: Theme.text; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12.5); font.weight: Font.Medium; textFormat: Text.PlainText }
            Rectangle {
                objectName: "linkCardInsecure"; visible: card.insecure
                implicitWidth: insecureText.implicitWidth + 12; implicitHeight: insecureText.implicitHeight + 2; radius: 5
                color: "transparent"; border.color: Theme.withAlpha(Theme.warn, .4)
                Text { id: insecureText; anchors.centerIn: parent; text: "Not secure"; color: Theme.warn; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(10.5) }
                HoverHandler { id: insecureHover }
                FmToolTip { shown: insecureHover.hovered; text: "This link is not encrypted" }
            }
            Rectangle {
                objectName: "linkCardBlocked"; visible: !!card.blockedBy
                implicitWidth: blockedText.implicitWidth + 12; implicitHeight: blockedText.implicitHeight + 2; radius: 5
                color: "transparent"; border.color: Theme.withAlpha(Theme.red, .4)
                Text { id: blockedText; anchors.centerIn: parent; text: card.blockedBy && card.blockedBy !== card.host ? "Blocked by " + card.blockedBy : "Blocked"; color: Theme.red; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(10.5); textFormat: Text.PlainText }
            }
        }
        Text {
            objectName: "linkCardUrl"
            Layout.fillWidth: true; wrapMode: Text.WrapAnywhere; textFormat: Text.StyledText; lineHeight: 1.5
            text: "<font color=\"" + Theme.inkAlpha(.45) + "\">" + card.htmlText(card.parts.pre) + "</font>" + card.htmlText(card.parts.host)
                + "<font color=\"" + Theme.inkAlpha(.45) + "\">" + card.htmlText(card.parts.rest) + "</font>"
            color: Theme.text; font.family: backend.fixedFontFamily(); font.pointSize: Theme.fontPoints(11)
        }
        Rectangle { Layout.fillWidth: true; implicitHeight: 1; color: Theme.line }
        RowLayout {
            Layout.fillWidth: true; spacing: 12
            Text {
                objectName: "linkCardSeen"; Layout.fillWidth: true; Layout.minimumWidth: 0; elide: Text.ElideRight
                readonly property int count: card.seen.messages || 0
                text: !card.seen.complete ? "Still indexing this account" : count ? "Seen in " + count + (count === 1 ? " other email" : " other emails") : "First time seeing this domain"
                color: card.seen.complete && !count ? Theme.warn : Theme.inkAlpha(.55)
                font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(11.5)
            }
            LinkCardAction { objectName: "linkCardCopy"; text: card.copied ? "Copied" : "Copy link"; onClicked: { backend.copyText(card.url); card.copied = true } }
            LinkCardAction {
                objectName: "linkCardBlock"; visible: !card.blockedBy && !!card.service; text: "Block domain"; idle: Theme.red; active: Theme.red
                onClicked: card.service.block(card.host, "link")
            }
            LinkCardAction { objectName: "linkCardUnblock"; visible: !!card.blockedBy && !!card.service; text: "Unblock"; onClicked: card.service.unblock(card.blockedBy) }
        }
    }
    function htmlText(text) { return String(text).replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;") }
    component LinkCardAction: AbstractButton {
        id: action
        property color idle: Theme.inkAlpha(.7)
        property color active: Theme.text
        implicitWidth: actionText.implicitWidth; implicitHeight: actionText.implicitHeight
        hoverEnabled: true
        HoverHandler { cursorShape: Qt.PointingHandCursor }
        contentItem: Text { id: actionText; text: action.text; color: action.hovered ? action.active : action.idle; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(11.5) }
        background: null
    }
}
