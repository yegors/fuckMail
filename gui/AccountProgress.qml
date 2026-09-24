import QtQuick

// Per-account pipeline: Download (mail and calendar from Google) then Local
// model (summaries and labels for downloaded mail). The Accounts card, the
// top-bar account tabs and their hover card all read view(account).
// Counts come from the engine; rates are observed here (download) or by the
// engine (seconds per analysed message), never assumed.
QtObject {
    id: model
    property bool active: true
    // account -> assist_progress row
    property var analysis: ({})
    // account -> [{t, mail}] samples over the last minute, for download rates
    property var samples: ({})
    // account -> true once a calendar pass has finished this session
    property var calendarSettled: ({})
    // account -> Date.now() when this session paused it
    property var pausedAt: ({})
    property int tick: 0
    property string ticket: ""

    function fmtN(n) {
        n = Math.max(0, Math.round(Number(n) || 0))
        if (n < 1000) return String(n)
        if (n < 1000000) { const k = n / 1000; return (k < 100 ? k.toFixed(1).replace(/\.0$/, "") : Math.round(k)) + "k" }
        const m = n / 1000000; return (m < 100 ? m.toFixed(1).replace(/\.0$/, "") : Math.round(m)) + "M"
    }
    function full(n) { return Math.max(0, Math.round(Number(n) || 0)).toLocaleString(Qt.locale(), "f", 0) }
    function eta(seconds) {
        seconds = Math.round(seconds)
        if (seconds < 60) return "under a minute"
        const minutes = Math.round(seconds / 60)
        if (minutes < 60) return "about " + minutes + " min"
        const hours = seconds / 3600
        if (hours < 24) return "about " + (hours < 10 ? Math.round(hours * 2) / 2 : Math.round(hours)) + " h"
        return "about " + Math.round(hours / 24) + " days"
    }
    function shortEta(seconds) { return eta(seconds).replace("about ", "") }
    function ago(ms) {
        const seconds = Math.max(0, (Date.now() - ms) / 1000)
        return seconds < 60 ? "just now" : seconds < 3600 ? Math.floor(seconds / 60) + " min ago"
            : seconds < 86400 ? Math.floor(seconds / 3600) + " h ago" : Math.floor(seconds / 86400) + " days ago"
    }
    function refresh() {
        if (!active || !backend.connected || ticket.length) return
        ticket = backend.assistanceRequest("assist_progress", {}, "")
    }
    function markPaused(account, paused) {
        const next = Object.assign({}, pausedAt)
        if (paused) next[account] = Date.now(); else delete next[account]
        pausedAt = next
    }
    function sample() {
        const now = Date.now(), next = {}
        for (const row of backend.accounts) {
            const sync = (row.google || {}).sync || {}
            const list = (samples[row.id] || []).filter(s => now - s.t <= 60000)
            list.push({t: now, mail: Number(sync.retained_sources || 0)})
            next[row.id] = list
            const state = backend.googleSyncState(row.id), calendar = (state.progress || {}).calendar
            if (!calendarSettled[row.id] && (calendar === "waiting" || calendar === "current")) {
                const settled = Object.assign({}, calendarSettled); settled[row.id] = true; calendarSettled = settled
            }
        }
        samples = next
    }
    function mailRate(account) {
        const list = samples[account] || []
        if (list.length < 2) return 0
        const first = list[0], last = list[list.length - 1]
        return last.t > first.t ? Math.max(0, (last.mail - first.mail) / ((last.t - first.t) / 1000)) : 0
    }
    function serviceOn(account, service) { return (((backend.preferences.google_sync_services || {})[account] || {})[service]) !== false }

    readonly property color amber: Theme.amber
    readonly property color aiColor: Theme.ai
    function view(account) {
        const _ = tick
        const row = backend.allAccounts.find(a => a.id === account) || {}
        const google = row.google || {}, sync = google.sync || {}, caps = google.capabilities || {}
        const state = backend.googleSyncState(account), progress = state.progress || {}
        const A = analysis[account] || {}
        const err = !backend.syntheticProfile && (google.connection_state !== "connected" || state.credential_access_required === true)
        const paused = !err && !backend.syntheticProfile && !state.active
        const mailShown = serviceOn(account, "mail") && caps.mail_read !== false
        const calShown = serviceOn(account, "calendar") && caps.calendar_read !== false
        // Download: mail
        const local = Number(sync.retained_sources || 0), remote = Math.max(local, Number(sync.observed_messages || 0))
        const listing = sync.inventory_complete !== true
        const dlLeft = Math.max(0, remote - local), dlDone = !listing && dlLeft === 0
        const dlOn = mailShown && !dlDone && !paused && !err
        const dlPct = remote ? Math.floor(local / remote * 100) : 100
        const rate = mailRate(account), dlEta = rate > 0 && !listing ? dlLeft / rate : -1
        const mail = {
            count: fmtN(local) + " of " + fmtN(remote) + (listing ? "+" : ""),
            countTitle: full(local) + " of " + full(remote) + (listing ? " or more" : "") + " messages are on this machine",
            frac: remote ? local / remote : (dlDone ? 1 : 0),
            // Until Google's list is counted there is no honest percentage.
            short: err ? (listing ? "Stopped" : "Stopped at " + dlPct + "%") : paused && !dlDone ? (listing ? "Paused" : "Paused at " + dlPct + "%")
                : dlOn ? (listing ? "Counting mail…" : dlPct + "%" + (dlEta >= 0 ? " · " + shortEta(dlEta) : "")) : "Complete",
            status: err ? (listing ? "Stopped · reconnect" : "Stopped at " + dlPct + "% · reconnect")
                : paused && !dlDone ? (listing ? "Paused · still counting mail in Google" : "Paused at " + dlPct + "% · " + fmtN(dlLeft) + " to go")
                : dlOn ? (listing ? "Counting mail in Google…" : dlPct + "% · " + fmtN(dlLeft) + " to go" + (dlEta >= 0 ? " · " + eta(dlEta) + " left" : ""))
                : "All mail is on this machine",
            label: dlOn ? "Downloading" : "Mail",
            fill: err ? Theme.error : dlOn ? amber : Theme.inkAlpha(.35),
            live: dlOn, eta: dlEta
        }
        // Download: calendar. Google reports no event total, so the first pass
        // shows its count while it runs and is complete once a pass finishes.
        const events = Number(row.calendar_events || 0)
        const calRunning = progress.calendar === "starting" || progress.calendar === "running"
        const calDone = !!calendarSettled[account] || (!calRunning && events > 0)
        const calOn = calShown && !calDone && !paused && !err
        const cal = {
            count: fmtN(events) + (events === 1 ? " event" : " events"),
            countTitle: full(events) + " events are on this machine",
            frac: calDone ? 1 : 0,
            short: err ? "Stopped" : paused && !calDone ? "Paused" : calOn ? "Downloading…" : calDone ? "Complete" : "No events yet",
            fill: err ? Theme.error : calOn ? amber : Theme.inkAlpha(.35),
            live: calOn
        }
        // Local model
        const modelReady = A.model_configured === true, sOn = A.summaries !== false, lOn = A.classification !== false
        const done = Number(A.done || 0), queued = Number(A.queued || 0), rest = Number(A.unprocessed || 0), eligible = Math.max(1, Number(A.eligible || 0))
        const aiPaused = A.paused === true
        const aiOn = modelReady && !err && !paused && !aiPaused && queued > 0 && (sOn || lOn)
        const aiPct = Math.floor(done / eligible * 100), perMessage = Number(A.seconds_per_message || 0)
        const aiEta = perMessage > 0 ? queued * perMessage : -1
        const what = sOn && lOn ? "Summarizing & labeling" : sOn ? "Summarizing" : lOn ? "Labeling" : "Processing"
        const ai = {
            count: fmtN(done) + " of " + fmtN(A.eligible || 0),
            countTitle: full(done) + " of " + full(A.eligible || 0) + " received messages processed by the local model · " + full(queued) + " queued"
                + (perMessage > 0 ? " · about " + perMessage.toFixed(1) + " s each" : ""),
            frac: done / eligible, qfrac: queued / eligible,
            short: A.account === undefined ? "…" : !modelReady ? "No model" : !sOn && !lOn ? "Off" + (rest ? " · " + fmtN(rest) + " not processed" : "")
                : (aiPaused || paused || err) && queued ? "Paused · " + fmtN(queued) + " queued"
                : aiOn ? aiPct + "% · " + fmtN(queued) + " queued" + (aiEta >= 0 ? " · " + shortEta(aiEta) : "")
                : rest ? fmtN(rest) + " not processed" : "Complete",
            status: !modelReady ? "No local model set up" : err && queued ? "Waiting · " + fmtN(queued) + " queued"
                : (aiPaused || paused) && queued ? "Paused at " + aiPct + "% · " + fmtN(queued) + " queued"
                : aiOn ? aiPct + "% · " + fmtN(queued) + " queued" + (aiEta >= 0 ? " · " + eta(aiEta) + " left" : "")
                : !sOn && !lOn ? "Off for this account" + (rest ? " · " + fmtN(rest) + " not processed" : "")
                : rest ? fmtN(rest) + " not processed" : "Every message is processed",
            label: aiOn ? what : "Processed",
            live: aiOn, eta: aiEta, modelReady: modelReady, paused: aiPaused, summaries: sOn, labels: lOn,
            unprocessed: rest, queued: queued, catchingUp: A.catching_up === true,
            canToggle: modelReady && !err && !paused && (queued > 0 || aiPaused)
        }
        const phase = err ? "err" : paused ? "paused" : mail.live || cal.live ? "dl" : ai.live ? "ai" : "idle"
        const lastSync = Number(state.last_mail_sync_at || row.last_mail_sync_at || 0)
        const dlEtaAll = mail.live ? mail.eta : -1
        const status = phase === "err" ? "Disconnected"
            : phase === "paused" ? "Paused" + (pausedAt[account] ? " " + ago(pausedAt[account]) : "")
            : phase === "dl" ? "Downloading" + (dlEtaAll >= 0 ? " · " + eta(dlEtaAll) + " left" : "")
            : phase === "ai" ? "Processing" + (ai.eta >= 0 ? " · " + eta(ai.eta) + " left" : "")
            : "Up to date" + (lastSync > 0 ? " · " + ago(lastSync * 1000) : "")
        return {phase: phase, status: status, err: err, paused: paused, mail: mail, cal: cal, ai: ai, mailShown: mailShown, calShown: calShown,
            ring: phase === "dl" ? mail.frac : phase === "ai" ? ai.frac : 0, ringColor: phase === "dl" ? amber : aiColor,
            ringShown: phase === "dl" || phase === "ai"}
    }

    property Timer poll: Timer {
        interval: 3000; repeat: true; triggeredOnStart: true
        running: model.active && backend.connected
        onTriggered: { model.sample(); model.refresh(); model.tick++ }
    }
    property Connections replies: Connections {
        target: backend
        function onAssistanceReply(ticket, method, result, ok) {
            // A person's action shows at once rather than on the next poll.
            if (ok && ["assist_batch_start", "assist_cancel", "assist_settings_save"].indexOf(method) >= 0) { model.ticket = ""; model.refresh() }
            if (ticket !== model.ticket) return
            model.ticket = ""
            if (!ok) return
            const next = {}
            for (const row of result.accounts || []) next[row.account] = row
            model.analysis = next
        }
        function onConnectedChanged() { model.ticket = "" }
    }
}
