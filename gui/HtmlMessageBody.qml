import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import FuckMail.Html

// One message's sanitised HTML on a light paper card. Remote images stay
// placeholders until Load images; blocked hosts never load. `failed` asks the
// reader to show Text instead, with `failure` as its notice.
ColumnLayout {
    id: body
    objectName: "htmlMessageBody_" + (message.id || "")
    property var message: ({})
    property string account: ""
    property string conversation: ""
    property bool active: false
    property var result: ({})
    property string ticket: ""
    property string error: ""
    // Resource id → {state, path?}; see FmHtmlView.images.
    property var images: ({})
    property var imageTickets: ({})
    property bool loadRequested: false
    property bool proxyFailed: false
    readonly property var service: backend.htmlContent || null
    readonly property string identity: account + "/" + (message.id || "") + "/" + (message.source_version || "")
    readonly property string sender: message.sender_email || ""
    readonly property bool failed: !!error || !!result.too_large || result.has_html === false || view.status === "failed"
    readonly property string failure: error || (result.too_large ? "This message's HTML is too large to show." : view.failure)
    readonly property bool ready: view.status === "ready"
    // A message shown again already has its final height while it lays out.
    readonly property bool shown: ready || (view.status === "loading" && view.implicitHeight > 0)
    readonly property var remote: (result.resources || []).filter(function(r) { return r.kind === "remote" })
    readonly property int hiddenCount: remote.filter(function(r) { const s = (images[r.id] || {}).state; return !s || s === "hidden" }).length
    readonly property var blockedLinks: {
        if (!service || service.revision < 0) return []
        return (result.links || []).filter(function(l) { return !!l.host && !!service.blockedBy(l.host) }).map(function(l) { return l.id })
    }
    signal linkHovered(string href, rect box)
    signal linkExited()
    spacing: 10

    function link(id) { return (result.links || []).find(function(l) { return l.id === id }) || null }
    // A result the GUI already holds is shown without waiting for the engine.
    function fromCache() {
        if (!active || !service || ticket || result.id || error || !message.id || !message.source_version) return false
        const hit = service.cached(account, message.id, message.source_version)
        if (!hit || hit.id !== message.id || hit.source_version !== message.source_version) return false
        result = hit
        assign()
        return true
    }
    function load() {
        if (!active || !service || ticket || result.id || error || fromCache() || !backend.connected) return
        if (!message.source_available || !message.source_version) { error = "This message's source is not available."; return }
        ticket = service.load(account, message.id, message.source_version)
    }
    function request(id, cachedOnly) {
        const ticket = service.loadImage(account, message.id, message.source_version, id, cachedOnly)
        const next = Object.assign({}, imageTickets); next[ticket] = id; imageTickets = next
    }
    function setImage(id, value) { const next = Object.assign({}, images); next[id] = value; images = next }
    // Inline and cached images show at once; remote ones wait for Load images.
    function assign() {
        const next = {}, allowed = loadRequested || service.imagesAllowed(account, sender)
        for (const r of (result.resources || [])) {
            const blocked = r.host ? service.blockedBy(r.host) : ""
            if (r.kind === "insecure") next[r.id] = {state: "insecure"}
            else if (blocked) next[r.id] = {state: "blocked", by: blocked}
            else if (r.image) next[r.id] = {state: "ready", path: r.image.path}
            else if (r.kind !== "remote" || allowed) { next[r.id] = {state: "loading"}; request(r.id, false) }
            else next[r.id] = {state: "hidden"}
        }
        images = next
    }
    function loadImages() {
        loadRequested = true; proxyFailed = false
        const next = Object.assign({}, images)
        for (const r of remote) {
            const state = (next[r.id] || {}).state
            if (state === "hidden" || state === "failed") { next[r.id] = {state: "loading"}; request(r.id, false) }
        }
        images = next
    }
    function allowSender() { service.allowImages(account, sender); loadImages() }
    function refreshBlocked() {
        if (!result.resources) return
        const next = Object.assign({}, images)
        let changed = false
        for (const r of result.resources) {
            if (!r.host) continue
            const blocked = service.blockedBy(r.host), state = (next[r.id] || {}).state
            if (blocked && state !== "blocked") { next[r.id] = {state: "blocked", by: blocked}; changed = true }
            else if (!blocked && state === "blocked") { next[r.id] = r.image ? {state: "ready", path: r.image.path} : {state: "hidden"}; changed = true }
        }
        if (changed) images = next
    }
    function activate(id) {
        const target = link(id)
        if (!target || (target.host && service.blockedBy(target.host))) return
        backend.openLink(target.href)
    }
    onIdentityChanged: {
        if (service && message.id) service.cancelMessage(account, message.id)
        ticket = ""; result = ({}); error = ""; images = ({}); imageTickets = ({}); loadRequested = false; proxyFailed = false
        if (!fromCache()) Qt.callLater(load)
    }
    onActiveChanged: load()
    Component.onCompleted: load()
    Component.onDestruction: if (service && message.id) service.cancelMessage(account, message.id)

    Connections {
        target: body.service
        function onHtmlReady(request, value, ok) {
            if (request !== body.ticket) return
            body.ticket = ""
            if (!ok) { body.error = value.message || "This message's HTML is unavailable."; return }
            if (value.id !== body.message.id || value.source_version !== body.message.source_version) return
            body.result = value
            body.assign()
        }
        function onImageReady(request, value, ok) {
            const id = body.imageTickets[request]
            if (id === undefined) return
            const tickets = Object.assign({}, body.imageTickets); delete tickets[request]; body.imageTickets = tickets
            const state = ok ? value.state : "failed"
            if (state === "ready") body.setImage(id, {state: "ready", path: value.path})
            else if (state === "not_cached") body.setImage(id, {state: "hidden"})
            else if (state === "blocked") body.setImage(id, {state: "blocked", by: value.by})
            else if (state === "insecure") body.setImage(id, {state: "insecure"})
            else if (state === "proxy_unavailable") { body.proxyFailed = true; body.setImage(id, {state: "hidden"}) }
            else body.setImage(id, {state: "failed", reason: value.reason || value.message || ""})
        }
        function onBlockedHostsChanged() { body.refreshBlocked() }
    }
    Connections { target: backend; function onConnectedChanged() { if (backend.connected) body.load() } }

    RowLayout {
        objectName: "htmlImageBar_" + (body.message.id || "")
        visible: body.shown && (body.hiddenCount > 0 || body.proxyFailed)
        Layout.fillWidth: true; spacing: 10
        FmIcon { name: "imageOff"; size: 13; color: Theme.inkAlpha(.5) }
        Text {
            text: body.proxyFailed ? "Proxy unavailable — images not loaded"
                : (body.hiddenCount === 1 ? "1 image hidden" : body.hiddenCount + " images hidden") + " to keep senders from tracking you"
            color: Theme.inkAlpha(.5); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12.5)
            elide: Text.ElideRight; Layout.maximumWidth: implicitWidth; Layout.fillWidth: true; Layout.minimumWidth: 0
        }
        Text {
            objectName: "htmlLoadImages_" + (body.message.id || "")
            text: body.proxyFailed ? "Retry" : "Load images"
            color: Theme.text; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12.5)
            Rectangle { anchors.left: parent.left; anchors.right: parent.right; anchors.bottom: parent.bottom; anchors.bottomMargin: -1; height: 1; color: loadArea.containsMouse ? Theme.text : Theme.inkAlpha(.3) }
            MouseArea { id: loadArea; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: body.loadImages() }
            Accessible.role: Accessible.Button; Accessible.name: text
        }
        Text { visible: !body.proxyFailed && !!body.sender; text: "·"; opacity: .5; color: Theme.inkAlpha(.5); font.pointSize: Theme.fontPoints(12.5) }
        Text {
            objectName: "htmlAllowImages_" + (body.message.id || "")
            visible: !body.proxyFailed && !!body.sender
            text: "Always from " + (body.message.sender_name || body.sender)
            color: allowArea.containsMouse ? Theme.text : Theme.inkAlpha(.5); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12.5)
            elide: Text.ElideRight; Layout.fillWidth: true; Layout.minimumWidth: 0
            MouseArea { id: allowArea; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: body.allowSender() }
            Accessible.role: Accessible.Button; Accessible.name: text
        }
    }
    Rectangle {
        id: paper
        // The item needs a width to lay out, so the card stays in the layout
        // with no height until the first layout is ready or its height is known.
        Layout.fillWidth: true
        implicitHeight: body.shown ? view.implicitHeight : 0
        opacity: body.shown ? 1 : 0
        color: view.paper; radius: 6
        FmHtmlView {
            id: view
            objectName: "messageBody_" + (body.message.id || "")
            width: parent.width; height: implicitHeight
            content: body.result.html || body.result.html_path ? body.result : ({})
            images: body.images
            blockedLinks: body.blockedLinks
            fontPixels: Theme.fontSize(16)
            onLinkActivated: function(id) { body.activate(id) }
            onHoveredLinkChanged: {
                const target = hoveredLink ? body.link(hoveredLink) : null
                if (target) body.linkHovered(target.href, mapToItem(body, hoveredLinkRect))
                else body.linkExited()
            }
            Accessible.role: Accessible.StaticText; Accessible.name: text
        }
    }
}
