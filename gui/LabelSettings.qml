import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

ColumnLayout {
    id: panel
    spacing: 0
    property string account: backend.account
    property bool showAccountPicker: true
    // The Accounts labels modal supplies its own title and explanation.
    property bool showHeading: true
    property var labels: []
    property var requests: ({})
    property var edits: ({})
    property var newNames: ({})
    property var syncJobs: ({})
    property var pendingRetry: null
    property string status: ""
    property string loadTicket: ""
    readonly property bool busy: !!syncJobs[account] || Object.values(requests).some(r=>r.account===account&&r.method!=="label_catalog")
    property var removal: ({})
    readonly property string accountName: (backend.accounts.find(a=>a.id===account)||{}).name || "this account"
    function key(id) { return account + "/" + id }
    function send(method, params, scope) {
        const target=scope || account, values=Object.assign({},params)
        const ticket=backend.labelRequest(method,values,target)
        if(method!=="label_catalog"&&!method.endsWith("_status")&&!method.endsWith("_cancel")) {
            values.request_id=values.request_id || ticket
            if(method==="label_create") values.id=values.id || "label-"+values.request_id
        }
        const next=Object.assign({},requests);next[ticket]={method:method,params:values,account:target};requests=next
        if(method==="label_catalog")loadTicket=ticket
        else if(!method.endsWith("_status")&&!method.endsWith("_cancel"))status=method==="google_labels_sync" ? "Syncing with Google…" : "Saving…"
        return ticket
    }
    function load() { if(visible&&backend.connected&&account)send("label_catalog",{},account) }
    function edit(id,text) {const next=Object.assign({},edits);next[key(id)]=text;edits=next}
    function describe(row) {
        const value=edits[key(row.id)]
        if(busy||value===undefined||value===row.description)return
        send("label_describe",{id:row.id,expected_description:row.description||"",description:value})
    }
    function add() {const name=newLabel.text.trim();if(name&&!busy)send("label_create",{name:name})}
    onAccountChanged: {labels=[];status=syncJobs[account]?"Syncing with Google…":"";pendingRetry=null;load()}
    onVisibleChanged: load()
    Component.onCompleted: load()
    Connections {
        target: backend
        function onConnectedChanged(){if(backend.connected)panel.load();else {
            panel.pendingRetry=Object.values(panel.requests).find(r=>r.account===panel.account&&["label_create","label_describe","label_remove","label_auto_read","google_labels_sync"].indexOf(r.method)>=0)||null
            panel.requests={};panel.syncJobs={};panel.loadTicket="";panel.status="Connection lost. Your edits are kept."
        }}
        function onLabelReply(ticket,method,result,ok) {
            const req=panel.requests[ticket];if(!req)return
            const next=Object.assign({},panel.requests);delete next[ticket];panel.requests=next
            if(method.startsWith("google_labels_sync")) {
                const jobs=Object.assign({},panel.syncJobs)
                if(ok&&result.state==="running")jobs[req.account]=result.job_id
                else delete jobs[req.account]
                panel.syncJobs=jobs
                if(req.account!==panel.account)return
                if(ok&&result.state==="running")return
                if(ok&&result.state==="complete") {panel.status=result.result.message||"Labels are in sync.";panel.pendingRetry=null;panel.load();return}
                panel.status=ok ? (result.state==="cancelled"?"Sync stopped. Sync again to check Google.":(result.error||{}).message||"Label sync stopped.") : result.message||"Could not check label sync."
                panel.pendingRetry={method:"google_labels_sync",account:req.account,params:(!ok&&method==="google_labels_sync")?req.params:{}}
                panel.load();return
            }
            if(req.account!==panel.account)return
            if(method==="label_catalog"&&ticket!==panel.loadTicket)return
            if(!ok){panel.status=result.message||"The label change could not be saved.";if(method!=="label_catalog")panel.pendingRetry=req;return}
            if(method==="label_catalog"){panel.labels=result.categories||[];return}
            panel.pendingRetry=null
            if(method==="label_describe") {
                const k=panel.key(req.params.id)
                if(panel.edits[k]===req.params.description){const draft=Object.assign({},panel.edits);delete draft[k];panel.edits=draft}
            }
            if(method==="label_create"&&newLabel.text.trim()===req.params.name){const names=Object.assign({},panel.newNames);delete names[panel.account];panel.newNames=names}
            panel.status=method==="google_labels_sync" ? result.message||"Labels are in sync." : "Saved"
            panel.load()
        }
    }
    Timer {
        interval:500;repeat:true;running:backend.connected&&Object.keys(panel.syncJobs).length>0
        onTriggered: {
            for(const scope of Object.keys(panel.syncJobs)) {
                if(!Object.values(panel.requests).some(r=>r.account===scope&&r.method.startsWith("google_labels_sync")))panel.send("google_labels_sync_status",{job_id:panel.syncJobs[scope]},scope)
            }
        }
    }
    RowLayout {
        visible: panel.showHeading
        Layout.fillWidth:true; Layout.topMargin:28; spacing:16
        Text{text:"Labels";color:Theme.inkAlpha(0.45);font.family:Theme.fontFamily;font.pointSize:Theme.fontPoints(11.5);Layout.fillWidth:true}
        FmComboBox {visible:panel.showAccountPicker;objectName:"labelsAccount";Layout.preferredWidth:160;model:backend.accounts;textRole:"name";currentIndex:backend.accounts.findIndex(a=>a.id===panel.account);onActivated:panel.account=backend.accounts[currentIndex].id;Accessible.name:"Labels account"}
    }
    Text {visible:panel.showHeading;Layout.fillWidth:true;Layout.topMargin:6;Layout.bottomMargin:14;text:"Each account keeps its own labels. Descriptions tell the local model what belongs where. Removing a label takes it off every conversation in "+panel.accountName+" here.";wrapMode:Text.Wrap;lineHeightMode: Text.FixedHeight; lineHeight: Math.round(Theme.fontPixels(font) * 1.5); topPadding: Theme.leadingTop(font, lineHeight); bottomPadding: Theme.leadingBottom(font, lineHeight);color:Theme.muted;font.family:Theme.fontFamily;font.pointSize:Theme.fontPoints(13.5)}
    Rectangle{Layout.fillWidth:true;implicitHeight:1;color:Theme.line}
    RowLayout { Layout.fillWidth:true; Layout.minimumHeight:30; spacing:12
        Text { text:"LABEL"; Layout.preferredWidth:150; color:Theme.subtle; font.family:Theme.fontFamily; font.pointSize:Theme.fontPoints(11); font.letterSpacing:0.66 }
        Text { text:"DESCRIPTION"; Layout.fillWidth:true; color:Theme.subtle; font.family:Theme.fontFamily; font.pointSize:Theme.fontPoints(11); font.letterSpacing:0.66 }
        Row { Layout.preferredWidth:84; spacing:5
            FmIcon { name:"summary"; size:10; color:Theme.ai; anchors.verticalCenter:parent.verticalCenter }
            Text { text:"AUTO-READ"; color:Theme.ai; font.family:Theme.fontFamily; font.pointSize:Theme.fontPoints(11); font.letterSpacing:0.66 }
        }
        Item { Layout.preferredWidth:28 }
    }
    Rectangle{Layout.fillWidth:true;implicitHeight:1;color:Theme.line}
    Repeater {
        model:panel.labels
        ColumnLayout {
            required property var modelData
            Layout.fillWidth:true;spacing:0
            RowLayout {
                Layout.fillWidth:true;Layout.minimumHeight:44;spacing:12
                RowLayout {
                    Layout.preferredWidth:150;Layout.minimumWidth:150;Layout.maximumWidth:150;spacing:8
                    Text{text:modelData.name;Layout.maximumWidth:150-countText.implicitWidth-8;elide:Text.ElideRight;color:Theme.text;font.family:Theme.fontFamily;font.pointSize:Theme.fontPoints(13.5)}
                    Text{id:countText;text:modelData.conversation_count;color:Theme.inkAlpha(0.4);font.family:Theme.fontFamily;font.pointSize:Theme.fontPoints(11.5)}
                    Item{Layout.fillWidth:true}
                }
                TextField {
                    objectName:"labelDescription_"+modelData.id;Layout.fillWidth:true;implicitHeight:30;padding:0;maximumLength:512
                    text:panel.edits[panel.key(modelData.id)]===undefined ? modelData.description||"" : panel.edits[panel.key(modelData.id)]
                    placeholderText:"What belongs here — optional, read by the model";color:Theme.inkAlpha(0.75);placeholderTextColor:Theme.subtle;font.family:Theme.fontFamily;font.pointSize:Theme.fontPoints(13);selectByMouse:true
                    background:Rectangle{color:"transparent";Rectangle{anchors.bottom:parent.bottom;width:parent.width;height:1;color:parent.parent.activeFocus?Theme.focusBorder:"transparent"}}
                    onTextEdited:panel.edit(modelData.id,text)
                    onEditingFinished:panel.describe(modelData)
                    Accessible.name:"Instructions for "+modelData.name
                }
                CheckBox {
                    id:autoRead; objectName:"labelAutoRead_"+modelData.id; Layout.preferredWidth:84; implicitHeight:28
                    checked:modelData.auto_read===true; enabled:!panel.busy&&backend.connected
                    onClicked: panel.send("label_auto_read",{id:modelData.id,expected_auto_read:modelData.auto_read===true,auto_read:checked})
                    Accessible.name:"Auto-read "+modelData.name
                    ToolTip.visible:hovered; ToolTip.text:checked ? "Incoming mail assigned this label is marked read" : "Incoming mail assigned this label stays unread"
                    indicator: Rectangle { width:16; height:16; radius:4; x:0; y:(parent.height-height)/2; color:autoRead.checked ? Theme.ai : "transparent"; border.color:autoRead.checked ? Theme.ai : Theme.line3
                        FmIcon { anchors.centerIn:parent; name:"check"; size:11; color:Theme.canvas; visible:autoRead.checked }
                    }
                    contentItem:Item {}
                }
                FmButton {objectName:"removeLabel_"+modelData.id;iconName:"close";iconSize:9;foreground:hovered?Theme.text:Theme.inkAlpha(0.5);implicitWidth:28;implicitHeight:28;leftPadding:0;rightPadding:0;enabled:!panel.busy&&backend.connected;Accessible.name:"Remove "+modelData.name;onClicked:{panel.removal={account:panel.account,id:modelData.id,name:modelData.name,provider:modelData.provider_label_id||null};removeDialog.open()}}
            }
            Rectangle{Layout.fillWidth:true;implicitHeight:1;color:Theme.line}
        }
    }
    RowLayout {
        Layout.fillWidth:true;Layout.minimumHeight:44;spacing:12
        TextField {
            id:newLabel;objectName:"newLabelName";Layout.preferredWidth:150;implicitHeight:30;padding:0;maximumLength:256
            text:panel.newNames[panel.account]||""
            onTextEdited:{const names=Object.assign({},panel.newNames);names[panel.account]=text;panel.newNames=names}
            placeholderText:"New label";color:Theme.text;placeholderTextColor:Theme.subtle;font.family:Theme.fontFamily;font.pointSize:Theme.fontPoints(13.5)
            selectByMouse:true;enabled:backend.connected&&!panel.busy;onAccepted:panel.add()
            background:Rectangle{color:"transparent"}
            Accessible.name:"New label name"
        }
        Text {text:"Enter adds it to "+panel.accountName;Layout.fillWidth:true;wrapMode:Text.Wrap;color:Theme.inkAlpha(0.35);font.family:Theme.fontFamily;font.pointSize:Theme.fontPoints(12)}
    }
    Rectangle{Layout.fillWidth:true;implicitHeight:1;color:Theme.line}
    RowLayout {
        Layout.fillWidth:true;Layout.topMargin:14;spacing:8
        FmButton {objectName:"syncAccountLabels";text:"Sync with Google";outlined:true;implicitHeight:30;font.pointSize:Theme.fontPoints(12.5);enabled:backend.connected&&!panel.busy&&backend.accountCapability(panel.account,"mail_write");onClicked:panel.send("google_labels_sync",{})}
        Text {text:panel.status||"Google keeps the names; descriptions stay on this machine.";Layout.fillWidth:true;wrapMode:Text.Wrap;color:Theme.inkAlpha(0.45);font.family:Theme.fontFamily;font.pointSize:Theme.fontPoints(12)}
        FmButton {objectName:"retryLabelChange";text:"Retry";visible:!!panel.pendingRetry;enabled:!panel.busy&&backend.connected;onClicked:panel.send(panel.pendingRetry.method,panel.pendingRetry.params,panel.pendingRetry.account)}
        FmButton {text:"Stop";visible:!!panel.syncJobs[panel.account];enabled:backend.connected;onClicked:panel.send("google_labels_sync_cancel",{job_id:panel.syncJobs[panel.account]})}
    }
    FmConfirmDialog {
        id:removeDialog;objectName:"removeLabelConfirmation";title:"Remove "+(panel.removal.name||"label")+"?"
        actionObjectName:"confirmRemoveLabel"
        message:"This removes the label from every conversation in this account inside fuckMail. Your emails and the label in Gmail stay unchanged.";actionText:"Remove label";destructive:true
        onAccepted:panel.send("label_remove",{id:panel.removal.id,expected_name:panel.removal.name,expected_provider_label_id:panel.removal.provider},panel.removal.account)
    }
}
