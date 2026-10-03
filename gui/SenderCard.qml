// Contact popover for a sender or recipient name in MailView: name, address,
// a star for the contact, recent conversations, a filter link and Write. Opens on click or after a
// hover dwell (hoverSender / leaveSender); data comes from backend.senderCard.
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Effects

Popup {
    id: card
    objectName: "senderCard"
    parent: Overlay.overlay
    property string email: ""
    property string displayName: ""
    property string account: ""
    property real originX: 16
    property real originY: 64
    property var formatAge: function(value) { return "" }
    readonly property var cardData: backend.senderCard || ({})
    readonly property bool current: cardData.account === account && cardData.email === email
    readonly property var conversations: current && !cardData.loading && !cardData.error ? cardData.conversations || [] : []
    readonly property int total: current ? cardData.conversation_count || 0 : 0
    readonly property bool mine: email.toLowerCase() === (backend.currentAccount.email || "").toLowerCase()
    // A starred contact shows a star by their name across this account's mail
    // (preferences.starred_senders, lowercase addresses per account).
    readonly property string address: email.trim().toLowerCase()
    readonly property bool starred: (backend.preferences.starred_senders || []).some(function(entry) { return entry.account === account && entry.email === address })
    function toggleStar() {
        const list = (backend.preferences.starred_senders || []).filter(function(entry) { return !(entry.account === account && entry.email === address) })
        if (!starred) list.push({account: account, email: address})
        backend.setPreferences({starred_senders: list})
    }
    // Name, address and status can be selected. While one holds a selection
    // the card stays open until Esc or a click outside; Ctrl+C copies it.
    readonly property Item selectionField: nameField.selectedText ? nameField : emailField.selectedText ? emailField : statusField.selectedText ? statusField : null
    readonly property bool holding: selectionField !== null
    function clearSelection() { nameField.deselect(); emailField.deselect(); statusField.deselect() }
    signal conversationOpened(string id)
    width: Math.min(300, parent.width - 32); padding: 16; topPadding: 14; bottomPadding: 14
    height: Math.min(content.implicitHeight + 28, parent.height - 80)
    x: Math.max(16, Math.min(originX, parent.width - width - 16))
    y: Math.max(64, Math.min(originY, parent.height - height - 16))
    focus: false; modal: false
    closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
    background: Rectangle {
        color: Theme.surface; radius: 8; border.color: Theme.controlBorder
        layer.enabled: GraphicsInfo.api !== GraphicsInfo.Software
        layer.effect: MultiEffect { shadowEnabled: true; shadowColor: Theme.shadowAlpha(.6); shadowBlur: 0.8; shadowVerticalOffset: 12 }
        // An invisible 12px strip above the card counts as the card, so the
        // gap under the name never closes it on the way in.
        Item { objectName: "senderCardBridge"; y: -12; width: parent.width; height: 12; HoverHandler { onHoveredChanged: hovered ? card.stayOpen() : card.scheduleClose() } }
    }
    enter: Transition { NumberAnimation { property: "opacity"; from: 0; to: 1; duration: 150 } }
    // Opens at once for a click. Hover goes through hoverSender/leaveSender:
    // a short dwell before opening, a longer one before an open card switches
    // to another contact, and none while the pointer heads into the card, so
    // names passed on the way to it never steal it.
    function showSender(address, name, anchor) {
        if (!address || !backend.connected) return
        hideDelay.stop(); switchDelay.stop(); pending = null
        const canonical = address.trim().toLowerCase()
        const point = anchor.mapToItem(parent, 0, anchor.height + 4)
        originX = point.x; originY = point.y
        if (!opened || email !== canonical || account !== backend.account) {
            clearSelection()
            email = canonical; displayName = name || canonical; account = backend.account
            backend.requestSenderCard(canonical)
        }
        open()
    }
    property var pending: null
    property point lastPointer: Qt.point(-1, -1)
    function hoverSender(address, name, anchor) {
        if (!address || !backend.connected) return
        if (opened && email === address.trim().toLowerCase() && account === backend.account) { hideDelay.stop(); switchDelay.stop(); pending = null; return }
        // Passing over another name never takes away a selection being copied.
        if (holding) return
        pending = {address: address, name: name, anchor: anchor}
        switchDelay.interval = opened ? 350 : 200
        switchDelay.restart()
    }
    function leaveSender(anchor) {
        if (pending && pending.anchor === anchor) { switchDelay.stop(); pending = null }
        if (opened) scheduleClose()
    }
    // True while the pointer moves toward the open card: another name crossed
    // on the way keeps waiting instead of switching.
    function headingToCard(point) {
        if (!opened || lastPointer.x < 0) return false
        const dy = point.y - lastPointer.y, dx = point.x - lastPointer.x
        const above = point.y < y
        return above ? dy > 0 && Math.abs(dx) <= Math.max(dy * 3, 24) && point.x >= x - 40 && point.x <= x + width + 40 : false
    }
    function scheduleClose() { if (!holding) hideDelay.restart() }
    function stayOpen() { hideDelay.stop(); switchDelay.stop(); pending = null }
    function openConversation(id) { close(); conversationOpened(id) }
    // The search box shows this filter, so it can be read and removed there.
    // All mail is the scope the card's count covers; a bare address matches
    // the person as sender or on To or Cc, like the count.
    function filterSender() {
        const address = email
        close(); backend.folder = "all"; backend.query = address
    }
    function writeMessage() {
        const address = email, name = displayName
        close(); backend.writeToSender(address, name)
    }
    onClosed: { hideDelay.stop(); switchDelay.stop(); pending = null; lastPointer = Qt.point(-1, -1); clearSelection(); backend.clearSenderCard() }
    onHoldingChanged: if (holding) { hideDelay.stop(); switchDelay.stop(); pending = null }
    Shortcut { sequences: [StandardKey.Copy]; enabled: card.opened && card.holding; onActivated: card.selectionField.copy() }
    Connections { target: backend; function onAccountChanged() { card.close() } }
    Timer { id: hideDelay; interval: 220; onTriggered: if (!card.holding) card.close() }
    Timer {
        id: switchDelay
        onTriggered: {
            const next = card.pending
            if (!next || !next.anchor || !next.anchor.visible) return
            // Still travelling into the card: look again shortly.
            if (card.opened && card.pointerMoving) { restart(); return }
            card.showSender(next.address, next.name, next.anchor)
        }
    }
    // The pointer over the workspace while the card is open, reported by the
    // view that holds the card. Not a hover handler on the overlay: that takes
    // hover from the name under the pointer, which closed and reopened the
    // card in a loop.
    property bool pointerMoving: false
    function trackPointer(p) {
        pointerMoving = headingToCard(p)
        lastPointer = p
        settle.restart()
    }
    Timer { id: settle; interval: 120; onTriggered: card.pointerMoving = false }
    // Selectable, read-only text that wraps instead of eliding, so all of it
    // can be selected. It never takes keyboard focus from the workspace.
    component CardText: TextEdit {
        Layout.fillWidth: true
        readOnly: true; selectByMouse: true; activeFocusOnPress: false; persistentSelection: true
        textFormat: TextEdit.PlainText; wrapMode: TextEdit.Wrap
        selectionColor: Theme.selection; selectedTextColor: Theme.text
        font.family: Theme.fontFamily
        HoverHandler { cursorShape: Qt.IBeamCursor }
    }
    contentItem: ScrollView {
                ScrollBar.vertical: FmScrollBar {}
        property bool blocksWorkspaceShortcuts: true
        clip: true; contentWidth: availableWidth; ScrollBar.horizontal.policy: ScrollBar.AlwaysOff
        HoverHandler { onHoveredChanged: hovered ? card.stayOpen() : card.scheduleClose() }
        ColumnLayout {
            id: content; width: card.availableWidth; spacing: 12
            RowLayout {
                Layout.fillWidth: true; spacing: 12
                // A photo fills the circle, a brand icon sits at 66% on white;
                // otherwise initials on the name's hue.
                SenderAvatar {
                    objectName: "senderCardAvatar"
                    readonly property bool mine: card.mine
                    diameter: 36; brandSize: 24; own: mine
                    name: card.displayName; email: card.email
                    discColor: mine ? Theme.accentAlpha(0.18) : Theme.oklch(.36, .06, hue)
                    initialsColor: mine ? Theme.accent : Theme.oklch(.9, .05, hue)
                    initials: Theme.initialsOf(mine && backend.currentAccount.name ? backend.currentAccount.name : card.displayName || card.email)
                    fontPoints: Theme.fontPoints(12.5); fontWeight: Font.Medium
                }
                ColumnLayout {
                    Layout.fillWidth: true; spacing: 2
                    CardText { id: nameField; objectName: "senderCardName"; text: card.displayName; color: Theme.text; font.pointSize: Theme.fontPoints(14); font.weight: Font.Medium }
                    CardText { id: emailField; objectName: "senderCardEmail"; text: card.email; color: Theme.muted; font.pointSize: Theme.fontPoints(12.5); wrapMode: TextEdit.WrapAnywhere }
                }
                FmButton {
                    id: starButton; objectName: "senderCardStar"
                    visible: !card.mine && card.address.indexOf("@") > 0
                    Layout.alignment: Qt.AlignTop
                    implicitWidth: 28; implicitHeight: 28; leftPadding: 0; rightPadding: 0; topPadding: 0; bottomPadding: 0
                    iconName: card.starred ? "contactStarFilled" : "contactStar"; iconSize: 14
                    iconColor: card.starred ? Theme.amber : Theme.inkAlpha(.4)
                    tip: card.starred ? "Starred — click to remove" : "Star this contact"
                    background: Rectangle { radius: 4; color: starButton.hovered ? Theme.bg4 : "transparent" }
                    onClicked: card.toggleStar()
                }
            }
            CardText {
                id: statusField
                objectName: "senderCardStatus"
                font.pointSize: Theme.fontPoints(12)
                color: card.current && card.cardData.error ? Theme.error : Theme.muted
                text: !card.current || card.cardData.loading ? "Loading conversations…" : card.cardData.error || (card.total + (card.total === 1 ? " conversation" : " conversations") + " · " + (backend.currentAccount.name || card.account))
            }
            ColumnLayout {
                visible: card.conversations.length > 0; Layout.fillWidth: true; spacing: 0
                Rectangle { Layout.fillWidth: true; implicitHeight: 1; color: Theme.line }
                Repeater {
                    model: card.conversations
                    ItemDelegate {
                        required property var modelData
                        objectName: "senderCardRow_" + modelData.id
                        Layout.fillWidth: true; implicitHeight: Math.max(30, row.implicitHeight + 12); leftPadding: 0; rightPadding: 0
                        background: Rectangle { color: parent.hovered ? Theme.selected : "transparent"; radius: 3 }
                        contentItem: RowLayout {
                            id: row; spacing: 10
                            Text { text: Theme.oneLine(modelData.subject); maximumLineCount: 1; Layout.fillWidth: true; elide: Text.ElideRight; color: modelData.unread ? Theme.text : Theme.secondary; font.pointSize: Theme.fontPoints(12.5); font.weight: modelData.unread ? Font.Medium : Font.Normal; textFormat: Text.PlainText }
                            Text { text: card.formatAge(modelData.latest_at); color: Theme.subtle; font.pointSize: Theme.fontPoints(11.5) }
                        }
                        onClicked: card.openConversation(modelData.id)
                    }
                }
                FmButton {
                    objectName: "senderCardFilter"; Layout.fillWidth: true; textAlignment: Text.AlignLeft; leftPadding: 0; rightPadding: 0
                    text: (card.total > 3 ? "All " + card.total + " with " : "Filter to ") + card.displayName.split(" ")[0] + " →"
                    font.pointSize: Theme.fontPoints(12); foreground: Theme.accent
                    onHoveredChanged: if (hovered) backend.prefetchSearch("all", card.email)
                    onClicked: card.filterSender()
                }
            }
            FmButton {
                objectName: "senderCardWrite"; text: "Write"; outlined: true; implicitHeight: 28; font.pointSize: Theme.fontPoints(12)
                enabled: backend.connected && !backend.draftBusy && !backend.accountArchived && !backend.unsavedAccountBuffer
                onClicked: card.writeMessage()
            }
        }
    }
}
