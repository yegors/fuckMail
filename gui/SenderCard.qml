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
    }
    enter: Transition { NumberAnimation { property: "opacity"; from: 0; to: 1; duration: 150 } }
    function showSender(address, name, anchor) {
        if (!address || !backend.connected) return
        hideDelay.stop()
        const canonical = address.trim().toLowerCase()
        const point = anchor.mapToItem(parent, 0, anchor.height + 8)
        originX = point.x; originY = point.y
        if (!opened || email !== canonical || account !== backend.account) {
            email = canonical; displayName = name || canonical; account = backend.account
            backend.requestSenderCard(canonical)
        }
        open()
    }
    function scheduleClose() { hideDelay.restart() }
    function stayOpen() { hideDelay.stop() }
    function openConversation(id) { close(); conversationOpened(id) }
    function filterSender() {
        const filters = Object.assign({}, backend.filters, {exact_sender: email})
        close(); backend.applyFilters(filters)
    }
    function writeMessage() {
        const address = email, name = displayName
        close(); backend.writeToSender(address, name)
    }
    onClosed: { hideDelay.stop(); backend.clearSenderCard() }
    Connections { target: backend; function onAccountChanged() { card.close() } }
    Timer { id: hideDelay; interval: 220; onTriggered: card.close() }
    contentItem: ScrollView {
        property bool blocksWorkspaceShortcuts: true
        clip: true; contentWidth: availableWidth; ScrollBar.horizontal.policy: ScrollBar.AlwaysOff
        HoverHandler { onHoveredChanged: hovered ? card.stayOpen() : card.scheduleClose() }
        ColumnLayout {
            id: content; width: card.availableWidth; spacing: 12
            RowLayout {
                Layout.fillWidth: true; spacing: 12
                Rectangle {
                    implicitWidth: 36; implicitHeight: 36; radius: 18; color: Theme.hover
                    Text { anchors.centerIn: parent; text: card.displayName.split(/\s+/).slice(0, 2).map(function(part) { return part.charAt(0) }).join(""); color: Theme.text; font.pixelSize: Theme.fontSize(12.5); font.weight: Font.Medium }
                }
                ColumnLayout {
                    Layout.fillWidth: true; spacing: 2
                    Text { text: card.displayName; Layout.fillWidth: true; color: Theme.text; font.pixelSize: Theme.fontSize(14); font.weight: Font.Medium; elide: Text.ElideRight; textFormat: Text.PlainText }
                    Text { text: card.email; Layout.fillWidth: true; color: Theme.muted; font.pixelSize: Theme.fontSize(12.5); elide: Text.ElideRight; textFormat: Text.PlainText }
                }
            }
            Text {
                objectName: "senderCardStatus"
                Layout.fillWidth: true; wrapMode: Text.Wrap; textFormat: Text.PlainText; font.pixelSize: Theme.fontSize(12)
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
                            Text { text: modelData.subject; Layout.fillWidth: true; elide: Text.ElideRight; color: modelData.unread ? Theme.text : Theme.secondary; font.pixelSize: Theme.fontSize(12.5); font.weight: modelData.unread ? Font.Medium : Font.Normal; textFormat: Text.PlainText }
                            Text { text: card.formatAge(modelData.latest_at); color: Theme.subtle; font.pixelSize: Theme.fontSize(11.5) }
                        }
                        onClicked: card.openConversation(modelData.id)
                    }
                }
                FmButton {
                    objectName: "senderCardFilter"; Layout.fillWidth: true; textAlignment: Text.AlignLeft; leftPadding: 0; rightPadding: 0
                    text: (card.total > 3 ? "All " + card.total + " from " : "Filter to ") + card.displayName.split(" ")[0] + " →"
                    font.pixelSize: Theme.fontSize(12); foreground: Theme.accent
                    onClicked: card.filterSender()
                }
            }
            FmButton {
                objectName: "senderCardWrite"; text: "Write"; outlined: true; implicitHeight: 28; font.pixelSize: Theme.fontSize(12)
                enabled: backend.connected && !backend.draftBusy && !backend.accountArchived && !backend.unsavedAccountBuffer
                onClicked: card.writeMessage()
            }
        }
    }
}
