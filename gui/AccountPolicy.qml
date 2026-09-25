import QtQuick

// Per-account local model choices shown on the Accounts card: Summarize,
// Label and the model's own pause. Saved by revision into the shared model
// settings, which the CLI edits too; a conflict reloads and asks to retry.
QtObject {
    id: policy
    property var saved: ({})
    property int revision: -1
    property string ticket: ""
    property bool saving: false
    property string status: ""
    // The account whose save produced `status`.
    property string statusAccount: ""
    signal changed()
    function of(account) { return ((saved || {}).accounts || {})[account] || {summaries: true, classification: true} }
    function load() {
        if (backend.connected && !saving && !ticket.length) ticket = backend.assistanceRequest("assist_settings_get", {}, "")
    }
    function change(account, patch) {
        if (saving || revision < 0 || !account) return false
        const settings = JSON.parse(JSON.stringify(saved)), choice = Object.assign({}, of(account), patch)
        settings.accounts = Object.assign({}, settings.accounts || {})
        settings.accounts[account] = choice
        saving = true; status = ""; statusAccount = account
        ticket = backend.assistanceRequest("assist_settings_save", {expected_revision: revision, settings: settings}, "")
        return true
    }
    property Connections replies: Connections {
        target: backend
        function onConnectedChanged() { policy.ticket = ""; policy.saving = false; policy.load() }
        function onAssistanceReply(reply, method, result, ok) {
            if (method === "assist_settings_save" && ok && reply !== policy.ticket && result.settings) { policy.saved = result.settings; policy.revision = result.revision; return }
            if (reply !== policy.ticket) return
            policy.ticket = ""
            const wasSaving = policy.saving
            policy.saving = false
            if (!ok) { policy.status = result.message || "Could not save this choice. Try again."; policy.revision = -1; policy.load(); return }
            policy.saved = result.settings || {}; policy.revision = result.revision === undefined ? -1 : result.revision
            if (wasSaving) policy.changed()
        }
    }
    Component.onCompleted: load()
}
