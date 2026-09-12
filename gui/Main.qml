import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

ApplicationWindow {
    id: window
    objectName: "mainWindow"
    width: 1440; height: 900
    minimumWidth: 1000; minimumHeight: 650
    visible: true
    title: "fuckMail — local fixture"
    color: Theme.canvas
    property string workspace: "mail"
    property bool settingsOpen: false
    property bool allowClose: false
    property date now: backend.clockNow
    property var mailFocus: null
    property var calendarFocus: null
    readonly property bool workspaceShortcutsAllowed: !blocksWorkspaceShortcuts(activeFocusItem)
    palette.text: Theme.text
    palette.base: Theme.surface
    palette.window: Theme.canvas
    palette.button: Theme.surface
    palette.buttonText: Theme.text
    palette.highlight: "#525860"
    palette.highlightedText: Theme.text
    font.family: Theme.fontFamily
    font.pixelSize: 13

    function belongsTo(item, view) {
        while (item) { if (item === view) return true; item = item.parent }
        return false
    }
    function blocksWorkspaceShortcuts(item) {
        while (item) { if (item.blocksWorkspaceShortcuts === true) return true; item = item.parent }
        return false
    }
    onActiveFocusItemChanged: {
        if (belongsTo(activeFocusItem, mailView)) mailFocus = activeFocusItem
        else if (belongsTo(activeFocusItem, calendarView)) calendarFocus = activeFocusItem
    }
    function navigate(target) {
        settingsOpen = target === "settings"
        if (!settingsOpen) workspace = target
        Qt.callLater(function() {
            const saved = workspace === "mail" ? mailFocus : calendarFocus
            if (!settingsOpen && saved && saved.visible) saved.forceActiveFocus()
        })
    }
    function findMail() { navigate("mail"); Qt.callLater(function() { search.forceActiveFocus(); search.selectAll() }) }
    onClosing: function(event) {
        if (!allowClose) { event.accepted = false; backend.shutdown() }
    }
    Connections {
        target: backend
        function onShutdownReady() { window.allowClose = true; window.close() }
    }
    Shortcut { sequences: [StandardKey.Find, "Ctrl+K"]; enabled: window.workspaceShortcutsAllowed; onActivated: window.findMail() }
    Shortcut { sequence: "Ctrl+1"; enabled: window.workspaceShortcutsAllowed; onActivated: window.navigate("mail") }
    Shortcut { sequence: "Ctrl+2"; enabled: window.workspaceShortcutsAllowed; onActivated: window.navigate("calendar") }
    Shortcut { sequence: "Ctrl+,"; enabled: window.workspaceShortcutsAllowed; onActivated: window.navigate("settings") }
    Shortcut { sequence: "Escape"; enabled: window.settingsOpen; onActivated: window.navigate(window.workspace) }
    header: Rectangle {
        height: Theme.headerHeight
        color: Theme.canvas
        Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 1; color: Theme.line }
        RowLayout {
            anchors.fill: parent; anchors.leftMargin: 24; anchors.rightMargin: 24; spacing: 14
            Rectangle {
                implicitWidth: viewTabs.implicitWidth + 4; implicitHeight: 32
                radius: 6; color: Theme.surface
                Row {
                    id: viewTabs; anchors.centerIn: parent; spacing: 2
                    FmButton { objectName: "mailWorkspaceButton"; height: 28; text: "Mail"; iconName: "mail"; prominent: !window.settingsOpen && window.workspace === "mail"; tip: "Mail · Ctrl+1"; onClicked: window.navigate("mail") }
                    FmButton { objectName: "calendarWorkspaceButton"; height: 28; text: "Calendar"; iconName: "calendar"; prominent: !window.settingsOpen && window.workspace === "calendar"; tip: "Combined calendar · Ctrl+2"; onClicked: window.navigate("calendar") }
                }
            }
            Rectangle { implicitWidth: 1; implicitHeight: 20; color: Theme.line }
            Flickable {
                id: accountTabs
                Layout.fillWidth: true; Layout.minimumWidth: 160; Layout.preferredHeight: 34
                contentWidth: accountsRow.width; contentHeight: height; clip: true
                boundsBehavior: Flickable.StopAtBounds
                flickableDirection: Flickable.HorizontalFlick
                ScrollBar.horizontal: ScrollBar { policy: accountTabs.contentWidth > accountTabs.width ? ScrollBar.AsNeeded : ScrollBar.AlwaysOff; height: 3 }
                Row {
                    id: accountsRow; anchors.verticalCenter: parent.verticalCenter; spacing: 2
                    Repeater {
                        model: backend.accounts
                        delegate: FmButton {
                            required property var modelData
                            required property int index
                            objectName: "accountTab_" + modelData.id
                            property int reviewCount: modelData.unread_conversation_count || 0
                            text: modelData.name + (reviewCount ? "  " + reviewCount : "")
                            iconName: index === 0 ? "person" : "accounts"
                            implicitWidth: Math.min(158, contentItem.implicitWidth + 22)
                            height: 28
                            selected: backend.account === modelData.id
                            tip: modelData.email + "\n" + reviewCount + " conversations to review\n" + modelData.unread_count + " unread messages · " + modelData.starred_count + " held messages\nMail stays in this account. Calendar uses its own source selection."
                            onClicked: { backend.selectAccount(modelData.id); window.navigate("mail") }
                        }
                    }
                }
            }
            Rectangle {
                Layout.preferredWidth: Math.min(280, window.width * 0.2)
                implicitHeight: 32
                visible: window.workspace === "mail" && !window.settingsOpen
                color: Theme.surface; radius: 6
                border.color: search.activeFocus ? Theme.accent : Theme.controlBorder
                RowLayout {
                    anchors.fill: parent; anchors.leftMargin: 11; anchors.rightMargin: 8; spacing: 9
                    FmIcon { name: "search"; size: 14 }
                    TextField {
                        id: search; objectName: "searchField"
                        Layout.fillWidth: true; padding: 0
                        placeholderText: "Search " + (backend.currentAccount.name || "mail")
                        color: Theme.text; placeholderTextColor: Theme.subtle
                        font.family: Theme.fontFamily; font.pixelSize: 13; selectByMouse: true
                        text: backend.query
                        onTextEdited: backend.query = text
                        background: Item { }
                        Accessible.name: "Search current mailbox"
                    }
                    FmButton { visible: search.text.length > 0; iconName: "close"; iconSize: 12; implicitWidth: 20; implicitHeight: 24; tip: "Clear search"; onClicked: backend.query = "" }
                    Rectangle {
                        visible: search.text.length === 0 && window.width >= 1280
                        implicitWidth: shortcutLabel.implicitWidth + 8; implicitHeight: 19; radius: 3; color: "transparent"; border.color: Theme.line
                        Text { id: shortcutLabel; anchors.centerIn: parent; text: "Ctrl K"; font.family: Theme.fontFamily; font.pixelSize: 10; color: Theme.subtle }
                    }
                }
            }
            Rectangle { implicitWidth: 1; implicitHeight: 20; color: Theme.line }
            Row {
                spacing: 8
                Text { visible: window.width >= 1280; text: Qt.formatDateTime(window.now, "ddd d MMM"); color: Theme.muted; font.pixelSize: 13; font.family: Theme.fontFamily }
                Text { text: Qt.formatDateTime(window.now, "HH:mm"); color: Theme.accent; font.pixelSize: 13; font.weight: Font.Medium; font.family: Theme.fontFamily }
            }
            FmButton {
                iconName: "info"; iconSize: 13
                text: window.width >= 1380 ? "Local fixture" : ""
                font.pixelSize: 12; implicitHeight: 28
                tip: backend.connected ? "Connected to the local engine. Synthetic data; no live account or network sync." : "Connecting to the local engine."
                onClicked: { window.navigate("settings"); settingsView.page = "Diagnostics" }
            }
            Rectangle { implicitWidth: 1; implicitHeight: 20; color: Theme.line }
            FmButton { objectName: "settingsButton"; iconName: "settings"; iconSize: 16; implicitWidth: 32; selected: window.settingsOpen; tip: "Settings · Ctrl+,"; onClicked: window.navigate(window.settingsOpen ? window.workspace : "settings") }
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
            MailView { id: mailView }
            CalendarView { id: calendarView }
            SettingsView { id: settingsView; onDone: window.navigate(window.workspace) }
        }
        TodayStrip {
            visible: window.workspace === "mail" && !window.settingsOpen
            Layout.fillWidth: true
            onOpenCalendar: window.navigate("calendar")
        }
    }
}
