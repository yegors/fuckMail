// Settings > Accounts "Edit account" dialog: display name, From name, local mail retention
// and which Google services sync (Mail, Calendar; read or read & write). Opened
// with show(row); emits reconnectRequested when the new access needs a fresh sign-in.
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

FmDialog {
    id: editor; objectName:"accountEditor"; title:"Edit account"
    width:Math.min(460,parent.width-48); height:Math.min(implicitHeight,parent.height-48)
    spacing:22;cornerRadius:10
    header:Column {
        topPadding:22;leftPadding:22;rightPadding:22;spacing:3
        Text {text:editor.title;color:Theme.text;font.pointSize:Theme.fontPoints(15);font.weight:Font.Medium}
        Text {text:editor.account.email || "";color:Theme.muted;font.pointSize:Theme.fontPoints(12)}
    }
    property var account: ({})
    property bool saving:false
    property string error:""
    readonly property var keys:["90_days","1_year","2_years","5_years","all"]
    readonly property var labels:["90 days","1 year","2 years","5 years","Everything"]
    readonly property var sync:({mail:mailEnabled.checked,calendar:calendarEnabled.checked,mail_access:mailMode.currentIndex ? "write" : "read",calendar_access:calendarMode.currentIndex ? "write" : "read"})
    readonly property var requestedAccess:({mail:mailEnabled.checked ? sync.mail_access : "none",calendar:calendarEnabled.checked ? sync.calendar_access : "none"})
    readonly property bool shortening:({"1_month":30,"3_months":90,"90_days":90,"1_year":365,"2_years":730,"5_years":1825,"all":Infinity})[keys[retention.currentIndex]] < ({"1_month":30,"3_months":90,"90_days":90,"1_year":365,"2_years":730,"5_years":1825,"all":Infinity})[account.mail_retention || "all"]
    signal reconnectRequested(var account,var access)
    function show(row) {
        account=row;saving=false;error="";displayName.text=row.name;fromName.text=row.from_name || ""
        retention.currentIndex=Math.max(0,keys.indexOf(row.mail_retention || "all"))
        const choice=(backend.preferences.google_sync_services || {})[row.id] || {}, caps=(row.google || {}).granted_capabilities || (row.google || {}).capabilities || {}
        mailEnabled.checked=choice.mail !== false;calendarEnabled.checked=choice.calendar !== false
        mailMode.currentIndex=choice.mail_access ? choice.mail_access === "write" ? 1 : 0 : caps.mail_write || backend.syntheticProfile ? 1 : 0
        calendarMode.currentIndex=choice.calendar_access ? choice.calendar_access === "write" ? 1 : 0 : caps.calendar_write || backend.syntheticProfile ? 1 : 0
        open();displayName.forceActiveFocus();displayName.selectAll()
    }
    function save() {
        if(saving || !displayName.text.trim() || (!mailEnabled.checked && !calendarEnabled.checked))return
        saving=true;error=""
        backend.updateAccountPreferences(account.id,displayName.text.trim(),account.signature_text || "",keys[retention.currentIndex],sync,false,"",fromName.text.trim())
    }
    Connections {
        target:backend
        function onAccountPreferencesFinished(account,ok) {
            if(!editor.visible || !editor.saving || account!==editor.account.id)return
            editor.saving=false
            if(!ok) {editor.error=backend.preferencesStatus;return}
            const row=editor.account, access=editor.requestedAccess, caps=(row.google || {}).granted_capabilities || (row.google || {}).capabilities || {}
            const reconnect=!backend.syntheticProfile && ((row.google || {}).connection_state!=="connected" || (access.mail!=="none" && !caps[access.mail==="write" ? "mail_write" : "mail_read"]) || (access.calendar!=="none" && !caps[access.calendar==="write" ? "calendar_write" : "calendar_read"]))
            editor.close()
            if(reconnect)editor.reconnectRequested(row,access)
            else if(!backend.syntheticProfile)backend.refreshGoogleSync(account)
        }
    }
    contentItem:ScrollView {
        implicitHeight:fields.implicitHeight;contentWidth:availableWidth;clip:true
        ScrollBar.vertical:FmScrollBar {}
        property bool blocksWorkspaceShortcuts:true
        ColumnLayout {
            id:fields;width:editor.availableWidth;spacing:7
            Text {text:"Display name";color:Theme.muted;font.pointSize:Theme.fontPoints(11)}
            FmTextField {id:displayName;objectName:"editAccountName";Layout.fillWidth:true;implicitHeight:34;padding:7;font.pointSize:Theme.fontPoints(12.5);maximumLength:80;enabled:!editor.saving;Accessible.name:"Display name"
                background:Rectangle {radius:6;color:Theme.canvas;border.color:displayName.activeFocus ? Theme.line3 : Theme.line2}
            }
            Text {text:"Shown in fuckMail only.";color:Theme.inkAlpha(0.45);font.pointSize:Theme.fontPoints(11.5);Layout.fillWidth:true;wrapMode:Text.Wrap}
            Text {text:"From name";color:Theme.muted;font.pointSize:Theme.fontPoints(11);Layout.topMargin:15}
            FmTextField {id:fromName;objectName:"editAccountFromName";Layout.fillWidth:true;implicitHeight:34;padding:7;font.pointSize:Theme.fontPoints(12.5);maximumLength:80;enabled:!editor.saving;Accessible.name:"From name"
                placeholderText:editor.account.default_from_name || "No name, only the address"
                background:Rectangle {radius:6;color:Theme.canvas;border.color:fromName.activeFocus ? Theme.line3 : Theme.line2}
            }
            Text {text:"What people see when you write to them. Leave it empty to use your Gmail name. Drafts you already started keep the name they had.";color:Theme.inkAlpha(0.45);font.pointSize:Theme.fontPoints(11.5);Layout.fillWidth:true;wrapMode:Text.Wrap;lineHeightMode: Text.FixedHeight; lineHeight: Math.round(Theme.fontPixels(font) * 1.5); topPadding: Theme.leadingTop(font, lineHeight); bottomPadding: Theme.leadingBottom(font, lineHeight)}
            Text {text:"Local retention";color:Theme.muted;font.pointSize:Theme.fontPoints(11);Layout.topMargin:15}
            FmSegmented {id:retention;objectName:"editAccountRetention";compact:true;model:editor.labels;enabled:!editor.saving;Accessible.name:"Local retention"}
            Text {text:"How much mail to keep on this machine. Older mail stays in Google and can be downloaded again by extending this period.";color:Theme.inkAlpha(0.45);font.pointSize:Theme.fontPoints(11.5);Layout.fillWidth:true;wrapMode:Text.Wrap;lineHeightMode: Text.FixedHeight; lineHeight: Math.round(Theme.fontPixels(font) * 1.5); topPadding: Theme.leadingTop(font, lineHeight); bottomPadding: Theme.leadingBottom(font, lineHeight)}
            Text {visible:editor.shortening;text:"Mail older than "+editor.labels[retention.currentIndex]+" will be removed from this machine on save. Nothing is deleted from Google.";color:Theme.accent;Layout.fillWidth:true;wrapMode:Text.Wrap;font.pointSize:Theme.fontPoints(12);lineHeightMode: Text.FixedHeight; lineHeight: Math.round(Theme.fontPixels(font) * 1.45); topPadding: Theme.leadingTop(font, lineHeight); bottomPadding: Theme.leadingBottom(font, lineHeight)}
            Text {text:"Sync";color:Theme.muted;font.pointSize:Theme.fontPoints(11);Layout.topMargin:15}
            RowLayout {Layout.fillWidth:true;spacing:12
                FmCheckBox {id:mailEnabled;objectName:"editAccountMail";text:"Mail";Layout.fillWidth:true;topPadding:4;bottomPadding:4;implicitHeight:28;background:null;enabled:!editor.saving && (calendarEnabled.checked || !checked)}
                FmSegmented {id:mailMode;objectName:"editAccountMailMode";compact:true;model:["Read","Read & write"];enabled:mailEnabled.checked && !editor.saving}
            }
            RowLayout {Layout.fillWidth:true;spacing:12
                FmCheckBox {id:calendarEnabled;objectName:"editAccountCalendar";text:"Calendar";Layout.fillWidth:true;topPadding:4;bottomPadding:4;implicitHeight:28;background:null;enabled:!editor.saving && (mailEnabled.checked || !checked)}
                FmSegmented {id:calendarMode;objectName:"editAccountCalendarMode";compact:true;model:["Read","Read & write"];enabled:calendarEnabled.checked && !editor.saving}
            }
            Text {visible:!!editor.error;text:editor.error;Layout.fillWidth:true;wrapMode:Text.Wrap;color:Theme.error;font.pointSize:Theme.fontPoints(11.5)}
        }
    }
    footer:Item {
        implicitHeight:54
        RowLayout {
            anchors.left:parent.left;anchors.right:parent.right;anchors.top:parent.top;anchors.leftMargin:22;anchors.rightMargin:22;spacing:6
            Item {Layout.fillWidth:true}
            FmButton {text:"Cancel";outlined:true;implicitHeight:32;leftPadding:14;rightPadding:14;enabled:!editor.saving;onClicked:editor.close()}
            FmButton {objectName:"saveAccountEdit";text:editor.saving ? "Saving…" : "Save";prominent:true;implicitHeight:32;leftPadding:14;rightPadding:14;enabled:!editor.saving && !!displayName.text.trim() && (mailEnabled.checked || calendarEnabled.checked);onClicked:editor.save()}
        }
    }
}
