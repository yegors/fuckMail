// Root window, loaded by main.cpp from the FuckMail QML module: the top bar
// (Mail/Calendar switch, account tabs, search), the shortcut bar, the Mail /
// Calendar / Settings stack, global shortcuts and app-wide overlays. State comes from the `backend` context object.
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
    // Settings is built on its first opening and then kept; most sessions never open it.
    readonly property Item settingsView: settingsLoader.item
    property bool suppressStartupPreference: false
    property bool startupPreferenceApplied: false
    readonly property bool timelineAtTop: backend.preferences.timeline_position === "header"
    readonly property bool timelineVisible: workspace === "mail" && !settingsOpen
    readonly property bool searchPopupOpen: searchPopup.opened || calendarSearchPopup.opened
    onWidthChanged: if (searchPopupOpen) Qt.callLater(positionSearchPopup)
    onHeightChanged: if (searchPopupOpen) Qt.callLater(positionSearchPopup)
    property bool allowClose: false
    property var mailFocus: null
    property var calendarFocus: null
    readonly property bool modalReminderOpen: backend.reminders.length > 0 && !startupOverlay.visible
    readonly property bool workspaceShortcutsAllowed: !modalReminderOpen && !debugConsole.visible && !startupOverlay.visible && !accountLifecycleDialogs.popupOpen && !blocksWorkspaceShortcuts(activeFocusItem)
    readonly property bool singleKeysAllowed: workspaceShortcutsAllowed && !typingInField(activeFocusItem)
    readonly property bool accountKeysAllowed: singleKeysAllowed && !settingsOpen && !searchPopupOpen
        && !(workspace === "mail" ? mailView.popupOpen : calendarView.popupOpen)
    readonly property string commandKey: Qt.platform.os === "osx" ? "⌘" : "Ctrl+"
    palette.text: Theme.text
    palette.base: Theme.surface
    palette.window: Theme.canvas
    // Windows picks a dark or light frame by comparing these two.
    palette.windowText: Theme.text
    palette.button: Theme.surface
    palette.buttonText: Theme.text
    palette.highlight: Theme.focusBorder
    palette.highlightedText: Theme.text
    palette.mid: Theme.scrollPalette
    palette.dark: Theme.scrollPalette
    font.family: Theme.fontFamily
    font.pointSize: Theme.fontPoints(13)
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
            backend.rememberWorkspace(workspace)
    }
    onWorkspaceChanged: {
        if (startupPreferenceApplied && backend.preferences.last_workspace !== workspace)
            backend.rememberWorkspace(workspace)
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
    // Mail's empty inbox opens today's next event in the Calendar day view.
    function showEventInCalendar(event) { navigate("calendar"); calendarView.showEventInDay(event) }
    function navigate(target) {
        if (settingsOpen && target !== "settings" && settingsView && settingsView.modelDirty) { settingsView.confirmLeave(function(){window.navigate(target)}); return }
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
        const popup = workspace === "calendar" ? calendarSearchPopup : searchPopup
        positionSearchPopup()
        popup.open()
    }
    // Both dropdowns hang from the search box's right edge and open leftwards,
    // so they never clip. The Mail dropdown is parented to the box itself.
    function positionSearchPopup() {
        const origin = searchBox.mapToItem(Overlay.overlay, 0, 0)
        const right = origin.x + searchBox.width
        if (workspace === "calendar") {
            calendarSearchPopup.x = Math.max(16, right - calendarSearchPopup.width)
            calendarSearchPopup.y = origin.y + searchBox.height + 6
        } else {
            searchPopup.x = Math.max(16, right - searchPopup.width) - origin.x
            searchPopup.y = searchBox.height + 6
            searchPopup.maxHeight = Math.max(160, Overlay.overlay.height - origin.y - searchBox.height - 20)
        }
    }
    // Enter or Esc in the box hands the keyboard back to the workspace.
    function leaveSearch() {
        const saved = workspace === "mail" ? mailFocus : calendarFocus
        if (saved && saved.visible) saved.forceActiveFocus()
        else (workspace === "mail" ? mailView : calendarView).forceActiveFocus()
    }
    Connections {
        target: searchBox
        function onXChanged() { if (window.searchPopupOpen) Qt.callLater(window.positionSearchPopup) }
    }
    function commitSearch() {
        if (workspace === "mail") searchPopup.commit()
        else { calendarSearchPopup.commit(); leaveSearch() }
    }
    function findMail() {
        if (settingsOpen) navigate(workspace)
        Qt.callLater(function() { search.forceActiveFocus(); search.selectAll(); window.openSearch() })
    }
    onClosing: function(event) { if (!allowClose) { event.accepted = false; backend.shutdown() } }
    Connections { target: backend; function onShutdownReady() { window.allowClose = true; window.close() } }
    AccountLifecycleDialogs { id: accountLifecycleDialogs }
    // One pipeline model for the account tabs, their hover card and Settings.
    AccountProgress { id: accountProgress }
    AccountHoverCard { id: accountTip; parent: Overlay.overlay; progress: accountProgress }
    StartupOverlay {
        id: startupOverlay
        parent: Overlay.overlay
        anchors.fill: parent
        startupReady: backend.startupReady === true
        startupFailed: backend.error.length > 0
        engineUnavailable: startupFailed && !backend.connected && backend.startupReady !== true
        failureMessage: backend.error
        profilePath: backend.profilePath
        onRetryRequested: backend.start()
        onRevealProfileRequested: backend.revealProfile()
        onQuitRequested: window.close()
    }
    Shortcut { sequence: "Ctrl+K"; enabled: window.workspaceShortcutsAllowed; onActivated: window.findMail() }
    Shortcut { sequence: ShortcutRegistry.sequence("search",backend.preferences); enabled: window.singleKeysAllowed && !window.settingsOpen && window.workspace==="mail"; onActivated: window.findMail() }
    Shortcut { sequence: ShortcutRegistry.sequence("view",backend.preferences); enabled: window.singleKeysAllowed && !window.settingsOpen; onActivated: window.navigate(window.workspace === "mail" ? "calendar" : "mail") }
    Shortcut { sequence: ShortcutRegistry.sequence("settings",backend.preferences); enabled: window.singleKeysAllowed; onActivated: window.navigate("settings") }
    Shortcut { sequence: "Escape"; enabled: !window.modalReminderOpen && window.settingsOpen && !!settingsView && !settingsView.recordingShortcut && !settingsView.popupOpen && !debugConsole.visible; onActivated: window.navigate(window.workspace) }
    Shortcut { sequence: "Escape"; enabled: !window.modalReminderOpen && searchPopupOpen; onActivated: { searchPopup.close(); calendarSearchPopup.close(); if (search.activeFocus) window.leaveSearch() } }
    Shortcut { sequence: ShortcutRegistry.sequence("undo",backend.preferences); enabled: window.singleKeysAllowed && backend.actionToast.visible && backend.actionToast.action_label === "Undo"; onActivated: backend.activateActionToast() }
    Shortcut { sequences: [StandardKey.Undo]; enabled: ShortcutRegistry.unassigned("Ctrl+Z",window.workspace,backend.preferences) && window.singleKeysAllowed && backend.actionToast.visible && backend.actionToast.action_label === "Undo"; onActivated: backend.activateActionToast() }
    Shortcut { sequence: "Tab"; enabled: window.accountKeysAllowed; onActivated: window.cycleAccount(1) }
    Shortcut { sequence: "Shift+Tab"; enabled: window.accountKeysAllowed; onActivated: window.cycleAccount(-1) }
    DebugConsole {
        id: debugConsole
        hostWindow: window
        diagnostics: typeof debugDiagnostics !== "undefined" ? debugDiagnostics : null
        searchActive: window.searchPopupOpen
        uiState: ({view: window.settingsOpen ? "settings" : window.workspace, theme: Theme.themeId,
            settings_tab: window.settingsOpen && settingsView ? settingsView.page : null})
    }
    SearchPopup {
        id: searchPopup
        parent: searchBox
        field: search
        onCommitted: window.leaveSearch()
    }
    // Calendar search runs while its dropdown is open and, with text in the
    // box, while Calendar is shown, so the views stay filtered.
    readonly property bool calendarSearchLive: workspace === "calendar" && !settingsOpen
        && (calendarSearchPopup.opened || backend.calendarSearchQuery.trim().length > 0)
    function syncCalendarSearch() { backend.setCalendarSearchContext(calendarSearchLive, calendarView.combinedAccounts) }
    // Deferred: the backend's reply to a context change notifies this binding again.
    onCalendarSearchLiveChanged: Qt.callLater(syncCalendarSearch)
    CalendarSearchPopup {
        id: calendarSearchPopup
        parent: Overlay.overlay
        field: search
        combined: calendarView.combinedAccounts
        // Closing fences an event still opening from the dropdown; the
        // search itself starts again if the box keeps its text.
        onOpenedChanged: { if (!opened) backend.setCalendarSearchContext(false, combined); Qt.callLater(window.syncCalendarSearch) }
        onCombinedChanged: Qt.callLater(window.syncCalendarSearch)
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
    // One 48px row on a single centre line: the Mail / Calendar switch, the
    // mailbox tabs, then Sync, search and Settings at the right.
    header: Rectangle {
        height: Theme.headerHeight
        color: Theme.canvas
        Rectangle { x: Theme.snapX(parent, 0); y: Theme.snapY(parent, parent.height - 1); width: parent.width; height: Theme.ruleWidth; color: Theme.line }
        RowLayout {
            anchors.fill: parent; anchors.leftMargin: 12; anchors.rightMargin: 12; spacing: 12
            // The view switch: the open view is inverted, page colour on ink.
            Rectangle {
                implicitWidth: viewTabs.implicitWidth + 4; implicitHeight: 30
                radius: 7; color: Theme.surface
                Row {
                    id: viewTabs; anchors.centerIn: parent; spacing: 2
                    Repeater {
                        model: [{view: "mail", icon: "mail", tip: "Mail"}, {view: "calendar", icon: "calendar", tip: "Calendar"}]
                        FmButton {
                            id: viewTab
                            required property var modelData
                            objectName: modelData.view + "WorkspaceButton"
                            readonly property bool open: !window.settingsOpen && window.workspace === modelData.view
                            height: 26; implicitHeight: 26; radius: 5; leftPadding: 10; rightPadding: 12; topPadding: 0; bottomPadding: 0
                            text: modelData.view; iconName: modelData.icon; iconSize: 13; iconSpacing: 7
                            font.pointSize: Theme.fontPoints(12); font.weight: Font.Normal
                            prominent: open; hoverBrightness: 1
                            foreground: open ? Theme.canvas : hovered ? Theme.text : Theme.inkAlpha(.5)
                            tip: modelData.tip + " — " + ShortcutRegistry.label(ShortcutRegistry.sequence("view", backend.preferences)).toLowerCase()
                            background: Rectangle { radius: 5; color: viewTab.open ? Theme.text : "transparent"; border.width: viewTab.visualFocus ? 1 : 0; border.color: Theme.focusBorder }
                            onClicked: window.navigate(modelData.view)
                        }
                    }
                }
            }
            Rectangle { implicitWidth: 1; implicitHeight: 20; Layout.leftMargin: 4; Layout.rightMargin: 4; color: Theme.line2 }
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
                Layout.preferredWidth: Math.min(accountsRow.width, Math.max(140, window.width - 760))
                Layout.minimumWidth: 140; Layout.preferredHeight: 30
                contentWidth: accountsRow.width; contentHeight: height; clip: true
                boundsBehavior: Flickable.StopAtBounds; flickableDirection: Flickable.HorizontalFlick
                Row {
                    id: accountsRow; anchors.verticalCenter: parent.verticalCenter; spacing: 4
                    Repeater {
                        id: accountRepeater
                        model: backend.accountModel
                        // A mailbox: its letter tile, lowercase name and count to
                        // review. A dot on the tile's corner says what it is doing:
                        // amber pulsing while mail downloads, the model colour
                        // pulsing while the local model works, red when disconnected.
                        delegate: FmButton {
                            required property var rowData
                            readonly property var modelData: rowData
                            id: accountTab
                            objectName: "accountTab_" + modelData.id
                            readonly property var pipeline: accountProgress.view(modelData.id)
                            readonly property color hue: Qt.color(Theme.accountColor(modelData.id))
                            property int reviewCount: (modelData.inbox_review_count ?? modelData.unread_conversation_count) || 0
                            readonly property string badgeText: reviewCount > 0 ? String(reviewCount) : ""
                            readonly property real naturalWidth: 6 + 18 + 8 + accountName.implicitWidth
                                + (badgeText.length ? accountCount.implicitWidth + 8 : 0) + 10
                            readonly property color ink: selected || hovered ? Theme.text : Theme.inkAlpha(.5)
                            Accessible.description: pipeline.status
                            text: modelData.name
                            height: 30; implicitWidth: Math.min(naturalWidth, Math.max(140, window.width - 760))
                            leftPadding: 6; rightPadding: 10; topPadding: 0; bottomPadding: 0; radius: 6
                            selected: backend.account === modelData.id
                            Component.onCompleted: if (selected) Qt.callLater(accountTabs.revealSelected)
                            onWidthChanged: if (selected) Qt.callLater(accountTabs.revealSelected)
                            onXChanged: if (selected) Qt.callLater(accountTabs.revealSelected)
                            onSelectedChanged: if (selected) Qt.callLater(accountTabs.revealSelected)
                            onHoveredChanged: {
                                if (hovered) { const point = mapToItem(Overlay.overlay, 0, height + 8); accountTip.show(modelData, point.x, point.y) }
                                else if (accountTip.account === modelData.id) accountTip.close()
                            }
                            onClicked: { accountTip.close(); backend.selectAccount(modelData.id); if (window.settingsOpen) window.navigate("mail") }
                            background: Rectangle {
                                radius: 6
                                color: accountTab.selected ? Theme.bg4 : accountTab.hovered ? Theme.inkAlpha(.05) : "transparent"
                                border.width: accountTab.visualFocus ? 1 : 0; border.color: Theme.focusBorder
                            }
                            contentItem: Row {
                                spacing: 8
                                Item {
                                    objectName: "accountTabBadge_" + accountTab.modelData.id
                                    anchors.verticalCenter: parent.verticalCenter; width: 18; height: 18
                                    Rectangle {
                                        objectName: "accountBadgeLetter"
                                        anchors.fill: parent; radius: 5
                                        color: Theme.withAlpha(accountTab.hue, accountTab.selected ? .24 : .1)
                                        Text {
                                            anchors.centerIn: parent
                                            text: accountTab.modelData.name.trim().charAt(0).toUpperCase()
                                            color: accountTab.selected ? accountTab.hue : Theme.withAlpha(accountTab.hue, .6)
                                            font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(10.5); font.weight: Font.Medium
                                        }
                                    }
                                    // A 6px dot in a 2px ring of the bar colour, on the tile's
                                    // top-right corner. It pulses at 12.5 frames a second rather
                                    // than every frame (syncing can last for days) and rests
                                    // while the app is in the back.
                                    Rectangle {
                                        id: statusDot
                                        objectName: "accountStatusDot_" + accountTab.modelData.id
                                        readonly property bool failed: accountTab.pipeline.err === true
                                        readonly property bool busy: !failed && accountTab.pipeline.busy === true
                                        property real phase: 0
                                        visible: failed || busy
                                        x: parent.width - width + 4; y: -4; width: 10; height: 10; radius: 5
                                        color: failed ? Theme.error : accountTab.pipeline.progressColor
                                        border.width: 2; border.color: Theme.canvas
                                        opacity: busy ? .65 + .35 * Math.cos(phase * 2 * Math.PI) : 1
                                        Timer {
                                            interval: 80; repeat: true
                                            running: statusDot.busy && Qt.application.state === Qt.ApplicationActive
                                            onTriggered: statusDot.phase = (statusDot.phase + 0.05) % 1
                                        }
                                    }
                                }
                                Text { id: accountName; anchors.verticalCenter: parent.verticalCenter; text: accountTab.text.toLowerCase(); width: Math.max(0, accountTab.availableWidth - 18 - 8 - (accountTab.badgeText.length ? accountCount.implicitWidth + 8 : 0)); elide: Text.ElideRight; color: accountTab.ink; font { family: Theme.fontFamily; pointSize: Theme.fontPoints(12) } }
                                Text { id: accountCount; anchors.verticalCenter: parent.verticalCenter; visible: accountTab.badgeText.length > 0; text: accountTab.badgeText; color: accountTab.ink; font { family: Theme.fontFamily; pointSize: Theme.fontPoints(12); features: {"tnum": 1} } }
                            }
                        }
                    }
                }
            }
            RowLayout {
                Layout.fillWidth: true; spacing: 8
                Item { Layout.fillWidth: true; Layout.minimumWidth: 0 }
                FmButton {
                    id: syncButton
                    objectName: "headerSyncButton"
                    // Explicit property reads keep invokable-derived state reactive.
                    readonly property var syncState: { const states = backend.googleSync; return backend.googleSyncState(backend.account) }
                    readonly property var accountRow: backend.accounts.find(a => a.id === backend.account) || ({})
                    readonly property var progress: syncState.progress || ({})
                    readonly property bool mailGranted: (accountRow.google || {}).capabilities?.mail_read === true
                    readonly property bool calendarGranted: (accountRow.google || {}).capabilities?.calendar_read === true
                    readonly property bool blocked: syncState.credential_access_required === true
                    // Downloading older mail runs in the background and has its
                    // own progress in Accounts; only new-mail checks, a requested
                    // refresh and the first sync show as syncing here.
                    readonly property bool mailSyncing: progress.mail === "starting"
                        || (progress.mail === "running" && (progress.mail_backfill !== true || lastSync <= 0))
                    readonly property bool syncing: !blocked && (syncState.changing === true || syncState.refresh_requested === true
                        || (syncState.active === true && ((mailGranted && mailSyncing)
                            || (calendarGranted && (progress.calendar === "starting" || progress.calendar === "running")))))
                    readonly property double lastSync: syncState.last_mail_sync_at || accountRow.last_mail_sync_at || 0
                    property int agoTick: 0
                    Timer { interval: 15000; repeat: true; running: syncButton.visible && syncButton.lastSync > 0; onTriggered: syncButton.agoTick++ }
                    readonly property string syncedAgo: {
                        if (lastSync <= 0) return ""
                        const seconds = Math.max(0, Math.round((Date.now() + agoTick * 0 - lastSync * 1000) / 1000))
                        return seconds < 5 ? "just now" : seconds < 60 ? seconds + " s ago" : seconds < 3600 ? Math.round(seconds / 60) + " min ago"
                            : seconds < 172800 ? Math.round(seconds / 3600) + " h ago" : Math.round(seconds / 86400) + " d ago"
                    }
                    // Icon plus time only; the tooltip carries the full state.
                    text: blocked || (syncState.active && !calendarGranted && progress.mail === "paused") ? "Retry"
                        : syncing ? "Syncing" : mailGranted && syncedAgo.length ? syncedAgo.replace(" ago", "") : "Sync"
                    iconName: "refresh"; iconSize: 11
                    height: 30; implicitHeight: 30; radius: 6; leftPadding: 10; rightPadding: 10; topPadding: 0; bottomPadding: 0
                    font.pointSize: Theme.fontPoints(12); font.features: {"tnum": 1}; hoverBrightness: 1
                    foreground: hovered && enabled ? Theme.text : Theme.inkAlpha(.5)
                    background: Rectangle { radius: 6; color: syncButton.hovered && syncButton.enabled ? Theme.inkAlpha(.05) : "transparent"; border.width: syncButton.visualFocus ? 1 : 0; border.color: Theme.focusBorder }
                    enabled: backend.connected && !backend.syntheticProfile && !syncState.changing
                        && (mailGranted || calendarGranted) && !accountRow.read_only
                    Accessible.name: syncing ? "Syncing" : mailGranted && syncedAgo.length ? "Synced " + syncedAgo : text
                    tip: (syncing ? "Syncing now. " : mailGranted && syncedAgo.length ? "Synced " + syncedAgo + ". " : "")
                        + (blocked ? "Keychain access was denied or is locked. Click to try again. macOS may ask for your login keychain password. "
                        : mailGranted && calendarGranted ? "Fetch new mail and Calendar events for this account. "
                        : calendarGranted ? "Fetch new Calendar events for this account. " : "Fetch new mail for this account. ")
                        + (mailGranted ? (lastSync > 0 ? "Last successful mail sync: " + new Date(lastSync * 1000).toLocaleString() : "No successful mail sync yet.") : "")
                    onClicked: backend.refreshGoogleSync(backend.account)
                }
                Rectangle {
                    id: searchBox; objectName: "searchBox"
                    Layout.preferredWidth: 300; Layout.minimumWidth: 140; Layout.maximumWidth: 300
                    Layout.fillWidth: true; implicitHeight: 30
                    color: Theme.surface; radius: 6
                    border.color: window.searchPopupOpen || search.activeFocus ? Theme.line3 : Theme.line
                    Behavior on border.color { ColorAnimation { duration: 150 } }
                    RowLayout {
                        anchors.fill: parent; anchors.leftMargin: 10; anchors.rightMargin: 6; spacing: 8
                        FmIcon { name: "search"; size: 12; color: Theme.inkAlpha(.5) }
                        TextField {
                            ContextMenu.menu: null
                            TextMenuHandler {}
                            id: search; objectName: "searchField"
                            Layout.fillWidth: true; padding: 0
                            placeholderText: window.workspace === "calendar" ? "Search events" : "Search " + (backend.currentAccount.name || "mail") + (backend.folder === "sent" ? " sent" : backend.folder === "drafts" ? " drafts" : "")
                            color: Theme.text; placeholderTextColor: Theme.faint
                            font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12); selectByMouse: true
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
                            Keys.onEscapePressed: { searchPopup.close(); calendarSearchPopup.close(); window.leaveSearch() }
                            // Enter runs the search, saves it to Recent and closes the dropdown.
                            Keys.onReturnPressed: window.commitSearch()
                            Keys.onEnterPressed: window.commitSearch()
                            TapHandler { onTapped: { if (window.settingsOpen) window.findMail(); else window.openSearch() } }
                            background: Item { }
                            Accessible.name: window.workspace === "calendar" ? "Search saved calendar events" : "Search current mailbox"
                        }
                        FmButton {
                            objectName: "clearSearch"
                            visible: search.text.length > 0; text: "clear"; implicitHeight: 20; leftPadding: 0; rightPadding: 0; topPadding: 0; bottomPadding: 0
                            font.pointSize: Theme.fontPoints(12); hoverBrightness: 1
                            foreground: hovered ? Theme.text : Theme.inkAlpha(.5); background: Item {}
                            tip: "Clear search"
                            onClicked: { if (window.workspace === "calendar") backend.calendarSearchQuery = ""; else backend.query = "" }
                        }
                        Rectangle {
                            implicitWidth: shortcutLabel.implicitWidth + 10; implicitHeight: 18; radius: 4; color: Theme.inkAlpha(.07)
                            Text { id: shortcutLabel; anchors.centerIn: parent; text: window.commandKey + "K"; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(10.5); color: Theme.inkAlpha(.5) }
                        }
                    }
                }
                FmButton {
                    id: settingsButton
                    objectName: "settingsButton"
                    width: 30; height: 30; implicitWidth: 30; implicitHeight: 30; leftPadding: 0; rightPadding: 0; topPadding: 0; bottomPadding: 0
                    iconName: "settings"; iconSize: 14; radius: 6; hoverBrightness: 1
                    prominent: window.settingsOpen
                    foreground: prominent ? Theme.canvas : hovered ? Theme.text : Theme.inkAlpha(.5)
                    background: Rectangle { radius: 6; color: settingsButton.prominent ? Theme.text : settingsButton.hovered ? Theme.inkAlpha(.05) : "transparent"; border.width: settingsButton.visualFocus ? 1 : 0; border.color: Theme.focusBorder }
                    tip: "Settings — " + ShortcutRegistry.label(ShortcutRegistry.sequence("settings", backend.preferences)).toLowerCase(); Accessible.name: "Settings"
                    onClicked: window.navigate(window.settingsOpen ? window.workspace : "settings")
                }
            }
        }
    }
    // The shortcut bar along the bottom: the current view's keys at the
    // left, the ones that work everywhere at the right. Settings → General
    // hides it.
    footer: KeyHints {
        objectName: "keyHints"
        visible: (backend.preferences || {}).key_hints !== false
        view: window.settingsOpen ? "settings" : window.workspace
    }
    ColumnLayout {
        id: workspaceLayer
        anchors.fill: parent; spacing: 0
        StackLayout {
            Layout.fillWidth: true; Layout.fillHeight: true
            currentIndex: window.settingsOpen ? 2 : window.workspace === "mail" ? 0 : 1
            // The timeline belongs to the Mail page, so switching workspaces
            // never resizes either view and neither has to lay out again.
            ColumnLayout {
                spacing: 0
                Item { id: timelineHeaderSlot; visible: window.timelineAtTop; Layout.fillWidth: true; implicitHeight: visible ? timeline.implicitHeight : 0 }
                MailView { id: mailView; Layout.fillWidth: true; Layout.fillHeight: true }
                Item { id: timelineFooterSlot; visible: !window.timelineAtTop; Layout.fillWidth: true; implicitHeight: visible ? timeline.implicitHeight : 0 }
            }
            CalendarView { id: calendarView; onOpenMailRequested: window.navigate("mail") }
            Loader {
                id: settingsLoader
                property bool built: false
                active: window.settingsOpen || built
                onLoaded: built = true
                sourceComponent: SettingsView { progress: accountProgress; onDone: window.navigate(window.workspace); onArchiveOpened: window.navigate("mail") }
            }
        }
    }
    TodayStrip {
        id: timeline
        parent: window.timelineAtTop ? timelineHeaderSlot : timelineFooterSlot
        width: parent.width; height: implicitHeight; visible: window.timelineVisible
        headerPosition: window.timelineAtTop
        onOpenCalendar: window.navigate("calendar")
    }
    ReminderPopup {
        anchors.fill: parent
        backgroundSource: workspaceLayer
        z: 300
        visible: backend.reminders.length > 0 && !startupOverlay.visible
    }
    ActionToast {
        anchors.horizontalCenter: parent.horizontalCenter; anchors.bottom: parent.bottom
        anchors.bottomMargin: 16 + (window.workspace === "mail" && window.timelineVisible && !window.timelineAtTop ? timeline.height : 0)
        maximumWidth: window.width * 0.7; z: 210
    }
    CalendarOperations { id: toastCalendarActivity }
    Connections { target: backend; function onCalendarActivityRequested() { toastCalendarActivity.open() } }

}
