import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    id: source; objectName: "messageSourceController_" + message.id
    property var message: ({})
    property string account: backend.account
    property bool active: false
    property string ticket: ""
    property var result: ({})
    property string error: ""
    property bool plain: false
    readonly property string identity: account + "/" + (message.id || "") + "/" + (message.source_version || "")
    readonly property bool hasHtml: !!result.body_html && !result.html_truncated
    readonly property string html: hasHtml ? backend.receivedHtml(result.body_html) : ""
    readonly property var metadata: (result.metadata || []).filter(function(row) { return row.people.length > 0 })
    readonly property bool popupOpen: original.opened
    onIdentityChanged: { ticket = ""; result = ({}); error = ""; plain = false; original.close(); Qt.callLater(load) }
    onActiveChanged: if (active) load()
    Component.onCompleted: load()
    function load() {
        if (!active || !backend.connected || !message.source_available || !message.source_version || ticket || result.id || error) return
        ticket = backend.requestMessageSource(message.id, message.source_version)
    }
    function show() { load(); original.open() }
    Connections {
        target: backend
        function onConnectedChanged() { if (!backend.connected) ticket = ""; else source.load() }
        function onMessageSourceReady(request, content, ok) {
            if (request !== source.ticket) return
            source.ticket = ""
            if (ok && content.id === source.message.id && content.account === source.account && content.source_version === source.message.source_version) source.result = content
            else source.error = content.message || "Message source is unavailable."
        }
    }
    FmDialog {
        id: original; objectName: "messageSourceDialog_" + source.message.id
        width: Math.min(800, parent.width - 48); height: Math.min(640, parent.height - 48)
        title: "Message source"; standardButtons: Dialog.Close
        contentItem: ColumnLayout {
            spacing: 12; property bool blocksWorkspaceShortcuts: true
            RowLayout {
                Layout.fillWidth: true
                Text { text: source.message.subject || ""; Layout.fillWidth: true; elide: Text.ElideRight; color: Theme.secondary; font.pixelSize: Theme.fontSize(13); textFormat: Text.PlainText }
                FmButton { text: "Copy"; outlined: true; implicitHeight: 28; enabled: !!source.result.raw; onClicked: backend.copyText(source.result.raw) }
            }
            Text { visible: !!source.error || !!source.result.raw_truncated; text: source.error || "This source preview is shortened. The full message remains stored locally."; color: source.error ? Theme.error : Theme.muted; font.pixelSize: Theme.fontSize(12); wrapMode: Text.Wrap; Layout.fillWidth: true }
            ScrollView {
                Layout.fillWidth: true; Layout.fillHeight: true; clip: true
                TextArea { objectName: "messageSourceText_" + source.message.id; text: source.result.raw || (source.ticket ? "Loading source…" : "Source unavailable."); textFormat: TextEdit.PlainText; readOnly: true; selectByMouse: true; wrapMode: TextEdit.WrapAnywhere; font.family: backend.fixedFontFamily(); font.pixelSize: Theme.fontSize(12); color: Theme.secondary; background: Rectangle { color: Theme.canvas; radius: 4 } }
            }
        }
    }
}
