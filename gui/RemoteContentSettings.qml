import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

// Settings → Mail: the optional HTTPS proxy every remote image request uses,
// and the bounded image cache. The password goes to the OS vault, never to
// preferences, and is never shown again.
ColumnLayout {
    id: panel
    objectName: "remoteContentSettings"
    spacing: 0
    readonly property var service: backend.htmlContent || null
    readonly property var proxy: service ? service.proxy : ({})
    readonly property var cache: service ? service.imageCache : ({})
    property string error: ""
    property bool editing: false
    function hostText() { return !proxy.host ? "" : proxy.port && proxy.port !== 443 ? proxy.host + ":" + proxy.port : proxy.host }
    function reset() { hostField.text = hostText(); userField.text = proxy.username || ""; passwordField.text = ""; editing = false; error = "" }
    function save(enabled) {
        if (!service) return
        error = service.saveProxy(enabled, hostField.text, userField.text)
        if (error) return
        if (passwordField.text) { service.setProxyPassword(passwordField.text); passwordField.text = "" }
        editing = false
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

    Text {
        Layout.topMargin: 28; text: "Remote images"
        color: Theme.text; font { family: Theme.fontFamily; pointSize: Theme.fontPoints(15); weight: Font.Medium }
    }
    Text {
        Layout.topMargin: 6; Layout.fillWidth: true; Layout.maximumWidth: 560; wrapMode: Text.Wrap
        text: "HTML mail shows images only after you choose Load images. They are fetched by fuckMail over https, never http, without cookies or referrers."
        color: Theme.muted; font { family: Theme.fontFamily; pointSize: Theme.fontPoints(13.5) }
        lineHeight: 1.3
    }
    RowLayout {
        Layout.fillWidth: true; Layout.topMargin: 14; Layout.bottomMargin: 14; spacing: 24
        ColumnLayout {
            Layout.fillWidth: true; spacing: 3
            Text { text: "Fetch through an HTTPS proxy"; color: Theme.text; font { family: Theme.fontFamily; pointSize: Theme.fontPoints(14) } }
            Text {
                Layout.fillWidth: true; wrapMode: Text.Wrap
                text: "When on, every image request goes through the proxy, and nothing goes direct if it is unavailable. The proxy sees which sites images come from."
                color: Theme.inkAlpha(0.5); font { family: Theme.fontFamily; pointSize: Theme.fontPoints(12.5) }
            }
        }
        FmSwitch {
            id: proxySwitch; objectName: "remoteProxyEnabled"
            checked: !!panel.proxy.enabled
            enabled: !!panel.service && backend.connected
            onToggled: { panel.save(checked); checked = Qt.binding(function() { return !!panel.proxy.enabled }) }
            Accessible.name: "Fetch images through an HTTPS proxy"
        }
    }
    GridLayout {
        Layout.fillWidth: true; Layout.maximumWidth: 640; columns: 2; columnSpacing: 14; rowSpacing: 10
        Text { text: "Host"; color: Theme.secondary; font { family: Theme.fontFamily; pointSize: Theme.fontPoints(13) } }
        FmTextField {
            id: hostField; objectName: "remoteProxyHost"; Layout.fillWidth: true
            placeholderText: "proxy.example.net or proxy.example.net:8443"
            onTextEdited: panel.editing = true
            Accessible.name: "Proxy host"
        }
        Text { text: "Username"; color: Theme.secondary; font { family: Theme.fontFamily; pointSize: Theme.fontPoints(13) } }
        FmTextField {
            id: userField; objectName: "remoteProxyUsername"; Layout.fillWidth: true
            onTextEdited: panel.editing = true
            Accessible.name: "Proxy username"
        }
        Text { text: "Password"; color: Theme.secondary; font { family: Theme.fontFamily; pointSize: Theme.fontPoints(13) } }
        RowLayout {
            Layout.fillWidth: true; spacing: 8
            FmTextField {
                id: passwordField; objectName: "remoteProxyPassword"; Layout.fillWidth: true
                echoMode: TextInput.Password
                placeholderText: panel.proxy.password_saved ? "Saved in your system keychain" : "Not saved"
                onTextEdited: panel.editing = true
                Accessible.name: "Proxy password"
            }
            FmButton {
                visible: !!panel.proxy.password_saved; text: "Forget"; outlined: true
                onClicked: panel.service.clearProxyPassword()
            }
        }
    }
    RowLayout {
        Layout.topMargin: 12; spacing: 10
        FmButton {
            objectName: "remoteProxySave"; text: "Save"; prominent: true
            enabled: !!panel.service && backend.connected && panel.editing
            onClicked: panel.save(!!panel.proxy.enabled)
        }
        FmButton {
            objectName: "remoteProxyTest"; text: panel.proxy.testing ? "Testing…" : "Test"; outlined: true
            enabled: !!panel.service && backend.connected && !panel.proxy.testing && !!panel.proxy.enabled && !panel.editing
            onClicked: panel.service.testProxy()
        }
        Text {
            objectName: "remoteProxyStatus"
            Layout.fillWidth: true; wrapMode: Text.Wrap
            readonly property var test: panel.proxy.test || ({})
            text: panel.error ? panel.error
                : test.ok === true ? "Proxy works · " + test.ms + " ms"
                : test.ok === false ? "Proxy failed · " + (test.reason || "unavailable")
                : backend.preferencesStatus && backend.preferencesStatus.indexOf("Not saved") === 0 ? backend.preferencesStatus : ""
            color: panel.error || test.ok === false ? Theme.error : Theme.muted
            font { family: Theme.fontFamily; pointSize: Theme.fontPoints(12.5) }
        }
    }
    Text {
        Layout.topMargin: 8; Layout.fillWidth: true; Layout.maximumWidth: 560; wrapMode: Text.Wrap
        text: "The proxy's own name is looked up on this computer; image sites are looked up by the proxy."
        color: Theme.subtle; font { family: Theme.fontFamily; pointSize: Theme.fontPoints(12) }
    }
    Rectangle { Layout.fillWidth: true; Layout.topMargin: 14; implicitHeight: 1; color: Theme.line }
    RowLayout {
        Layout.fillWidth: true; Layout.topMargin: 14; Layout.bottomMargin: 14; spacing: 24
        ColumnLayout {
            Layout.fillWidth: true; spacing: 3
            Text { text: "Image cache"; color: Theme.text; font { family: Theme.fontFamily; pointSize: Theme.fontPoints(14) } }
            Text {
                objectName: "remoteImageCacheSize"
                text: panel.size(panel.cache.bytes) + " of " + panel.size(panel.cache.limit === undefined ? 256 * 1024 * 1024 : panel.cache.limit) + " · loaded images show again without a request"
                color: Theme.inkAlpha(0.5); font { family: Theme.fontFamily; pointSize: Theme.fontPoints(12.5) }
                Layout.fillWidth: true; wrapMode: Text.Wrap
            }
        }
        FmButton {
            objectName: "remoteImageCacheClear"; text: "Clear"; outlined: true
            enabled: !!panel.service && backend.connected && (panel.cache.bytes || 0) > 0
            onClicked: panel.service.clearImageCache()
        }
    }
}
