import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

ColumnLayout {
    id: panel
    property string account: backend.account
    readonly property string accountName: (backend.accounts.find(a => a.id === account) || {}).name || "this account"
    property var saved: ({})
    property int revision: -1
    property string ticket: ""
    property bool saving: false
    property string status: ""
    readonly property var policy: (saved.accounts || {})[account] || {summaries:true, classification:true}
    spacing: 0
    function load() {
        if (visible && backend.connected && !saving) ticket = backend.assistanceRequest("assist_settings_get",{},"")
    }
    function change(key,value) {
        if (saving || revision < 0 || !account) return
        const settings = JSON.parse(JSON.stringify(saved)), choice = Object.assign({},policy)
        choice[key] = value
        settings.accounts = Object.assign({},settings.accounts || {})
        settings.accounts[account] = choice
        saving = true; status = ""
        ticket = backend.assistanceRequest("assist_settings_save",{expected_revision:revision,settings:settings},"")
    }
    onAccountChanged: load()
    onVisibleChanged: load()
    Component.onCompleted: load()
    Connections {
        target: backend
        function onConnectedChanged() { panel.load() }
        function onAssistanceReply(reply,method,result,ok) {
            if (reply !== panel.ticket) return
            panel.ticket = ""; panel.saving = false
            if (!ok) { panel.status = result.message || "Could not save this choice. Try again."; panel.revision=-1; return }
            panel.saved = result.settings; panel.revision = result.revision
        }
    }
    RowLayout {
        Layout.fillWidth:true; Layout.topMargin:20; spacing:6
        FmIcon {name:"summary";size:10;color:Theme.ai}
        Text {text:"On new mail in "+panel.accountName;color:Theme.ai;font.pointSize:Theme.fontPoints(11.5);Layout.fillWidth:true}
    }
    FmCheckBox {
        objectName:"enableSummaries";Layout.fillWidth:true;text:"Auto-summarize"
        description:"Write a one-line summary for the list and a short summary for each message as it arrives. Off: summarize by hand from the message."
        checked:panel.policy.summaries !== false; enabled:backend.connected && panel.revision >= 0 && !!panel.account && !panel.saving
        onClicked: {panel.change("summaries",checked);checked=Qt.binding(function(){return panel.policy.summaries !== false})}
    }
    FmCheckBox {
        objectName:"enableClassification";Layout.fillWidth:true;text:"Auto-label"
        description:"Pick one label from your list for each new conversation and flag automated senders. Off: mail arrives unlabelled."
        checked:panel.policy.classification !== false; enabled:backend.connected && panel.revision >= 0 && !!panel.account && !panel.saving
        onClicked: {panel.change("classification",checked);checked=Qt.binding(function(){return panel.policy.classification !== false})}
    }
    RowLayout {
        visible:panel.status.length>0;Layout.fillWidth:true
        Text {text:panel.status;color:Theme.error;Layout.fillWidth:true;wrapMode:Text.Wrap;font.pointSize:Theme.fontPoints(12)}
        FmButton {text:"Retry";onClicked:{panel.status="";panel.load()}}
    }
}
