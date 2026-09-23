import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

FmDialog {
    id: googleSignIn; objectName: "googleSignInDialog"
    property string reconnectAccount: ""
    property var initialAccess: ({mail:"write",calendar:"write"})
    property double restoreRevision: -1
    property alias email: googleEmail.text
    property alias renewAccess: renewGoogleAccess.checked
    signal syncRequested(string account)
    width: Math.min(440, parent.width - 48); padding: 24; bottomPadding: 0; spacing: 14
    height: Math.min(implicitHeight, parent.height - 48)
    property int step: 0
    property string connectedAccount: ""
    property bool finishing: false
    property bool authStarted: false
    readonly property var retentionKeys: ["90_days", "1_year", "2_years", "5_years", "all"]
    readonly property var accountRow: backend.accounts.find(function(account) { return account.id === googleSignIn.connectedAccount }) || ({})
    readonly property var requestedAccess: ({mail: connectMail.checked ? (connectMailMode.currentIndex === 0 ? "read" : "write") : "none", calendar: connectCalendar.checked ? (connectCalendarMode.currentIndex === 0 ? "read" : "write") : "none"})
    onOpened: {
        step = 0; connectedAccount = ""; finishing = false; authStarted = false; connectName.text = ""
        connectRetention.currentIndex = 1
        connectMail.checked = initialAccess.mail!=="none"; connectCalendar.checked = initialAccess.calendar!=="none"; connectMailMode.currentIndex = initialAccess.mail==="read" ? 0 : 1; connectCalendarMode.currentIndex = initialAccess.calendar==="read" ? 0 : 1
        backend.prepareGoogleSignIn(); googleEmail.forceActiveFocus()
    }
    onClosed: { finishing = false; authStarted = false; if (backend.googleAuthBusy) backend.cancelGoogleAuth() }
    function signIn() {
        if (!backend.connected || backend.googleAuthBusy || !backend.googleAuthConfiguration.available || !googleEmail.acceptableInput || (!connectMail.checked && !connectCalendar.checked)) return
        backend.clearError(); authStarted = true; step = 1
        if (googleSignIn.restoreRevision > 0) backend.restoreGoogleAccount(googleEmail.text.trim(), googleSignIn.reconnectAccount, googleSignIn.restoreRevision, renewGoogleAccess.checked, requestedAccess)
        else backend.connectGoogle(googleEmail.text.trim(), googleSignIn.reconnectAccount, renewGoogleAccess.checked, requestedAccess)
    }
    function finish() {
        if (finishing || !accountRow.id || !connectName.text.trim().length) return
        finishing = true
        backend.updateAccountPreferences(connectedAccount, connectName.text.trim(), accountRow.signature_text || "", reconnectAccount.length ? "" : retentionKeys[connectRetention.currentIndex],{mail:connectMail.checked,calendar:connectCalendar.checked,mail_access:connectMailMode.currentIndex ? "write" : "read",calendar_access:connectCalendarMode.currentIndex ? "write" : "read"})
    }
    function grantedAccess() {
        if (!accountRow.id) return "Checking granted access…"
        const granted = []
        if (backend.accountCapability(connectedAccount,"mail_read")) granted.push(backend.accountCapability(connectedAccount,"mail_write") ? "Mail read & write" : "Mail read")
        if (backend.accountCapability(connectedAccount,"calendar_read")) granted.push(backend.accountCapability(connectedAccount,"calendar_write") ? "Calendar read & write" : "Calendar read")
        return granted.length ? "Google granted: " + granted.join(" · ") : "Signed in · no Mail or Calendar access granted"
    }
    onAccountRowChanged: if (step === 2 && !connectName.text.length && accountRow.id) connectName.text = accountRow.name || accountRow.email.split("@")[0]
    Connections {
        target: backend
        function onConnectedChanged() { if (!backend.connected) googleSignIn.finishing = false }
        function onGoogleAuthChanged() {
            if (!googleSignIn.opened || !googleSignIn.authStarted) return
            if (backend.googleAuth.phase === "connected") {
                googleSignIn.connectedAccount = backend.googleAuth.account || ""; googleSignIn.step = 2
                connectName.text = googleSignIn.accountRow.name || googleEmail.text.split("@")[0]
                connectName.forceActiveFocus(); connectName.selectAll()
            } else if (["failed","cancelled"].indexOf(backend.googleAuth.phase) >= 0) { googleSignIn.step = 0; googleSignIn.authStarted = false }
        }
        function onAccountPreferencesFinished(account, ok) {
            if (!googleSignIn.opened || !googleSignIn.finishing || account !== googleSignIn.connectedAccount) return
            googleSignIn.finishing = false
            if (ok) { googleSignIn.close(); backend.account = account; googleSignIn.syncRequested(account) }
        }
    }
    contentItem: ScrollView {
                ScrollBar.vertical: FmScrollBar {}
        implicitHeight: connectionContents.implicitHeight; contentWidth: availableWidth; clip: true
        ScrollBar.horizontal.policy: ScrollBar.AlwaysOff
        property bool blocksWorkspaceShortcuts: true
        ColumnLayout {
        id: connectionContents; width: googleSignIn.availableWidth; spacing: 14
        RowLayout {
            Layout.fillWidth: true; spacing: 12
            Rectangle { implicitWidth: 34; implicitHeight: 34; radius: 17; color: "transparent"; border.color: Theme.line2; Text { anchors.centerIn: parent; text: "G"; color: Theme.secondary; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(16); font.weight: Font.Medium } }
            ColumnLayout {
                Layout.fillWidth: true; spacing: 2
                Text { text: googleSignIn.restoreRevision > 0 ? "Restore Google account" : googleSignIn.reconnectAccount.length ? "Reconnect "+((backend.accounts.find(row=>row.id===googleSignIn.reconnectAccount) || {}).name || "Google") : "Connect Google"; color: Theme.text; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(16); font.weight: Font.Medium; font.letterSpacing: -0.16 }
                Text { text: ["Step 1 of 3 · Your address", "Step 2 of 3 · Sign in with Google", "Step 3 of 3 · Ready to sync"][googleSignIn.step]; color: Theme.inkAlpha(0.5); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12.5) }
            }
        }
        ColumnLayout {
            visible: googleSignIn.step === 0; Layout.fillWidth: true; spacing: 14
            Description { text: googleSignIn.restoreRevision > 0 ? "Sign in as the same Google account to use your saved mail and calendars again." : "Enter your Gmail or Workspace address. Sign-in finishes in your browser."; color: Theme.secondary }
            FmTextField {
                id: googleEmail; objectName: "googleEmailAddress"; Layout.fillWidth: true; implicitHeight: 42; padding: 10; leftPadding: 12; rightPadding: 12
                placeholderText: "you@gmail.com"; enabled: !backend.googleAuthBusy;readOnly:!!googleSignIn.reconnectAccount; font.pointSize: Theme.fontPoints(15)
                Accessible.name: "Gmail or Google Workspace email address"
                inputMethodHints: Qt.ImhEmailCharactersOnly | Qt.ImhNoAutoUppercase; maximumLength: 320
                validator: RegularExpressionValidator { regularExpression: / *[^\s@<>,;]+@[^\s@<>,;]+ */ }
                onAccepted: googleSignIn.signIn()
            }
            ColumnLayout {
                Layout.fillWidth: true; spacing: 0
                Text { text: "Request access to"; color: Theme.inkAlpha(0.45); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(11.5); Layout.bottomMargin: 4 }
                RowLayout {
                    Layout.fillWidth: true; spacing: 12
                    FmCheckBox { id: connectMail; objectName: "connectMailAccess"; Layout.fillWidth: true; checked: true; text: "Mail"; description: connectMailMode.currentIndex === 0 ? "Read your mail" : "Read, send and label your mail" }
                    FmSegmented { id: connectMailMode; objectName: "connectMailMode"; model: ["Read", "Read & write"]; currentIndex: 1; enabled: connectMail.checked; onActivated: function(index) { currentIndex = index } }
                }
                RowLayout {
                    Layout.fillWidth: true; spacing: 12
                    FmCheckBox { id: connectCalendar; objectName: "connectCalendarAccess"; Layout.fillWidth: true; checked: true; text: "Calendar"; description: connectCalendarMode.currentIndex === 0 ? "See events and invites" : "See and edit events and invites" }
                    FmSegmented { id: connectCalendarMode; objectName: "connectCalendarMode"; model: ["Read", "Read & write"]; currentIndex: 1; enabled: connectCalendar.checked; onActivated: function(index) { currentIndex = index } }
                }
            }
            ColumnLayout {
                visible: !googleSignIn.reconnectAccount.length; Layout.fillWidth: true; spacing: 6
                Caption { text: "Local retention" }
                FmComboBox { id: connectRetention; objectName: "connectRetention"; Layout.fillWidth: true; model: ["90 days", "1 year", "2 years", "5 years", "Everything"]; currentIndex: 1; Accessible.name: "Local retention" }
                Caption { text: "How much mail history to download. Mail already saved stays on this machine."; Layout.fillWidth: true; wrapMode: Text.Wrap }
            }
            FmCheckBox {
                id: renewGoogleAccess; text: "Ask Google to renew access"; description: "Use this when access expired or Google did not supply renewable access."
                visible: googleSignIn.reconnectAccount.length > 0 || backend.googleAuth.error_code === "reauthorization_required"; enabled: !backend.googleAuthBusy; Layout.fillWidth: true
            }
        }
        Rectangle {
            visible: googleSignIn.step === 1; Layout.fillWidth: true; implicitHeight: browserStatus.implicitHeight + 28; radius: 6; color: Theme.canvas; border.color: Theme.line
            RowLayout {
                id: browserStatus; anchors.left: parent.left; anchors.right: parent.right; anchors.top: parent.top; anchors.margins: 14; spacing: 12
                Rectangle { implicitWidth: 8; implicitHeight: 8; radius: 4; color: Theme.accent }
                ColumnLayout {
                    Layout.fillWidth: true; spacing: 2
                    Description { text: backend.googleAuth.message || "Finish signing in with your browser"; color: Theme.text }
                    Caption { text: googleEmail.text; elide: Text.ElideRight; maximumLineCount: 1 }
                }
            }
        }
        FmButton { visible: googleSignIn.step === 1; text: "Browser didn’t open? Try again"; leftPadding: 0; font.pointSize: Theme.fontPoints(12.5); onClicked: backend.cancelGoogleAuth() }
        ColumnLayout {
            visible: googleSignIn.step === 2; Layout.fillWidth: true; spacing: 14
            Rectangle {
                Layout.fillWidth: true; implicitHeight: connectedInfo.implicitHeight + 28; radius: 6; color: Theme.canvas; border.color: Theme.line
                ColumnLayout { id: connectedInfo; anchors.left: parent.left; anchors.right: parent.right; anchors.top: parent.top; anchors.margins: 14; spacing: 3; Description { text: googleSignIn.accountRow.email || googleEmail.text; color: Theme.text } Caption { text: googleSignIn.grantedAccess() } }
            }
            ColumnLayout {
                Layout.fillWidth: true; spacing: 6
                Caption { text: "Display name — shown in the top bar" }
                FmTextField { id: connectName; objectName: "connectedAccountName"; Layout.fillWidth: true; implicitHeight: 42; font.pointSize: Theme.fontPoints(15); maximumLength: 80; enabled: !googleSignIn.finishing; Accessible.name: "New account display name"; onAccepted: googleSignIn.finish() }
            }
            Caption { text: "Sync runs while the app is open. New mail arrives first; the offline copy builds in the background." }
        }
        }
    }
    footer: Item {
        implicitHeight: connectionFooter.implicitHeight + 24
        ColumnLayout {
        id: connectionFooter; anchors.left: parent.left; anchors.right: parent.right; anchors.margins: 24; spacing: 14
        Caption {
            objectName: "connectionError"
            readonly property string errorText: googleSignIn.step === 0 && backend.googleAuthConfiguration.error ? backend.googleAuthConfiguration.error.message || ""
                : backend.googleAuth.phase === "failed" ? backend.googleAuth.message || ""
                : googleSignIn.step === 2 && backend.preferencesStatus.indexOf("Not saved") === 0 ? backend.preferencesStatus : ""
            visible: errorText.length > 0; text: errorText; color: Theme.error
        }
        RowLayout {
            id: connectionButtons; Layout.fillWidth: true; spacing: 6
            Row { spacing: 4; Repeater { model: 3; Rectangle { required property int index; width: 5; height: 5; radius: 2.5; color: index === googleSignIn.step ? Theme.text : Theme.line3 } } }
            Item { Layout.fillWidth: true }
            FmButton { text: "Cancel"; implicitHeight: 32; outlined: true; onClicked: googleSignIn.close() }
            FmButton {
                objectName: googleSignIn.step === 2 ? "finishGoogleConnection" : "beginGoogleSignIn"
                text: googleSignIn.finishing ? "Saving…" : ["Continue with Google", "Waiting for Google…", googleSignIn.reconnectAccount ? "Resume syncing" : "Start syncing"][googleSignIn.step]
                implicitHeight: 32; prominent: true; shortcutHint: "↵"; font.pointSize: Theme.fontPoints(12.5)
                enabled: backend.connected && !googleSignIn.finishing && (googleSignIn.step === 2 ? !!googleSignIn.accountRow.id && !!connectName.text.trim().length : googleSignIn.step === 0 && !backend.googleAuthBusy && !!backend.googleAuthConfiguration.available && googleEmail.acceptableInput && (connectMail.checked || connectCalendar.checked))
                onClicked: googleSignIn.step === 2 ? googleSignIn.finish() : googleSignIn.signIn()
            }
        }
        }
    }
    component Description: Text {
        color: Theme.muted; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(13.5)
        Layout.fillWidth: true; wrapMode: Text.Wrap; lineHeightMode: Text.FixedHeight; lineHeight: Math.round(Theme.fontPixels(font) * 1.5); topPadding: Theme.leadingTop(font, lineHeight); bottomPadding: Theme.leadingBottom(font, lineHeight); textFormat: Text.PlainText
    }
    component Caption: Description { font.pointSize: Theme.fontPoints(12.5); color: Theme.inkAlpha(0.5) }
}
