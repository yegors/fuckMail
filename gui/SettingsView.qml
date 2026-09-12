import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    id: settings
    objectName: "settingsView"
    signal done()
    property string page: "Accounts"
    property var accountEdits: ({})
    property var pageScroll: ({})
    property string localError: ""
    readonly property string modifier: Qt.platform.os === "osx" ? "Cmd" : "Ctrl"
    readonly property var hourLabels: Array.from({length: 25}, function(_, h) { return (h < 10 ? "0" : "") + h + ":00" })

    Connections {
        target: backend
        function onAccountsChanged() {
            const pending = Object.assign({}, settings.accountEdits)
            for (let i = 0; i < backend.accounts.length; ++i) {
                const account = backend.accounts[i]
                if (!pending[account.id]) continue
                const fields = Object.assign({}, pending[account.id])
                if (fields.name === account.name) delete fields.name
                if (fields.signature_text === account.signature_text) delete fields.signature_text
                if (Object.keys(fields).length) pending[account.id] = fields
                else delete pending[account.id]
            }
            settings.accountEdits = pending
        }
    }

    function accountValue(account, field) {
        const edited = accountEdits[account.id]
        return edited && edited[field] !== undefined ? edited[field] : account[field] || ""
    }
    function changeAccount(account, field, value) {
        const copy = Object.assign({}, accountEdits)
        const fields = Object.assign({}, copy[account.id] || {})
        fields[field] = value
        copy[account.id] = fields
        accountEdits = copy
    }
    function accountDirty(account) {
        return accountValue(account, "name") !== account.name || accountValue(account, "signature_text") !== (account.signature_text || "")
    }
    function saveAccount(account) {
        localError = ""
        const name = accountValue(account, "name").trim()
        if (!name.length || name.length > 80) { localError = "Use a display name between 1 and 80 characters."; return }
        backend.updateAccountPreferences(account.id, name, accountValue(account, "signature_text"))
    }
    function sourceSelected(source) {
        const chosen = backend.preferences.selected_calendars
        if (chosen === null || chosen === undefined) return true
        for (let i = 0; i < chosen.length; ++i) {
            if (chosen[i].account === source.account && chosen[i].calendar_id === source.calendar_id) return true
        }
        return false
    }
    function calendarPermissionLabel(permission) {
        switch (permission) {
        case "none": return "No access"
        case "free_busy": return "Availability only"
        case "reader": return "Can view events"
        case "writer": return "Can edit events"
        case "owner": return "Owner"
        default: return "Unknown access"
        }
    }
    function saveHour(key, otherKey, value, isStart) {
        const other = Number(backend.preferences[otherKey])
        if ((isStart && value >= other) || (!isStart && value <= other)) {
            localError = "The end of the visible range must be later than its start."
            return
        }
        localError = ""
        const patch = {}; patch[key] = value; backend.setPreferences(patch)
    }
    function selectPage(nextPage) {
        const scrolls = Object.assign({}, pageScroll)
        scrolls[page] = contentFlick.contentY
        pageScroll = scrolls
        page = nextPage
        localError = ""
        Qt.callLater(function() { contentFlick.contentY = pageScroll[nextPage] || 0 })
    }

    component Heading: Text {
        color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: 22; font.weight: Font.Medium
        Layout.fillWidth: true; textFormat: Text.PlainText
    }
    component Description: Text {
        color: Theme.muted; font.family: Theme.fontFamily; font.pixelSize: 13
        Layout.fillWidth: true; wrapMode: Text.Wrap; lineHeight: 1.5; textFormat: Text.PlainText
    }
    component SectionLabel: Text {
        color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: 14; font.weight: Font.Medium
        Layout.fillWidth: true; textFormat: Text.PlainText
    }
    component Divider: Rectangle { Layout.fillWidth: true; implicitHeight: 1; color: Theme.line }

    RowLayout {
        anchors.fill: parent; spacing: 0
        Rectangle {
            Layout.fillHeight: true; Layout.preferredWidth: 220; color: Theme.canvas
            Rectangle { anchors.right: parent.right; width: 1; height: parent.height; color: Theme.line }
            ColumnLayout {
                anchors.fill: parent; anchors.leftMargin: 24; anchors.rightMargin: 16; anchors.topMargin: 24; anchors.bottomMargin: 20; spacing: 2
                Heading { text: "Settings"; Layout.leftMargin: 10; Layout.bottomMargin: 20 }
                Repeater {
                    model: [{name:"Accounts", icon:"accounts"}, {name:"Mail", icon:"mail"}, {name:"Calendar", icon:"calendar"}, {name:"Shortcuts", icon:"keyboard"}, {name:"Diagnostics", icon:"info"}]
                    FmButton {
                        required property var modelData
                        objectName: "settingsPage_" + modelData.name
                        text: modelData.name; iconName: modelData.icon; textAlignment: Text.AlignLeft
                        Layout.fillWidth: true; selected: settings.page === modelData.name
                        onClicked: settings.selectPage(modelData.name)
                    }
                }
                Item { Layout.fillHeight: true }
                Text {
                    text: "Local fixture\nMail and calendar stay on this machine."
                    color: Theme.subtle; font.family: Theme.fontFamily; font.pixelSize: 11
                    wrapMode: Text.Wrap; Layout.fillWidth: true; Layout.leftMargin: 10; lineHeight: 1.5
                }
            }
        }
        Item {
            Layout.fillWidth: true; Layout.fillHeight: true
            FmButton {
                objectName: "settingsDoneButton"
                z: 2; anchors.top: parent.top; anchors.right: parent.right; anchors.topMargin: 22; anchors.rightMargin: 24
                text: "Done  Esc"; outlined: true; onClicked: settings.done()
            }
            Flickable {
                id: contentFlick
                objectName: "settingsScroll"
                anchors.fill: parent
                contentWidth: width; contentHeight: contents.implicitHeight + 64
                clip: true; boundsBehavior: Flickable.StopAtBounds
                ScrollBar.vertical: ScrollBar { }
                ColumnLayout {
                    id: contents
                    x: 40; y: 24; width: Math.min(640, contentFlick.width - 80); spacing: 10
                    Heading { text: settings.page; Layout.rightMargin: 110 }
                    Text {
                        objectName: "settingsStatus"
                        visible: settings.localError.length > 0 || backend.preferencesStatus.length > 0
                        text: settings.localError.length ? settings.localError : backend.preferencesStatus
                        color: settings.localError.length ? Theme.error : Theme.muted
                        font.family: Theme.fontFamily; font.pixelSize: 12; wrapMode: Text.Wrap; Layout.fillWidth: true
                        Accessible.role: Accessible.AlertMessage
                    }
                    FmButton {
                        visible: backend.preferencesPending && backend.preferencesStatus.indexOf("Not saved") === 0
                        text: "Retry display preferences"; outlined: true
                        onClicked: backend.setPreferences({})
                    }
                    Description {
                        visible: settings.page === "Accounts" && Object.keys(settings.accountEdits).length > 0 && backend.preferencesStatus.indexOf("Not saved") === 0
                        text: "Your account edits are still here. Use Save beside the account to retry."
                    }
                    ColumnLayout {
                        visible: settings.page === "Accounts"
                        Layout.fillWidth: true; spacing: 12
                        Description { text: "Each account has its own mail. Calendar combines the details you can see. The display name is what the top bar shows."; Layout.bottomMargin: 12 }
                        Repeater {
                            model: backend.accounts
                            Rectangle {
                                required property var modelData
                                Layout.fillWidth: true; implicitHeight: accountContent.implicitHeight + 32
                                color: "transparent"; border.color: Theme.line; radius: 8
                                ColumnLayout {
                                    id: accountContent
                                    anchors.fill: parent; anchors.margins: 16; spacing: 12
                                    RowLayout {
                                        spacing: 12
                                        Rectangle { implicitWidth: 32; implicitHeight: 32; color: Theme.surface; radius: 6; FmIcon { anchors.centerIn: parent; name: "accounts"; size: 16; color: Theme.text } }
                                        ColumnLayout {
                                            Layout.fillWidth: true; spacing: 5
                                            FmTextField {
                                                objectName: "accountName_" + modelData.id
                                                Layout.fillWidth: true; text: settings.accountValue(modelData, "name"); maximumLength: 80
                                                font.weight: Font.Medium
                                                Accessible.name: "Display name for " + modelData.email
                                                onTextEdited: settings.changeAccount(modelData, "name", text)
                                                onAccepted: settings.saveAccount(modelData)
                                            }
                                            Text { text: modelData.email; color: Theme.muted; font.family: Theme.fontFamily; font.pixelSize: 12; textFormat: Text.PlainText; elide: Text.ElideRight; Layout.fillWidth: true }
                                        }
                                        FmButton { objectName: "saveAccount_" + modelData.id; text: "Save"; outlined: true; enabled: settings.accountDirty(modelData); onClicked: settings.saveAccount(modelData) }
                                    }
                                    Divider { }
                                    RowLayout {
                                        Text { text: modelData.conversation_count + " local conversations"; color: Theme.muted; font.family: Theme.fontFamily; font.pixelSize: 12; Layout.fillWidth: true }
                                        Text { text: "Synthetic account"; color: Theme.subtle; font.family: Theme.fontFamily; font.pixelSize: 12 }
                                    }
                                }
                            }
                        }
                        Description { Layout.topMargin: 10; text: "Live Google sign-in and account removal arrive with the Google connection workflow." }
                    }
                    ColumnLayout {
                        visible: settings.page === "Mail"
                        Layout.fillWidth: true; spacing: 18
                        Description { text: "Keep the conversation list comfortable and give each account its own signature." }
                        RowLayout {
                            Layout.fillWidth: true; Layout.topMargin: 8
                            ColumnLayout {
                                Layout.fillWidth: true; spacing: 4
                                SectionLabel { text: "Conversation density" }
                                Description { text: "Comfortable gives each row more breathing room." }
                            }
                            FmComboBox {
                                objectName: "densityPreference"
                                model: ["Comfortable", "Compact"]
                                currentIndex: backend.preferences.density === "compact" ? 1 : 0
                                onActivated: backend.setPreferences({density: currentIndex === 1 ? "compact" : "comfortable"})
                                Accessible.name: "Conversation density"
                            }
                        }
                        Divider { }
                        SectionLabel { text: "Signatures" }
                        Description { text: "Applied to new replies from this account. Changing a signature leaves existing drafts and your edits intact." }
                        Repeater {
                            model: backend.accounts
                            ColumnLayout {
                                required property var modelData
                                Layout.fillWidth: true; spacing: 8
                                RowLayout {
                                    SectionLabel { text: modelData.name }
                                    FmButton { objectName: "saveSignature_" + modelData.id; text: "Save signature"; outlined: true; enabled: settings.accountDirty(modelData); onClicked: settings.saveAccount(modelData) }
                                }
                                Text { text: modelData.email; color: Theme.subtle; font.family: Theme.fontFamily; font.pixelSize: 12; textFormat: Text.PlainText }
                                ScrollView {
                                    Layout.fillWidth: true; Layout.preferredHeight: 115
                                    TextArea {
                                        objectName: "signature_" + modelData.id
                                        text: settings.accountValue(modelData, "signature_text")
                                        placeholderText: "Your signature"
                                        padding: 12; color: Theme.text; placeholderTextColor: Theme.subtle
                                        font.family: Theme.fontFamily; font.pixelSize: 13
                                        textFormat: TextEdit.PlainText; wrapMode: TextEdit.Wrap; selectByMouse: true
                                        onTextChanged: if (activeFocus) settings.changeAccount(modelData, "signature_text", text)
                                        background: Rectangle { color: Theme.surface; radius: 6; border.color: parent.activeFocus ? Theme.accent : Theme.controlBorder }
                                        Accessible.name: "Signature for " + modelData.email
                                    }
                                }
                            }
                        }
                        Divider { }
                        Description { text: "Opening mail reviews it. Delay keeps an untimed hold; Done clears unread attention and the hold. These actions always belong to you." }
                    }
                    ColumnLayout {
                        visible: settings.page === "Calendar"
                        Layout.fillWidth: true; spacing: 18
                        Description { text: "One view of the calendars you choose, with each source's permitted details. These choices also apply to the timeline under Mail." }
                        RowLayout {
                            Layout.fillWidth: true; Layout.topMargin: 8
                            SectionLabel { text: "Start the week on" }
                            FmComboBox {
                                objectName: "weekStartPreference"
                                model: ["Monday", "Sunday"]
                                currentIndex: backend.preferences.week_start === "sunday" ? 1 : 0
                                onActivated: backend.setPreferences({week_start: currentIndex === 1 ? "sunday" : "monday"})
                                Accessible.name: "Week starts on"
                            }
                        }
                        Divider { }
                        Repeater {
                            model: [{title:"Calendar hours",note:"Visible hours in day and week views.",start:"calendar_start_hour",end:"calendar_end_hour"}, {title:"Mail timeline hours",note:"The time range beneath your mail.",start:"timeline_start_hour",end:"timeline_end_hour"}]
                            ColumnLayout {
                                required property var modelData
                                Layout.fillWidth: true; spacing: 7
                                SectionLabel { text: modelData.title }
                                RowLayout {
                                    Layout.fillWidth: true; spacing: 10
                                    Description { text: modelData.note }
                                    FmComboBox {
                                        objectName: modelData.start; implicitWidth: 94
                                        model: settings.hourLabels.slice(0,24)
                                        currentIndex: Number(backend.preferences[modelData.start] || 0)
                                        onActivated: { settings.saveHour(modelData.start, modelData.end, currentIndex, true); currentIndex = Qt.binding(function() { return Number(backend.preferences[modelData.start]) }) }
                                        Accessible.name: modelData.title + " start"
                                    }
                                    Text { text: "to"; color: Theme.subtle; font.family: Theme.fontFamily; font.pixelSize: 12 }
                                    FmComboBox {
                                        objectName: modelData.end; implicitWidth: 94
                                        model: settings.hourLabels.slice(1)
                                        currentIndex: Math.max(0, Number(backend.preferences[modelData.end] || 24) - 1)
                                        onActivated: { settings.saveHour(modelData.end, modelData.start, currentIndex + 1, false); currentIndex = Qt.binding(function() { return Number(backend.preferences[modelData.end]) - 1 }) }
                                        Accessible.name: modelData.title + " end"
                                    }
                                }
                            }
                        }
                        Description { text: "Events outside these hours remain available in the agenda and overflow details." }
                        Divider { }
                        RowLayout {
                            SectionLabel { text: "Visible calendars" }
                            FmButton { objectName: "showAllCalendars"; text: "Show all"; onClicked: backend.setPreferences({selected_calendars: null}) }
                            FmButton { objectName: "hideAllCalendars"; text: "Hide all"; onClicked: backend.setPreferences({selected_calendars: []}) }
                        }
                        Repeater {
                            model: backend.calendarSources
                            CheckBox {
                                required property var modelData
                                id: calendarCheck
                                objectName: "calendarSelected_" + modelData.account + "_" + modelData.calendar_id
                                Layout.fillWidth: true
                                implicitHeight: 50; spacing: 12; leftPadding: 28; rightPadding: 0
                                checked: settings.sourceSelected(modelData)
                                onClicked: backend.setCalendarSelected(modelData.account, modelData.calendar_id, checked)
                                indicator: Rectangle {
                                    x: 0; y: (parent.height - height)/2; width: 16; height: 16; radius: 3
                                    color: calendarCheck.checked ? Theme.text : "transparent"
                                    border.color: calendarCheck.activeFocus ? Theme.accent : calendarCheck.checked ? Theme.text : Theme.controlBorder
                                    FmIcon { anchors.centerIn: parent; name: "check"; size: 13; color: Theme.canvas; visible: calendarCheck.checked }
                                }
                                contentItem: Column {
                                    spacing: 4
                                    Text { text: modelData.name; color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: 13; textFormat: Text.PlainText }
                                    Text { text: modelData.account_name + " · " + settings.calendarPermissionLabel(modelData.permission); color: Theme.subtle; font.family: Theme.fontFamily; font.pixelSize: 12; textFormat: Text.PlainText }
                                }
                                Accessible.name: modelData.name + " from " + modelData.account_name
                            }
                        }
                        Description { text: "Selecting a mailbox never changes your calendar selection." }
                    }
                    ColumnLayout {
                        visible: settings.page === "Shortcuts"
                        Layout.fillWidth: true; spacing: 12
                        Description { text: "The same actions are also available with the mouse. Typing in a reply stays in the editor."; Layout.bottomMargin: 12 }
                        Repeater {
                            model: [{action:"Search current mailbox",key:settings.modifier + " F / " + settings.modifier + " K"}, {action:"Open Mail",key:settings.modifier + " 1"}, {action:"Open Calendar",key:settings.modifier + " 2"}, {action:"Open Settings",key:settings.modifier + " ,"}, {action:"Close Settings or a popover",key:"Esc"}, {action:"Move between controls",key:"Tab / Shift Tab"}, {action:"Activate focused control",key:"Enter / Space"}, {action:"Undo in a text editor",key:settings.modifier + " Z"}]
                            Rectangle {
                                required property var modelData
                                Layout.fillWidth: true; implicitHeight: 42; color: "transparent"
                                RowLayout {
                                    anchors.fill: parent
                                    Text { text: modelData.action; color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: 13; Layout.fillWidth: true }
                                    Rectangle {
                                        implicitWidth: keyText.implicitWidth + 14; implicitHeight: 25; radius: 4; color: Theme.surface; border.color: Theme.line
                                        Text { id: keyText; anchors.centerIn: parent; text: modelData.key; color: Theme.muted; font.family: Theme.fontFamily; font.pixelSize: 12 }
                                    }
                                }
                                Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 1; color: Theme.line }
                            }
                        }
                    }
                    ColumnLayout {
                        visible: settings.page === "Diagnostics"
                        Layout.fillWidth: true; spacing: 12
                        Description { text: "Actual local engine information. This profile uses synthetic data; no live mailbox or model is connected."; Layout.bottomMargin: 12 }
                        Repeater {
                            model: Object.keys(backend.diagnostics)
                            ColumnLayout {
                                required property string modelData
                                Layout.fillWidth: true; spacing: 6
                                SectionLabel { text: modelData.replace(/_/g, " ") }
                                TextEdit {
                                    text: typeof backend.diagnostics[modelData] === "object" ? JSON.stringify(backend.diagnostics[modelData], null, 2) : String(backend.diagnostics[modelData])
                                    readOnly: true; selectByMouse: true; textFormat: TextEdit.PlainText
                                    color: Theme.muted; font.family: Theme.fontFamily; font.pixelSize: 12; wrapMode: TextEdit.Wrap
                                    Layout.fillWidth: true
                                }
                                Divider { }
                            }
                        }
                        Description { visible: Object.keys(backend.diagnostics).length === 0; text: backend.connected ? "Local engine connected." : "Waiting for the local engine." }
                        Description { text: "Local assistance is not configured. Reading, composing, searching and calendar actions work without a model." }
                    }
                }
            }
        }
    }
}
