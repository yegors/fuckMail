import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

ColumnLayout {
    id: panel
    spacing: 14
    signal saveCompleted()
    property var saved: ({})
    property var draft: ({})
    property var defaults: ({})
    property int revision: -1
    property var models: []
    property var labels: []
    property var requests: ({})
    property string accountId: backend.account
    property string status: ""
    property var sampleResult: null
    property string sampleTask: ""
    property bool startingSample: false
    property bool pollingSample: false
    property int sampleVersion: 0
    property bool conflict: false
    property string discoveryTicket: ""
    readonly property bool sampleBusy: startingSample || job.length > 0
    enabled: !saving
    property string jobErrors: ""
    property string job: ""
    property bool saving: false
    readonly property bool dirty: revision >= 0 && JSON.stringify(saved) !== JSON.stringify(draft)
    readonly property var choices: [{name:"Choose a model",digest:""}].concat(models.filter(m => m.eligible))
    readonly property var policy: (draft.accounts || {})[accountId] || {classification:false,summaries:false,descriptions:{}}
    function copy(v) { return JSON.parse(JSON.stringify(v)) }
    function send(method, params, kind, account, extra) {
        const ticket = backend.assistanceRequest(method, params || {}, account || "")
        const r = Object.assign({}, requests); r[ticket] = Object.assign({kind:kind || method,account:account || "",sampleVersion:sampleVersion},extra || {}); requests = r
        return ticket
    }
    function edit(key,value) { if(JSON.stringify(draft[key])===JSON.stringify(value))return; const d=copy(draft); d[key]=value; draft=d }
    function accountEdit(key,value) { const d=copy(draft); if (!d.accounts) d.accounts={}; const a=copy(policy); a[key]=value; d.accounts[accountId]=a; draft=d }
    function describe(id,value) { const descriptions=copy(policy.descriptions || {}); descriptions[id]=value; accountEdit("descriptions",descriptions) }
    function indexFor(task) { const selected=draft[task+"_model"]; return selected ? choices.findIndex(m=>m.name===selected.name && m.digest===selected.digest) : 0 }
    function refresh() {
        const endpoint = draft.endpoint || "http://127.0.0.1:11434"
        models = []; status = "Looking for Ollama…"
        discoveryTicket = send("assist_discover", {endpoint:endpoint}, "discover", "", {endpoint:endpoint})
    }
    function save() { if(saving)return; saving=true; send("assist_settings_save",{expected_revision:revision,settings:draft},"save") }
    function discard() { cancelSample(); draft=copy(saved); status="Changes discarded" }
    function reloadSaved() { cancelSample(); send("assist_settings_get",{},"reload") }
    function cancelSample() {
        ++sampleVersion
        if(job && backend.connected)send("assist_cancel",{job_id:job},"ignore")
        job=""; startingSample=false; pollingSample=false
    }
    function sample(task) {
        if(sampleBusy || !backend.connected)return
        ++sampleVersion; sampleTask=task; sampleResult=null; startingSample=true; status="Testing synthetic mail…"
        send("assist_start",{task:task,sample:true,settings:draft},"sample",accountId,{model:copy(draft[task+"_model"])})
    }
    onVisibleChanged: { if (visible && backend.connected) send("assist_settings_get",{},"load"); else if(!visible)cancelSample() }
    onAccountIdChanged: { cancelSample(); sampleResult=null; labels=[]; if (accountId && visible && backend.connected) send("assist_catalog",{},"labels",accountId) }
    Component.onCompleted: if (visible && backend.connected) send("assist_settings_get",{},"load")
    Connections {
        target: backend
        function onConnectedChanged() {
            if (!backend.connected) {panel.cancelSample();panel.requests=({});panel.saving=false}
            else if (panel.visible) panel.send("assist_settings_get",{},"load")
        }
        function onAssistanceReply(ticket,method,result,ok) {
            const req=panel.requests[ticket]; if (!req) return
            const kind=req.kind
            const r=Object.assign({},panel.requests); delete r[ticket]; panel.requests=r
            if(kind==="ignore")return
            if(kind==="sample" && req.sampleVersion!==panel.sampleVersion){
                if(ok && result.job_id)panel.send("assist_cancel",{job_id:result.job_id},"ignore")
                return
            }
            if(kind==="job" && (req.sampleVersion!==panel.sampleVersion || req.job!==panel.job))return
            if(kind==="labels" && req.account!==panel.accountId)return
            if(kind==="discover" && (ticket!==panel.discoveryTicket || req.endpoint!==panel.draft.endpoint))return
            if(kind==="sample")panel.startingSample=false
            if(kind==="job")panel.pollingSample=false
            if (kind==="save") panel.saving=false
            if (!ok) {
                panel.status=result.message || "Local model request failed"
                if(kind==="save")panel.conflict=true
                if(kind==="sample" || kind==="job")panel.cancelSample()
                return
            }
            if (kind==="load" || kind==="save" || kind==="reload") {
                if (kind!=="load" || !panel.dirty) { panel.saved=panel.copy(result.settings); panel.draft=panel.copy(result.settings); panel.revision=result.revision }
                panel.defaults=result.defaults
                if (kind!=="load")panel.conflict=false
                if (kind==="save") { panel.cancelSample(); panel.status="Saved"; panel.saveCompleted() }
                if (panel.visible) { panel.refresh(); panel.send("assist_jobs",{},"jobs"); if(panel.accountId)panel.send("assist_catalog",{},"labels",panel.accountId) }
            } else if (kind==="discover") { panel.models=result.models || []; panel.status="Ollama "+result.version+" · "+panel.models.length+" models" }
            else if (kind==="labels") panel.labels=result.categories || []
            else if (kind==="jobs") panel.jobErrors=(result.items || []).filter(j=>j.error).map(j=>j.error.message).join("\n")
            else if (kind==="sample") {panel.job=result.job_id; panel.sampleResult={model:req.model}; panel.status="Testing synthetic mail…"}
            else if (kind==="job") {
                if (["running","queued"].indexOf(result.status)>=0) return
                panel.job=""; panel.status=result.error ? result.error.message : result.status
                panel.sampleResult=result.items && result.items.length ? Object.assign({},result.items[0],{model:(panel.sampleResult || {}).model}) : null
            }
        }
    }
    Timer { interval: 350; running: panel.job.length>0 && backend.connected; repeat: true
        onTriggered: if(!panel.pollingSample){panel.pollingSample=true;panel.send("assist_job",{job_id:panel.job},"job","",{job:panel.job})}
    }
    Text { Layout.fillWidth:true; text:"Optional summaries and label suggestions run on this machine. You choose the models and instructions."; wrapMode:Text.Wrap; color:Theme.secondary; font.pixelSize:Theme.fontSize(13) }
    RowLayout { Layout.fillWidth:true
        Text { text:panel.status; color:Theme.secondary; wrapMode:Text.Wrap; Layout.fillWidth:true; font.pixelSize:Theme.fontSize(12) }
        FmButton { objectName:"ollamaRefresh"; text:"Refresh models"; onClicked:panel.refresh() }
    }
    FmTextField { objectName:"ollamaEndpoint"; Layout.fillWidth:true; text:panel.draft.endpoint || ""; placeholderText:"http://127.0.0.1:11434"; onTextEdited:panel.edit("endpoint",text); Accessible.name:"Local Ollama endpoint" }
    Repeater {
        model:[{task:"summary",title:"Summaries"},{task:"classification",title:"Classification"}]
        ColumnLayout {
            id: taskPane
            required property var modelData
            Layout.fillWidth:true; spacing:8
            Text { text:taskPane.modelData.title; color:Theme.text; font {pixelSize:Theme.fontSize(15);weight:Font.Medium} }
            FmComboBox { objectName:taskPane.modelData.task+"Model"; Layout.fillWidth:true; textRole:"name"; model:panel.choices; currentIndex:panel.indexFor(taskPane.modelData.task); onActivated:{const m=panel.choices[currentIndex];panel.edit(taskPane.modelData.task+"_model",currentIndex===0?null:{name:m.name,digest:m.digest})} Accessible.name:taskPane.modelData.title+" model" }
            Text { visible:panel.indexFor(taskPane.modelData.task)<0; text:"Selected model is missing or changed. Choose an installed model."; color:Theme.secondary; wrapMode:Text.Wrap; Layout.fillWidth:true; font.pixelSize:Theme.fontSize(12) }
            ScrollView { Layout.fillWidth:true; Layout.preferredHeight:180
                TextArea { objectName:taskPane.modelData.task+"Prompt"; text:panel.draft[taskPane.modelData.task+"_prompt"] || ""; onTextChanged:if(activeFocus && text!==panel.draft[taskPane.modelData.task+"_prompt"])panel.edit(taskPane.modelData.task+"_prompt",text); wrapMode:TextEdit.Wrap; textFormat:TextEdit.PlainText; selectByMouse:true; color:Theme.text; font.pixelSize:Theme.fontSize(12); padding:12; background:Rectangle{color:Theme.surface;radius:5;border.color:Theme.controlBorder} Accessible.name:taskPane.modelData.title+" instructions" }
            }
            RowLayout {
                FmButton { text:"Restore default"; onClicked:panel.edit(taskPane.modelData.task+"_prompt",panel.defaults[taskPane.modelData.task+"_prompt"]) }
                FmButton { text:"Test with sample"; enabled:!panel.sampleBusy && !!panel.draft[taskPane.modelData.task+"_model"]; onClicked:panel.sample(taskPane.modelData.task) }
            }
        }
    }
    Text { text:"Account"; color:Theme.text; font.pixelSize:Theme.fontSize(15) }
    FmComboBox { objectName:"assistanceAccount"; Layout.fillWidth:true; model:backend.accounts; textRole:"name"; currentIndex:backend.accounts.findIndex(a=>a.id===panel.accountId); onActivated:panel.accountId=backend.accounts[currentIndex].id; Accessible.name:"Assistance account" }
    CheckBox { objectName:"enableSummaries"; text:"Enable on-demand summaries"; checked:panel.policy.summaries || false; enabled:!!panel.accountId; onClicked:panel.accountEdit("summaries",checked) }
    CheckBox { objectName:"enableClassification"; text:"Suggest labels and classify new incoming mail"; checked:panel.policy.classification || false; enabled:!!panel.accountId; onClicked:panel.accountEdit("classification",checked) }
    Text { Layout.fillWidth:true; text:"Suggestions use this account’s labels. Accepting a suggestion applies that label. Human / Automated / Unknown is a separate, correctable annotation."; wrapMode:Text.Wrap; color:Theme.secondary; font.pixelSize:Theme.fontSize(12) }
    Repeater {
        model:panel.labels
        ColumnLayout {
            required property var modelData
            Layout.fillWidth:true
            Text { text:modelData.name; color:Theme.text; font.pixelSize:Theme.fontSize(12) }
            FmTextField { Layout.fillWidth:true; text:(panel.policy.descriptions || {})[modelData.id] || ""; placeholderText:"When should this label apply? (optional)"; onTextEdited:panel.describe(modelData.id,text); Accessible.name:"Classification description for "+modelData.name }
        }
    }
    RowLayout {
        FmButton { objectName:"saveLocalModel"; text:"Save"; enabled:panel.dirty && !panel.saving; onClicked:panel.save() }
        FmButton { text:"Cancel edits"; enabled:panel.dirty; onClicked:panel.discard() }
        FmButton { objectName:"reloadModelSettings"; text:"Reload saved settings"; visible:panel.conflict; onClicked:panel.reloadSaved() }
        FmButton { text:"Cancel sample"; visible:panel.sampleBusy; onClicked:{panel.cancelSample();panel.status="Sample canceled";panel.sampleResult=null} }
    }
    Text { visible:panel.jobErrors.length>0; text:panel.jobErrors; Layout.fillWidth:true; wrapMode:Text.Wrap; color:Theme.secondary; font.pixelSize:Theme.fontSize(12) }
    SummaryCard {
        objectName: "sampleSummary"; Layout.fillWidth: true; title: "Sample summary"; showSources: false
        result: panel.sampleTask === "summary" && panel.sampleResult && panel.sampleResult.output ? panel.sampleResult : null
        busy: panel.sampleTask === "summary" && panel.sampleBusy
        canSummarize: !panel.sampleBusy && !!panel.draft.summary_model && backend.connected
        onRegenerate: panel.sample("summary")
        onCancelRequested: { panel.cancelSample(); panel.sampleResult=null; panel.status="Sample canceled" }
    }
    Text {
        objectName: "sampleClassification"; Layout.fillWidth: true
        visible: panel.sampleTask === "classification" && panel.sampleResult && !!panel.sampleResult.output
        text: visible ? ({human:"Human",automated:"Automated",unknown:"Unknown sender kind"})[panel.sampleResult.output.sender_kind]
            + " · " + (panel.sampleResult.output.category_abstain ? "Not enough information to label" : (panel.sampleResult.output.category_ids || []).map(id => (panel.labels.find(c=>c.id===id) || {}).name || id).join(", ") || "No matching labels")
            + "\n" + panel.sampleResult.output.reason : ""
        wrapMode: Text.Wrap; textFormat: Text.PlainText; color: Theme.secondary; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(13)
    }
    Text { visible:panel.models.some(m=>!m.eligible); Layout.fillWidth:true; text:panel.models.filter(m=>!m.eligible).map(m=>m.name+": "+m.reason).join("\n"); wrapMode:Text.Wrap; color:Theme.subtle; font.pixelSize:Theme.fontSize(11) }
}
