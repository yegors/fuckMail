import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Popup {
    id: popup
    objectName: "mailFilterPopup"
    property var currentFilters: backend.filters
    property string openedAccount: ""
    property string validationError: ""
    width: Math.min(440, parent ? parent.width - 24 : 440)
    padding: 16
    focus: true
    closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside

    function validDate(value) {
        if (!value.length) return true
        if (!/^\d{4}-\d{2}-\d{2}$/.test(value)) return false
        const year = Number(value.slice(0,4)), month = Number(value.slice(5,7)), day = Number(value.slice(8,10))
        if (year < 1 || month < 1 || month > 12 || day < 1) return false
        const leap = year % 4 === 0 && (year % 100 !== 0 || year % 400 === 0)
        return day <= [31, leap ? 29 : 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31][month - 1]
    }
    function apply() {
        const from = sender.text.trim(), afterDate = after.text.trim(), beforeDate = before.text.trim()
        if (!backend.validFilterSender(from)) { validationError = "Use a shorter sender name or email without control characters."; return }
        if (!validDate(afterDate) || !validDate(beforeDate)) { validationError = "Enter real dates as YYYY-MM-DD, or leave them empty."; return }
        if (afterDate.length && beforeDate.length && afterDate >= beforeDate) { validationError = "The before date must be later than the on or after date."; return }
        if (openedAccount !== backend.account) { validationError = "The mailbox changed. Reopen filters for this account."; return }
        const filters = {}
        if (from.length) filters.sender = from
        if (status.currentIndex > 0) filters.status = ["all", "review", "held", "done"][status.currentIndex]
        if (attachments.currentIndex > 0) filters.has_attachment = attachments.currentIndex === 1
        if (afterDate.length) filters.after = afterDate
        if (beforeDate.length) filters.before = beforeDate
        backend.applyFilters(filters)
        close()
    }
    onOpened: {
        openedAccount = backend.account
        validationError = ""
        sender.text = currentFilters.sender || ""
        status.currentIndex = Math.max(0, ["all", "review", "held", "done"].indexOf(currentFilters.status || "all"))
        attachments.currentIndex = currentFilters.has_attachment === true ? 1 : currentFilters.has_attachment === false ? 2 : 0
        after.text = currentFilters.after || ""
        before.text = currentFilters.before || ""
        sender.forceActiveFocus()
    }
    Connections { target: backend; function onAccountChanged() { popup.close() } }
    background: Rectangle { color: Theme.surface; border.color: Theme.controlBorder; radius: 8 }
    contentItem: ColumnLayout {
        property bool blocksWorkspaceShortcuts: true
        spacing: 14
        RowLayout {
            Text { text: "Filter " + (backend.currentAccount.name || "mail"); color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: 13; font.weight: Font.Medium; Layout.fillWidth: true; elide: Text.ElideRight }
            FmButton { iconName: "close"; iconSize: 12; implicitWidth: 24; implicitHeight: 24; tip: "Close filters · Esc"; onClicked: popup.close() }
        }
        ColumnLayout {
            Layout.fillWidth: true; spacing: 7
            Text { text: "From"; color: Theme.muted; font.family: Theme.fontFamily; font.pixelSize: 12 }
            FmTextField {
                id: sender; objectName: "filterSender"
                Layout.fillWidth: true; placeholderText: "Sender name or email"; maximumLength: 256
                onTextEdited: popup.validationError = ""
                onAccepted: popup.apply()
                Accessible.name: "Filter by sender name or email"
            }
        }
        RowLayout {
            Layout.fillWidth: true; spacing: 12
            ColumnLayout {
                Layout.fillWidth: true; spacing: 7
                Text { text: "Review state"; color: Theme.muted; font.family: Theme.fontFamily; font.pixelSize: 12 }
                FmComboBox { id: status; objectName: "filterStatus"; Layout.fillWidth: true; model: ["Any state", "To review", "Delayed", "Done"]; Accessible.name: "Filter by review state" }
            }
            ColumnLayout {
                Layout.fillWidth: true; spacing: 7
                Text { text: "Attachments"; color: Theme.muted; font.family: Theme.fontFamily; font.pixelSize: 12 }
                FmComboBox { id: attachments; objectName: "filterAttachments"; Layout.fillWidth: true; model: ["Any", "With attachments", "Without attachments"]; Accessible.name: "Filter by attachment presence" }
            }
        }
        RowLayout {
            Layout.fillWidth: true; spacing: 12
            ColumnLayout {
                Layout.fillWidth: true; spacing: 7
                Text { text: "On or after · UTC"; color: Theme.muted; font.family: Theme.fontFamily; font.pixelSize: 12 }
                FmTextField { id: after; objectName: "filterAfter"; Layout.fillWidth: true; placeholderText: "YYYY-MM-DD"; maximumLength: 10; onTextEdited: popup.validationError = ""; onAccepted: popup.apply(); Accessible.name: "On or after date, inclusive at midnight UTC" }
            }
            ColumnLayout {
                Layout.fillWidth: true; spacing: 7
                Text { text: "Before · UTC"; color: Theme.muted; font.family: Theme.fontFamily; font.pixelSize: 12 }
                FmTextField { id: before; objectName: "filterBefore"; Layout.fillWidth: true; placeholderText: "YYYY-MM-DD"; maximumLength: 10; onTextEdited: popup.validationError = ""; onAccepted: popup.apply(); Accessible.name: "Before date, exclusive at midnight UTC" }
            }
        }
        Text { text: "Dates include the start day and exclude the before day. Filters match original messages in this mailbox."; color: Theme.subtle; font.family: Theme.fontFamily; font.pixelSize: 11; wrapMode: Text.Wrap; Layout.fillWidth: true; lineHeight: 1.4 }
        Text { objectName: "filterValidation"; visible: popup.validationError.length > 0; text: popup.validationError; color: Theme.error; font.family: Theme.fontFamily; font.pixelSize: 12; wrapMode: Text.Wrap; Layout.fillWidth: true; Accessible.role: Accessible.AlertMessage }
        Rectangle { Layout.fillWidth: true; implicitHeight: 1; color: Theme.line }
        RowLayout {
            Layout.fillWidth: true
            FmButton { objectName: "clearMailFilters"; text: "Clear filters"; onClicked: { backend.applyFilters({}); popup.close() } }
            Item { Layout.fillWidth: true }
            FmButton { objectName: "applyMailFilters"; text: "Apply filters"; prominent: true; onClicked: popup.apply() }
        }
    }
}
