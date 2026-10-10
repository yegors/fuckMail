// The Settings workspace: the page list (General, Accounts, Local LLM, Mail,
// Calendar, Shortcuts, Diagnostics), each page's content and the account
// dialogs. Main.qml loads it on first open; each page is built when first shown.
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    id: settings
    objectName: "settingsView"
    TapHandler { onPressedChanged: if(pressed && settings.recordingShortcut) shortcutsPage.item.cancelOutside(point.scenePosition) }
    signal done()
    signal archiveOpened()
    property string page: "General"
    readonly property bool popupOpen: accountEditor.opened || googleSignIn.opened || modelUnsaved.opened || labelsDialog.opened || colorPicker.opened || signatureDialog.opened
    readonly property bool recordingShortcut: !!shortcutsPage.item && shortcutsPage.item.recording.length > 0
    property var pageScroll: ({})
    property string localError: ""
    property bool calendarDetailsOpen: false
    property string reconnectAccount: ""
    property double restoreRevision: -1
    AccountAnalysis { id: accountAnalysis; objectName: "accountAnalysis" }
    // Main hands in the app's pipeline model; loaded alone, Settings keeps its own.
    property var progress: localProgress
    AccountProgress { id: localProgress; active: settings.progress === localProgress }
    AccountPolicy { id: accountPolicy; onChanged: settings.progress.refresh(true) }
    AccountColorPicker { id: colorPicker }
    LabelsDialog { id: labelsDialog; onChanged: settings.labelsEdited(account) }
    SignatureDialog { id: signatureDialog }
    signal labelsEdited(string account)
    readonly property var subtitles: ({
        "General": "Appearance, text size and notifications for the whole app.",
        "Accounts": "Connect accounts and see what each one is syncing and processing.",
        "Local LLM": "Pick the on-device model that summarizes and labels your mail.",
        "Mail": "How the mail list looks, what stays blocked, and how images load.",
        "Calendar": "Defaults for new events, reminders and the calendar view.",
        "Shortcuts": "Keyboard shortcuts. Click any row to change it.",
        "Diagnostics": "Local engine and stored-copy information."
    })
    // AccountCard actions.
    function editAccount(row) { accountEditor.show(row) }
    function openLabels(account) { labelsDialog.show(account) }
    function editSignature(account) { signatureDialog.show(account) }
    function pickColor(account, x, y) { colorPicker.show(account, x, y) }
    function openArchive(account) { backend.openAccountArchive(account); if (!backend.unsavedAccountBuffer) archiveOpened() }
    function restore(row) {
        reconnectAccount = row.id; restoreRevision = (row.lifecycle || {}).revision
        googleSignIn.email = row.email; googleSignIn.renewAccess = false; googleSignIn.open()
    }
    readonly property string modifier: Qt.platform.os === "osx" ? "⌘" : "Ctrl+"
    readonly property var hourLabels: Array.from({length:25}, (_, h) => (h < 10 ? "0" : "") + h + ":00")
    function reconnect(row, access) {
        restoreRevision=-1; reconnectAccount=row.id
        googleSignIn.email=row.email;googleSignIn.renewAccess=true
        const choice=(backend.preferences.google_sync_services || {})[row.id] || {}
        googleSignIn.initialAccess=access || {mail:choice.mail===false ? "none" : choice.mail_access || "write",calendar:choice.calendar===false ? "none" : choice.calendar_access || "write"}
        googleSignIn.open()
    }
    AccountEditor {id:accountEditor;onReconnectRequested:(row,access)=>settings.reconnect(row,access)}
    function startSync(account) { backend.startGoogleSync(account, {}) }
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
    readonly property bool modelDirty: !!modelPage.item && modelPage.item.dirty
    property var pendingModelLeave: null
    function confirmLeave(action) {
        if (!modelDirty) { action(); return }
        pendingModelLeave=action; modelUnsaved.open()
    }
    function finishModelLeave() { const action=pendingModelLeave;pendingModelLeave=null;modelUnsaved.close();if(action)action() }
    function selectPage(nextPage) {
        if (page === "Local LLM" && modelDirty) { confirmLeave(function(){settings.selectPage(nextPage)}); return }

        const scrolls = Object.assign({},pageScroll); scrolls[page] = contentFlick.contentY; pageScroll = scrolls
        page = nextPage; localError = ""
        Qt.callLater(function() { contentFlick.contentY = pageScroll[nextPage] || 0 })
    }
    FmDialog {
        id:modelUnsaved; parent:Overlay.overlay; anchors.centerIn:parent; modal:true; title:"Save local model changes?"
        contentItem: Label {
            text: "Keep your endpoint, model and prompt changes before leaving this section?"
            color: Theme.inkAlpha(0.6); font.pointSize: Theme.fontPoints(12); wrapMode: Text.Wrap; lineHeightMode: Text.FixedHeight; lineHeight: Math.round(Theme.fontPixels(font) * 1.5); topPadding: Theme.leadingTop(font, lineHeight); bottomPadding: Theme.leadingBottom(font, lineHeight)
        }
        footer: RowLayout {
            spacing: 6
            Item { Layout.fillWidth: true }
            FmButton {text:"Stay";implicitHeight:32;outlined:true;onClicked:{settings.pendingModelLeave=null;modelUnsaved.close()}}
            FmButton {text:"Discard";implicitHeight:32;outlined:true;onClicked:{modelPage.item.discard();settings.finishModelLeave()}}
            FmButton {text:"Save";implicitHeight:32;prominent:true;enabled:!!modelPage.item && !modelPage.item.saving;onClicked:modelPage.item.save()}
            Item { Layout.preferredWidth: 16 }
        }
        bottomPadding: 18
    }
    GoogleConnectionDialog {
        id: googleSignIn
        reconnectAccount: settings.reconnectAccount
        restoreRevision: settings.restoreRevision
        onSyncRequested: function(account) { settings.startSync(account) }
    }
    component Heading: Text {
        property real leading: 1.1
        color: Theme.text; font { family: Theme.fontFamily; pointSize: Theme.fontPoints(20.5); weight: Font.Medium; letterSpacing: 0}
        Layout.fillWidth: true; textFormat: Text.PlainText; lineHeightMode: Text.FixedHeight; lineHeight: Math.round(Theme.fontPixels(font) * leading)
        topPadding: Theme.leadingTop(font, lineHeight); bottomPadding: Theme.leadingBottom(font, lineHeight)
    }
    component Description: Text {
        color: Theme.muted; font { family: Theme.fontFamily; pointSize: Theme.fontPoints(12.5) }
        Layout.fillWidth: true; wrapMode: Text.Wrap; lineHeightMode: Text.FixedHeight; lineHeight: Math.round(Theme.fontSize(12.5) * 1.5); textFormat: Text.PlainText
        topPadding: Theme.leadingTop(font, lineHeight); bottomPadding: Theme.leadingBottom(font, lineHeight)
    }
    component Caption: Text {
        color: Theme.subtle; font { family: Theme.fontFamily; pointSize: Theme.fontPoints(11.5) } Layout.fillWidth: true; wrapMode: Text.Wrap; lineHeightMode: Text.FixedHeight; lineHeight: Math.round(Theme.fontSize(11.5) * 1.4)
        topPadding: Theme.leadingTop(font, lineHeight); bottomPadding: Theme.leadingBottom(font, lineHeight)
    }
    component Divider: Rectangle { Layout.fillWidth: true; implicitHeight: 1; color: Theme.line }
    component KeyCap: Rectangle {
        property string label: ""
        implicitWidth: Math.max(22,keyText.implicitWidth + 12); implicitHeight: 22; color: "transparent"; border.color: Theme.controlBorder; radius: 5
        Text { id: keyText; anchors.centerIn: parent; text: parent.label; color: Theme.secondary; font { family: Theme.fontFamily; pointSize: Theme.fontPoints(11.5) } }
    }
    component PreferenceLabel: ColumnLayout {
        property string label: ""
        property string detail: ""
        Layout.fillWidth: true; spacing: 3
        Text { text: parent.label; color: Theme.text; font { family: Theme.fontFamily; pointSize: Theme.fontPoints(13) } Layout.fillWidth: true; wrapMode: Text.Wrap; lineHeightMode: Text.FixedHeight; lineHeight: Math.round(Theme.fontSize(13) * 1.4); topPadding: Theme.leadingTop(font, lineHeight); bottomPadding: Theme.leadingBottom(font, lineHeight) }
        Text { text: parent.detail; visible: text.length > 0; color: Theme.inkAlpha(0.5); font { family: Theme.fontFamily; pointSize: Theme.fontPoints(12) } Layout.fillWidth: true; wrapMode: Text.Wrap; lineHeightMode: Text.FixedHeight; lineHeight: Math.round(Theme.fontSize(12) * 1.45); topPadding: Theme.leadingTop(font, lineHeight); bottomPadding: Theme.leadingBottom(font, lineHeight) }
    }
    // Each page is built when it is first shown and kept afterwards.
    component SettingsPage: Loader {
        required property string name
        property bool built: false
        active: settings.page === name || built
        visible: settings.page === name
        onLoaded: built = true
        Layout.fillWidth: true
        // A page that limits its own width keeps that limit inside the loader.
        Layout.maximumWidth: item ? item.Layout.maximumWidth : Infinity
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
                width: 68; height: 46; radius: 5; color: swatch.paletteData.bg
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
                Rectangle { anchors.fill: parent; radius: 5; color: "transparent"; border.width: 2; border.color: swatch.checked ? Theme.text : swatch.visualFocus ? Theme.focusBorder : Theme.controlBorder }
            }
            Text { width: 68; horizontalAlignment: Text.AlignHCenter; text: swatch.paletteData.name; color: swatch.checked ? Theme.text : Theme.inkAlpha(0.5); font { family: Theme.fontFamily; pointSize: Theme.fontPoints(11); weight: swatch.checked ? Font.Medium : Font.Normal } lineHeightMode: Text.FixedHeight; lineHeight: Math.round(Theme.fontSize(11) * 1.4); topPadding: Theme.leadingTop(font, lineHeight); bottomPadding: Theme.leadingBottom(font, lineHeight) }
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
                Heading { text: "Settings"; leading: 1; Layout.leftMargin: 10; Layout.bottomMargin: 18 }
                Repeater {
                    model: [{name:"General",icon:"general"},{name:"Accounts",icon:"accounts"},{name:"Local LLM",icon:"chip"},{name:"Mail",icon:"mail"},{name:"Calendar",icon:"calendar"},{name:"Shortcuts",icon:"keyboard"}]
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
                    text: "Version " + Qt.application.version + " · everything here stays on this machine"; wrapMode: Text.Wrap
                    color: Theme.faint; font { family: Theme.fontFamily; pointSize: Theme.fontPoints(11) } lineHeightMode: Text.FixedHeight; lineHeight: Theme.fontSize(11) * 1.5
                    topPadding: Theme.leadingTop(font, lineHeight); bottomPadding: Theme.leadingBottom(font, lineHeight); Layout.rightMargin: 10
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
                onClicked: settings.confirmLeave(function(){settings.done()})
                background: Rectangle { radius: 5; color: doneButton.hovered ? Theme.surface : Theme.canvas; border.color: doneButton.visualFocus ? Theme.focusBorder : Theme.controlBorder }
                contentItem: Row {
                    id: doneContent; spacing: 8
                    Text { anchors.verticalCenter: parent.verticalCenter; text: "Done"; color: Theme.text; font { family: Theme.fontFamily; pointSize: Theme.fontPoints(12); weight: Font.Medium } }
                    KeyCap { anchors.verticalCenter: parent.verticalCenter; label: "Esc"; implicitHeight: 19; border.color: Theme.line }
                }
            }
            Flickable {
                id: contentFlick; objectName: "settingsScroll"
                anchors.fill: parent; contentWidth: width; contentHeight: contents.implicitHeight + 64
                clip: true; boundsBehavior: Flickable.StopAtBounds
                ScrollBar.vertical: FmScrollBar {}
                ColumnLayout {
                    id: contents
                    // One 880 px column for every section; rows fill it and only
                    // body paragraphs stop near 640 px.
                    x: 40; y: 24; width: Math.min(880, contentFlick.width - 80); spacing: 8
                    RowLayout {
                        Layout.fillWidth: true; Layout.rightMargin: 120; spacing: 10
                        FmIcon { visible: settings.page === "Local LLM"; name: "summary"; size: 16; color: Theme.ai }
                        Heading { text: settings.page === "Diagnostics" ? "Local copy" : settings.page }
                    }
                    Text {
                        objectName: "settingsSubtitle"
                        text: settings.subtitles[settings.page] || ""
                        Layout.fillWidth: true; Layout.bottomMargin: 8; elide: Text.ElideRight; maximumLineCount: 1
                        color: Theme.muted; font { family: Theme.fontFamily; pointSize: Theme.fontPoints(12.5) }
                    }
                    Text {
                        objectName: "settingsStatus"
                        visible: settings.localError.length > 0 || backend.preferencesStatus.indexOf("Not saved") === 0
                        text: settings.localError.length ? settings.localError : backend.preferencesStatus
                        color: Theme.error; font { family: Theme.fontFamily; pointSize: Theme.fontPoints(11.5) } wrapMode: Text.Wrap; Layout.fillWidth: true
                        Accessible.role: Accessible.AlertMessage
                    }
                    FmButton { visible: backend.preferencesPending && backend.preferencesStatus.indexOf("Not saved") === 0; text: "Retry display preferences"; outlined: true; onClicked: backend.setPreferences({}) }
                    SettingsPage {
                        name: "General"
                        sourceComponent: ColumnLayout {
                            spacing: 0
                            RowLayout {
                                Layout.fillWidth: true; Layout.topMargin: 14; Layout.bottomMargin: 14; spacing: 24
                                PreferenceLabel { label: "Theme"; detail: Theme.current.description; Layout.alignment: Qt.AlignTop }
                                Row { spacing: 10; Repeater { model: Theme.themes.filter(function(theme) { return theme.id !== "harbour" }); ThemeSwatch { required property var modelData; paletteData: modelData } } }
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
                                PreferenceLabel { label: "Motion"; detail: Theme.reducedMotion ? "Changes happen at once, without movement." : "Rows fold, menus and dialogs ease in, and finishing To review gets a small check." }
                                FmSegmented {
                                    objectName: "motionPreference"; model: ["System","Full","Reduced"]; optionNames: ["motion_system","motion_full","motion_reduced"]
                                    currentIndex: ["system","full","reduced"].indexOf(backend.preferences.motion || "system")
                                    onActivated: function(index) { backend.setPreferences({motion:["system","full","reduced"][index]}); currentIndex = Qt.binding(function() { return ["system","full","reduced"].indexOf(backend.preferences.motion || "system") }) }
                                    Accessible.name: "Motion"
                                }
                            }
                            Divider {}
                            RowLayout {
                                Layout.fillWidth: true; Layout.topMargin: 14; Layout.bottomMargin: 14; spacing: 24
                                PreferenceLabel { label: "Shortcut bar"; detail: "The keys for the current view, along the bottom of the window." }
                                FmSegmented {
                                    objectName: "keyHintsPreference"; model: ["Show","Hide"]
                                    currentIndex: backend.preferences.key_hints === false ? 1 : 0
                                    onActivated: function(index) { backend.setPreferences({key_hints:index === 0}); currentIndex = Qt.binding(function() { return backend.preferences.key_hints === false ? 1 : 0 }) }
                                    Accessible.name: "Shortcut bar"
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
                                PreferenceLabel { label: "Sync every"; detail: "Mail and calendar, per account. Sending is always immediate." }
                                FmSegmented { objectName: "syncIntervalPreference"; model: ["1 min","5 min","15 min"]; currentIndex: [1,5,15].indexOf(backend.preferences.sync_every_minutes || 1); onActivated: function(index) { backend.setPreferences({sync_every_minutes:[1,5,15][index]}); currentIndex = Qt.binding(function() { return [1,5,15].indexOf(backend.preferences.sync_every_minutes || 1) }) } Accessible.name: "Sync interval" }
                            }
                            Divider {}
                            RowLayout {
                                Layout.fillWidth: true; Layout.topMargin: 14; Layout.bottomMargin: 14; spacing: 24
                                PreferenceLabel { label: "Notifications"; detail: "Alerts while the app is open. Event reminders use the time you choose." }
                                FmSegmented { model: ["All","Events only","Off"]; currentIndex: ["all","events","off"].indexOf(backend.preferences.notifications || "events"); onActivated: function(index) { backend.setPreferences({notifications:["all","events","off"][index]}); currentIndex = Qt.binding(function() { return ["all","events","off"].indexOf(backend.preferences.notifications || "events") }) }; Accessible.name: "Notifications" }
                            }
                            Divider {}
                        }
                    }
                    SettingsPage {
                        name: "Accounts"
                        sourceComponent: ColumnLayout {
                            spacing: 12
                            Repeater {
                                model: backend.allAccountModel
                                AccountCard { progress: settings.progress; policy: accountPolicy; analysis: accountAnalysis; host: settings }
                            }
                            FmButton { objectName: "connectGoogleButton"; visible: !backend.syntheticProfile; text: "Connect Google account"; iconName: "plus"; outlined: true; enabled: backend.connected && !backend.googleAuthBusy; onClicked: { settings.restoreRevision = -1; settings.reconnectAccount = ""; googleSignIn.initialAccess=({mail:"write",calendar:"write"}); googleSignIn.email = ""; googleSignIn.renewAccess = false; googleSignIn.open() } }
                            Caption { visible: !backend.syntheticProfile && (backend.googleAuth.message || "").length > 0; text: backend.googleAuth.message || "" }
                            Caption { Layout.topMargin: 4; visible: backend.syntheticProfile; text: "Open a separate live profile to connect your Google accounts." }
                        }
                    }
                    SettingsPage { id: modelPage; name: "Local LLM"; sourceComponent: LocalModelSettings { onSaveCompleted: if(settings.pendingModelLeave)settings.finishModelLeave() } }
                    SettingsPage {
                        name: "Mail"
                        sourceComponent: ColumnLayout {
                            spacing: 0
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
                            RowLayout {
                                Layout.fillWidth: true; Layout.topMargin: 14; Layout.bottomMargin: 14; spacing: 24
                                PreferenceLabel { label: "Show drafts in the corner"; detail: "Unsent drafts for this account sit bottom-right, one click from where you left off." }
                                FmSegmented {
                                    objectName: "draftChipsPreference"; model: ["Show","Hide"]
                                    currentIndex: backend.preferences.show_draft_chips === false ? 1 : 0
                                    onActivated: function(index) { backend.setPreferences({show_draft_chips:index === 0}); currentIndex = Qt.binding(function() { return backend.preferences.show_draft_chips === false ? 1 : 0 }) }
                                    Accessible.name: "Show drafts in the corner"
                                }
                            }
                            Divider {}
                            RowLayout {
                                Layout.fillWidth: true; Layout.topMargin: 14; Layout.bottomMargin: 14; spacing: 24
                                PreferenceLabel { label: "Replying finishes Delayed mail"; detail: "Sending a reply in a Delayed thread marks it Done once Gmail accepts the reply." }
                                FmSegmented {
                                    objectName: "replyDonePreference"; model: ["On","Off"]
                                    currentIndex: backend.preferences.reply_marks_delayed_done === false ? 1 : 0
                                    onActivated: function(index) { backend.setPreferences({reply_marks_delayed_done:index === 0}); currentIndex = Qt.binding(function() { return backend.preferences.reply_marks_delayed_done === false ? 1 : 0 }) }
                                    Accessible.name: "Replying finishes Delayed mail"
                                }
                            }
                            Divider {}
                            RowLayout {
                                Layout.fillWidth: true; Layout.topMargin: 14; Layout.bottomMargin: 14; spacing: 24
                                PreferenceLabel { label: "Reviewed mail leaves To review"; detail: "Mail you open moves on to Done after this long, even while it stays open. Off waits until you open something else." }
                                FmSegmented {
                                    readonly property var seconds: [5, 10, 30, 0]
                                    objectName: "reviewLeavePreference"; model: ["5 s","10 s","30 s","Off"]
                                    currentIndex: Math.max(0, seconds.indexOf(Number(backend.preferences.review_leave_seconds ?? 10)))
                                    onActivated: function(index) { backend.setPreferences({review_leave_seconds:seconds[index]}); currentIndex = Qt.binding(function() { return Math.max(0, seconds.indexOf(Number(backend.preferences.review_leave_seconds ?? 10))) }) }
                                    Accessible.name: "Reviewed mail leaves To review"
                                }
                            }
                            Divider {}
                            RowLayout {
                                Layout.fillWidth: true; Layout.topMargin: 14; Layout.bottomMargin: 14; spacing: 24
                                PreferenceLabel { label: "Week starts on"; detail: "Where “This Week” begins in the mail list." }
                                FmComboBox {
                                    readonly property var days: ["monday","tuesday","wednesday","thursday","friday","saturday","sunday"]
                                    objectName: "mailWeekStartPreference"; implicitWidth: 140; implicitHeight: 32
                                    model: ["Monday","Tuesday","Wednesday","Thursday","Friday","Saturday","Sunday"]
                                    currentIndex: Math.max(0, days.indexOf(backend.preferences.mail_week_start))
                                    onActivated: function(index) { backend.setPreferences({mail_week_start:days[index]}); currentIndex = Qt.binding(function() { return Math.max(0, days.indexOf(backend.preferences.mail_week_start)) }) }
                                    Accessible.name: "Week starts on"
                                }
                            }
                            Divider {}
                            BlockedDomainsSettings { Layout.fillWidth: true; Layout.topMargin: 28 }
                            RemoteContentSettings { Layout.fillWidth: true; Layout.topMargin: 28 }
                        }
                    }
                    SettingsPage {
                        name: "Calendar"
                        sourceComponent: ColumnLayout {
                            spacing: 0
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
                            RowLayout {
                                Layout.fillWidth: true; Layout.topMargin: 14; Layout.bottomMargin: 14; spacing: 24
                                PreferenceLabel { label: "Default reminder"; detail: "New events start with this reminder. Existing events keep theirs." }
                                FmSegmented {
                                    objectName: "defaultReminderPreference"; model: ["5 min","15 min","1 h"]
                                    currentIndex: [5,15,60].indexOf(Number(backend.preferences.default_reminder_minutes || 15))
                                    onActivated: function(index) { backend.setPreferences({default_reminder_minutes:[5,15,60][index]}); currentIndex = Qt.binding(function() { return [5,15,60].indexOf(Number(backend.preferences.default_reminder_minutes || 15)) }) }
                                    Accessible.name: "Default reminder"
                                }
                            }
                            Divider {}
                            RowLayout {
                                Layout.fillWidth: true; Layout.topMargin: 14; Layout.bottomMargin: 14; spacing: 24
                                PreferenceLabel { label: "Snooze for"; detail: "How long a snoozed reminder waits before returning." }
                                FmSegmented {
                                    objectName: "reminderSnoozePreference"; model: ["1 min","5 min","10 min"]
                                    currentIndex: [1,5,10].indexOf(Number(backend.preferences.reminder_snooze_minutes || 5))
                                    onActivated: function(index) { backend.setPreferences({reminder_snooze_minutes:[1,5,10][index]}); currentIndex = Qt.binding(function() { return [1,5,10].indexOf(Number(backend.preferences.reminder_snooze_minutes || 5)) }) }
                                    Accessible.name: "Reminder snooze duration"
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
                                    Text { text: "to"; color: Theme.subtle; font { family: Theme.fontFamily; pointSize: Theme.fontPoints(12) } }
                                    FmComboBox { objectName: "timeline_end_hour"; implicitWidth: 92; implicitHeight: 30; model: settings.hourLabels.slice(1); currentIndex: Number(backend.preferences.timeline_end_hour)-1; onActivated: { settings.saveHour("timeline_end_hour","timeline_start_hour",currentIndex+1,false); currentIndex = Qt.binding(function() { return Number(backend.preferences.timeline_end_hour)-1 }) } }
                                }
                            }
                            Divider {}
                            RowLayout {
                                Layout.fillWidth: true; Layout.topMargin: 14; Layout.bottomMargin: 14; spacing: 24
                                Text { text: "Week starts on"; color: Theme.text; font { family: Theme.fontFamily; pointSize: Theme.fontPoints(13) } Layout.fillWidth: true }
                                FmSegmented { objectName: "weekStartPreference"; model: ["Monday","Sunday"]; currentIndex: backend.preferences.week_start === "sunday" ? 1 : 0; onActivated: function(index) { backend.setPreferences({week_start:index === 1 ? "sunday" : "monday"}); currentIndex = Qt.binding(function() { return backend.preferences.week_start === "sunday" ? 1 : 0 }) } Accessible.name: "Week starts on" }
                            }
                            Divider {}
                            FmButton { objectName: "calendarDisplayButton"; Layout.topMargin: 20; text: "Calendars and time zone"; iconName: settings.calendarDetailsOpen ? "down" : "next"; textAlignment: Text.AlignLeft; leftPadding: 0; onClicked: settings.calendarDetailsOpen = !settings.calendarDetailsOpen }
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
                                        indicator: Rectangle { x: 0; y: (parent.height-height)/2; width: 16; height: 16; radius: 5; color: calendarCheck.checked ? Theme.text : "transparent"; border.color: calendarCheck.visualFocus ? Theme.focusBorder : calendarCheck.checked ? Theme.text : Theme.controlBorder; FmIcon { anchors.centerIn: parent; name: "check"; size: 13; color: Theme.canvas; visible: calendarCheck.checked } }
                                        contentItem: Column { spacing: 4; Text { text: modelData.name; color: Theme.text; font { family: Theme.fontFamily; pointSize: Theme.fontPoints(12) } } Text { text: modelData.account_name + " · " + settings.calendarPermissionLabel(modelData.permission); color: Theme.subtle; font { family: Theme.fontFamily; pointSize: Theme.fontPoints(11.5) } } }
                                        Accessible.name: modelData.name + " from " + modelData.account_name
                                    }
                                }
                            }
                            RowLayout {
                                visible: settings.calendarDetailsOpen && !backend.syntheticProfile; Layout.fillWidth: true; Layout.topMargin: 12; spacing: 12
                                PreferenceLabel { label: "Display time zone"; detail: "Used for calendar grids, the Mail timeline and Today." }
                                FmTextField { id: displayTimeZone; objectName: "calendarDisplayTimeZone"; Layout.preferredWidth: 200; text: backend.preferences.calendar_display_time_zone || ""; placeholderText: "America/Toronto"; Accessible.name: "Calendar display time zone" }
                                FmButton { text: "Save"; outlined: true; enabled: displayTimeZone.text.trim().length > 0 && displayTimeZone.text.trim() !== backend.preferences.calendar_display_time_zone; onClicked: backend.setPreferences({calendar_display_time_zone:displayTimeZone.text.trim()}) }
                            }
                        }
                    }
                    SettingsPage { id: shortcutsPage; name: "Shortcuts"; sourceComponent: ShortcutSettings {} }
                    SettingsPage {
                        name: "Diagnostics"
                        sourceComponent: ColumnLayout {
                            spacing: 12
                            Repeater {
                                model: Object.keys(backend.diagnostics)
                                ColumnLayout { required property string modelData; Layout.fillWidth: true; spacing: 6; Caption { text: modelData.replace(/_/g," ") } TextEdit { text: typeof backend.diagnostics[modelData] === "object" ? JSON.stringify(backend.diagnostics[modelData],null,2) : String(backend.diagnostics[modelData]); readOnly: true; selectByMouse: true; textFormat: TextEdit.PlainText; color: Theme.secondary; font { family: Theme.fontFamily; pointSize: Theme.fontPoints(11.5) } wrapMode: TextEdit.Wrap; Layout.fillWidth: true } }
                            }
                        }
                    }
                }
            }
        }
    }
}
