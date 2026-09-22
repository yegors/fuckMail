import QtQuick

// One controller for the loaded conversation; views never own independent jobs.
Item {
    id: assistance
    property string conversationId: backend.selectedId
    property string accountId: backend.account
    property var settings: ({accounts: {}})
    property int settingsRevision: -1
    property var items: []
    property var requests: ({})
    property int context: 0
    property int session: 0
    property int operation: 0
    property string latestResults: ""
    property string job: ""
    property bool starting: false
    property bool completing: false
    property bool polling: false
    property bool watching: false
    property string jobTask: ""
    property string jobTarget: ""
    property string status: ""
    property string error: ""
    property var changeErrors: ({})
    readonly property bool busy: starting || completing || job.length > 0
    readonly property var policy: (settings.accounts || {})[accountId] || {}
    readonly property bool summariesEnabled: settingsRevision >= 0 && policy.summaries !== false
    readonly property bool classificationEnabled: settingsRevision >= 0 && policy.classification !== false
    signal openSource(string messageId)

    function send(method, params, kind, extra) {
        const ticket = backend.assistanceRequest(method, params || {}, accountId)
        const next = Object.assign({}, requests)
        next[ticket] = Object.assign({kind: kind || method, context: context, session: session, operation: operation}, extra || {})
        requests = next
        return ticket
    }
    function resultFor(task, target) {
        return items.find(item => item.task === task && item.target === target && !(task === "summary" && item.format_outdated)) || null
    }
    function workingOn(task, target) { return busy && (jobTask === task || jobTask === "message") && jobTarget === target }
    function errorFor(task, target) { return ((jobTask === task || jobTask === "message") && jobTarget === target ? error : "") || (task === "classification" ? changeErrors[target] || "" : "") }
    function refresh() {
        if (backend.connected && accountId && conversationId)
            latestResults = send("assist_results", {conversation_id: conversationId}, "results")
    }
    function adoptReader() {
        const snapshot = backend.conversation
        if (!snapshot || snapshot.id !== conversationId || snapshot.assistance_results === undefined) return false
        if (snapshot.assistance_settings && snapshot.assistance_settings.settings) adoptSettings(snapshot.assistance_settings)
        items = snapshot.assistance_results
        return true
    }
    function loadSettings() { if (backend.connected) send("assist_settings_get", {}, "settings") }
    function adoptSettings(result) {
        if (!result || typeof result.revision !== "number" || !result.settings) return
        if (result.revision < settingsRevision) return
        const changed = settingsRevision !== result.revision
        settingsRevision = result.revision
        settings = result.settings
        if (backend.connected && (changed || !watching)) {
            watching = true
            send("assist_watch", {}, "ignore")
        }
    }
    function start(task, target) {
        if (busy || !backend.connected || !conversationId) return
        ++operation
        starting = true
        jobTask = "message"
        jobTarget = target || ""
        status = "Queued"
        error = ""
        send("assist_start", {task: jobTask, conversation_id: conversationId, message_id: jobTarget}, "start")
    }
    function cancel() {
        ++operation
        if (job && backend.connected) send("assist_cancel", {job_id: job}, "ignore")
        job = ""
        starting = false
        completing = false
        polling = false
        status = "Canceled"
        error = ""
    }
    function current() {
        cancel()
        ++context
        items = []
        status = ""
        error = ""
        latestResults = ""
        changeErrors = ({})
        if (!adoptReader()) refresh()
    }
    function correct(member, kind) {
        send("assist_correct", {conversation_id: conversationId, message_id: member.id,
             kind: kind, expected_revision: member.revision}, "changed", {target: member.id})
    }
    onConversationIdChanged: current()
    onAccountIdChanged: current()
    Component.onCompleted: loadSettings()
    Connections {
        target: backend
        function onConnectedChanged() {
            if (!backend.connected) {
                ++assistance.session
                assistance.requests = ({})
                assistance.current()
                assistance.watching = false
            } else { assistance.loadSettings(); assistance.refresh() }
        }
        function onConversationChanged() {
            if (!assistance.adoptReader()) { assistance.refresh(); assistance.loadSettings() }
            else if (assistance.settingsRevision < 0) assistance.loadSettings()
        }
        function onAssistanceReply(ticket, method, result, ok) {
            if (method === "assist_settings_save" && ok) {
                assistance.adoptSettings(result)
                assistance.cancel()
                assistance.refresh()
            }
            const req = assistance.requests[ticket]
            if (!req) return
            const next = Object.assign({}, assistance.requests)
            delete next[ticket]
            assistance.requests = next
            if (req.session !== assistance.session) return
            if (req.kind === "start" && (req.context !== assistance.context || req.operation !== assistance.operation)) {
                if (ok && result.job_id) assistance.send("assist_cancel", {job_id: result.job_id}, "ignore")
                return
            }
            if (req.kind === "ignore") return
            if (req.kind !== "settings" && req.context !== assistance.context) return
            if (req.kind === "results" && ticket !== assistance.latestResults) return
            if (req.kind === "job" && (req.operation !== assistance.operation || req.job !== assistance.job)) return
            if (req.kind === "start") assistance.starting = false
            if (req.kind === "job") assistance.polling = false
            if (!ok) {
                if (req.kind === "changed") {
                    const errors = Object.assign({}, assistance.changeErrors)
                    errors[req.target] = result.message || "Could not save this change. Try again."
                    assistance.changeErrors = errors
                } else if (req.kind === "start" || req.kind === "job" || (req.kind === "results" && assistance.completing)) {
                    if (assistance.job) assistance.send("assist_cancel", {job_id: assistance.job}, "ignore")
                    assistance.job = ""
                    assistance.completing = false
                    assistance.error = result.message || "Local assistance unavailable. Try again."
                }
                return
            }
            if (req.kind === "settings") {
                assistance.adoptSettings(result)
            } else if (req.kind === "results") {
                assistance.items = result.items || []
                if (req.operation === assistance.operation) assistance.completing = false
            }
            else if (req.kind === "start") { assistance.job = result.job_id; assistance.status = "Queued" }
            else if (req.kind === "job") {
                assistance.status = result.status
                if (["running", "queued"].indexOf(result.status) < 0) {
                    // Keep the progress surface until the committed result arrives.
                    assistance.completing = result.status === "completed"
                    assistance.job = ""
                    assistance.error = result.error ? result.error.message : ""
                    assistance.refresh()
                    backend.refresh()
                }
            } else if (req.kind === "changed") {
                const errors = Object.assign({}, assistance.changeErrors)
                delete errors[req.target]
                assistance.changeErrors = errors
                assistance.refresh()
            }
        }
    }
    Timer {
        interval: 400; repeat: true
        running: assistance.job.length > 0 && backend.connected
        onTriggered: if (!assistance.polling) {
            assistance.polling = true
            assistance.send("assist_job", {job_id: assistance.job}, "job", {job: assistance.job})
        }
    }
}
