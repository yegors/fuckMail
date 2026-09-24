import QtQuick

// Lives with Settings, not a card delegate: refreshes/navigation cannot lose job ownership.
Item {
    id: analysis
    property bool showing: false
    property var stats: ({})
    property var jobs: ({})
    property var scans: ({})
    property var errors: ({})
    property var requests: ({})
    property int session: 0
    readonly property bool busy: Object.keys(jobs).some(function(a) { return running(a) })
    function running(account) { return ["starting", "queued", "scanning", "running"].indexOf((jobs[account] || {}).status) >= 0 }
    function put(field, key, value) { const next = Object.assign({}, analysis[field]); next[key] = value; analysis[field] = next }
    function send(method, account, params, kind) {
        const ticket = backend.assistanceRequest(method, params || {}, account)
        if (!ticket) return
        const next = Object.assign({}, requests)
        next[ticket] = {account:account, kind:kind, session:session}
        requests = next
    }
    function pending(account, kind) { return Object.keys(requests).some(function(k) { return requests[k].account === account && requests[k].kind === kind }) }
    function refresh(account) {
        if (!backend.connected || pending(account,"stats") || scans[account]) return
        send("assist_backlog", account, {}, "stats")
    }
    function refreshAll(force) {
        if (!showing || !backend.connected) return
        for (const account of backend.allAccounts) if ((account.lifecycle || {}).state !== "removed" && (force || !stats[account.id])) refresh(account.id)
    }
    function start(account, range) {
        if (!backend.connected || running(account) || !(stats[account] || {}).enabled) return
        put("errors", account, "")
        put("jobs", account, {status:"starting", range:range, completed:0, failed:0, processed:0, total:0})
        send("assist_batch_start", account, {range:range}, "start")
    }
    function stop(account) {
        const job = jobs[account] || {}
        if (job.job_id && !pending(account,"cancel")) send("assist_cancel", account, {job_id:job.job_id}, "cancel")
        else if (job.status === "starting") put("jobs", account, Object.assign({},job,{stopRequested:true}))
    }
    // The exact per-range scan reads every message, so it runs only when the
    // Catch up menu opens; the card's live counts come from assist_progress.
    Component.onCompleted: if (backend.connected) send("assist_jobs", "", {}, "jobs")
    Connections {
        target: backend
        function onConnectedChanged() {
            ++analysis.session
            analysis.requests = ({})
            analysis.scans = ({})
            if (!backend.connected) {
                for (const account of Object.keys(analysis.jobs)) if (analysis.running(account)) {
                    analysis.put("jobs",account,Object.assign({},analysis.jobs[account],{status:"canceled"}))
                    analysis.put("errors",account,"Session ended. Finished results are saved.")
                }
            } else analysis.send("assist_jobs","",{},"jobs")
        }
        function onAssistanceReply(ticket, method, result, ok) {
            if (method === "assist_settings_save" && ok) analysis.stats = ({})
            const request = analysis.requests[ticket]
            if (!request) return
            const next = Object.assign({},analysis.requests); delete next[ticket]; analysis.requests=next
            if (request.session !== analysis.session) return
            const account = request.account, kind = request.kind
            if (!ok) {
                analysis.put("errors",account,result.message || "Local analysis unavailable. Try again.")
                if (kind === "start" || kind === "job") analysis.put("jobs",account,Object.assign({},analysis.jobs[account] || {},{status:"failed"}))
                if (kind === "scan" || kind === "stats") analysis.put("scans",account,"")
                return
            }
            if (kind === "stats" || kind === "scan") {
                if (result.job_id && ["scanning","queued","running"].indexOf(result.status) >= 0) analysis.put("scans",account,result.job_id)
                else {
                    analysis.put("scans",account,"")
                    if (result.error) analysis.put("errors",account,result.error.message)
                    else if (result.status !== "canceled") analysis.put("stats",account,result)
                }
            }
            else if (kind === "jobs") {
                for (const job of result.items || []) if (job.kind === "backlog") analysis.put("jobs",job.account,job)
            } else {
                const old = analysis.jobs[account] || {}
                analysis.put("jobs",account,Object.assign({},old,result))
                if (kind === "start" && old.stopRequested) analysis.stop(account)
                if (!analysis.running(account)) {
                    if (result.error) analysis.put("errors",account,result.error.message || "Some messages could not be processed.")
                    analysis.refresh(account)
                    backend.refresh()
                }
            }
        }
    }
    Timer {
        interval: 500; repeat: true; running: backend.connected && (analysis.busy || Object.keys(analysis.scans).some(function(a) { return !!analysis.scans[a] }))
        onTriggered: {
            for (const account of Object.keys(analysis.jobs)) {
                const job = analysis.jobs[account]
                if (analysis.running(account) && job.job_id && !analysis.pending(account,"job"))
                    analysis.send("assist_job",account,{job_id:job.job_id},"job")
            }
            for (const account of Object.keys(analysis.scans)) if (analysis.scans[account] && !analysis.pending(account,"scan"))
                analysis.send("assist_job",account,{job_id:analysis.scans[account]},"scan")
        }
    }
}
