import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Popup {
    id: popup
    objectName: "mailSearchPopup"
    signal moreFiltersRequested()
    property var senderOptions: []
    readonly property bool draftScope: backend.folder === "drafts"
    width: Math.min(440, parent ? parent.width - 24 : 440)
    height: Math.min(contents.implicitHeight + 32, parent ? parent.height - y - 12 : 560)
    padding: 16
    focus: false
    closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
    function setFilter(key, value) {
        const filters = Object.assign({}, backend.filters)
        if (filters[key] === value) delete filters[key]
        else filters[key] = value
        backend.applyFilters(filters)
    }
    function removeFilter(key) {
        const filters = Object.assign({}, backend.filters)
        delete filters[key]
        backend.applyFilters(filters)
    }
    onOpened: {
        const senders = []
        for (const row of backend.conversations) {
            if (!row.sender_email || senders.some(s => s.email === row.sender_email)) continue
            senders.push({name: row.sender_name || row.sender_email, email: row.sender_email})
            if (senders.length === 5) break
        }
        senderOptions = senders
    }
    Connections { target: backend; function onAccountChanged() { popup.close() } }
    component Label: Text { color: Theme.subtle; font { family: Theme.fontFamily; pixelSize: Theme.fontSize(11) } Layout.fillWidth: true }
    component Chip: FmButton {
        id: chip
        implicitHeight: 28; implicitWidth: Math.min(popup.availableWidth, contentItem.implicitWidth + leftPadding + rightPadding)
        font.pixelSize: Theme.fontSize(12)
        background: Rectangle { radius: 14; color: chip.selected ? Theme.selected : "transparent"; border.color: chip.activeFocus ? Theme.focusBorder : chip.selected ? Theme.controlBorder : Theme.line }
    }
    background: Rectangle { color: Theme.surface; border.color: Theme.controlBorder; radius: 8 }
    contentItem: ScrollView {
        clip: true
        contentWidth: availableWidth
        ScrollBar.horizontal.policy: ScrollBar.AlwaysOff
        ColumnLayout {
            id: contents
            width: popup.availableWidth
            spacing: 14
            RowLayout {
                Layout.fillWidth: true
                Text { text: "Search " + (backend.currentAccount.name || "mail") + " — " + (popup.draftScope ? "drafts" : "sender, words, or filters"); color: Theme.subtle; font { family: Theme.fontFamily; pixelSize: Theme.fontSize(12) } elide: Text.ElideRight; Layout.fillWidth: true }
                FmButton { text: "Esc"; implicitHeight: 21; implicitWidth: 31; leftPadding: 4; rightPadding: 4; font.pixelSize: Theme.fontSize(11); outlined: true; onClicked: popup.close() }
            }
            ColumnLayout {
                Layout.fillWidth: true; spacing: 8
                Label { text: "Looking in" }
                Flow {
                    Layout.fillWidth: true; spacing: 6
                    Repeater {
                        model: [{name:"Inbox",value:"inbox"},{name:"Sent",value:"sent"},{name:"Drafts",value:"drafts"},{name:"All mail",value:"all"}]
                        Chip { required property var modelData; text: modelData.name; selected: backend.folder === modelData.value; onClicked: backend.folder = modelData.value }
                    }
                }
            }
            ColumnLayout {
                visible: !popup.draftScope && (!!backend.filters.exact_sender || !!backend.filters.kind)
                Layout.fillWidth: true; spacing: 8
                Label { text: "Active filters" }
                Flow {
                    Layout.fillWidth: true; spacing: 6
                    Chip { objectName: "activeExactSenderFilter"; visible: !!backend.filters.exact_sender; text: "From: " + (backend.filters.exact_sender || ""); iconName: "close"; iconSize: 10; selected: true; tip: "Remove exact sender filter: " + (backend.filters.exact_sender || ""); onClicked: popup.removeFilter("exact_sender") }
                    Chip { objectName: "activeKindFilter"; visible: !!backend.filters.kind; text: ({people:"People",automated:"Automated",unknown:"Unclassified"})[backend.filters.kind] || "Sender kind"; iconName: "close"; iconSize: 10; selected: true; tip: "Remove sender kind filter"; onClicked: popup.removeFilter("kind") }
                }
            }
            ColumnLayout {
                visible: !popup.draftScope && popup.senderOptions.length > 0
                Layout.fillWidth: true; spacing: 8
                Label { text: "From" }
                Flow {
                    Layout.fillWidth: true; spacing: 6
                    Repeater { model: popup.senderOptions; Chip { required property var modelData; text: modelData.name; tip: modelData.email; selected: backend.filters.sender === modelData.email; onClicked: popup.setFilter("sender",modelData.email) } }
                }
            }
            ColumnLayout {
                visible: !popup.draftScope
                Layout.fillWidth: true; spacing: 8
                Label { text: "Category" }
                Flow {
                    Layout.fillWidth: true; spacing: 6
                    Repeater { model: backend.categories; Chip { required property var modelData; text: modelData.name; selected: backend.category === modelData.id; onClicked: backend.category = selected ? "" : modelData.id } }
                }
            }
            ColumnLayout {
                visible: !popup.draftScope
                Layout.fillWidth: true; spacing: 8
                Label { text: "Status" }
                Flow {
                    Layout.fillWidth: true; spacing: 6
                    Repeater { model: [{name:"To review",value:"review"},{name:"Delayed",value:"held"},{name:"Done",value:"done"}]; Chip { required property var modelData; text: modelData.name; selected: backend.filters.status === modelData.value; onClicked: popup.setFilter("status",modelData.value) } }
                }
            }
            RowLayout {
                visible: !popup.draftScope; Layout.fillWidth: true; spacing: 6
                Chip { text: "Has attachment"; selected: backend.filters.has_attachment === true; onClicked: popup.setFilter("has_attachment",true) }
                Item { Layout.fillWidth: true }
                FmButton { objectName: "mailFiltersButton"; text: "More filters"; font.pixelSize: Theme.fontSize(12); implicitHeight: 28; tip: "Sender, attachment presence and UTC date bounds"; onClicked: popup.moreFiltersRequested() }
            }
            Rectangle { Layout.fillWidth: true; implicitHeight: 1; color: Theme.line }
            RowLayout {
                Layout.fillWidth: true
                Text {
                    Layout.fillWidth: true; wrapMode: Text.Wrap
                    text: popup.draftScope ? "Saved on this machine" : "Indexed " + ((backend.currentAccount.coverage || {}).indexed_messages || 0) + " of " + (backend.currentAccount.message_count || 0) + " messages"
                    color: Theme.subtle; font { family: Theme.fontFamily; pixelSize: Theme.fontSize(12) }
                }
                FmButton { text: "Clear filters"; foreground: Theme.text; font.pixelSize: Theme.fontSize(12); implicitHeight: 25; onClicked: { backend.applyFilters({}); backend.category = ""; backend.query = "" } }
            }
        }
    }
}
