import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

ColumnLayout {
    id: panel
    spacing: 14
    signal saveCompleted()
    property var saved: ({})
    property var draft: ({})
    property int revision: -1
    property var models: []
    property var labels: []
    property var requests: ({})
    property string accountId: backend.account
    property string status: ""
    property bool conflict: false
    property string discoveryTicket: ""
    enabled: !saving
    property string jobErrors: ""
    property bool saving: false
    readonly property string provider: draft.provider || "ollama"
    readonly property var providers: [{name:"Ollama",id:"ollama"},{name:"OpenAI-compatible · local",id:"openai_compatible"}]
    readonly property bool dirty: revision >= 0 && JSON.stringify(saved) !== JSON.stringify(draft)
    readonly property var choices: [{name:"Choose a model",digest:""}].concat(models.filter(m => m.eligible))
    readonly property var policy: (draft.accounts || {})[accountId] || {classification:true,summaries:true,descriptions:{}}
    readonly property var requestTemplate: {
        try { return JSON.parse(draft.prompt || "{}") } catch (e) { return ({}) }
    }
    readonly property var promptMessages: requestTemplate.messages || []
    function editMessage(index, content) {
        if (!promptMessages[index] || promptMessages[index].content === content) return
        const request = copy(requestTemplate)
        request.messages[index].content = content
        edit("prompt", JSON.stringify(request, null, 2))
    }
    function copy(v) { return JSON.parse(JSON.stringify(v)) }
    function send(method, params, kind, account, extra) {
        const ticket = backend.assistanceRequest(method, params || {}, account || "")
        const r = Object.assign({}, requests); r[ticket] = Object.assign({kind:kind || method,account:account || ""},extra || {}); requests = r
        return ticket
    }
    function edit(key,value) { if(JSON.stringify(draft[key])===JSON.stringify(value))return; const d=copy(draft); d[key]=value; draft=d }
    function accountEdit(key,value) { const d=copy(draft); if (!d.accounts) d.accounts={}; const a=copy(policy); a[key]=value; d.accounts[accountId]=a; draft=d }
    function describe(id,value) { const descriptions=copy(policy.descriptions || {}); descriptions[id]=value; accountEdit("descriptions",descriptions) }
    function indexFor(task) { const selected=draft.model; return selected ? choices.findIndex(m=>m.name===selected.name && m.digest===selected.digest) : 0 }
    function changeProvider(value) {
        if (value === provider) return
        const d = copy(draft)
        if (value === "ollama") delete d.provider; else d.provider = value
        d.endpoint = value === "ollama" ? "http://127.0.0.1:11434" : "http://127.0.0.1:1234"
        d.model = null; draft = d
        models = []; discoveryTicket = ""; status = "Enter the server address, then Detect."
    }
    function editEndpoint(value) {
        if (value === draft.endpoint) return
        edit("endpoint",value); edit("model",null)
        models = []; discoveryTicket = ""; status = "Server address changed. Detect models to continue."
    }
    function refresh() {
        const endpoint = draft.endpoint || "http://127.0.0.1:11434"
        models = []; status = "Looking for models…"
        discoveryTicket = send("assist_discover", {endpoint:endpoint,provider:provider}, "discover", "", {endpoint:endpoint,provider:provider})
    }
    function save() { if(saving)return; saving=true; send("assist_settings_save",{expected_revision:revision,settings:draft},"save") }
    function discard() { draft=copy(saved); status="Changes discarded" }
    function reloadSaved() { send("assist_settings_get",{},"reload") }
    onVisibleChanged: { if (visible && backend.connected) send("assist_settings_get",{},"load"); }
    onAccountIdChanged: { labels=[]; if (accountId && visible && backend.connected) send("assist_catalog",{},"labels",accountId) }
    Component.onCompleted: if (visible && backend.connected) send("assist_settings_get",{},"load")
    Connections {
        target: backend
        function onConnectedChanged() {
            if (!backend.connected) {panel.requests=({});panel.saving=false}
            else if (panel.visible) panel.send("assist_settings_get",{},"load")
        }
        function onAssistanceReply(ticket,method,result,ok) {
            const req=panel.requests[ticket]; if (!req) return
            const kind=req.kind
            const r=Object.assign({},panel.requests); delete r[ticket]; panel.requests=r
            if(kind==="ignore")return
            if(kind==="labels" && req.account!==panel.accountId)return
            if(kind==="discover" && (ticket!==panel.discoveryTicket || req.endpoint!==panel.draft.endpoint || req.provider!==panel.provider))return
            if (kind==="save") panel.saving=false
            if (!ok) {
                panel.status=result.message || "Local model request failed"
                if(kind==="save")panel.conflict=true
                return
            }
            if (kind==="load" || kind==="save" || kind==="reload") {
                if (kind!=="load" || !panel.dirty) { panel.saved=panel.copy(result.settings); panel.draft=panel.copy(result.settings); panel.revision=result.revision }
                if (kind!=="load")panel.conflict=false
                if (kind==="save") { panel.status="Saved"; panel.saveCompleted() }
                if (panel.visible) { panel.refresh(); panel.send("assist_jobs",{},"jobs"); if(panel.accountId)panel.send("assist_catalog",{},"labels",panel.accountId) }
            } else if (kind==="discover") { panel.models=result.models || []; panel.status=(panel.provider === "ollama" ? "Ollama "+(result.version || "") : "Local server")+" · "+panel.models.length+(panel.models.length === 1 ? " model" : " models") }
            else if (kind==="labels") panel.labels=result.categories || []
            else if (kind==="jobs") panel.jobErrors=(result.items || []).filter(j=>j.error).map(j=>j.error.message).join("\n")
        }
    }
    Text { Layout.fillWidth:true; Layout.maximumWidth:560; text:"Summarizes messages, labels new mail, and tells people from automated senders. Mail is sent to the local server you choose."; wrapMode:Text.Wrap; lineHeight:1.5; color:Theme.muted; font.family:Theme.fontFamily; font.pixelSize:Theme.fontSize(13.5) }
    Text { text:"Provider"; Layout.topMargin:14; color:Theme.inkAlpha(0.45); font.family:Theme.fontFamily; font.pixelSize:Theme.fontSize(11.5) }
    FmComboBox { objectName:"localModelProvider"; Layout.fillWidth:true; Layout.maximumWidth:360; textRole:"name"; model:panel.providers; currentIndex:panel.providers.findIndex(p=>p.id===panel.provider); onActivated:panel.changeProvider(panel.providers[currentIndex].id); Accessible.name:"Local model provider" }
    Text { Layout.fillWidth:true; Layout.maximumWidth:560; text:panel.provider === "ollama" ? "Connect to Ollama running on this machine to list its installed models." : "Connect to LM Studio, vLLM, or another local compatible server. Its configuration controls where the model runs and its context size."; wrapMode:Text.Wrap; lineHeight:1.5; color:Theme.muted; font.family:Theme.fontFamily; font.pixelSize:Theme.fontSize(13.5) }
    RowLayout {
        Layout.fillWidth:true; spacing:10
        FmTextField { objectName:"ollamaEndpoint"; Layout.fillWidth:true; Layout.maximumWidth:360; text:panel.draft.endpoint || ""; placeholderText:panel.provider === "ollama" ? "http://localhost:11434" : "http://localhost:1234"; onTextEdited:panel.editEndpoint(text); Accessible.name:"Local model endpoint" }
        FmButton { objectName:"ollamaRefresh"; text:"Detect"; outlined:true; implicitHeight:34; leftPadding:14; rightPadding:14; onClicked:panel.refresh() }
        Item { Layout.fillWidth:true }
    }
    RowLayout {
        Layout.fillWidth:true; Layout.maximumWidth:560; spacing:10
        Rectangle { implicitWidth:8; implicitHeight:8; radius:4; color:panel.models.length ? Theme.accent : Theme.subtle }
        Text { text:panel.status.length ? panel.status : "Choose a local endpoint, then Detect."; color:Theme.secondary; wrapMode:Text.Wrap; Layout.fillWidth:true; font.family:Theme.fontFamily; font.pixelSize:Theme.fontSize(13) }
    }
    Text { text:"Model"; Layout.topMargin:8; color:Theme.inkAlpha(0.45); font.family:Theme.fontFamily; font.pixelSize:Theme.fontSize(11.5) }
    FmComboBox { objectName:"mailModel"; Layout.fillWidth:true; Layout.maximumWidth:360; textRole:"name"; model:panel.choices; currentIndex:panel.indexFor(); onActivated:{const m=panel.choices[currentIndex];panel.edit("model",currentIndex===0?null:{name:m.name,digest:m.digest})} Accessible.name:"Mail model" }
    Text { visible:panel.indexFor()<0; text:"Selected model is missing or changed. Detect models and choose again."; color:Theme.secondary; wrapMode:Text.Wrap; Layout.fillWidth:true; font.family:Theme.fontFamily; font.pixelSize:Theme.fontSize(12) }
    RowLayout {
        Layout.fillWidth:true; Layout.maximumWidth:560; Layout.topMargin:18
        Text { text:"On new mail"; color:Theme.inkAlpha(0.45); font.family:Theme.fontFamily; font.pixelSize:Theme.fontSize(11.5); Layout.fillWidth:true }
        FmComboBox { objectName:"assistanceAccount"; Layout.preferredWidth:160; model:backend.accounts; textRole:"name"; currentIndex:backend.accounts.findIndex(a=>a.id===panel.accountId); onActivated:panel.accountId=backend.accounts[currentIndex].id; Accessible.name:"Assistance account" }
    }
    ColumnLayout {
        Layout.fillWidth:true; Layout.maximumWidth:560; spacing:0
        FmCheckBox { objectName:"enableSummaries"; Layout.fillWidth:true; text:"Summarize messages"; description:"A short summary of the main point and anything you need to do."; checked:panel.policy.summaries || false; enabled:!!panel.accountId; onClicked:panel.accountEdit("summaries",checked) }
        FmCheckBox { objectName:"enableClassification"; Layout.fillWidth:true; text:"Label new mail"; description:"Apply one matching label and identify human or automated senders. You can correct either."; checked:panel.policy.classification || false; enabled:!!panel.accountId; onClicked:panel.accountEdit("classification",checked) }
    }
    Text { text:"Prompt"; Layout.topMargin:18; color:Theme.inkAlpha(0.45); font.family:Theme.fontFamily; font.pixelSize:Theme.fontSize(11.5) }
    Text { Layout.fillWidth:true; Layout.maximumWidth:560; text:"Sent with each incoming message. Labels and their instructions from Settings → Mail fill {{labels}}; {{messages}} contains the email being analyzed."; wrapMode:Text.Wrap; lineHeight:1.5; textFormat:Text.PlainText; color:Theme.muted; font.family:Theme.fontFamily; font.pixelSize:Theme.fontSize(13.5) }
    Repeater {
        model:panel.promptMessages.length
        ColumnLayout {
            required property int index
            Layout.fillWidth:true; Layout.maximumWidth:560
            readonly property string roleName:panel.promptMessages[index].role === "system" ? "System" : "User"
            Text { text:parent.roleName; color:Theme.text; font.family:Theme.fontFamily; font.pixelSize:Theme.fontSize(13) }
            TextArea {
                objectName:"mailPrompt"+index; Layout.fillWidth:true; Layout.preferredHeight:Math.max(100,contentHeight+24)
                text:panel.promptMessages[index].content
                onTextChanged:if(activeFocus)panel.editMessage(index,text)
                wrapMode:TextEdit.Wrap; textFormat:TextEdit.PlainText; selectByMouse:true; color:Theme.text
                font.family:Theme.fontFamily; font.pixelSize:Theme.fontSize(13); padding:12
                background:Rectangle{color:Theme.bg2;radius:6;border.color:parent.activeFocus ? Theme.focusBorder : Theme.line2}
                Accessible.name:parent.roleName+" prompt"
            }
        }
    }
    RowLayout {
        FmButton { objectName:"saveLocalModel"; text:"Save"; enabled:panel.dirty && !panel.saving; onClicked:panel.save() }
        FmButton { text:"Cancel edits"; enabled:panel.dirty; onClicked:panel.discard() }
        FmButton { objectName:"reloadModelSettings"; text:"Reload saved settings"; visible:panel.conflict; onClicked:panel.reloadSaved() }
    }
    Text { visible:panel.jobErrors.length>0; text:panel.jobErrors; Layout.fillWidth:true; wrapMode:Text.Wrap; color:Theme.secondary; font.family:Theme.fontFamily; font.pixelSize:Theme.fontSize(12) }
    Text { visible:panel.models.some(m=>!m.eligible); Layout.fillWidth:true; text:panel.models.filter(m=>!m.eligible).map(m=>m.name+": "+m.reason).join("\n"); wrapMode:Text.Wrap; color:Theme.subtle; font.family:Theme.fontFamily; font.pixelSize:Theme.fontSize(11) }
}
