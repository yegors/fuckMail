import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

ApplicationWindow {
    id: window
    objectName: "mainWindow"
    width: 1440; height: 950
    minimumWidth: 980; minimumHeight: 650
    visible: true
    title: "fuckMail — offline fixture"
    color: Theme.canvas
    property string workspace: "mail"
    property bool settingsOpen: false
    property bool allowClose: false
    property date now: new Date()
    palette.text: Theme.text
    palette.base: Theme.surface
    palette.highlight: "#525860"
    palette.highlightedText: Theme.text
    font.pixelSize: 13

    onClosing: function(event) {
        if (!allowClose) { event.accepted = false; backend.shutdown() }
    }
    Connections {
        target: backend
        function onShutdownReady() { window.allowClose = true; window.close() }
    }
    Timer { interval: 30000; running: true; repeat: true; onTriggered: window.now = new Date() }
    Shortcut { sequences: [StandardKey.Find]; onActivated: search.forceActiveFocus() }
    Shortcut { sequence: "Escape"; enabled: window.settingsOpen; onActivated: window.settingsOpen = false }
    header: Rectangle {
        height: 48; color: Theme.canvas
        Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 1; color: Theme.line }
        RowLayout {
            anchors.fill: parent; anchors.leftMargin: 16; anchors.rightMargin: 14; spacing: 5
            FmButton { text: "✉  Mail"; prominent: !window.settingsOpen && window.workspace === "mail"; onClicked: { window.workspace = "mail"; window.settingsOpen = false } }
            FmButton { text: "▦  Calendar"; prominent: !window.settingsOpen && window.workspace === "calendar"; onClicked: { window.workspace = "calendar"; window.settingsOpen = false } }
            Rectangle { Layout.leftMargin: 10; Layout.rightMargin: 10; width: 1; height: 20; color: Theme.line }
            Repeater {
                model: backend.accounts
                delegate: FmButton {
                    required property var modelData
                    text: modelData.name + "  " + modelData.unread_count
                    selected: backend.account === modelData.id
                    tip: modelData.email + " · " + modelData.unread_count + " unread messages · " + modelData.starred_count + " starred messages"
                    onClicked: backend.selectAccount(modelData.id)
                }
            }
            Item { Layout.fillWidth: true }
            TextField {
                id: search; objectName: "searchField"
                Layout.preferredWidth: Math.min(230, window.width * 0.18)
                implicitHeight: 30
                visible: window.workspace === "mail" && !window.settingsOpen
                placeholderText: "Search " + (backend.currentAccount.name || "mail")
                color: Theme.text; placeholderTextColor: Theme.subtle
                font.pixelSize: 12; selectByMouse: true
                text: backend.query
                onTextEdited: backend.query = text
                background: Rectangle { color: Theme.surface; radius: 4; border.color: search.activeFocus ? Theme.accent : Theme.line }
            }
            Text { Layout.leftMargin: 10; text: "11 Sep 2026"; color: Theme.muted; font.pixelSize: 11 }
            Text { text: "Offline fixture"; color: Theme.accent; font.pixelSize: 11; Layout.leftMargin: 6 }
            FmButton { text: "⚙"; selected: window.settingsOpen; tip: "Settings"; onClicked: window.settingsOpen = !window.settingsOpen; Layout.leftMargin: 8 }
        }
    }
    ColumnLayout {
        anchors.fill: parent; spacing: 0
        Rectangle {
            visible: backend.error.length > 0; Layout.fillWidth: true; implicitHeight: errorRow.implicitHeight + 16
            color: "#302321"
            RowLayout {
                id: errorRow; anchors.fill: parent; anchors.margins: 8
                Text { text: backend.error; color: Theme.error; wrapMode: Text.Wrap; Layout.fillWidth: true; textFormat: Text.PlainText }
                FmButton { text: backend.connected ? "Dismiss" : "Reconnect"; onClicked: backend.connected ? backend.clearError() : backend.start() }
            }
        }
        StackLayout {
            Layout.fillWidth: true; Layout.fillHeight: true
            currentIndex: window.settingsOpen ? 2 : window.workspace === "mail" ? 0 : 1
            MailView { }
            CalendarView { }
            SettingsView { onDone: window.settingsOpen = false }
        }
        TodayStrip { Layout.fillWidth: true; onOpenCalendar: { window.settingsOpen = false; window.workspace = "calendar" } }
    }
}
