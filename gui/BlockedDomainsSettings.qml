import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

// Settings > Mail > Blocked domains: one list for every account. The HTML
// reader's content service owns matching, storage and enforcement; this is its
// list, add row and unblock controls.
ColumnLayout {
    id: blocked
    objectName: "blockedDomains"
    readonly property var service: backend.htmlContent || null
    readonly property string mono: backend.fixedFontFamily()
    readonly property var rows: service && service.revision >= 0 ? service.blockedHosts || [] : []
    readonly property var counts: service && service.revision >= 0 ? (service.blockCounts || {}).counts || {} : {}
    property string error: ""
    readonly property string hint: error || (input.text.trim().length ? (service && service.normalizePattern(input.text) ? "Enter to block" : "Not a domain yet") : "")
    spacing: 0
    Component.onCompleted: if (service) service.refreshBlockCounts()
    function added(row) {
        const at = row.added_at ? new Date(row.added_at) : null
        return (row.source === "link" ? "From a link" : "Added here") + (at && !isNaN(at.getTime()) ? " · " + Qt.formatDate(at, "d MMM") : "")
    }
    function hits(pattern) {
        const n = counts[pattern] || 0
        return n ? "in " + n + (n === 1 ? " email" : " emails") : ""
    }
    Text { text: "Blocked domains"; color: Theme.text; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(14); font.weight: Font.Medium }
    Text {
        Layout.fillWidth: true; Layout.maximumWidth: 640; Layout.topMargin: 6; wrapMode: Text.Wrap; textFormat: Text.RichText; lineHeight: 1.5
        text: "Links to these are disabled and their images never load — not even with Load images. Applies to every account. <span style=\"font-family:'" + blocked.mono + "'; font-size:" + Theme.fontPoints(12) + "pt; color:" + Theme.text + "\">*.example.com</span> covers the domain and all its subdomains."
        color: Theme.inkAlpha(.55); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12.5)
    }
    Rectangle { Layout.fillWidth: true; Layout.topMargin: 14; implicitHeight: 1; color: Theme.line }
    RowLayout {
        Layout.fillWidth: true; Layout.minimumHeight: 44; spacing: 10
        FmIcon { name: "plus"; size: 10; color: Theme.inkAlpha(.45) }
        TextField {
            id: input; objectName: "blockedDomainInput"
            Layout.fillWidth: true; implicitHeight: 28; leftPadding: 0; rightPadding: 0; topPadding: 0; bottomPadding: 0
            enabled: !!blocked.service
            placeholderText: "Add a domain — tracker.io or *.tracker.io"; placeholderTextColor: Theme.inkAlpha(.35)
            color: Theme.text; font.family: blocked.mono; font.pointSize: Theme.fontPoints(12)
            selectionColor: Theme.selection; selectedTextColor: Theme.selectedText
            inputMethodHints: Qt.ImhNoAutoUppercase | Qt.ImhNoPredictiveText | Qt.ImhUrlCharactersOnly
            background: null
            onTextEdited: blocked.error = ""
            Keys.onReturnPressed: blocked.add()
            Keys.onEnterPressed: blocked.add()
            Accessible.name: "Add a blocked domain"
        }
        Text { objectName: "blockedDomainHint"; text: blocked.hint; color: blocked.error ? Theme.warn : Theme.inkAlpha(.4); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(11.5) }
    }
    Rectangle { Layout.fillWidth: true; implicitHeight: 1; color: Theme.line }
    Repeater {
        model: blocked.rows
        ColumnLayout {
            id: blockedRow
            required property var modelData
            required property int index
            Layout.fillWidth: true; spacing: 0
            RowLayout {
                Layout.fillWidth: true; Layout.minimumHeight: 44; spacing: 14
                Row {
                    Layout.fillWidth: true; Layout.minimumWidth: 0; spacing: 10; clip: true
                    Text { objectName: "blockedDomain_" + blockedRow.modelData.pattern; text: blockedRow.modelData.pattern; color: Theme.text; font.family: blocked.mono; font.pointSize: Theme.fontPoints(12); textFormat: Text.PlainText }
                    Text { anchors.baseline: parent.children[0].baseline; text: blocked.added(blockedRow.modelData); color: Theme.inkAlpha(.4); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(11.5) }
                }
                Text { text: blocked.hits(blockedRow.modelData.pattern); color: Theme.inkAlpha(.45); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(11.5) }
                FmButton {
                    objectName: "unblockDomain_" + blockedRow.modelData.pattern
                    implicitWidth: 28; implicitHeight: 28; leftPadding: 0; rightPadding: 0; topPadding: 0; bottomPadding: 0
                    iconName: "close"; iconSize: 10; radius: 5
                    foreground: hovered ? Theme.text : Theme.inkAlpha(.4)
                    background: Rectangle { radius: 5; color: parent.hovered ? Theme.bg4 : "transparent" }
                    tip: "Unblock " + blockedRow.modelData.pattern
                    onClicked: blocked.service.unblock(blockedRow.modelData.pattern)
                }
            }
            Rectangle { Layout.fillWidth: true; implicitHeight: 1; color: Theme.line }
        }
    }
    Text {
        objectName: "blockedDomainsEmpty"; visible: blocked.rows.length === 0
        Layout.fillWidth: true; Layout.minimumHeight: 44; verticalAlignment: Text.AlignVCenter
        text: "Nothing blocked yet."; color: Theme.inkAlpha(.45); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12)
    }
    Rectangle { visible: blocked.rows.length === 0; Layout.fillWidth: true; implicitHeight: 1; color: Theme.line }
    function add() {
        if (!service) return
        const result = service.block(input.text, "settings")
        if (result) { error = result; return }
        error = ""; input.clear()
    }
}
