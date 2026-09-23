import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

FmTextField {
    id: field
    implicitWidth: 138; implicitHeight: Math.max(30, contentHeight + topPadding + bottomPadding)
    topPadding: 4; bottomPadding: 4
    rightPadding: 30
    placeholderText: "YYYY-MM-DD"
    maximumLength: 10
    font.pointSize: Theme.fontPoints(13)
    property bool weekStartsMonday: true
    readonly property bool validDate: parseDate(text) !== null
    function parseDate(value) {
        const parts = /^(\d{4})-(\d{2})-(\d{2})$/.exec(value)
        if (!parts || Number(parts[1]) < 1900 || Number(parts[1]) > 9999) return null
        const date = new Date(Number(parts[1]), Number(parts[2]) - 1, Number(parts[3]), 12)
        return Qt.formatDate(date, "yyyy-MM-dd") === value ? date : null
    }
    function showPicker() {
        const date = parseDate(text) || new Date()
        picker.month = date.getMonth(); picker.year = date.getFullYear()
        picker.open()
    }
    FmButton {
        anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter
        implicitWidth: 28; implicitHeight: 28; leftPadding: 6; rightPadding: 6
        iconName: "calendar"; iconSize: 13; tip: "Choose a date"
        onClicked: field.showPicker()
    }
    Popup {
        id: picker
        parent: Overlay.overlay
        width: 264; padding: 12; focus: true
        closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
        property int year: 2026
        property int month: 0
        readonly property int offset: (new Date(year, month, 1).getDay() + (field.weekStartsMonday ? 6 : 0)) % 7
        function moveMonth(delta) { const next = new Date(year, month + delta, 1); year = next.getFullYear(); month = next.getMonth() }
        onAboutToShow: {
            const point = field.mapToItem(parent, 0, field.height + 5)
            x = Math.max(12, Math.min(point.x, parent.width - width - 12))
            y = Math.max(12, Math.min(point.y, parent.height - implicitHeight - 12))
        }
        background: Rectangle { color: Theme.bg2; border.color: Theme.line2; radius: 6 }
        contentItem: ColumnLayout {
            property bool blocksWorkspaceShortcuts: true
            spacing: 8
            RowLayout {
                Layout.fillWidth: true
                FmButton { iconName: "previous"; implicitWidth: 28; tip: "Previous month"; onClicked: picker.moveMonth(-1) }
                Text { Layout.fillWidth: true; horizontalAlignment: Text.AlignHCenter; text: Qt.formatDate(new Date(picker.year, picker.month, 1), "MMMM yyyy"); color: Theme.text; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(13); font.weight: Font.Medium }
                FmButton { iconName: "next"; implicitWidth: 28; tip: "Next month"; onClicked: picker.moveMonth(1) }
            }
            GridLayout {
                Layout.fillWidth: true; columns: 7; columnSpacing: 2; rowSpacing: 2
                Repeater {
                    model: field.weekStartsMonday ? ["M","T","W","T","F","S","S"] : ["S","M","T","W","T","F","S"]
                    Text { required property string modelData; text: modelData; Layout.fillWidth: true; horizontalAlignment: Text.AlignHCenter; color: Theme.subtle; font.pointSize: Theme.fontPoints(11); Layout.bottomMargin: 3 }
                }
                Repeater {
                    model: 42
                    FmButton {
                        required property int index
                        readonly property date day: new Date(picker.year, picker.month, index - picker.offset + 1, 12)
                        readonly property string iso: Qt.formatDate(day, "yyyy-MM-dd")
                        Layout.fillWidth: true; implicitWidth: 30; implicitHeight: 28; leftPadding: 0; rightPadding: 0
                        text: String(day.getDate()); selected: iso === field.text
                        foreground: day.getMonth() === picker.month ? Theme.text : Theme.subtle
                        font.pointSize: Theme.fontPoints(12)
                        Accessible.name: Qt.formatDate(day, "dddd, MMMM d, yyyy")
                        onClicked: { field.text = iso; field.textEdited(); picker.close(); field.forceActiveFocus() }
                    }
                }
            }
            FmButton { text: "Today"; Layout.alignment: Qt.AlignHCenter; onClicked: { field.text = Qt.formatDate(new Date(), "yyyy-MM-dd"); field.textEdited(); picker.close(); field.forceActiveFocus() } }
        }
    }
}
