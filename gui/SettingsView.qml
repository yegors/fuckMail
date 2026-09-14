import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Dialogs

Item {
    id: settings
    objectName: "settingsView"
    signal done()
    signal archiveOpened()
    property string page: "General"
    property var accountEdits: ({})
    property var pageScroll: ({})
    property string localError: ""
    property bool signaturesOpen: false
    property bool calendarDetailsOpen: false
    property string reconnectAccount: ""
    property double restoreRevision: -1
    property string syncAccount: ""
    property var quotaValues: ({})
    readonly property var quotaFields: [
        {key:"gmail_project_units_per_minute",label:"Gmail · project units / minute"},
        {key:"gmail_account_units_per_minute",label:"Gmail · user units / minute"},
        {key:"calendar_project_units_per_minute",label:"Calendar · project units / minute"},
        {key:"calendar_account_units_per_minute",label:"Calendar · user units / minute"}
    ]
    readonly property string modifier: Qt.platform.os === "osx" ? "⌘" : "Ctrl+"
    readonly property var hourLabels: Array.from({length:25}, (_, h) => (h < 10 ? "0" : "") + h + ":00")
    Connections {
        target: backend
        function onAccountsChanged() {
            const pending = Object.assign({}, settings.accountEdits)
            for (const account of backend.accounts) {
                if (!pending[account.id]) continue
                const fields = Object.assign({}, pending[account.id])
                if (fields.name === account.name) delete fields.name
                if (fields.signature_text === account.signature_text) delete fields.signature_text
                if (Object.keys(fields).length) pending[account.id] = fields
                else delete pending[account.id]
            }
            settings.accountEdits = pending
        }
        function onGoogleAuthChanged() { if (backend.googleAuth.phase === "connected") googleSignIn.close() }
        function onGoogleSyncChanged() {
            if (googleQuota.visible && settings.syncAccount.length && backend.googleSyncState(settings.syncAccount).owned) googleQuota.close()
        }
    }
    function accountValue(account, field) {
        const edited = accountEdits[account.id]
        return edited && edited[field] !== undefined ? edited[field] : account[field] || ""
    }
    function editSyncLimits(account) {
        syncAccount = account
        const active = backend.googleSyncLimits || {}
        quotaValues = Object.assign({},Object.keys(active).length ? active : backend.preferences.google_sync_limits || {})
        localError = ""; googleQuota.open()
    }
    function startSync(account) {
        const active = backend.googleSyncLimits || {}
        const limits = Object.keys(active).length ? active : backend.preferences.google_sync_limits || {}
        if (Object.keys(limits).length === 4) backend.startGoogleSync(account,limits)
        else editSyncLimits(account)
    }
    function syncProgress(state) {
        const progress = state.progress || {}, parts = []
        const names = {mail:"Mail",calendar:"Calendar",mail_mutations:"Mail changes",labels:"Labels"}
        for (const key of Object.keys(names)) {
            if (!progress[key]) continue
            const error = progress[key + "_error"]
            parts.push(names[key] + ": " + String(progress[key]).replace(/_/g," ") + (error ? " · " + String(error).replace(/_/g," ") : ""))
        }
        return parts.join("  /  ")
    }
    function saveSyncLimits() {
        const limits = {}
        for (const field of quotaFields) {
            const text = String(quotaValues[field.key] || "")
            const value = Number(text)
            if (!/^\d+$/.test(text) || value < 100 || value > 100000000) {
                localError = "Enter all four Console limits, from 100 to 100000000."; return
            }
            limits[field.key] = value
        }
        localError = ""; backend.clearError()
        if (syncAccount.length) backend.startGoogleSync(syncAccount,limits)
        else { backend.setPreferences({google_sync_limits:limits}); googleQuota.close() }
    }
    function changeAccount(account, field, value) {
        const copy = Object.assign({}, accountEdits), fields = Object.assign({}, copy[account.id] || {})
        fields[field] = value; copy[account.id] = fields; accountEdits = copy
    }
    function accountDirty(account) { return accountValue(account,"name") !== account.name || accountValue(account,"signature_text") !== (account.signature_text || "") }
    function saveAccount(account) {
        localError = ""
        const name = accountValue(account,"name").trim()
        if (!name.length || name.length > 80) { localError = "Use a display name between 1 and 80 characters."; return }
        backend.updateAccountPreferences(account.id,name,accountValue(account,"signature_text"))
    }
    function sourceSelected(source) {
        const chosen = backend.preferences.selected_calendars
        return chosen === null || chosen === undefined || chosen.some(item => item.account === source.account && item.calendar_id === source.calendar_id)
    }
    function calendarPermissionLabel(permission) { return ({none:"No access",free_busy:"Availability only",reader:"Can view events",writer_without_private_access:"Can edit public events",writer:"Can edit events",owner:"Owner"})[permission] || "Unknown access" }
    function saveHour(key, otherKey, value, isStart) {
        const other = Number(backend.preferences[otherKey])
        if ((isStart && value >= other) || (!isStart && value <= other)) { localError = "The end of the visible range must be later than its start."; return }
        localError = ""
        const patch = {}; patch[key] = value; backend.setPreferences(patch)
    }
    function selectPage(nextPage) {
        const scrolls = Object.assign({},pageScroll); scrolls[page] = contentFlick.contentY; pageScroll = scrolls
        page = nextPage; localError = ""
        Qt.callLater(function() { contentFlick.contentY = pageScroll[nextPage] || 0 })
    }
    FileDialog {
        id: googleClientFile
        title: "Choose the Google desktop client configuration"
        nameFilters: ["Google client configuration (*.json)"]
        fileMode: FileDialog.OpenFile
        onAccepted: googleConfigPath.text = selectedFile.toString()
    }
    Dialog {
        id: googleSignIn
        objectName: "googleSignInDialog"
        parent: Overlay.overlay
        anchors.centerIn: parent
        width: Math.min(520, parent.width - 32)
        modal: true
        padding: 24
        closePolicy: Popup.CloseOnEscape
        background: Rectangle { color: Theme.canvas; border.color: Theme.controlBorder; radius: 8 }
        onClosed: if (backend.googleAuthBusy) backend.cancelGoogleAuth()
        contentItem: ColumnLayout {
            spacing: 16
            property bool blocksWorkspaceShortcuts: true
            Heading { text: settings.restoreRevision > 0 ? "Restore archived account" : settings.reconnectAccount.length ? "Reconnect Google" : "Connect Google" }
            Description { text: settings.restoreRevision > 0 ? "Sign in as the same Google account. Your retained data stays here. Restoration does not start sync or Send." : "Choose your Google desktop client JSON file, then sign in with your browser. Google will ask for Mail and Calendar access." }
            RowLayout {
                Layout.fillWidth: true; spacing: 8
                FmTextField { id: googleConfigPath; objectName: "googleClientConfigPath"; Layout.fillWidth: true; placeholderText: "Desktop client JSON file"; enabled: !backend.googleAuthBusy; Accessible.name: "Google desktop client configuration file" }
                FmButton { text: "Choose file"; outlined: true; enabled: !backend.googleAuthBusy; onClicked: googleClientFile.open() }
            }
            CheckBox {
                id: renewGoogleAccess
                text: "Ask Google to renew access"
                enabled: !backend.googleAuthBusy
                font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(13)
                palette.windowText: Theme.text; palette.buttonText: Theme.text
            }
            Caption { text: renewGoogleAccess.checked ? "Use this if previous access expired or Google did not supply renewable access." : "You can connect personal Gmail and Google Workspace accounts." }
            Description { visible: (backend.googleAuth.message || "").length > 0; text: backend.googleAuth.message || "" }
            RowLayout {
                Layout.fillWidth: true; spacing: 10
                Item { Layout.fillWidth: true }
                FmButton { text: "Cancel"; onClicked: googleSignIn.close() }
                FmButton { objectName: "beginGoogleSignIn"; text: "Sign in"; prominent: true; enabled: backend.connected && !backend.googleAuthBusy && googleConfigPath.text.trim().length > 0; onClicked: { backend.clearError(); if (settings.restoreRevision > 0) backend.restoreGoogleAccount(googleConfigPath.text.trim(), settings.reconnectAccount, settings.restoreRevision, renewGoogleAccess.checked); else backend.connectGoogle(googleConfigPath.text.trim(), settings.reconnectAccount, renewGoogleAccess.checked) } }
            }
        }
    }
    Dialog {
        id: googleQuota
        objectName: "googleSyncLimitsDialog"
        parent: Overlay.overlay; anchors.centerIn: parent
        width: Math.min(540,parent.width-32); modal: true; padding: 24
        closePolicy: Popup.CloseOnEscape
        background: Rectangle { color: Theme.canvas; border.color: Theme.controlBorder; radius: 8 }
        contentItem: ColumnLayout {
            spacing: 14
            property bool blocksWorkspaceShortcuts: true
            Heading { text: "Google sync limits" }
            Description { text: "Enter the four limits shown for your project in Google Cloud Console. Mail and Calendar share these limits across your accounts; the app reserves 20% headroom." }
            Caption { visible: Object.keys(backend.googleSyncLimits || {}).length > 0; text: "These limits are active. Close all app and CLI sessions before changing them." }
            Repeater {
                model: settings.quotaFields
                ColumnLayout {
                    required property var modelData
                    Layout.fillWidth: true; spacing: 4
                    Caption { text: modelData.label }
                    FmTextField {
                        objectName: modelData.key
                        Layout.fillWidth: true; text: String(settings.quotaValues[modelData.key] || "")
                        placeholderText: "Confirmed Console limit"; maximumLength: 9
                        validator: IntValidator { bottom: 100; top: 100000000 }
                        enabled: Object.keys(backend.googleSyncLimits || {}).length === 0 && (!settings.syncAccount.length || !backend.googleSyncState(settings.syncAccount).changing)
                        Accessible.name: modelData.label
                        onTextEdited: { const next = Object.assign({},settings.quotaValues); next[modelData.key] = text; settings.quotaValues = next }
                    }
                }
            }
            Description { visible: settings.localError.length > 0 || backend.error.length > 0; text: settings.localError || backend.error; color: Theme.error }
            Caption { text: "Sync runs only for accounts you start. Closing the app stops its work." }
            RowLayout {
                Layout.fillWidth: true; spacing: 10
                Item { Layout.fillWidth: true }
                FmButton { text: "Cancel"; onClicked: googleQuota.close() }
                FmButton {
                    objectName: "saveGoogleSyncLimits"; text: settings.syncAccount.length ? "Start sync" : "Save limits"; prominent: true
                    enabled: backend.connected && (settings.syncAccount.length ? !backend.googleSyncState(settings.syncAccount).changing : Object.keys(backend.googleSyncLimits || {}).length === 0)
                    onClicked: settings.saveSyncLimits()
                }
            }
        }
    }
    component Heading: Text {
        color: Theme.text; font { family: Theme.fontFamily; pixelSize: Theme.fontSize(22); weight: Font.Medium; letterSpacing: -0.44 }
        Layout.fillWidth: true; textFormat: Text.PlainText; lineHeightMode: Text.FixedHeight; lineHeight: Theme.fontSize(22) * 1.1
    }
    component Description: Text {
        color: Theme.muted; font { family: Theme.fontFamily; pixelSize: Theme.fontSize(13.5) }
        Layout.fillWidth: true; wrapMode: Text.Wrap; lineHeightMode: Text.FixedHeight; lineHeight: Theme.fontSize(13.5) * 1.5; textFormat: Text.PlainText
    }
    component Caption: Text { color: Theme.subtle; font { family: Theme.fontFamily; pixelSize: Theme.fontSize(12) } Layout.fillWidth: true; wrapMode: Text.Wrap; lineHeightMode: Text.FixedHeight; lineHeight: Theme.fontSize(12) * 1.4 }
    component Divider: Rectangle { Layout.fillWidth: true; implicitHeight: 1; color: Theme.line }
    component KeyCap: Rectangle {
        property string label: ""
        implicitWidth: Math.max(22,keyText.implicitWidth + 12); implicitHeight: 22; color: "transparent"; border.color: Theme.controlBorder; radius: 4
        Text { id: keyText; anchors.centerIn: parent; text: parent.label; color: Theme.secondary; font { family: Theme.fontFamily; pixelSize: Theme.fontSize(12) } }
    }
    component PreferenceLabel: ColumnLayout {
        property string label: ""
        property string detail: ""
        Layout.fillWidth: true; spacing: 3
        Text { text: parent.label; color: Theme.text; font { family: Theme.fontFamily; pixelSize: Theme.fontSize(14) } Layout.fillWidth: true; wrapMode: Text.Wrap; lineHeightMode: Text.FixedHeight; lineHeight: Theme.fontSize(14) * 1.4 }
        Text { text: parent.detail; visible: text.length > 0; color: Theme.inkAlpha(0.5); font { family: Theme.fontFamily; pixelSize: Theme.fontSize(12.5) } Layout.fillWidth: true; wrapMode: Text.Wrap; lineHeightMode: Text.FixedHeight; lineHeight: Theme.fontSize(12.5) * 1.45 }
    }
    component ThemeSwatch: AbstractButton {
        id: swatch
        required property var paletteData
        objectName: "theme_" + paletteData.id
        implicitWidth: 68; implicitHeight: preview.implicitHeight
        checked: (backend.preferences.theme || "graphite") === paletteData.id
        onClicked: backend.setPreferences({theme:paletteData.id})
        contentItem: Column {
            id: preview; spacing: 6
            Rectangle {
                width: 68; height: 46; radius: 6; color: swatch.paletteData.bg
                Item {
                    anchors.fill: parent; anchors.margins: 2
                    Item {
                        width: parent.width; height: 10
                        Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 1; color: swatch.paletteData.line }
                        Rectangle { anchors.right: parent.right; anchors.rightMargin: 5; y: 3.5; width: 10; height: 2; radius: 1; color: swatch.paletteData.ink; opacity: 0.6 }
                    }
                    Item {
                        y: 10; width: parent.width; height: parent.height - 18
                        Item {
                            width: 37; height: parent.height
                            Rectangle { anchors.right: parent.right; width: 1; height: parent.height; color: swatch.paletteData.line }
                            Column {
                                x: 4; y: 4; spacing: 3
                                Rectangle { width: 28; height: 2; radius: 1; color: swatch.paletteData.ink; opacity: 0.9 }
                                Rectangle { width: 28; height: 2; radius: 1; color: swatch.paletteData.ink; opacity: 0.35 }
                                Rectangle { width: 28; height: 2; radius: 1; color: swatch.paletteData.ink; opacity: 0.35 }
                            }
                        }
                        Column {
                            x: 41; y: 4; width: parent.width - 45; spacing: 3
                            Rectangle { width: parent.width * 0.6; height: 3; radius: 1; color: swatch.paletteData.ink; opacity: 0.85 }
                            Rectangle { width: parent.width * 0.8; height: 2; radius: 1; color: swatch.paletteData.ink; opacity: 0.3 }
                        }
                    }
                    Rectangle {
                        anchors.bottom: parent.bottom; width: parent.width; height: 8; color: swatch.paletteData.bg0
                        bottomLeftRadius: 4; bottomRightRadius: 4
                        Rectangle { width: parent.width; height: 1; color: swatch.paletteData.line }
                        Row {
                            x: 5; y: 3.5; spacing: 2
                            Rectangle { width: 8; height: 2; color: swatch.paletteData.ink; opacity: 0.5 }
                            Rectangle { width: 6; height: 2; color: swatch.paletteData.accent }
                        }
                    }
                }
                Rectangle { anchors.fill: parent; radius: 6; color: "transparent"; border.width: 2; border.color: swatch.checked ? Theme.text : swatch.activeFocus ? Theme.focusBorder : Theme.controlBorder }
            }
            Text { width: 68; horizontalAlignment: Text.AlignHCenter; text: swatch.paletteData.name; color: swatch.checked ? Theme.text : Theme.inkAlpha(0.5); font { family: Theme.fontFamily; pixelSize: Theme.fontSize(11.5); weight: swatch.checked ? Font.Medium : Font.Normal } }
        }
        Accessible.name: paletteData.name + " theme"
        Accessible.role: Accessible.RadioButton
        Accessible.checked: checked
    }
    RowLayout {
        anchors.fill: parent; spacing: 0
        Rectangle {
            Layout.fillHeight: true; Layout.preferredWidth: 220; color: Theme.canvas
            Rectangle { anchors.right: parent.right; width: 1; height: parent.height; color: Theme.line }
            ColumnLayout {
                anchors.fill: parent; anchors.leftMargin: 24; anchors.rightMargin: 16; anchors.topMargin: 24; anchors.bottomMargin: 20; spacing: 2
                Heading { text: "Settings"; Layout.leftMargin: 10; Layout.bottomMargin: 18 }
                Repeater {
                    model: [{name:"General",icon:"general"},{name:"Accounts",icon:"accounts"},{name:"Local model",icon:"chip"},{name:"Mail",icon:"mail"},{name:"Calendar",icon:"calendar"},{name:"Shortcuts",icon:"keyboard"}]
                    FmButton {
                        required property var modelData
                        objectName: "settingsPage_" + modelData.name
                        text: modelData.name; iconName: modelData.icon; iconSpacing: 10; textAlignment: Text.AlignLeft
                        Layout.fillWidth: true; selected: settings.page === modelData.name
                        leftPadding: 10; rightPadding: 10
                        onClicked: settings.selectPage(modelData.name)
                    }
                }
                Item { Layout.fillHeight: true }
                Text {
                    text: "Everything here stays\non this machine"
                    color: Theme.faint; font { family: Theme.fontFamily; pixelSize: Theme.fontSize(11) } lineHeightMode: Text.FixedHeight; lineHeight: Theme.fontSize(11) * 1.5
                    Layout.fillWidth: true; Layout.leftMargin: 10
                    MouseArea { anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: settings.selectPage("Diagnostics") }
                    Accessible.name: "Local copy details"
                    Accessible.role: Accessible.Button
                    Accessible.onPressAction: settings.selectPage("Diagnostics")
                }
            }
        }
        Item {
            Layout.fillWidth: true; Layout.fillHeight: true
            FmButton {
                id: doneButton
                objectName: "settingsDoneButton"
                z: 2; anchors.top: parent.top; anchors.right: parent.right; anchors.topMargin: 22; anchors.rightMargin: 24
                text: "Done"; outlined: true; implicitWidth: doneContent.implicitWidth + 24
                onClicked: settings.done()
                background: Rectangle { radius: 5; color: doneButton.hovered ? Theme.surface : Theme.canvas; border.color: doneButton.activeFocus ? Theme.focusBorder : Theme.controlBorder }
                contentItem: Row {
                    id: doneContent; spacing: 8
                    Text { anchors.verticalCenter: parent.verticalCenter; text: "Done"; color: Theme.text; font { family: Theme.fontFamily; pixelSize: Theme.fontSize(13); weight: Font.Medium } }
                    KeyCap { anchors.verticalCenter: parent.verticalCenter; label: "Esc"; implicitHeight: 19; border.color: Theme.line }
                }
            }
            Flickable {
                id: contentFlick; objectName: "settingsScroll"
                anchors.fill: parent; contentWidth: width; contentHeight: contents.implicitHeight + 64
                clip: true; boundsBehavior: Flickable.StopAtBounds
                ScrollBar.vertical: ScrollBar {}
                ColumnLayout {
                    id: contents
                    x: 40; y: 24; width: Math.min(640,contentFlick.width - 80); spacing: 8
                    Heading { text: settings.page === "Diagnostics" ? "Local copy" : settings.page; Layout.rightMargin: 120 }
                    Text {
                        objectName: "settingsStatus"
                        visible: settings.localError.length > 0 || backend.preferencesStatus.indexOf("Not saved") === 0
                        text: settings.localError.length ? settings.localError : backend.preferencesStatus
                        color: Theme.error; font { family: Theme.fontFamily; pixelSize: Theme.fontSize(12) } wrapMode: Text.Wrap; Layout.fillWidth: true
                        Accessible.role: Accessible.AlertMessage
                    }
                    FmButton { visible: backend.preferencesPending && backend.preferencesStatus.indexOf("Not saved") === 0; text: "Retry display preferences"; outlined: true; onClicked: backend.setPreferences({}) }
                    Description { visible: Object.keys(settings.accountEdits).length > 0 && backend.preferencesStatus.indexOf("Not saved") === 0; text: "Your edits are still here. Use Save beside the account or signature to retry." }
                    ColumnLayout {
                        visible: settings.page === "General"; Layout.fillWidth: true; spacing: 0
                        Description { text: "How the whole app looks and behaves, across Mail and Calendar."; Layout.maximumWidth: 560; Layout.bottomMargin: 16 }
                        RowLayout {
                            Layout.fillWidth: true; Layout.topMargin: 14; Layout.bottomMargin: 14; spacing: 24
                            PreferenceLabel { label: "Theme"; detail: Theme.current.description; Layout.alignment: Qt.AlignTop }
                            Row { spacing: 10; Repeater { model: Theme.themes; ThemeSwatch { required property var modelData; paletteData: modelData } } }
                        }
                        Divider {}
                        RowLayout {
                            Layout.fillWidth: true; Layout.topMargin: 14; Layout.bottomMargin: 14; spacing: 24
                            PreferenceLabel { label: "Text size"; detail: "Scales every label, list and message." }
                            FmSegmented {
                                objectName: "textSizePreference"; model: ["Small","Default","Large"]; optionNames: ["textSize_small","textSize_default","textSize_large"]
                                currentIndex: ["small","default","large"].indexOf(backend.preferences.text_size || "default")
                                onActivated: function(index) { backend.setPreferences({text_size:["small","default","large"][index]}); currentIndex = Qt.binding(function() { return ["small","default","large"].indexOf(backend.preferences.text_size || "default") }) }
                                Accessible.name: "Text size"
                            }
                        }
                        Divider {}
                        RowLayout {
                            Layout.fillWidth: true; Layout.topMargin: 14; Layout.bottomMargin: 14; spacing: 24
                            PreferenceLabel { label: "Open on"; detail: "What the app shows at launch." }
                            FmSegmented {
                                objectName: "startupPreference"; model: ["Mail","Calendar","Where I left off"]; optionNames: ["startup_mail","startup_calendar","startup_last"]
                                currentIndex: ["mail","calendar","last"].indexOf(backend.preferences.startup_view || "mail")
                                onActivated: function(index) { backend.setPreferences({startup_view:["mail","calendar","last"][index]}); currentIndex = Qt.binding(function() { return ["mail","calendar","last"].indexOf(backend.preferences.startup_view || "mail") }) }
                                Accessible.name: "Open on"
                            }
                        }
                        Divider {}
                        RowLayout {
                            Layout.fillWidth: true; Layout.topMargin: 14; Layout.bottomMargin: 14; spacing: 24
                            PreferenceLabel { label: "Sync every"; detail: backend.syntheticProfile ? "Connect a live profile to sync Google accounts." : "Active mail and Calendar check about every minute. Limits and backoff can slow checks." }
                            FmSegmented { model: ["1 min","5 min","15 min"]; currentIndex: backend.syntheticProfile ? -1 : 0; enabled: false; Accessible.name: "Current sync cadence" }
                        }
                        FmButton { visible: !backend.syntheticProfile; Layout.alignment: Qt.AlignRight; Layout.bottomMargin: 14; text: "Google limits"; outlined: true; onClicked: settings.editSyncLimits("") }
                        Divider {}
                        RowLayout {
                            Layout.fillWidth: true; Layout.topMargin: 14; Layout.bottomMargin: 14; spacing: 24
                            PreferenceLabel { label: "Notifications"; detail: "Alerts while the app is open. Events appear five minutes before they start." }
                            FmSegmented { model: ["All","Events only","Off"]; currentIndex: ["all","events","off"].indexOf(backend.preferences.notifications || "events"); onActivated: function(index) { backend.setPreferences({notifications:["all","events","off"][index]}); currentIndex = Qt.binding(function() { return ["all","events","off"].indexOf(backend.preferences.notifications || "events") }) }; Accessible.name: "Notifications" }
                        }
                        Divider {}
                    }
                    ColumnLayout {
                        visible: settings.page === "Accounts"; Layout.fillWidth: true; spacing: 12
                        Description { text: "Each account has its own local mail and calendar. The display name is what the top bar shows."; Layout.maximumWidth: 560; Layout.bottomMargin: 12 }
                        Repeater {
                            model: backend.allAccounts
                            Rectangle {
                                required property var modelData
                                id: accountCard
                                readonly property var lifecycle: modelData.lifecycle || {}
                                readonly property bool archived: lifecycle.state === "removed"
                                readonly property var cleanup: lifecycle.cleanup || {}
                                readonly property var removal: (backend.accountLifecycle.operations || {})[modelData.id] || {}
                                readonly property var syncState: { const changed = backend.googleSync; return backend.googleSyncState(modelData.id) }
                                readonly property var coverage: (modelData.google || {}).sync || {}
                                readonly property int observed: backend.syntheticProfile ? modelData.message_count : Number(coverage.observed_messages || 0)
                                readonly property int indexed: backend.syntheticProfile ? Number((modelData.coverage || {}).indexed_messages || 0) : Number(coverage.indexed_messages || 0)
                                Layout.fillWidth: true; implicitHeight: accountContent.implicitHeight + 32
                                color: "transparent"; border.color: Theme.line; radius: 8
                                ColumnLayout {
                                    id: accountContent
                                    anchors { left: parent.left; right: parent.right; top: parent.top; leftMargin: 18; rightMargin: 18; topMargin: 16 }
                                    spacing: 12
                                    RowLayout {
                                        spacing: 12; Layout.fillWidth: true
                                        Rectangle { implicitWidth: 32; implicitHeight: 32; color: Theme.surface; radius: 6; FmIcon { anchors.centerIn: parent; name: Theme.accountIcon(accountCard.modelData.id); size: 16; color: Theme.secondary } }
                                        ColumnLayout {
                                            Layout.fillWidth: true; spacing: 2
                                            TextField {
                                                id: accountName
                                                readOnly: accountCard.archived
                                                objectName: "accountName_" + accountCard.modelData.id
                                                Layout.fillWidth: true; implicitHeight: 20; padding: 0
                                                text: settings.accountValue(accountCard.modelData,"name"); maximumLength: 80
                                                color: Theme.text; font { family: Theme.fontFamily; pixelSize: Theme.fontSize(14); weight: Font.Medium }
                                                selectByMouse: true; hoverEnabled: true
                                                background: Rectangle { color: "transparent"; Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 1; color: accountName.activeFocus ? Theme.focusBorder : accountName.hovered ? Theme.controlBorder : "transparent" } }
                                                Accessible.name: "Display name for " + accountCard.modelData.email
                                                onTextEdited: settings.changeAccount(accountCard.modelData,"name",text)
                                                onAccepted: settings.saveAccount(accountCard.modelData)
                                            }
                                            Text { text: accountCard.modelData.email + " · Mail and Calendar"; color: Theme.muted; font { family: Theme.fontFamily; pixelSize: Theme.fontSize(12) } elide: Text.ElideRight; Layout.fillWidth: true; textFormat: Text.PlainText }
                                        }
                                        Row { visible: !settings.accountDirty(accountCard.modelData); spacing: 7; FmIcon { name: "refresh"; size: 13; color: Theme.subtle; anchors.verticalCenter: parent.verticalCenter } Text { text: accountCard.archived ? "Archived" : backend.syntheticProfile ? "Local copy" : (accountCard.modelData.google || {}).connection_state === "connected" ? "Connected" : "Access paused"; color: Theme.subtle; font { family: Theme.fontFamily; pixelSize: Theme.fontSize(12) } } }
                                        FmButton { objectName: "saveAccount_" + accountCard.modelData.id; visible: !accountCard.archived && settings.accountDirty(accountCard.modelData); text: "Save"; outlined: true; onClicked: settings.saveAccount(accountCard.modelData) }
                                    }
                                    Divider {}
                                    RowLayout {
                                        Layout.fillWidth: true
                                        Caption { text: "Indexed " + accountCard.indexed + " of " + accountCard.observed + (backend.syntheticProfile ? " messages" : " observed messages") }
                                        Text { text: backend.syntheticProfile ? (accountCard.modelData.coverage || {}).text_index_complete ? "Complete" : "Building index" : accountCard.coverage.offline_complete ? "Offline copy complete" : accountCard.coverage.inventory_complete ? "Downloading content" : "Inventory incomplete"; color: Theme.subtle; font { family: Theme.fontFamily; pixelSize: Theme.fontSize(12) } }
                                    }
                                    Rectangle {
                                        Layout.fillWidth: true; implicitHeight: 2; color: Theme.hover; radius: 1; Layout.topMargin: -6
                                        Rectangle { height: parent.height; width: parent.width * Math.max(0,Math.min(1,accountCard.indexed / Math.max(1,accountCard.observed))); color: Theme.subtle; radius: 1 }
                                    }
                                    RowLayout {
                                        visible: !backend.syntheticProfile && !accountCard.archived; Layout.fillWidth: true; spacing: 10
                                        Caption {
                                            text: accountCard.syncState.changing ? "Updating sync…" : accountCard.syncState.active
                                                ? accountCard.syncState.owned ? "Syncing while this app is open" : "Sync active in another session"
                                                : "Sync paused · cached mail remains available"
                                        }
                                        FmButton {
                                            objectName: "googleSync_" + accountCard.modelData.id
                                            text: accountCard.syncState.owned ? "Pause" : accountCard.syncState.active ? "Keep syncing here" : "Start sync"
                                            outlined: true
                                            enabled: backend.connected && !accountCard.syncState.changing && (accountCard.modelData.google || {}).connection_state === "connected"
                                            onClicked: accountCard.syncState.owned ? backend.pauseGoogleSync(accountCard.modelData.id) : settings.startSync(accountCard.modelData.id)
                                        }
                                    }
                                    Caption { visible: !backend.syntheticProfile && text.length > 0; text: settings.syncProgress(accountCard.syncState) }
                                    RowLayout {
                                        visible: !backend.syntheticProfile && !accountCard.archived; Layout.fillWidth: true; spacing: 10
                                        Caption { text: ((accountCard.modelData.google || {}).connection_state === "reauthorization_required") ? "Sign in again to restore Google access." : (accountCard.modelData.coverage || {}).text || "Your downloaded copy stays on this machine." }
                                        FmButton { objectName: "reconnectGoogle_" + accountCard.modelData.id; text: "Reconnect"; outlined: true; enabled: backend.connected && !backend.googleAuthBusy; onClicked: { settings.restoreRevision = -1; settings.reconnectAccount = accountCard.modelData.id; renewGoogleAccess.checked = true; googleSignIn.open() } }
                                        FmButton { objectName: "disconnectGoogle_" + accountCard.modelData.id; text: "Disconnect"; enabled: backend.connected && !backend.googleAuthBusy && (accountCard.modelData.google || {}).connection_state !== "disconnected"; onClicked: backend.disconnectGoogle(accountCard.modelData.id) }
                                    }
                                    ColumnLayout {
                                        visible: !backend.syntheticProfile; Layout.fillWidth: true; spacing: 8
                                        Caption { visible: accountCard.archived; text: "Read-only archive · saved mail, drafts, files and history" }
                                        Caption { visible: accountCard.cleanup.state !== undefined; text: "Local credential cleanup: " + (accountCard.cleanup.state || "") + (accountCard.cleanup.code ? " · " + accountCard.cleanup.code : "") }
                                        Caption { visible: (accountCard.removal.status || "").length > 0; text: accountCard.removal.status || "" }
                                        Flow {
                                            Layout.fillWidth: true; spacing: 8
                                            FmButton { objectName: "removeAccount_" + accountCard.modelData.id; text: "Remove…"; visible: !accountCard.archived; enabled: backend.connected && !backend.googleAuthBusy && !accountCard.removal.busy; onClicked: backend.previewAccountRemoval(accountCard.modelData.id) }
                                            FmButton { objectName: "openAccountArchive_" + accountCard.modelData.id; text: "Open archive"; visible: accountCard.archived; outlined: true; onClicked: { backend.openAccountArchive(accountCard.modelData.id); if (!backend.unsavedAccountBuffer) settings.archiveOpened() } }
                                            FmButton { objectName: "retryAccountCleanup_" + accountCard.modelData.id; text: "Retry cleanup"; visible: accountCard.archived && (accountCard.cleanup.state === "pending" || accountCard.cleanup.state === "failed"); enabled: backend.connected && !accountCard.removal.busy; onClicked: backend.retryAccountCleanup(accountCard.modelData.id) }
                                            FmButton { objectName: "checkAccountRemoval_" + accountCard.modelData.id; text: "Check saved result"; visible: accountCard.removal.uncertain || false; enabled: backend.connected && !accountCard.removal.busy; onClicked: backend.checkAccountRemoval(accountCard.modelData.id, accountCard.removal.request_id) }
                                            FmButton { objectName: "retryAccountRemoval_" + accountCard.modelData.id; text: "Retry same removal"; visible: accountCard.removal.uncertain || false; enabled: backend.connected && !accountCard.removal.busy; onClicked: backend.retryAccountCleanup(accountCard.modelData.id) }
                                            FmButton { objectName: "restoreAccount_" + accountCard.modelData.id; text: "Restore…"; visible: accountCard.archived; enabled: backend.connected && !backend.googleAuthBusy && !accountCard.removal.busy && accountCard.cleanup.state === "complete"; onClicked: { settings.reconnectAccount = accountCard.modelData.id; settings.restoreRevision = accountCard.lifecycle.revision; renewGoogleAccess.checked = false; googleSignIn.open() } }
                                        }
                                    }
                                }
                            }
                        }
                        FmButton { objectName: "connectGoogleButton"; visible: !backend.syntheticProfile; text: "Connect Google account"; iconName: "plus"; outlined: true; enabled: backend.connected && !backend.googleAuthBusy; onClicked: { settings.restoreRevision = -1; settings.reconnectAccount = ""; renewGoogleAccess.checked = false; googleSignIn.open() } }
                        Caption { visible: !backend.syntheticProfile && (backend.googleAuth.message || "").length > 0; text: backend.googleAuth.message || "" }
                        Caption { Layout.topMargin: 4; visible: backend.syntheticProfile; text: "Open a separate live profile to connect your Google accounts." }
                    }
                    ColumnLayout {
                        visible: settings.page === "Local model"; Layout.fillWidth: true; spacing: 14
                        Description { text: "Optional summaries and category suggestions run on this machine. Humans write the mail and choose its state."; Layout.maximumWidth: 560; Layout.bottomMargin: 10 }
                        Rectangle {
                            Layout.fillWidth: true; implicitHeight: modelStatus.implicitHeight + 28; color: Theme.surface; radius: 8
                            RowLayout {
                                id: modelStatus; anchors { left: parent.left; right: parent.right; top: parent.top; margins: 16; topMargin: 14 } spacing: 14
                                Rectangle { implicitWidth: 8; implicitHeight: 8; radius: 4; color: Theme.subtle }
                                ColumnLayout { Layout.fillWidth: true; spacing: 3; Text { text: "No model — summaries unavailable"; color: Theme.text; font { family: Theme.fontFamily; pixelSize: Theme.fontSize(14); weight: Font.Medium } } Description { text: "This version displays the original message text. No model provider is connected." } }
                            }
                        }
                        Caption { text: "Provider detection and model selection are not available in this version." }
                    }
                    ColumnLayout {
                        visible: settings.page === "Mail"; Layout.fillWidth: true; spacing: 0
                        Description { text: "Three states: to review, delayed, done. Delay leaves the action with you until you choose Done."; Layout.maximumWidth: 560; Layout.bottomMargin: 16 }
                        RowLayout {
                            Layout.fillWidth: true; Layout.topMargin: 14; Layout.bottomMargin: 14; spacing: 24
                            PreferenceLabel { label: "Row density"; detail: "Comfortable is 52 px a row; compact is 44." }
                            FmSegmented {
                                objectName: "densityPreference"; model: ["Comfortable","Compact"]
                                currentIndex: backend.preferences.density === "compact" ? 1 : 0
                                onActivated: function(index) { backend.setPreferences({density:index === 1 ? "compact" : "comfortable"}); currentIndex = Qt.binding(function() { return backend.preferences.density === "compact" ? 1 : 0 }) }
                                Accessible.name: "Row density"
                            }
                        }
                        Divider {}
                        Caption { text: "Labels"; Layout.topMargin: 28 }
                        Description { text: "Labels used in this account. Change a conversation's labels from its reader."; Layout.topMargin: 6 }
                        Flow {
                            Layout.fillWidth: true; Layout.topMargin: 14; spacing: 6
                            Repeater {
                                model: backend.categories
                                Rectangle {
                                    required property var modelData
                                    implicitHeight: 28; implicitWidth: labelRow.implicitWidth + 22; color: "transparent"; border.color: Theme.controlBorder; radius: 14
                                    Row { id: labelRow; anchors.centerIn: parent; spacing: 6; Text { text: modelData.name; color: Theme.text; font { family: Theme.fontFamily; pixelSize: Theme.fontSize(12) } } Text { text: modelData.conversation_count; color: Theme.subtle; font { family: Theme.fontFamily; pixelSize: Theme.fontSize(11) } } }
                                }
                            }
                        }
                        GoogleLabelSetup { visible: !backend.syntheticProfile; Layout.fillWidth: true; Layout.topMargin: 16 }
                        FmButton { objectName: "settingsSignaturesButton"; Layout.topMargin: 24; text: "Signatures"; iconName: settings.signaturesOpen ? "down" : "next"; textAlignment: Text.AlignLeft; leftPadding: 0; onClicked: settings.signaturesOpen = !settings.signaturesOpen }
                        ColumnLayout {
                            visible: settings.signaturesOpen; Layout.fillWidth: true; spacing: 12
                            Description { text: "Applied to new drafts. Changing a signature leaves existing replies and your edits intact." }
                            Repeater {
                                model: backend.accounts
                                ColumnLayout {
                                    required property var modelData
                                    Layout.fillWidth: true; spacing: 8
                                    RowLayout { Layout.fillWidth: true; Text { text: modelData.name; color: Theme.text; font { family: Theme.fontFamily; pixelSize: Theme.fontSize(13); weight: Font.Medium } Layout.fillWidth: true } FmButton { objectName: "saveSignature_" + modelData.id; text: "Save signature"; enabled: settings.accountDirty(modelData); outlined: true; onClicked: settings.saveAccount(modelData) } }
                                    ScrollView {
                                        Layout.fillWidth: true; Layout.preferredHeight: 100
                                        TextArea {
                                            objectName: "signature_" + modelData.id
                                            text: settings.accountValue(modelData,"signature_text"); placeholderText: "Your signature"
                                            padding: 12; color: Theme.text; placeholderTextColor: Theme.faint; font { family: Theme.fontFamily; pixelSize: Theme.fontSize(13) }
                                            textFormat: TextEdit.PlainText; wrapMode: TextEdit.Wrap; selectByMouse: true
                                            onTextChanged: if (activeFocus) settings.changeAccount(modelData,"signature_text",text)
                                            background: Rectangle { color: Theme.surface; radius: 5; border.color: parent.activeFocus ? Theme.focusBorder : Theme.controlBorder }
                                            Accessible.name: "Signature for " + modelData.email
                                        }
                                    }
                                }
                            }
                        }
                    }
                    ColumnLayout {
                        visible: settings.page === "Calendar"; Layout.fillWidth: true; spacing: 0
                        RowLayout {
                            visible: !backend.syntheticProfile; Layout.fillWidth: true; Layout.bottomMargin: 20; spacing: 12
                            PreferenceLabel { label: "Display time zone"; detail: "Used for calendar grids, the Mail timeline and Today." }
                            FmTextField { id: displayTimeZone; objectName: "calendarDisplayTimeZone"; Layout.preferredWidth: 200; text: backend.preferences.calendar_display_time_zone || ""; placeholderText: "America/Toronto"; Accessible.name: "Calendar display time zone" }
                            FmButton { text: "Save"; outlined: true; enabled: displayTimeZone.text.trim().length > 0 && displayTimeZone.text.trim() !== backend.preferences.calendar_display_time_zone; onClicked: backend.setPreferences({calendar_display_time_zone:displayTimeZone.text.trim()}) }
                        }
                        Description { text: "One combined feed across your selected calendars. Events keep their account name and the details you have permission to see."; Layout.maximumWidth: 560; Layout.bottomMargin: 16 }
                        RowLayout {
                            Layout.fillWidth: true; Layout.topMargin: 14; Layout.bottomMargin: 14; spacing: 24
                            PreferenceLabel { label: "Visible calendars"; detail: "Selected independently of the mailbox you are reading." }
                            FmButton { objectName: "calendarDisplayButton"; text: "Calendars"; iconName: settings.calendarDetailsOpen ? "down" : "next"; outlined: true; onClicked: settings.calendarDetailsOpen = !settings.calendarDetailsOpen }
                        }
                        ColumnLayout {
                            visible: settings.calendarDetailsOpen; Layout.fillWidth: true; spacing: 12
                            RowLayout { Layout.fillWidth: true; Caption { text: "Visible calendars" } FmButton { objectName: "showAllCalendars"; text: "Show all"; onClicked: backend.setPreferences({selected_calendars:null}) } FmButton { objectName: "hideAllCalendars"; text: "Hide all"; onClicked: backend.setPreferences({selected_calendars:[]}) } }
                            Repeater {
                                model: backend.calendarSources
                                CheckBox {
                                    required property var modelData
                                    id: calendarCheck
                                    objectName: "calendarSelected_" + modelData.account + "_" + modelData.calendar_id
                                    Layout.fillWidth: true; implicitHeight: 46; spacing: 12; leftPadding: 28; rightPadding: 0
                                    checked: settings.sourceSelected(modelData); onClicked: backend.setCalendarSelected(modelData.account,modelData.calendar_id,checked)
                                    indicator: Rectangle { x: 0; y: (parent.height-height)/2; width: 16; height: 16; radius: 3; color: calendarCheck.checked ? Theme.text : "transparent"; border.color: calendarCheck.activeFocus ? Theme.focusBorder : calendarCheck.checked ? Theme.text : Theme.controlBorder; FmIcon { anchors.centerIn: parent; name: "check"; size: 13; color: Theme.canvas; visible: calendarCheck.checked } }
                                    contentItem: Column { spacing: 4; Text { text: modelData.name; color: Theme.text; font { family: Theme.fontFamily; pixelSize: Theme.fontSize(13) } } Text { text: modelData.account_name + " · " + settings.calendarPermissionLabel(modelData.permission); color: Theme.subtle; font { family: Theme.fontFamily; pixelSize: Theme.fontSize(12) } } }
                                    Accessible.name: modelData.name + " from " + modelData.account_name
                                }
                            }
                        }
                        Divider { }
                        RowLayout {
                            Layout.fillWidth: true; Layout.topMargin: 14; Layout.bottomMargin: 14; spacing: 24
                            PreferenceLabel { label: "New events last"; detail: "The starting duration. You can change it in the event." }
                            FmSegmented {
                                objectName: "eventDurationPreference"; model: ["30 min","45 min","1 h"]; optionNames: ["duration_30","duration_45","duration_60"]
                                currentIndex: [30,45,60].indexOf(Number(backend.preferences.default_event_duration_minutes || 60))
                                onActivated: function(index) { backend.setPreferences({default_event_duration_minutes:[30,45,60][index]}); currentIndex = Qt.binding(function() { return [30,45,60].indexOf(Number(backend.preferences.default_event_duration_minutes || 60)) }) }
                                Accessible.name: "New event duration"
                            }
                        }
                        Divider {}
                        Caption { text: "Timeline"; Layout.topMargin: 28 }
                        Description { text: "The day strip shown with Mail. Drag it sideways for adjacent days; click a block for details."; Layout.topMargin: 6; Layout.bottomMargin: 8 }
                        RowLayout {
                            Layout.fillWidth: true; Layout.topMargin: 14; Layout.bottomMargin: 14; spacing: 24
                            PreferenceLabel { label: "Placement"; detail: "Along the bottom, or directly under the top bar." }
                            FmSegmented {
                                objectName: "timelinePlacementPreference"; model: ["Footer","Header"]; optionNames: ["timeline_footer","timeline_header"]
                                currentIndex: backend.preferences.timeline_position === "header" ? 1 : 0
                                onActivated: function(index) { backend.setPreferences({timeline_position:index === 1 ? "header" : "footer"}); currentIndex = Qt.binding(function() { return backend.preferences.timeline_position === "header" ? 1 : 0 }) }
                                Accessible.name: "Timeline placement"
                            }
                        }
                        Divider {}
                        RowLayout {
                            Layout.fillWidth: true; Layout.topMargin: 14; Layout.bottomMargin: 14; spacing: 24
                            PreferenceLabel { label: "Timeline hours"; detail: "The Mail strip. Drag it sideways for the next or previous day." }
                            RowLayout {
                                spacing: 8
                                FmComboBox { objectName: "timeline_start_hour"; implicitWidth: 92; implicitHeight: 30; model: settings.hourLabels.slice(0,24); currentIndex: Number(backend.preferences.timeline_start_hour); onActivated: { settings.saveHour("timeline_start_hour","timeline_end_hour",currentIndex,true); currentIndex = Qt.binding(function() { return Number(backend.preferences.timeline_start_hour) }) } }
                                Text { text: "to"; color: Theme.subtle; font { family: Theme.fontFamily; pixelSize: Theme.fontSize(13) } }
                                FmComboBox { objectName: "timeline_end_hour"; implicitWidth: 92; implicitHeight: 30; model: settings.hourLabels.slice(1); currentIndex: Number(backend.preferences.timeline_end_hour)-1; onActivated: { settings.saveHour("timeline_end_hour","timeline_start_hour",currentIndex+1,false); currentIndex = Qt.binding(function() { return Number(backend.preferences.timeline_end_hour)-1 }) } }
                            }
                        }
                        Divider {}
                        RowLayout {
                            Layout.fillWidth: true; Layout.topMargin: 14; Layout.bottomMargin: 14; spacing: 24
                            Text { text: "Week starts on"; color: Theme.text; font { family: Theme.fontFamily; pixelSize: Theme.fontSize(14) } Layout.fillWidth: true }
                            FmSegmented { objectName: "weekStartPreference"; model: ["Monday","Sunday"]; currentIndex: backend.preferences.week_start === "sunday" ? 1 : 0; onActivated: function(index) { backend.setPreferences({week_start:index === 1 ? "sunday" : "monday"}); currentIndex = Qt.binding(function() { return backend.preferences.week_start === "sunday" ? 1 : 0 }) } Accessible.name: "Week starts on" }
                        }
                        Divider {}
                    }
                    ColumnLayout {
                        visible: settings.page === "Shortcuts"; Layout.fillWidth: true; spacing: 0
                        Description { text: "Single keys, no modifiers. They work whenever the cursor is not in a text field."; Layout.maximumWidth: 560 }
                        Repeater {
                            model: [
                                {group:"Mail",rows:[{keys:["j","k"],action:"Next · previous conversation"},{keys:["d"],action:"Delay · undelay"},{keys:["e"],action:"Done"},{keys:["r","a","f"],action:"Reply · Reply all · Forward"},{keys:["n"],action:"New message"},{keys:["s"],action:"Inbox · Sent · Drafts"},{keys:["/",settings.modifier+"K"],action:"Search"},{keys:["Tab","⇧Tab"],action:"Next · previous mailbox"}]},
                                {group:"Calendar",rows:[{keys:["d","w","m"],action:"Day · Week · Month"},{keys:["←","→"],action:"Previous · next"},{keys:["t"],action:"Today"},{keys:["n"],action:"New event"}]},
                                {group:"Everywhere",rows:[{keys:["v"],action:"Mail · Calendar"},{keys:[","],action:"Settings"},{keys:["Esc"],action:"Close search, event details, the event editor or settings · minimise a draft"}]}
                            ]
                            ColumnLayout {
                                required property var modelData
                                Layout.fillWidth: true; Layout.topMargin: 28; spacing: 14
                                Caption { text: modelData.group }
                                Repeater {
                                    model: modelData.rows
                                    RowLayout {
                                        required property var modelData
                                        Layout.fillWidth: true; spacing: 20
                                        Row { Layout.preferredWidth: 132; spacing: 4; Repeater { model: modelData.keys; KeyCap { required property var modelData; label: modelData } } }
                                        Text { text: modelData.action; color: Theme.text; font { family: Theme.fontFamily; pixelSize: Theme.fontSize(13) } Layout.fillWidth: true; wrapMode: Text.Wrap }
                                    }
                                }
                            }
                        }
                    }
                    ColumnLayout {
                        visible: settings.page === "Diagnostics"; Layout.fillWidth: true; spacing: 12
                        Description { text: "Local engine and stored-copy information."; Layout.bottomMargin: 12 }
                        Repeater {
                            model: Object.keys(backend.diagnostics)
                            ColumnLayout { required property string modelData; Layout.fillWidth: true; spacing: 6; Caption { text: modelData.replace(/_/g," ") } TextEdit { text: typeof backend.diagnostics[modelData] === "object" ? JSON.stringify(backend.diagnostics[modelData],null,2) : String(backend.diagnostics[modelData]); readOnly: true; selectByMouse: true; textFormat: TextEdit.PlainText; color: Theme.secondary; font { family: Theme.fontFamily; pixelSize: Theme.fontSize(12) } wrapMode: TextEdit.Wrap; Layout.fillWidth: true } }
                        }
                    }
                }
            }
        }
    }
}
