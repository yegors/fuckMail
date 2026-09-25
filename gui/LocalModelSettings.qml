import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

// Settings → Local LLM: the provider, the model, a sample Test, and the two
// prompt parts. The system prompt holds behaviour rules and the reply shape;
// the user message is sent with each email and must carry {{messages}}
// ({{labels}} is optional). Choosing a model saves at once; each prompt box
// saves on its own. The engine and CLI use the same `system` and `user` text.
ColumnLayout {
    id: panel
    spacing: 0
    signal saveCompleted()
    property var saved: ({})
    property var draft: ({})
    property var defaults: ({})
    property int revision: -1
    property var models: []
    property var requests: ({})
    property string status: ""
    property bool conflict: false
    property string discoveryTicket: ""
    property bool discovering: false
    property bool discovered: false
    property bool saving: false
    // Box being saved (0 system, 1 user), -1 for the whole draft, -2 the model choice.
    property int savingPart: -1
    property var savedAt: ({})
    property int clock: 0
    // Sample test: {state: run|ok|fail|err, at, ms, metrics, out, message, model}
    property var test: null
    property string testJob: ""
    readonly property string provider: draft.provider || "ollama"
    readonly property var providers: [{name:"Ollama",id:"ollama"},{name:"OpenAI-compatible · local",id:"openai_compatible"}]
    readonly property bool dirty: revision >= 0 && JSON.stringify(saved) !== JSON.stringify(draft)
    readonly property var installedModels: models.filter(m => m.eligible)
    readonly property bool selectedAvailable: !!draft.model && installedModels.some(m => m.name === draft.model.name && m.digest === draft.model.digest)
    readonly property var choices: {
        const rows = [{name:"Choose a model",digest:""}].concat(installedModels)
        if (draft.model && !selectedAvailable) rows.push(draft.model)
        return rows
    }
    // Box 0 edits `system`, box 1 edits `user`.
    readonly property var promptKeys: ["system", "user"]
    function textOf(settings, index) { return (settings || {})[promptKeys[index]] || "" }
    function boxDirty(index) { return revision >= 0 && textOf(draft, index) !== textOf(saved, index) }
    readonly property var missingVariables: ["{{messages}}", "{{labels}}"].filter(v => textOf(draft, 1).indexOf(v) < 0)
    readonly property string promptText: textOf(draft, 0) + textOf(draft, 1)
    readonly property string modelNote: {
        const m = installedModels.find(m => draft.model && m.name === draft.model.name && m.digest === draft.model.digest)
        return m && m.size ? (m.size / 1e9).toFixed(1) + " GB" : ""
    }
    function editMessage(index, content) { edit(promptKeys[index], content) }
    function copy(v) { return JSON.parse(JSON.stringify(v)) }
    function send(method, params, kind, account, extra) {
        const ticket = backend.assistanceRequest(method, params || {}, account || "")
        const r = Object.assign({}, requests); r[ticket] = Object.assign({kind:kind || method,account:account || ""},extra || {}); requests = r
        return ticket
    }
    function edit(key,value) { if(JSON.stringify(draft[key])===JSON.stringify(value))return; const d=copy(draft); d[key]=value; draft=d }
    function indexFor() { const selected=draft.model; return selected ? choices.findIndex(m=>m.name===selected.name && m.digest===selected.digest) : 0 }
    function changeProvider(value) {
        if (value === provider) return
        const d = copy(draft)
        if (value === "ollama") delete d.provider; else d.provider = value
        d.endpoint = value === "ollama" ? "http://127.0.0.1:11434" : "http://127.0.0.1:1234"
        d.model = null; draft = d
        models = []; discoveryTicket = ""; discovering = false; discovered = false; status = "Enter the server address, then Detect."
    }
    function editEndpoint(value) {
        if (value === draft.endpoint) return
        edit("endpoint",value); edit("model",null)
        models = []; discoveryTicket = ""; discovering = false; discovered = false; status = "Server address changed. Detect models to continue."
    }
    function refresh() {
        const endpoint = draft.endpoint || "http://127.0.0.1:11434"
        discovering = true; discovered = false; status = "Looking for models…"
        discoveryTicket = send("assist_discover", {endpoint:endpoint,provider:provider}, "discover", "", {endpoint:endpoint,provider:provider})
    }
    // Saved settings with only the given changes applied.
    function savedWith(change) { const settings = copy(saved); change(settings); return settings }
    function commit(settings, part) {
        if (saving || revision < 0) return
        saving = true; savingPart = part
        send("assist_settings_save", {expected_revision: revision, settings: settings}, "save", "", {part: part})
    }
    function save() { commit(draft, -1) }
    function chooseModel(model) {
        edit("model", model)
        commit(savedWith(s => { s.endpoint = draft.endpoint; s.model = model; if (draft.provider) s.provider = draft.provider; else delete s.provider }), -2)
    }
    function saveBox(index) {
        if (!boxDirty(index)) return
        const content = textOf(draft, index)
        commit(savedWith(s => { s[promptKeys[index]] = content }), index)
    }
    function cancelBox(index) { editMessage(index, textOf(saved, index)) }
    function restoreBox(index) { editMessage(index, textOf(defaults, index)) }
    function discard() { draft=copy(saved); status="Changes discarded" }
    function reloadSaved() { send("assist_settings_get",{},"reload") }
    // After a partial save, the other unsaved edits stay in the draft.
    function adopt(settings, part) {
        const next = copy(settings)
        if (part >= 0 || part === -2) {
            for (let i = 0; i < promptKeys.length; ++i) if (i !== part) next[promptKeys[i]] = textOf(draft, i)
            if (part >= 0) { next.endpoint = draft.endpoint; next.model = draft.model; if (draft.provider) next.provider = draft.provider; else delete next.provider }
        }
        draft = next
    }
    function runTest() {
        if (test && test.state === "run") return
        const model = draft.model
        if (!model) return
        test = {state: "run", at: Date.now(), model: model.name}
        testJob = ""
        send("assist_start", {sample: true, sample_case: "personal", settings: draft}, "test", backend.account || "")
    }
    function testFinished(job) {
        const at = test ? test.at : Date.now(), ms = Date.now() - at, model = test ? test.model : ""
        if (job.status === "completed") {
            const item = (job.items || [])[0] || {}
            test = {state: "ok", at: at, ms: (item.metrics || {}).total_ms || ms, model: model, metrics: item.metrics || {}, out: JSON.stringify(item.output || {}, null, 2)}
            return
        }
        const error = job.error || {}
        const format = ["invalid_assistance_result", "assistance_output_limit"].indexOf(error.code) >= 0
        test = {state: format ? "fail" : "err", at: at, ms: (error.metrics || {}).total_ms || ms, model: model, metrics: error.metrics || {},
            out: format ? String(error.raw || "") : "", message: error.message || (job.status === "canceled" ? "The test was stopped." : "The model did not answer.")}
    }
    onVisibleChanged: { if (visible && backend.connected) send("assist_settings_get",{},"load") }
    Component.onCompleted: if (visible && backend.connected) send("assist_settings_get",{},"load")
    Timer { interval: 100; repeat: true; running: !!panel.test && panel.test.state === "run"; onTriggered: panel.clock++ }
    Timer { interval: 1600; running: Object.keys(panel.savedAt).length > 0; onTriggered: panel.savedAt = ({}) }
    Timer {
        interval: 300; repeat: true; running: panel.testJob.length > 0 && backend.connected
        onTriggered: if (!Object.values(panel.requests).some(r => r.kind === "testPoll")) panel.send("assist_job", {job_id: panel.testJob}, "testPoll")
    }
    Connections {
        target: backend
        function onConnectedChanged() {
            if (!backend.connected) {panel.requests=({});panel.saving=false;panel.discovering=false;panel.discovered=false;panel.testJob="";if(panel.test&&panel.test.state==="run")panel.test={state:"err",at:panel.test.at,ms:Date.now()-panel.test.at,model:panel.test.model,message:"The connection to the local engine closed.",metrics:{}}}
            else if (panel.visible) panel.send("assist_settings_get",{},"load")
        }
        function onAssistanceReply(ticket,method,result,ok) {
            const req=panel.requests[ticket]
            // Another control (an account's Summarize, Label or pause) saved settings:
            // adopt them while there is nothing unsaved here, so the next save is current.
            if (!req && method==="assist_settings_save" && ok && result.settings && !panel.dirty && !panel.saving) {
                panel.saved=panel.copy(result.settings); panel.draft=panel.copy(result.settings); panel.revision=result.revision; return
            }
            if (!req) return
            const kind=req.kind
            const r=Object.assign({},panel.requests); delete r[ticket]; panel.requests=r
            if(kind==="discover" && (ticket!==panel.discoveryTicket || req.endpoint!==panel.draft.endpoint || req.provider!==panel.provider))return
            if(kind==="discover") panel.discovering=false
            if (kind==="save") panel.saving=false
            if (kind==="test") {
                if (!ok) { panel.testFinished({status:"failed",error:result}); return }
                panel.testJob = result.job_id || ""; return
            }
            if (kind==="testPoll") {
                if (!ok) { panel.testJob=""; panel.testFinished({status:"failed",error:result}); return }
                if (["queued","running"].indexOf(result.status) < 0) { panel.testJob=""; panel.testFinished(result) }
                return
            }
            if (!ok) {
                panel.status=result.message || "Local model request failed"
                if(kind==="save")panel.conflict=true
                return
            }
            if (kind==="load" || kind==="save" || kind==="reload") {
                if (result.defaults) panel.defaults=result.defaults
                if (kind==="save" && req.part !== -1) { panel.saved=panel.copy(result.settings); panel.adopt(result.settings, req.part); panel.revision=result.revision }
                else if (kind!=="load" || !panel.dirty) { panel.saved=panel.copy(result.settings); panel.draft=panel.copy(result.settings); panel.revision=result.revision }
                if (kind!=="load")panel.conflict=false
                if (kind==="save") {
                    panel.status="Saved"
                    if (req.part >= 0) { const at=Object.assign({},panel.savedAt); at[req.part]=Date.now(); panel.savedAt=at }
                    panel.saveCompleted()
                }
                if (panel.visible && kind!=="save") panel.refresh()
            } else if (kind==="discover") { panel.models=result.models || []; panel.discovered=true; panel.status=(panel.provider === "ollama" ? "Ollama "+(result.version || "") : "Local server")+" · "+panel.models.length+(panel.models.length === 1 ? " model" : " models") }
        }
    }
    component Caption: Text { color: Theme.inkAlpha(0.45); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(11.5) }
    component Body: Text {
        Layout.fillWidth: true; Layout.maximumWidth: 640; wrapMode: Text.Wrap; textFormat: Text.PlainText
        color: Theme.muted; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(13.5)
        lineHeightMode: Text.FixedHeight; lineHeight: Math.round(Theme.fontSize(13.5) * 1.5)
    }
    // A prompt part with Restore default on the left and Cancel / Save on the right.
    component PromptBox: ColumnLayout {
        id: box
        property int index: 0
        property int minimum: 200
        property string title: ""
        property string description: ""
        default property alias extra: extraSlot.data
        Layout.fillWidth: true; spacing: 0
        readonly property bool dirty: panel.boxDirty(index)
        readonly property bool isDefault: panel.textOf(panel.draft, index) === panel.textOf(panel.defaults, index)
        Caption { text: box.title; Layout.topMargin: 32 }
        Body { text: box.description; Layout.topMargin: 6 }
        ColumnLayout { id: extraSlot; Layout.fillWidth: true; spacing: 0 }
        TextArea {
            objectName: "mailPrompt" + box.index
            Layout.fillWidth: true; Layout.topMargin: 12; Layout.preferredHeight: Math.max(box.minimum, contentHeight + 24)
            text: panel.textOf(panel.draft, box.index)
            onTextChanged: if (activeFocus) panel.editMessage(box.index, text)
            wrapMode: TextEdit.Wrap; textFormat: TextEdit.PlainText; selectByMouse: true; color: Theme.inkAlpha(0.85)
            font.family: backend.fixedFontFamily(); font.pointSize: Theme.fontPoints(12.5); padding: 12; leftPadding: 14; rightPadding: 14
            background: Rectangle { color: Theme.bg2; radius: 6; border.color: parent.activeFocus ? Theme.focusBorder : Theme.line2 }
            Accessible.name: box.title
        }
        RowLayout {
            Layout.fillWidth: true; Layout.topMargin: 10; spacing: 8
            AbstractButton {
                id: restore
                objectName: "restorePrompt" + box.index
                enabled: !box.isDefault && panel.textOf(panel.defaults, box.index).length > 0
                hoverEnabled: true
                onClicked: panel.restoreBox(box.index)
                contentItem: Text { text: "Restore default"; color: !restore.enabled ? Theme.inkAlpha(.3) : restore.hovered ? Theme.text : Theme.amber; font { family: Theme.fontFamily; pointSize: Theme.fontPoints(12) } }
                background: null
                FmToolTip { shown: restore.hovered && restore.enabled; text: "Load the built-in text — you can still cancel" }
            }
            Item { Layout.fillWidth: true }
            // Reserved so the buttons do not move when it appears.
            Text {
                text: "Unsaved changes"; opacity: box.dirty ? 1 : 0
                color: Theme.amber; font { family: Theme.fontFamily; pointSize: Theme.fontPoints(12) }
            }
            FmButton { objectName: "cancelPrompt" + box.index; text: "Cancel"; outlined: true; implicitHeight: 30; enabled: box.dirty && !panel.saving; opacity: enabled ? 1 : .45; onClicked: panel.cancelBox(box.index) }
            FmButton {
                objectName: "savePrompt" + box.index
                text: panel.savedAt[box.index] ? "Saved" : panel.saving && panel.savingPart === box.index ? "Saving…" : "Save"
                prominent: true; implicitHeight: 30; enabled: box.dirty && !panel.saving; opacity: enabled || !!panel.savedAt[box.index] ? 1 : .45
                onClicked: panel.saveBox(box.index)
            }
        }
    }
    component VariableChip: RowLayout {
        id: variable
        property string name: ""
        property string meaning: ""
        spacing: 14
        Rectangle {
            implicitWidth: chipText.implicitWidth + 12; implicitHeight: chipText.implicitHeight + 2; radius: 3; color: Theme.withAlpha(Theme.ai, .1)
            Text { id: chipText; anchors.centerIn: parent; text: variable.name; color: Theme.ai; font.family: backend.fixedFontFamily(); font.pointSize: Theme.fontPoints(12) }
        }
        Text { text: variable.meaning; color: Theme.inkAlpha(.6); font { family: Theme.fontFamily; pointSize: Theme.fontPoints(13) } Layout.fillWidth: true; elide: Text.ElideRight }
    }

    Caption { text: "Provider"; Layout.topMargin: 20 }
    FmComboBox { objectName:"localModelProvider"; Layout.topMargin: 10; Layout.fillWidth:true; Layout.maximumWidth:360; textRole:"name"; model:panel.providers; currentIndex:panel.providers.findIndex(p=>p.id===panel.provider); onActivated:panel.changeProvider(panel.providers[currentIndex].id); Accessible.name:"Local model provider" }
    RowLayout {
        Layout.fillWidth: true; Layout.topMargin: 10; spacing: 10
        FmTextField { objectName:"ollamaEndpoint"; Layout.fillWidth:true; Layout.maximumWidth:360; implicitHeight: 34; text:panel.draft.endpoint || ""; placeholderText:panel.provider === "ollama" ? "http://localhost:11434" : "http://localhost:1234"; onTextEdited:panel.editEndpoint(text); onAccepted: panel.refresh(); Accessible.name:"Local model endpoint" }
        FmButton { objectName:"ollamaRefresh"; text:"Detect"; outlined:true; implicitHeight:34; leftPadding:14; rightPadding:14; onClicked:panel.refresh() }
        Item { Layout.fillWidth:true }
    }
    RowLayout {
        Layout.fillWidth: true; Layout.topMargin: 12; spacing: 10
        Rectangle {
            implicitWidth:8; implicitHeight:8; radius:4; color: panel.discovering ? Theme.ai : panel.models.length ? Theme.withAlpha(Qt.color("#7FC4A4"), 1) : panel.discovered ? Theme.error : Theme.subtle
            SequentialAnimation on opacity { running: panel.discovering; loops: Animation.Infinite; NumberAnimation { to: .35; duration: 500 } NumberAnimation { to: 1; duration: 500 } }
        }
        Text { text:panel.status.length ? panel.status : "Choose a local endpoint, then Detect."; color:Theme.secondary; wrapMode:Text.Wrap; Layout.fillWidth:true; font.family:Theme.fontFamily; font.pointSize:Theme.fontPoints(13) }
    }
    Caption { text: "Model"; Layout.topMargin: 22 }
    RowLayout {
        Layout.fillWidth: true; Layout.topMargin: 10; spacing: 10
        FmComboBox {
            objectName:"mailModel"; Layout.fillWidth:true; Layout.maximumWidth:480; textRole:"name"; model:panel.choices; currentIndex:panel.indexFor()
            // Model names like hf.co/org/name:Q4_K_M differ at both ends.
            textElide: Text.ElideMiddle; popupWidth: Math.min(640, panel.width)
            onActivated: function(index) { const m=panel.choices[index]; if(m) panel.chooseModel(index===0 ? null : {name:m.name,digest:m.digest}); currentIndex=Qt.binding(function(){return panel.indexFor()}) }
            Accessible.name:"Mail model"
        }
        Text { text: panel.modelNote; visible: text.length > 0; color: Theme.inkAlpha(.45); font { family: Theme.fontFamily; pointSize: Theme.fontPoints(12) } }
        Item { Layout.fillWidth: true }
        FmButton {
            id: testButton
            objectName: "testLocalModel"
            readonly property bool running: !!panel.test && panel.test.state === "run"
            text: running ? "Testing…" : "Test"; iconName: "summary"; iconSize: 12; iconColor: Theme.ai; foreground: Theme.ai
            implicitHeight: 34; leftPadding: 14; rightPadding: 14
            enabled: backend.connected && !!panel.draft.model && !running
            tip: "Summarize a sample email with this model and prompt"
            background: Rectangle { radius: 5; color: testButton.hovered && testButton.enabled ? Theme.withAlpha(Theme.ai, .08) : "transparent"; border.color: Theme.withAlpha(Theme.ai, .4) }
            onClicked: panel.runTest()
        }
    }
    Text { objectName:"modelUnavailable"; Layout.topMargin: 8; visible:panel.discovered && !panel.discovering && !!panel.draft.model && !panel.selectedAvailable; text:"The server no longer lists these model weights. Choose an available model."; color:Theme.secondary; wrapMode:Text.Wrap; Layout.fillWidth:true; font.family:Theme.fontFamily; font.pointSize:Theme.fontPoints(12) }
    // Sample test result: Running, Passed, Failed (not valid JSON) or Error.
    Rectangle {
        id: testPanel
        objectName: "localModelTest"
        visible: !!panel.test
        readonly property var t: panel.test || ({})
        readonly property var look: ({run: ["Running test…", Theme.text, Theme.ai], ok: ["Passed", "#7FC4A4", "#7FC4A4"], fail: ["Failed", Theme.amber, Theme.amber], err: ["Error", Theme.error, Theme.error]})[t.state || "run"]
        readonly property real elapsed: { const _ = panel.clock; return t.state === "run" ? Date.now() - t.at : t.ms || 0 }
        function sec(ms) { return (ms / 1000).toFixed(ms < 10000 ? 2 : 1) + " s" }
        readonly property var stats: {
            const m = t.metrics || {}
            if (t.state === "run") return [{k: "Elapsed", v: sec(elapsed), tip: "Time since the request was sent"}]
            if (t.state === "err") return [{k: "After", v: sec(elapsed), tip: ""}]
            const out = []
            if (m.first_token_ms !== undefined && m.first_token_ms !== null) out.push({k: "First token", v: sec(m.first_token_ms), tip: "Time until the model started answering"})
            out.push({k: "Total", v: sec(elapsed), tip: "Full response time"})
            if (m.tokens_per_second) out.push({k: "Speed", v: m.tokens_per_second + " tok/s", tip: "Output tokens per second"})
            return out
        }
        Layout.fillWidth: true; Layout.topMargin: 12
        implicitHeight: testContent.implicitHeight
        radius: 6; clip: true
        color: Theme.withAlpha(Qt.color(look[2]), .05); border.color: Theme.withAlpha(Qt.color(look[2]), .4)
        ColumnLayout {
            id: testContent
            width: parent.width; spacing: 0
            RowLayout {
                Layout.fillWidth: true; Layout.margins: 14; Layout.topMargin: 10; Layout.bottomMargin: 10; spacing: 10
                Rectangle {
                    implicitWidth: 8; implicitHeight: 8; radius: 4; color: testPanel.look[2]
                    SequentialAnimation on opacity { running: testPanel.t.state === "run"; loops: Animation.Infinite; NumberAnimation { to: .35; duration: 500 } NumberAnimation { to: 1; duration: 500 } }
                }
                Text { objectName: "localModelTestTitle"; text: testPanel.look[0]; color: testPanel.look[1]; font { family: Theme.fontFamily; pointSize: Theme.fontPoints(13); weight: Font.Medium } }
                Text {
                    Layout.fillWidth: true; elide: Text.ElideRight; color: Theme.inkAlpha(.5); font { family: Theme.fontFamily; pointSize: Theme.fontPoints(12.5) }
                    text: testPanel.t.state === "run" ? testPanel.t.model + " · sample email"
                        : testPanel.t.state === "ok" ? "Valid JSON · summary, label and sender returned"
                        : testPanel.t.state === "fail" ? "Response was not valid JSON — check the system prompt’s return shape"
                        : (testPanel.t.message || "No response") + " · is " + (panel.draft.endpoint || "the server") + " still running?"
                }
                Repeater {
                    model: testPanel.stats
                    Text {
                        required property var modelData
                        textFormat: Text.StyledText
                        text: "<font color='" + Theme.inkAlpha(.4) + "'>" + modelData.k + "</font> " + modelData.v
                        color: Theme.inkAlpha(.85); font { family: Theme.fontFamily; pointSize: Theme.fontPoints(12); features: {"tnum": 1} }
                        HoverHandler { id: statHover }
                        FmToolTip { shown: statHover.hovered && parent.modelData.tip.length > 0; text: parent.modelData.tip }
                    }
                }
            }
            Rectangle { visible: testOut.visible; Layout.fillWidth: true; implicitHeight: 1; color: Theme.line }
            TextEdit {
                id: testOut
                objectName: "localModelTestOutput"
                visible: testPanel.t.state !== "run" && testPanel.t.state !== "err" && (testPanel.t.out || "").length > 0
                Layout.fillWidth: true; Layout.margins: 14; Layout.topMargin: 10; Layout.bottomMargin: 12
                text: testPanel.t.out || ""; readOnly: true; selectByMouse: true; wrapMode: TextEdit.Wrap; textFormat: TextEdit.PlainText
                color: Theme.inkAlpha(.8); font.family: backend.fixedFontFamily(); font.pointSize: Theme.fontPoints(12)
            }
        }
    }
    PromptBox {
        index: 0; minimum: 200; title: "System prompt"
        description: "How the model behaves, and the JSON shape the app reads."
    }
    PromptBox {
        index: 1; minimum: 130; title: "User message"
        description: "Sent with each message. Add your own context around the variables."
        GridLayout {
            Layout.topMargin: 10; columns: 1; rowSpacing: 6
            VariableChip { name: "{{messages}}"; meaning: "The message or thread being summarized" }
            VariableChip { name: "{{labels}}"; meaning: "This account’s labels and their descriptions" }
        }
    }
    Text {
        objectName: "promptMissingVariables"
        visible: panel.revision >= 0 && panel.missingVariables.length > 0
        Layout.fillWidth: true; Layout.topMargin: 10; wrapMode: Text.Wrap
        text: "User message is missing " + panel.missingVariables.join(" and ") + " — the model won’t see "
            + (panel.missingVariables.length > 1 ? "the mail or your labels" : panel.missingVariables[0] === "{{messages}}" ? "the mail" : "your labels") + "."
        color: Theme.amber; font { family: Theme.fontFamily; pointSize: Theme.fontPoints(12.5) }
    }
    Text {
        objectName: "promptSize"
        Layout.fillWidth: true; Layout.topMargin: 10
        text: panel.promptText.length.toLocaleString(Qt.locale(), "f", 0) + " characters · about " + Math.round(panel.promptText.length / 4).toLocaleString(Qt.locale(), "f", 0) + " tokens before your mail is added"
        color: Theme.inkAlpha(.45); font { family: Theme.fontFamily; pointSize: Theme.fontPoints(12) }
    }
    RowLayout {
        visible: panel.conflict; Layout.topMargin: 10; spacing: 8
        Text { text: "These settings changed elsewhere."; color: Theme.secondary; font { family: Theme.fontFamily; pointSize: Theme.fontPoints(12.5) } }
        FmButton { objectName:"reloadModelSettings"; text:"Reload saved settings"; outlined: true; onClicked:panel.reloadSaved() }
    }
    Text { visible:panel.models.some(m=>!m.eligible); Layout.topMargin: 10; Layout.fillWidth:true; text:panel.models.filter(m=>!m.eligible).map(m=>m.name+": "+m.reason).join("\n"); wrapMode:Text.Wrap; color:Theme.subtle; font.family:Theme.fontFamily; font.pointSize:Theme.fontPoints(11) }
}
