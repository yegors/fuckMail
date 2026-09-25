import QtQuick

// Gmail Spam for the current account, read live while the Spam view is open.
// Sync never downloads spam and nothing here is stored: the list holds
// headers and Google's snippet, an opened message only its text.
QtObject {
    id: spam
    property bool active: false
    property string account: ""
    property var rows: []
    property string nextToken: ""
    property bool loading: false
    property string error: ""
    property string selected: ""
    readonly property var selectedRow: rows.find(r => r.id === selected) || ({})
    property string text: ""
    property bool opening: false
    property string openError: ""
    property string listTicket: ""
    property string openTicket: ""
    function explain(result) {
        const code = (result || {}).code || ""
        if (code === "sync_not_running") return "Start syncing this account to see its Spam."
        if (code === "capability_unavailable") return "This account has not allowed reading mail."
        if (code === "google_rate_limited") return "Google asked us to slow down. Try again in a minute."
        return (result || {}).message || "Spam could not be read from Google."
    }
    function reload() {
        account = backend.account; rows = []; nextToken = ""; error = ""
        selected = ""; text = ""; openError = ""; opening = false; openTicket = ""; loading = false
        more()
    }
    function more() {
        if (loading || !active) return
        loading = true; error = ""
        listTicket = backend.spamRequest("google_spam_list", nextToken.length ? {page_token: nextToken} : {})
    }
    function open(id) {
        if (!id || id === selected && (text.length || opening)) return
        selected = id; text = ""; openError = ""; opening = true
        openTicket = backend.spamRequest("google_spam_message", {id: id})
    }
    onActiveChanged: if (active) reload(); else { listTicket = ""; openTicket = ""; loading = false; opening = false }
    property Connections replies: Connections {
        target: backend
        function onSpamReply(ticket, method, account, result, ok) {
            if (account !== spam.account) return
            if (ticket === spam.listTicket) {
                spam.listTicket = ""; spam.loading = false
                if (!ok) { spam.error = spam.explain(result); return }
                const seen = {}
                spam.rows.forEach(r => seen[r.id] = true)
                spam.rows = spam.rows.concat((result.messages || []).filter(r => !seen[r.id]))
                spam.nextToken = result.next_page_token || ""
            } else if (ticket === spam.openTicket) {
                spam.openTicket = ""; spam.opening = false
                if (ok) spam.text = result.text || ""
                else spam.openError = spam.explain(result)
            }
        }
        function onAccountChanged() { if (spam.active && backend.account !== spam.account) spam.reload() }
    }
}
