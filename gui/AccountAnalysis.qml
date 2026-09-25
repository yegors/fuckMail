import QtQuick

// Catch up for the Accounts cards. Opening the menu reads each range's count
// (`assist_backlog`); choosing one moves the account's Catch up bound
// (`assist_catch_up`). The engine's single queue does the work after new mail,
// so nothing here owns a job; progress comes from AccountProgress.
Item {
    id: analysis
    property var stats: ({})
    property var errors: ({})
    property var requests: ({})
    function put(field, key, value) { const next = Object.assign({}, analysis[field]); next[key] = value; analysis[field] = next }
    function send(method, account, params, kind) {
        const ticket = backend.assistanceRequest(method, params || {}, account)
        if (!ticket) return
        const next = Object.assign({}, requests)
        next[ticket] = {account: account, kind: kind}
        requests = next
    }
    function pending(account, kind) { return Object.keys(requests).some(k => requests[k].account === account && requests[k].kind === kind) }
    function refresh(account) {
        if (backend.connected && !pending(account, "stats")) send("assist_backlog", account, {}, "stats")
    }
    function start(account, range) {
        if (!backend.connected || !(stats[account] || {}).enabled) return
        put("errors", account, "")
        send("assist_catch_up", account, {range: range}, "start")
    }
    Connections {
        target: backend
        function onConnectedChanged() { analysis.requests = ({}) }
        function onAssistanceReply(ticket, method, result, ok) {
            if (method === "assist_settings_save" && ok) analysis.stats = ({})
            const request = analysis.requests[ticket]
            if (!request) return
            const next = Object.assign({}, analysis.requests); delete next[ticket]; analysis.requests = next
            if (!ok) { analysis.put("errors", request.account, result.message || "Local model unavailable. Try again."); return }
            if (request.kind === "stats") analysis.put("stats", request.account, result)
            else analysis.refresh(request.account)
        }
    }
}
