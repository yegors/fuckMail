import QtQuick
import QtQuick.Controls
import QtQuick.Shapes
import QtQuick.Layouts

Rectangle {
    id: card
    property bool inlineAction: false
    property string messageId: ""
    property string modelName: ""
    property var result: null
    property var members: []
    property string title: "Summary"
    property bool busy: false
    property bool canSummarize: false
    property bool showSources: true
    property string error: ""
    property string retainedError: ""
    property bool details: false
    property real maximumBodyHeight: 100000
    property double startedAt: Date.now()
    property int elapsedSeconds: 0
    readonly property bool compact: inlineAction && !result && !busy && !error.length
    readonly property bool heightAnimating: false
    readonly property string summaryText: result && result.output ? result.output.summary || "" : ""
    readonly property var sources: result ? ((result.output || {}).source_ids || result.source_ids || []) : []
    readonly property bool popupOpen: actions.opened || sourcePopup.opened
    readonly property string resultIdentity: result ? [result.conversation_id || "", result.target || "", result.created_at || ""].join("/") : ""
    readonly property string captionModel: result ? (result.model || {}).name || modelName : modelName
    readonly property string generatedAt: result && result.created_at ? Qt.formatDateTime(new Date(result.created_at), "MMMM d yyyy HH:mm") : ""
    signal regenerate()
    signal cancelRequested()
    signal openSource(string messageId)
    onResultIdentityChanged: details = false
    onBusyChanged: { if (busy) { startedAt = Date.now(); elapsedSeconds = 0 } }
    onMessageIdChanged: { startedAt = Date.now(); elapsedSeconds = 0 }
    Timer { interval: 1000; running: card.busy && card.visible; repeat: true; onTriggered: card.elapsedSeconds = Math.floor((Date.now() - card.startedAt) / 1000) }
    implicitHeight: compact ? 26 : content.implicitHeight + 24
    color: compact ? "transparent" : Theme.withAlpha(Theme.ai, busy ? .04 : .07)
    radius: 8
    border.width: compact || busy ? 0 : 1
    border.color: Theme.withAlpha(Theme.ai,.16)
    visible: inlineAction || !!result || busy || error.length > 0
    Shape {
        anchors.fill: parent; visible: card.busy
        ShapePath {
            strokeColor: Theme.withAlpha(Theme.ai,.3); strokeWidth: 1
            strokeStyle: ShapePath.DashLine; dashPattern: [3,3]; fillColor: "transparent"
            PathSvg { path: "M 8 .5 H " + (card.width-8) + " Q " + (card.width-.5) + " .5 " + (card.width-.5) + " 8 V " + (card.height-8) + " Q " + (card.width-.5) + " " + (card.height-.5) + " " + (card.width-8) + " " + (card.height-.5) + " H 8 Q .5 " + (card.height-.5) + " .5 " + (card.height-8) + " V 8 Q .5 .5 8 .5 Z" }
        }
    }
    FmButton {
        id: summarize; objectName: "summarizeMessage_" + card.messageId
        visible: card.compact; enabled: card.canSummarize
        text: "Summarize"; iconName: "summary"; iconSize: 11; foreground: Theme.ai
        font.pointSize: Theme.fontPoints(12); implicitHeight: 26; leftPadding: 8; rightPadding: 10
        contentItem: RowLayout {
            spacing:7
            FmIcon {name:"summary";size:11;color:Theme.ai}
            Text {text:"Summarize";color:Theme.ai;font.pointSize:Theme.fontPoints(12)}
            Text {objectName:"summaryActionModel_"+card.messageId;visible:!!card.modelName;text:card.modelName;color:Theme.inkAlpha(.4);font.pointSize:Theme.fontPoints(11);Layout.fillWidth:true;Layout.minimumWidth:0;elide:Text.ElideRight}
        }
        implicitWidth: Math.min(card.width, contentItem.implicitWidth + 18)
        background: Rectangle {
            radius: 5; color: summarize.hovered ? Theme.withAlpha(Theme.ai,.08) : "transparent"
            Shape { anchors.fill: parent
                ShapePath {
                    strokeColor: summarize.hovered ? Theme.ai : Theme.withAlpha(Theme.ai,.45); strokeWidth: 1
                    strokeStyle: ShapePath.DashLine; dashPattern: [2,2]; fillColor: "transparent"
                    PathSvg { path: "M 5 .5 H " + (summarize.width-5) + " Q " + (summarize.width-.5) + " .5 " + (summarize.width-.5) + " 5 V 21 Q " + (summarize.width-.5) + " 25.5 " + (summarize.width-5) + " 25.5 H 5 Q .5 25.5 .5 21 V 5 Q .5 .5 5 .5 Z" }
                }
            }
        }
        onClicked: card.regenerate()
    }
    ColumnLayout {
        id: content; x: 14; y: 12; width: Math.max(0, parent.width-28); spacing: 6; visible: !card.compact
        RowLayout {
            visible: card.busy; Layout.fillWidth: true
            Text { text: "Writing the summary…"; color: Theme.ai; font.pointSize: Theme.fontPoints(13); Layout.fillWidth: true }
            Text { text: card.elapsedSeconds + "s"; color: Theme.withAlpha(Theme.ai,.55); font.pointSize: Theme.fontPoints(11); font.features: {"tnum":1} }
        }
        Rectangle {
            objectName: "summaryProgress"; visible: card.busy; Layout.fillWidth: true; implicitHeight: 2
            color: Theme.withAlpha(Theme.ai,.12); clip: true
            Rectangle {
                width: parent.width*.3; height: 2; color: Theme.withAlpha(Theme.ai,.65)
                SequentialAnimation on x { running: card.busy && card.visible; loops: Animation.Infinite
                    NumberAnimation { from: -card.width*.3; to: card.width; duration: 1400; easing.type: Easing.InOutSine }
                }
            }
        }
        TextEdit {
            id: summaryBody; objectName: "summaryBody"
            visible: card.summaryText.length > 0 && !card.busy
            Layout.fillWidth: true; text: Theme.proseMarkup(card.summaryText,1.55)
            textFormat: TextEdit.RichText; wrapMode: TextEdit.Wrap; readOnly: true; selectByMouse: true
            color: Theme.ai; selectionColor: Theme.selection; selectedTextColor: Theme.selectedText
            font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(13.5)
            Accessible.name: card.title
        }
        Text { objectName: "summaryError"; visible: card.error.length > 0; text: card.error; Layout.fillWidth: true; wrapMode: Text.Wrap; color: Theme.error; font.pointSize: Theme.fontPoints(12); textFormat: Text.PlainText }
        Text {
            objectName: "summaryCoverage"; visible: !!card.result && (card.result.stale || card.result.partial)
            Layout.fillWidth: true; wrapMode: Text.Wrap; color: Theme.muted; font.pointSize: Theme.fontPoints(11)
            text: [card.result && card.result.stale ? "Out of date · Re-summarize to update" : "", card.result && card.result.partial ? "Partial · Some conversation text was omitted" : ""].filter(Boolean).join(" · ")
        }
        RowLayout {
            Layout.fillWidth: true; spacing: 6
            FmIcon {
                name: "summary"; size: 10; color: Theme.withAlpha(Theme.ai,.6)
                RotationAnimator on rotation { running: card.busy && card.visible; from: 0; to: 360; duration: 4000; loops: Animation.Infinite }
                onVisibleChanged: if (!card.busy) rotation = 0
            }
            AbstractButton {
                objectName:"summaryActions"; Accessible.name:card.busy ? "Summary progress and cancel" : "Summary actions"; onClicked:actions.open()
                contentItem: Text { objectName: "summaryModelName"; Layout.fillWidth: true; Layout.minimumWidth: 0; elide: Text.ElideRight
                text: card.busy ? "Summarizing · " + card.modelName + " · on this machine · keep reading" : [card.captionModel,card.generatedAt].filter(Boolean).join(" · ")
                color: Theme.withAlpha(Theme.ai,.6); font.pointSize: Theme.fontPoints(11); textFormat: Text.PlainText
                HoverHandler { id: captionHover }
                FmToolTip { shown: captionHover.hovered; text: parent.text }
                }
                Layout.fillWidth:true;implicitHeight:contentItem.implicitHeight
            }
        }
    }
    onDetailsChanged: { if (details) sourcePopup.open(); else sourcePopup.close() }
    Popup {
        id: sourcePopup
        x: Math.max(0, card.width - width); y: 38
        width: Math.min(card.width, 320); padding: 12
        onClosed: card.details = false
        closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
        background: Rectangle { color: Theme.surface; radius: 6; border.color: Theme.controlBorder }
        contentItem: ScrollView {
                ScrollBar.vertical: FmScrollBar {}
            id: sourcesScroll
            implicitHeight: Math.min(sourceDetails.implicitHeight, 160)
            clip: true
            ColumnLayout {
                id: sourceDetails
                width: sourcesScroll.availableWidth; spacing: 2
                Text {
                    Layout.fillWidth: true; wrapMode: Text.Wrap
                    text: card.result && card.result.created_at ? "Generated " + new Date(card.result.created_at).toLocaleString() : "Generated on this machine"
                    color: Theme.subtle; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(11)
                }
                Repeater {
                    model: card.showSources && card.details ? card.sources : []
                    FmButton {
                        required property string modelData
                        readonly property var member: card.members.find(m => m.id === modelData) || {}
                        objectName: "summarySource_" + modelData
                        text: (member.sender_name || member.sender_email || "Source message") + (member.sent_at ? " · " + Qt.formatDateTime(new Date(member.sent_at), "d MMM, HH:mm") : "")
                        Layout.fillWidth: true; textAlignment: Text.AlignLeft; leftPadding: 0
                        font.pointSize: Theme.fontPoints(11); foreground: Theme.ai
                        tip: "Open this source message"; onClicked: { const owner = card; const source = modelData; owner.openSource(source); owner.details = false }
                    }
                }
            }
        }
    }
    Popup {
        id: actions
        x: Math.max(0, (card.compact ? summarize.width : card.width) - width); y: card.compact ? 33 : 38
        width: Math.min(card.width, 220); padding: 6
        closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
        background: Rectangle { color: Theme.surface; radius: 6; border.color: Theme.controlBorder }
        contentItem: ColumnLayout {
            spacing: 2
            FmButton {
                visible: !card.busy
                objectName: "regenerateSummary"; Layout.fillWidth: true; textAlignment: Text.AlignLeft
                text: card.result ? "Re-summarize" : "Try again"; iconName: "summary"
                enabled: card.canSummarize; onClicked: { actions.close(); card.regenerate() }
            }
            FmButton {
                objectName: "summaryCancel"; visible: card.busy
                Layout.fillWidth: true; textAlignment: Text.AlignLeft; text: "Cancel summarization"
                onClicked: { actions.close(); card.cancelRequested() }
            }
            FmButton {
                objectName: "summaryDetails"; Layout.fillWidth: true; textAlignment: Text.AlignLeft
                text: card.details ? "Hide sources and details" : "Sources and details"
                visible: !!card.result; onClicked: { actions.close(); card.details = !card.details }
            }
        }
    }
}
