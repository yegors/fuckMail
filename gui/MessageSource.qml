// Per-message controller in MailView: loads the parsed source (HTML body and
// header metadata) through backend.requestMessageSource() for the reader, and
// owns the "Message source" dialog that shows the raw message.
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
    // Raw text is read for the dialog only and released when it closes.
    property string raw: ""
    property string rawTicket: ""
    // Mail shows as plain text unless its sender is in this account's
    // html_senders; choosing HTML or text here applies to all their mail.
    readonly property string sender: String(message.sender_email || "").trim().toLowerCase()
    readonly property bool canChoose: !!account && sender.indexOf("@") > 0
    readonly property bool plain: !canChoose || !(backend.preferences.html_senders || []).some(function(entry) { return entry.account === account && entry.email === sender })
    function setPlain(value) {
        if (!canChoose || value === plain) return
        const list = (backend.preferences.html_senders || []).filter(function(entry) { return !(entry.account === account && entry.email === sender) })
        if (!value) {
            // The engine keeps at most 2,000; the oldest choice makes room.
            while (list.length >= 2000) list.shift()
            list.push({account: account, email: sender})
        }
        backend.setPreferences({html_senders: list})
    }
    readonly property string identity: account + "/" + (message.id || "") + "/" + (message.source_version || "")
    readonly property bool hasHtml: !!result.body_html && !result.html_truncated
    readonly property var metadata: (result.metadata || []).filter(function(row) { return row.people.length > 0 })
    readonly property bool popupOpen: original.opened
    function adopt() {
        const prepared = message.reader_source
        if (prepared && prepared.id === message.id && prepared.account === account && prepared.source_version === message.source_version) {
            result = prepared; error = ""; return true
        }
        if (message.reader_error) { error = message.reader_error; return true }
        return false
    }
    onIdentityChanged: { ticket = ""; result = ({}); error = ""; raw = ""; rawTicket = ""; original.close(); adopt(); deferredLoad.restart() }
    onMessageChanged: adopt()
    onActiveChanged: if (active) load()
    Component.onCompleted: { adopt(); load() }
    Timer {id:deferredLoad;interval:0;onTriggered:source.load()}
    function load() {
        if (!active || !backend.connected || !message.source_available || !message.source_version || ticket || result.id || error) return
        ticket = backend.requestMessageSource(message.id, message.source_version)
    }
    function loadRaw() {
        if (raw || rawTicket || !backend.connected || !message.source_available || !message.source_version) return
        rawTicket = backend.requestMessageSource(message.id, message.source_version, true)
    }
    function releaseRaw() { if (!original.opened) { raw = ""; rawTicket = "" } }
    function show() { if (error) error = ""; load(); loadRaw(); original.open() }
    Connections {
        target: backend
        function onConnectedChanged() { if (!backend.connected) { ticket = ""; rawTicket = "" } else source.load() }
        function onMessageSourceReady(request, content, ok) {
            if (request === source.rawTicket) {
                source.rawTicket = ""
                if (ok && content.id === source.message.id && content.account === source.account && content.source_version === source.message.source_version) source.raw = content.raw || ""
                else if (original.opened) source.error = content.message || "Message source is unavailable."
                return
            }
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
        onClosed: source.releaseRaw()
        contentItem: ColumnLayout {
            spacing: 12; property bool blocksWorkspaceShortcuts: true
            RowLayout {
                Layout.fillWidth: true
                Text { text: Theme.oneLine(source.message.subject); maximumLineCount: 1; Layout.fillWidth: true; elide: Text.ElideRight; color: Theme.secondary; font.pointSize: Theme.fontPoints(12); textFormat: Text.PlainText }
                FmButton { text: "Copy"; outlined: true; implicitHeight: 28; enabled: !!source.raw; onClicked: backend.copyText(source.raw) }
            }
            Text { visible: !!source.error || !!source.result.raw_truncated; text: source.error || "This source preview is shortened. The full message remains stored locally."; color: source.error ? Theme.error : Theme.muted; font.pointSize: Theme.fontPoints(11.5); wrapMode: Text.Wrap; Layout.fillWidth: true }
            ScrollView {
                ScrollBar.vertical: FmScrollBar {}
                Layout.fillWidth: true; Layout.fillHeight: true; clip: true
                TextArea { objectName: "messageSourceText_" + source.message.id; text: original.opened ? (source.raw || (source.ticket || source.rawTicket ? "Loading source…" : "Source unavailable.")) : ""; textFormat: TextEdit.PlainText; readOnly: true; selectByMouse: true; wrapMode: TextEdit.WrapAnywhere; font.family: backend.fixedFontFamily(); font.pointSize: Theme.fontPoints(11.5); color: Theme.secondary; background: Rectangle { color: Theme.canvas; radius: 5 } ContextMenu.menu: null; TextMenuHandler {} }
            }
        }
    }
}
