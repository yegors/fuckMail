import QtQuick
import QtQuick.Window
import QtQuick.Controls
import QtQuick.Layouts

ApplicationWindow {
    id: window
    objectName: "mainWindow"
    width: 1440; height: 900
    minimumWidth: 1000; minimumHeight: 650
    visible: true
    title: "fuckMail"
    color: Theme.canvas
    property string workspace: "mail"
    property bool settingsOpen: false
    property bool suppressStartupPreference: false
    property bool startupPreferenceApplied: false
    readonly property bool timelineAtTop: backend.preferences.timeline_position === "header"
    readonly property bool timelineVisible: workspace === "mail" && !settingsOpen
    readonly property bool searchPopupOpen: searchPopup.opened
    property bool allowClose: false
    property var mailFocus: null
    property var calendarFocus: null
    readonly property bool workspaceShortcutsAllowed: !blocksWorkspaceShortcuts(activeFocusItem)
    readonly property bool singleKeysAllowed: workspaceShortcutsAllowed && !typingInField(activeFocusItem)
    readonly property bool accountKeysAllowed: singleKeysAllowed && !settingsOpen && !searchPopup.opened
        && !(workspace === "mail" ? mailView.popupOpen : calendarView.popupOpen)
    readonly property string commandKey: Qt.platform.os === "osx" ? "⌘" : "Ctrl+"
    palette.text: Theme.text
    palette.base: Theme.surface
    palette.window: Theme.canvas
    palette.button: Theme.surface
    palette.buttonText: Theme.text
    palette.highlight: Theme.focusBorder
    palette.highlightedText: Theme.text
    palette.mid: Theme.scrollPalette
    palette.dark: Theme.scrollPalette
    font.family: Theme.fontFamily
    font.pixelSize: Theme.fontSize(14)
    Binding { target: Theme; property: "themeId"; value: backend.preferences.theme || "graphite" }
    Binding { target: Theme; property: "textSize"; value: backend.preferences.text_size || "default" }
    Binding { target: Theme; property: "devicePixelRatio"; value: window.Screen.devicePixelRatio }
    function applyStartupPreference() {
        if (startupPreferenceApplied || !Object.keys(backend.preferences).length) return
        if (!suppressStartupPreference) {
            const requested = backend.preferences.startup_view || "mail"
            workspace = requested === "last" ? (backend.preferences.last_workspace || "mail") : requested
        }
        startupPreferenceApplied = true
        if (!suppressStartupPreference && backend.preferences.last_workspace !== workspace)
            backend.setPreferences({last_workspace: workspace})
    }
    onWorkspaceChanged: if (startupPreferenceApplied && backend.preferences.last_workspace !== workspace)
        backend.setPreferences({last_workspace: workspace})
    Component.onCompleted: applyStartupPreference()
    Connections { target: backend; function onPreferencesChanged() { window.applyStartupPreference() } }
    function cycleAccount(direction) {
        const accounts = backend.accounts
        if (accounts.length < 2) return
        const index = accounts.findIndex(function(account) { return account.id === backend.account })
        backend.selectAccount(accounts[(index + direction + accounts.length) % accounts.length].id)
    }
    function belongsTo(item, view) {
        while (item) { if (item === view) return true; item = item.parent }
        return false
    }
    function blocksWorkspaceShortcuts(item) {
        while (item) { if (item.blocksWorkspaceShortcuts === true) return true; item = item.parent }
        return false
    }
    function typingInField(item) {
        while (item) {
            if ((item.cursorPosition !== undefined && item.readOnly !== true) || item.editText !== undefined) return true
            item = item.parent
        }
        return false
    }
    onActiveFocusItemChanged: {
        if (belongsTo(activeFocusItem, mailView)) mailFocus = activeFocusItem
        else if (belongsTo(activeFocusItem, calendarView)) calendarFocus = activeFocusItem
    }
    function navigate(target) {
        searchPopup.close()
        settingsOpen = target === "settings"
        if (!settingsOpen) workspace = target
        Qt.callLater(function() {
            const saved = workspace === "mail" ? mailFocus : calendarFocus
            if (!settingsOpen && saved && saved.visible) saved.forceActiveFocus()
        })
    }
    function openSearch() {
        if (settingsOpen || workspace !== "mail") return
        const point = searchBox.mapToItem(Overlay.overlay, 0, searchBox.height + 8)
        searchPopup.x = Math.max(12, Math.min(point.x, Overlay.overlay.width - searchPopup.width - 12))
        searchPopup.y = point.y
        searchPopup.open()
    }
    function findMail() {
        navigate("mail")
        Qt.callLater(function() { search.forceActiveFocus(); search.selectAll(); window.openSearch() })
    }
    onClosing: function(event) { if (!allowClose) { event.accepted = false; backend.shutdown() } }
    Connections { target: backend; function onShutdownReady() { window.allowClose = true; window.close() } }
    Shortcut { sequences: [StandardKey.Find, "Ctrl+K"]; enabled: window.workspaceShortcutsAllowed; onActivated: window.findMail() }
    Shortcut { sequence: "/"; enabled: window.singleKeysAllowed && !window.settingsOpen; onActivated: window.findMail() }
    Shortcut { sequence: "C"; enabled: window.singleKeysAllowed && !window.settingsOpen; onActivated: window.navigate(window.workspace === "mail" ? "calendar" : "mail") }
    Shortcut { sequence: ","; enabled: window.singleKeysAllowed; onActivated: window.navigate("settings") }
    Shortcut { sequence: "Ctrl+1"; enabled: window.workspaceShortcutsAllowed; onActivated: window.navigate("mail") }
    Shortcut { sequence: "Ctrl+2"; enabled: window.workspaceShortcutsAllowed; onActivated: window.navigate("calendar") }
    Shortcut { sequence: "Ctrl+,"; enabled: window.workspaceShortcutsAllowed; onActivated: window.navigate("settings") }
    Shortcut { sequence: "Escape"; enabled: window.settingsOpen; onActivated: window.navigate(window.workspace) }
    Shortcut { sequence: "Escape"; enabled: searchPopup.opened; onActivated: searchPopup.close() }
    Shortcut { sequence: "Tab"; enabled: window.accountKeysAllowed; onActivated: window.cycleAccount(1) }
    Shortcut { sequence: "Shift+Tab"; enabled: window.accountKeysAllowed; onActivated: window.cycleAccount(-1) }
    SearchPopup {
        id: searchPopup
        parent: Overlay.overlay
        onMoreFiltersRequested: { close(); mailView.openFilters(searchBox) }
    }
    header: Rectangle {
        height: Theme.headerHeight
        color: Theme.canvas
        Rectangle { x: Theme.snapX(parent, 0); y: Theme.snapY(parent, parent.height - 1); width: parent.width; height: Theme.ruleWidth; color: Theme.line }
        RowLayout {
            anchors.fill: parent; anchors.leftMargin: 24; anchors.rightMargin: 24; spacing: 24
            RowLayout {
                spacing: 16
                Rectangle {
                    implicitWidth: viewTabs.implicitWidth + 4; implicitHeight: 32
                    radius: 6; color: Theme.surface
                    Row {
                        id: viewTabs; anchors.centerIn: parent; spacing: 2
                        FmButton { objectName: "mailWorkspaceButton"; height: 28; text: "Mail"; iconName: "mail"; iconSize: 13; prominent: !window.settingsOpen && window.workspace === "mail"; tip: "Mail · c"; onClicked: window.navigate("mail") }
                        FmButton { objectName: "calendarWorkspaceButton"; height: 28; text: "Calendar"; iconName: "calendar"; iconSize: 13; prominent: !window.settingsOpen && window.workspace === "calendar"; tip: "Calendar · c"; onClicked: window.navigate("calendar") }
                    }
                }
                Item { implicitWidth: 1; implicitHeight: 20; Rectangle { x: Theme.snapX(parent, 0); y: Theme.snapY(parent, 0); width: Theme.ruleWidth; height: parent.height; color: Theme.line } }
                Flickable {
                    id: accountTabs
                    function revealSelected() {
                        for (let i = 0; i < accountRepeater.count; ++i) {
                            const tab = accountRepeater.itemAt(i)
                            if (!tab || !tab.selected) continue
                            const left = tab.x, right = left + tab.width
                            if (left < contentX) contentX = left
                            else if (right > contentX + width) contentX = Math.max(0, right - width)
                            contentX = Math.min(contentX, Math.max(0, contentWidth - width))
                            return
                        }
                    }
                    onWidthChanged: Qt.callLater(revealSelected)
                    onContentWidthChanged: Qt.callLater(revealSelected)
                    Connections {
                        target: backend
                        function onAccountChanged() { Qt.callLater(accountTabs.revealSelected) }
                        function onAccountsChanged() { Qt.callLater(accountTabs.revealSelected) }
                    }
                    Layout.preferredWidth: Math.min(accountsRow.width, Math.max(140, window.width - 820))
                    Layout.minimumWidth: 140; Layout.preferredHeight: 32
                    contentWidth: accountsRow.width; contentHeight: height; clip: true
                    boundsBehavior: Flickable.StopAtBounds; flickableDirection: Flickable.HorizontalFlick
                    ScrollBar.horizontal: ScrollBar { policy: accountTabs.contentWidth > accountTabs.width ? ScrollBar.AsNeeded : ScrollBar.AlwaysOff; height: 3 }
                    Row {
                        id: accountsRow; anchors.verticalCenter: parent.verticalCenter; spacing: 2
                        Repeater {
                            id: accountRepeater
                            model: backend.accounts
                            delegate: FmButton {
                                required property var modelData
                                id: accountTab
                                objectName: "accountTab_" + modelData.id
                                property int reviewCount: modelData.unread_conversation_count || 0
                                readonly property real naturalWidth: 20 + 14 + 8 + accountName.implicitWidth
                                    + (reviewCount > 0 ? accountCount.implicitWidth + 8 : 0)
                                text: modelData.name
                                height: 28; implicitWidth: Math.min(naturalWidth, Math.max(140, window.width - 820))
                                leftPadding: 10; rightPadding: 10
                                selected: backend.account === modelData.id
                                Component.onCompleted: if (selected) Qt.callLater(accountTabs.revealSelected)
                                onWidthChanged: if (selected) Qt.callLater(accountTabs.revealSelected)
                                onXChanged: if (selected) Qt.callLater(accountTabs.revealSelected)
                                onSelectedChanged: if (selected) Qt.callLater(accountTabs.revealSelected)
                                tip: modelData.name + " · " + modelData.email + " · " + reviewCount + " to review, " + (modelData.held_conversation_count || 0) + " delayed"
                                onClicked: { backend.selectAccount(modelData.id); if (window.settingsOpen) window.navigate("mail") }
                                contentItem: Row {
                                    id: accountContent
                                    spacing: 8
                                    FmIcon { anchors.verticalCenter: parent.verticalCenter; name: Theme.accountIcon(accountTab.modelData.id); size: 14; color: accountTab.selected ? Theme.secondary : Theme.subtle }
                                    Text { id: accountName; anchors.verticalCenter: parent.verticalCenter; text: accountTab.text; width: Math.max(0, accountTab.availableWidth - 14 - 8 - (accountTab.reviewCount > 0 ? accountCount.implicitWidth + 8 : 0)); elide: Text.ElideRight; color: accountTab.selected ? Theme.text : Theme.muted; font { family: Theme.fontFamily; pixelSize: Theme.fontSize(13); weight: accountTab.selected ? Font.Medium : Font.Normal } }
                                    Text { id: accountCount; anchors.verticalCenter: parent.verticalCenter; visible: accountTab.reviewCount > 0; text: accountTab.reviewCount; color: accountTab.selected ? Theme.text : Theme.muted; font { family: Theme.fontFamily; pixelSize: Theme.fontSize(12); weight: Font.Medium } }
                                }
                            }
                        }
                    }
                }
            }
            RowLayout {
                Layout.fillWidth: true; spacing: 12
                Item { Layout.fillWidth: true; Layout.minimumWidth: 0 }
                Item {
                    implicitWidth: statusContent.implicitWidth; implicitHeight: statusContent.implicitHeight
                    Row {
                        id: statusContent; spacing: 7
                        FmIcon { anchors.verticalCenter: parent.verticalCenter; name: "refresh"; size: 13; color: Theme.subtle }
                        Text { text: backend.connected ? "Local copy" : "Connecting…"; color: Theme.subtle; font { family: Theme.fontFamily; pixelSize: Theme.fontSize(12) } }
                    }
                    FmToolTip { visible: statusHover.containsMouse; text: backend.connected ? "Mail and calendar are read from this machine. No live provider sync is configured." : "Connecting to the local engine." }
                    MouseArea { id: statusHover; anchors.fill: parent; acceptedButtons: Qt.NoButton; hoverEnabled: true }
                }
                Rectangle {
                    id: searchBox
                    Layout.preferredWidth: 320; Layout.minimumWidth: 128; Layout.maximumWidth: 320
                    Layout.fillWidth: true; implicitHeight: 32
                    color: Theme.surface; radius: 6
                    border.color: searchPopup.opened || search.activeFocus ? Theme.focusBorder : Theme.line
                    RowLayout {
                        anchors.fill: parent; anchors.leftMargin: 12; anchors.rightMargin: 12; spacing: 10
                        FmIcon { name: "search"; size: 14; color: Theme.subtle }
                        TextField {
                            id: search; objectName: "searchField"
                            Layout.fillWidth: true; padding: 0
                            placeholderText: "Search " + (backend.currentAccount.name || "mail") + (backend.folder === "sent" ? " · sent" : backend.folder === "drafts" ? " · drafts" : "")
                            color: Theme.text; placeholderTextColor: Theme.faint
                            font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(13); selectByMouse: true
                            text: backend.query
                            onTextEdited: {
                                window.settingsOpen = false
                                window.workspace = "mail"
                                backend.query = text
                                window.openSearch()
                            }
                            onActiveFocusChanged: if (activeFocus) window.openSearch()
                            TapHandler { onTapped: { if (window.settingsOpen || window.workspace !== "mail") window.findMail(); else window.openSearch() } }
                            background: Item { }
                            Accessible.name: "Search current mailbox"
                        }
                        FmButton { visible: search.text.length > 0; iconName: "close"; iconSize: 12; implicitWidth: 12; implicitHeight: 20; leftPadding: 0; rightPadding: 0; tip: "Clear search"; onClicked: backend.query = "" }
                        Rectangle {
                            implicitWidth: shortcutLabel.implicitWidth + 10; implicitHeight: 19; radius: 3; color: "transparent"; border.color: Theme.line
                            Text { id: shortcutLabel; anchors.centerIn: parent; text: window.commandKey + "K"; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(11); color: Theme.faint }
                        }
                    }
                }
                FmButton {
                    objectName: window.workspace === "calendar" ? "newEventButton" : "newMessageButton"
                    visible: !window.settingsOpen && (window.workspace === "calendar" || mailView.newMessageAvailable)
                    text: window.workspace === "calendar" ? "New event" : "New message"
                    iconName: "plus"; iconSize: 13; prominent: true
                    implicitWidth: contentItem.implicitWidth + 22
                    tip: text + " · n"
                    onClicked: { searchPopup.close(); if (window.workspace === "calendar") calendarView.createEvent(); else mailView.newMessage() }
                }
                Item { implicitWidth: 1; implicitHeight: 20; Rectangle { x: Theme.snapX(parent, 0); y: Theme.snapY(parent, 0); width: Theme.ruleWidth; height: parent.height; color: Theme.line } }
                FmButton { objectName: "settingsButton"; iconName: "settings"; iconSize: 15; implicitWidth: 32; prominent: window.settingsOpen; tip: "Settings · ,"; onClicked: window.navigate(window.settingsOpen ? window.workspace : "settings") }
            }
        }
    }
    ColumnLayout {
        anchors.fill: parent; spacing: 0
        Rectangle {
            visible: backend.error.length > 0; Layout.fillWidth: true; implicitHeight: errorRow.implicitHeight + 16
            color: Theme.surface
            RowLayout {
                id: errorRow; anchors.fill: parent; anchors.margins: 8
                Text { text: backend.error; color: Theme.error; wrapMode: Text.Wrap; Layout.fillWidth: true; textFormat: Text.PlainText }
                FmButton { text: backend.connected ? "Dismiss" : "Reconnect"; onClicked: backend.connected ? backend.clearError() : backend.start() }
            }
        }
        Item { id: timelineHeaderSlot; visible: window.timelineVisible && window.timelineAtTop; Layout.fillWidth: true; implicitHeight: visible ? timeline.implicitHeight : 0 }
        StackLayout {
            Layout.fillWidth: true; Layout.fillHeight: true
            currentIndex: window.settingsOpen ? 2 : window.workspace === "mail" ? 0 : 1
            MailView { id: mailView }
            CalendarView { id: calendarView }
            SettingsView { id: settingsView; onDone: window.navigate(window.workspace) }
        }
        Item { id: timelineFooterSlot; visible: window.timelineVisible && !window.timelineAtTop; Layout.fillWidth: true; implicitHeight: visible ? timeline.implicitHeight : 0 }
    }
    TodayStrip {
        id: timeline
        parent: window.timelineAtTop ? timelineHeaderSlot : timelineFooterSlot
        width: parent.width; height: implicitHeight; visible: window.timelineVisible
        headerPosition: window.timelineAtTop
        onOpenCalendar: window.navigate("calendar")
    }
}
