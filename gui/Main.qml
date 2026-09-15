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
    readonly property bool searchPopupOpen: searchPopup.opened || calendarSearchPopup.opened
    property bool allowClose: false
    property var mailFocus: null
    property var calendarFocus: null
    readonly property bool workspaceShortcutsAllowed: !startupOverlay.visible && !accountLifecycleDialogs.popupOpen && !blocksWorkspaceShortcuts(activeFocusItem)
    readonly property bool singleKeysAllowed: workspaceShortcutsAllowed && !typingInField(activeFocusItem)
    readonly property bool accountKeysAllowed: singleKeysAllowed && !settingsOpen && !searchPopupOpen
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
    onWorkspaceChanged: {
        if (startupPreferenceApplied && backend.preferences.last_workspace !== workspace)
            backend.setPreferences({last_workspace: workspace})
        searchPopup.close(); calendarSearchPopup.close()
    }
    onSettingsOpenChanged: { searchPopup.close(); calendarSearchPopup.close() }
    Component.onCompleted: applyStartupPreference()
    Connections { target: backend; function onPreferencesChanged() { window.applyStartupPreference() } }
    function cycleAccount(direction) {
        const accounts = backend.accounts
        if (!accounts.length) return
        const index = accounts.findIndex(function(account) { return account.id === backend.account })
        backend.selectAccount(accounts[index < 0 ? (direction > 0 ? 0 : accounts.length - 1) : (index + direction + accounts.length) % accounts.length].id)
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
        if (settingsOpen && target !== "settings" && settingsView.modelDirty) { settingsView.confirmLeave(function(){window.navigate(target)}); return }
        if (!backend.accountNavigationAllowed()) return
        searchPopup.close()
        calendarSearchPopup.close()
        settingsOpen = target === "settings"
        if (!settingsOpen) workspace = target
        Qt.callLater(function() {
            const saved = workspace === "mail" ? mailFocus : calendarFocus
            if (!settingsOpen && saved && saved.visible) saved.forceActiveFocus()
        })
    }
    function openSearch() {
        if (settingsOpen) return
        if (workspace === "calendar" && !calendarView.prepareSearch()) return
        const point = searchBox.mapToItem(Overlay.overlay, 0, searchBox.height + 8)
        const popup = workspace === "calendar" ? calendarSearchPopup : searchPopup
        popup.x = Math.max(12, Math.min(point.x, Overlay.overlay.width - popup.width - 12))
        popup.y = point.y
        popup.open()
    }
    function findMail() {
        if (settingsOpen) navigate(workspace)
        Qt.callLater(function() { search.forceActiveFocus(); search.selectAll(); window.openSearch() })
    }
    onClosing: function(event) { if (!allowClose) { event.accepted = false; backend.shutdown() } }
    Connections { target: backend; function onShutdownReady() { window.allowClose = true; window.close() } }
    AccountLifecycleDialogs { id: accountLifecycleDialogs }
    StartupOverlay {
        id: startupOverlay
        parent: Overlay.overlay
        anchors.fill: parent
        startupReady: backend.startupReady === true
        startupFailed: backend.error.length > 0
    }
    Shortcut { sequences: [StandardKey.Find, "Ctrl+K"]; enabled: window.workspaceShortcutsAllowed; onActivated: window.findMail() }
    Shortcut { sequence: "/"; enabled: window.singleKeysAllowed && !window.settingsOpen; onActivated: window.findMail() }
    Shortcut { sequence: "V"; enabled: window.singleKeysAllowed && !window.settingsOpen; onActivated: window.navigate(window.workspace === "mail" ? "calendar" : "mail") }
    Shortcut { sequence: ","; enabled: window.singleKeysAllowed; onActivated: window.navigate("settings") }
    Shortcut { sequence: "Ctrl+1"; enabled: window.workspaceShortcutsAllowed; onActivated: window.navigate("mail") }
    Shortcut { sequence: "Ctrl+2"; enabled: window.workspaceShortcutsAllowed; onActivated: window.navigate("calendar") }
    Shortcut { sequence: "Ctrl+,"; enabled: window.workspaceShortcutsAllowed; onActivated: window.navigate("settings") }
    Shortcut { sequence: "Escape"; enabled: window.settingsOpen; onActivated: window.navigate(window.workspace) }
    Shortcut { sequence: "Escape"; enabled: searchPopupOpen; onActivated: { searchPopup.close(); calendarSearchPopup.close() } }
    Shortcut { sequence: "Tab"; enabled: window.accountKeysAllowed; onActivated: window.cycleAccount(1) }
    Shortcut { sequence: "Shift+Tab"; enabled: window.accountKeysAllowed; onActivated: window.cycleAccount(-1) }
    SearchPopup {
        id: searchPopup
        parent: Overlay.overlay
        onMoreFiltersRequested: { close(); mailView.openFilters(searchBox) }
    }
    CalendarSearchPopup {
        id: calendarSearchPopup
        parent: Overlay.overlay
        combined: calendarView.combinedAccounts
        onOpenedChanged: backend.setCalendarSearchContext(opened && window.workspace === "calendar" && !window.settingsOpen, combined)
        onCombinedChanged: backend.setCalendarSearchContext(opened && window.workspace === "calendar" && !window.settingsOpen, combined)
        onEventRequested: function(event) { backend.openCalendarSearchResult(event) }
    }
    Connections {
        target: backend
        function onCalendarSearchResultReady(event) {
            if (!calendarSearchPopup.opened || window.settingsOpen || window.workspace !== "calendar") return
            calendarSearchPopup.close()
            calendarView.showEvent(event, calendarView)
        }
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
                        FmButton { objectName: "mailWorkspaceButton"; height: 28; text: "Mail"; iconName: "mail"; iconSize: 13; prominent: !window.settingsOpen && window.workspace === "mail"; tip: "Mail · v"; onClicked: window.navigate("mail") }
                        FmButton { objectName: "calendarWorkspaceButton"; height: 28; text: "Calendar"; iconName: "calendar"; iconSize: 13; prominent: !window.settingsOpen && window.workspace === "calendar"; tip: "Calendar · v"; onClicked: window.navigate("calendar") }
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
                            model: backend.accountModel
                            delegate: FmButton {
                                required property var rowData
                                readonly property var modelData: rowData
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
                FmButton {
                    id: syncButton
                    objectName: "headerSyncButton"
                    // Explicit property reads keep invokable-derived state reactive.
                    readonly property var syncState: { const states = backend.googleSync; return backend.googleSyncState(backend.account) }
                    readonly property var accountRow: backend.accounts.find(a => a.id === backend.account) || ({})
                    readonly property var progress: syncState.progress || ({})
                    readonly property bool blocked: syncState.credential_access_required === true
                    readonly property bool syncing: !blocked && (syncState.changing === true || syncState.refresh_requested === true
                        || (syncState.active === true && (progress.mail === "starting" || progress.mail === "running")))
                    readonly property double lastSync: syncState.last_mail_sync_at || accountRow.last_mail_sync_at || 0
                    readonly property string lastTime: lastSync > 0 ? Qt.formatDateTime(new Date(lastSync * 1000), new Date(lastSync * 1000).toDateString() === new Date().toDateString() ? "HH:mm" : "MMM d HH:mm") : ""
                    text: (blocked || (syncState.active && progress.mail === "paused") ? "Retry sync" : syncing ? "Syncing…" : "Sync") + (lastTime.length ? " · " + lastTime : "")
                    iconName: "refresh"; iconSize: 13
                    height: 30
                    enabled: backend.connected && !backend.syntheticProfile && !syncState.changing
                        && (accountRow.google || {}).capabilities?.mail_read === true && !accountRow.read_only
                    Accessible.name: text
                    tip: (blocked ? "Keychain access was denied or is locked. Click to try again. macOS may ask for your login keychain password. " : "Fetch new mail for this account. ")
                        + (lastSync > 0 ? "Last successful sync: " + new Date(lastSync * 1000).toLocaleString() : "No successful mail sync yet.")
                    onClicked: backend.refreshGoogleSync(backend.account)
                }
                Rectangle {
                    id: searchBox
                    Layout.preferredWidth: 320; Layout.minimumWidth: 128; Layout.maximumWidth: 320
                    Layout.fillWidth: true; implicitHeight: 32
                    color: Theme.surface; radius: 6
                    border.color: window.searchPopupOpen || search.activeFocus ? Theme.focusBorder : Theme.line
                    RowLayout {
                        anchors.fill: parent; anchors.leftMargin: 12; anchors.rightMargin: 12; spacing: 10
                        FmIcon { name: "search"; size: 14; color: Theme.subtle }
                        TextField {
                            id: search; objectName: "searchField"
                            Layout.fillWidth: true; padding: 0
                            placeholderText: window.workspace === "calendar" ? "Search calendar" : "Search " + (backend.currentAccount.name || "mail") + (backend.folder === "sent" ? " · sent" : backend.folder === "drafts" ? " · drafts" : "")
                            color: Theme.text; placeholderTextColor: Theme.faint
                            font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(13); selectByMouse: true
                            text: window.workspace === "calendar" ? backend.calendarSearchQuery : backend.query
                            onTextEdited: {
                                window.settingsOpen = false
                                if (window.workspace === "calendar") backend.calendarSearchQuery = text
                                else backend.query = text
                                window.openSearch()
                            }
                            onActiveFocusChanged: {
                                if (activeFocus && (focusReason === Qt.TabFocusReason || focusReason === Qt.BacktabFocusReason || focusReason === Qt.MouseFocusReason))
                                    window.openSearch()
                            }
                            Keys.onEscapePressed: { searchPopup.close(); calendarSearchPopup.close() }
                            TapHandler { onTapped: { if (window.settingsOpen) window.findMail(); else window.openSearch() } }
                            background: Item { }
                            Accessible.name: window.workspace === "calendar" ? "Search saved calendar events" : "Search current mailbox"
                        }
                        FmButton { visible: search.text.length > 0; iconName: "close"; iconSize: 12; implicitWidth: 12; implicitHeight: 20; leftPadding: 0; rightPadding: 0; tip: "Clear search"; onClicked: { if (window.workspace === "calendar") backend.calendarSearchQuery = ""; else backend.query = "" } }
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
                    onClicked: { searchPopup.close(); calendarSearchPopup.close(); if (window.workspace === "calendar") calendarView.createEvent(); else mailView.newMessage() }
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
            SettingsView { id: settingsView; onDone: window.navigate(window.workspace); onArchiveOpened: window.navigate("mail") }
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
    ReminderPopup {
        anchors.right: parent.right; anchors.bottom: parent.bottom; anchors.margins: 20
        z: 200
        visible: backend.reminders.length > 0 && !startupOverlay.visible
    }
}
