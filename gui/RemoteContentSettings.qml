import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

// Settings → Mail → Image loading: the optional HTTPS proxy every remote image
// request uses, and the bounded image cache. The password goes to the OS
// vault, never to preferences, and is never shown again.
ColumnLayout {
    id: panel
    objectName: "remoteContentSettings"
    spacing: 0
    readonly property var service: backend.htmlContent || null
    readonly property var proxy: service ? service.proxy : ({})
    readonly property var cache: service ? service.imageCache : ({})
    property string error: ""
    property bool editing: false
    // On before its first host is saved: the fields show, nothing is enabled yet.
    property bool wantOn: false
    readonly property bool on: !!proxy.enabled || wantOn
    // Names, IPv4, bracketed IPv6 and localhost, each with an optional port.
    readonly property bool hostValid: /^(([a-z0-9-]+\.)+[a-z]{2,}|\d{1,3}(\.\d{1,3}){3}|\[[0-9a-f:]+\]|localhost)(:\d{2,5})?$/i.test(hostField.text.trim())
    function hostText() { return !proxy.host ? "" : proxy.port && proxy.port !== 443 ? proxy.host + ":" + proxy.port : proxy.host }
    function reset() { hostField.text = hostText(); userField.text = proxy.username || ""; passwordField.text = ""; editing = false; error = "" }
    function save(enabled) {
        if (!service) return
        error = service.saveProxy(enabled, hostField.text, userField.text)
        if (error) return
        if (passwordField.text) { service.setProxyPassword(passwordField.text); passwordField.text = "" }
        editing = false; wantOn = false
    }
    function toggle() {
        if (panel.on) { wantOn = false; if (proxy.enabled) save(false); else reset() }
        else if (proxy.host) save(true)
        else wantOn = true
    }
    function size(bytes) {
        if (bytes === undefined) return "…"
        if (bytes < 1024 * 1024) return Math.round(bytes / 1024) + " KB"
        return (bytes / 1024 / 1024).toFixed(1) + " MB"
    }
    function refresh() { if (service && backend.connected && visible) { service.refreshProxy(); service.refreshImageCache() } }
    onVisibleChanged: refresh()
    Component.onCompleted: { reset(); refresh() }
    onProxyChanged: if (!editing) reset()
    Connections { target: backend; function onConnectedChanged() { panel.refresh() } }
    component Row2: RowLayout {
        id: row
        property string title: ""
        property string detail: ""
        property string detailName: ""
        default property alias control: controlSlot.data
        Layout.fillWidth: true; Layout.topMargin: 14; Layout.bottomMargin: 14; spacing: 24
        ColumnLayout {
            Layout.fillWidth: true; spacing: 3
            Text { text: row.title; color: Theme.text; font { family: Theme.fontFamily; pointSize: Theme.fontPoints(14) } }
            Text {
                objectName: row.detailName
                Layout.fillWidth: true; wrapMode: Text.Wrap; text: row.detail
                color: Theme.inkAlpha(0.5); font { family: Theme.fontFamily; pointSize: Theme.fontPoints(12.5) }
                lineHeightMode: Text.FixedHeight; lineHeight: Math.round(Theme.fontSize(12.5) * 1.45)
            }
        }
        Item { id: controlSlot; implicitWidth: childrenRect.width; implicitHeight: childrenRect.height }
    }
    component Label2: Text { Layout.preferredWidth: 100; color: Theme.inkAlpha(.6); font { family: Theme.fontFamily; pointSize: Theme.fontPoints(13) } }

    Text {
        text: "Image loading"
        color: Theme.text; font { family: Theme.fontFamily; pointSize: Theme.fontPoints(15); weight: Font.Medium }
    }
    Text {
        Layout.topMargin: 6; Layout.fillWidth: true; Layout.maximumWidth: 640; wrapMode: Text.Wrap
        text: "HTML mail shows images only when you choose Load images — always over HTTPS, without cookies or referrers."
        color: Theme.muted; font { family: Theme.fontFamily; pointSize: Theme.fontPoints(13.5) }
        lineHeightMode: Text.FixedHeight; lineHeight: Math.round(Theme.fontSize(13.5) * 1.5)
    }
    Rectangle { Layout.fillWidth: true; Layout.topMargin: 10; implicitHeight: 1; color: Theme.line }
    Row2 {
        title: "Sender pictures"
        detail: "Brand icons for automated senders and public profile photos for people, from Google and Gravatar. Initials otherwise."
        FmSwitch {
            objectName: "senderPicturesEnabled"
            checked: (backend.preferences || {}).sender_pictures !== false
            enabled: backend.connected
            onToggled: { backend.setPreferences({sender_pictures: checked}); checked = Qt.binding(function() { return (backend.preferences || {}).sender_pictures !== false }) }
            Accessible.name: "Show sender pictures"
        }
    }
    Rectangle { Layout.fillWidth: true; implicitHeight: 1; color: Theme.line }
    Row2 {
        title: "Load through a proxy"
        detail: "Image sites see the proxy, not you. If it’s down, images wait — nothing goes direct."
        FmSwitch {
            id: proxySwitch; objectName: "remoteProxyEnabled"
            checked: panel.on
            enabled: !!panel.service && backend.connected
            onToggled: { panel.toggle(); checked = Qt.binding(function() { return panel.on }) }
            Accessible.name: "Load images through a proxy"
        }
    }
    Rectangle { Layout.fillWidth: true; implicitHeight: 1; color: Theme.line }
    GridLayout {
        visible: panel.on
        Layout.fillWidth: true; Layout.topMargin: 14; Layout.bottomMargin: 16; columns: 2; columnSpacing: 14; rowSpacing: 10
        Label2 { text: "Host" }
        FmTextField {
            id: hostField; objectName: "remoteProxyHost"; Layout.fillWidth: true; implicitHeight: 34
            placeholderText: "proxy.example.net:8443"
            onTextEdited: panel.editing = true
            Accessible.name: "Proxy host"
        }
        Label2 { text: "Username" }
        FmTextField {
            id: userField; objectName: "remoteProxyUsername"; Layout.fillWidth: true; implicitHeight: 34
            placeholderText: "Optional"
            onTextEdited: panel.editing = true
            Accessible.name: "Proxy username"
        }
        Label2 { text: "Password" }
        RowLayout {
            Layout.fillWidth: true; spacing: 8
            FmTextField {
                id: passwordField; objectName: "remoteProxyPassword"; Layout.fillWidth: true; implicitHeight: 34
                echoMode: TextInput.Password
                placeholderText: panel.proxy.password_saved ? "Saved" : "Optional"
                onTextEdited: panel.editing = true
                Accessible.name: "Proxy password"
            }
            FmButton {
                objectName: "remoteProxyForget"
                visible: !!panel.proxy.password_saved && !passwordField.text.length; text: "Forget"; outlined: true; implicitHeight: 34
                tip: "Remove the saved password from your system keychain"
                onClicked: panel.service.clearProxyPassword()
            }
        }
        Item { implicitWidth: 1 }
        RowLayout {
            Layout.fillWidth: true; spacing: 8
            Rectangle {
                id: resultDot
                readonly property bool testing: !!panel.proxy.testing
                visible: testing || (panel.proxy.test || {}).ok !== undefined || panel.error.length > 0
                implicitWidth: 7; implicitHeight: 7; radius: 3.5; color: status.color
                SequentialAnimation on opacity { running: resultDot.testing; loops: Animation.Infinite; NumberAnimation { to: .35; duration: 500 } NumberAnimation { to: 1; duration: 500 } }
            }
            Text {
                id: status
                objectName: "remoteProxyStatus"
                Layout.fillWidth: true; elide: Text.ElideRight
                readonly property var test: panel.proxy.test || ({})
                readonly property bool saved: !!panel.proxy.enabled && !panel.editing
                text: panel.error ? panel.error
                    : panel.proxy.testing ? "Testing " + hostField.text.trim() + "…"
                    : !hostField.text.trim().length ? "Enter the proxy host to test it"
                    : !panel.hostValid ? "That doesn’t look like a host name"
                    : !saved ? "Not saved yet"
                    : test.ok === true ? "Reachable · fetched a test image in " + test.ms + " ms"
                    : test.ok === false ? "Proxy failed · " + (test.reason || "unavailable")
                    : "Saved"
                color: panel.error || test.ok === false && saved ? Theme.error
                    : panel.proxy.testing ? Theme.ai
                    : test.ok === true && saved ? "#7FC4A4"
                    : hostField.text.trim().length && !panel.hostValid ? Theme.amber
                    : Theme.inkAlpha(.45)
                font { family: Theme.fontFamily; pointSize: Theme.fontPoints(12.5) }
            }
            FmButton {
                objectName: "remoteProxyTest"; text: panel.proxy.testing ? "Testing…" : "Test"; outlined: true; implicitHeight: 32
                // The engine tests the saved proxy.
                enabled: !!panel.service && backend.connected && !panel.proxy.testing && status.saved
                opacity: enabled ? 1 : .35
                onClicked: panel.service.testProxy()
            }
            FmButton {
                objectName: "remoteProxySave"; text: !panel.editing && panel.proxy.enabled ? "Saved" : "Save"; prominent: true; implicitHeight: 32
                // The engine explains an unusable host, such as one with https://.
                enabled: !!panel.service && backend.connected && hostField.text.trim().length > 0 && (panel.editing || !panel.proxy.enabled)
                opacity: enabled ? 1 : .35
                onClicked: panel.save(true)
            }
        }
    }
    Rectangle { visible: panel.on; Layout.fillWidth: true; implicitHeight: 1; color: Theme.line }
    Row2 {
        title: "Image cache"; detailName: "remoteImageCacheSize"
        detail: panel.size(panel.cache.bytes) + " of " + panel.size(panel.cache.limit === undefined ? 256 * 1024 * 1024 : panel.cache.limit) + " · loaded images show again without a new request"
        FmButton {
            objectName: "remoteImageCacheClear"; text: "Clear"; outlined: true; implicitHeight: 32
            enabled: !!panel.service && backend.connected && (panel.cache.bytes || 0) > 0
            opacity: enabled ? 1 : .35
            onClicked: panel.service.clearImageCache()
        }
    }
    Rectangle { Layout.fillWidth: true; implicitHeight: 1; color: Theme.line }
}
