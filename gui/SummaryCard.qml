import QtQuick
import QtQuick.Controls
import QtQuick.Shapes
import QtQuick.Layouts

Rectangle {
    id: card
    property bool inlineAction: false
    property string messageId: ""
    property string modelName: ""
    property bool revealed: false
    readonly property bool compact: inlineAction && !revealed
    onResultChanged: if (result) revealed = true
    onErrorChanged: {
        if (error.length) { revealed = true; retainedError = error }
        else clearSettledError()
    }
    onBusyChanged: {
        if (busy) { finishMotion.stop(); motionReady = true }
        else { clearSettledError(); finishMotion.restart() }
    }
    function clearSettledError() {
        // Job/error bindings settle together; do not clear a retry's visible copy mid-update.
        Qt.callLater(function() { if (!card.busy && !card.error.length) card.retainedError = "" })
    }
    onMessageIdChanged: { revealed = !!result || error.length > 0; motionReady = false; finishMotion.stop() }
    property bool motionReady: false
    Timer { id: finishMotion; interval: 350; onTriggered: card.motionReady = false }
    property var result: null
    property var members: []
    property string title: "Summary"
    property bool busy: false
    property bool canSummarize: false
    property bool showSources: true
    property string error: ""
    property string retainedError: ""
    readonly property string displayedError: error || (busy ? retainedError : "")
    property bool details: false
    property real maximumBodyHeight: 210
    readonly property string summaryText: result && result.output ? result.output.summary || "" : ""
    readonly property var sources: result ? ((result.output || {}).source_ids || result.source_ids || []) : []
    readonly property bool popupOpen: actions.opened || sourcePopup.opened
    signal regenerate()
    signal cancelRequested()
    signal openSource(string messageId)
    implicitHeight: compact ? summarize.implicitHeight : content.implicitHeight + 21
    readonly property bool heightAnimating: heightMotion.running
    Behavior on implicitHeight { enabled: card.inlineAction && card.motionReady; NumberAnimation { id: heightMotion; duration: 220; easing.type: Easing.InOutCubic } }
    clip: true
    color: "transparent"
    radius: 6
    border.width: 0
    visible: inlineAction || !!result || busy || error.length > 0
    readonly property string resultIdentity: result ? [result.conversation_id || "", result.target || "", result.created_at || ""].join("/") : ""
    onResultIdentityChanged: details = false
    Rectangle {
        anchors.fill: parent; color: Theme.withAlpha(Theme.ai, 0.06); radius: 6
        opacity: card.compact ? 0 : 1
        Behavior on opacity { enabled: card.motionReady; NumberAnimation { duration: 180 } }
        Rectangle { width: 2; height: parent.height; color: Theme.ai }
    }
    FmButton {
        id: summarize
        objectName: "summarizeMessage_" + card.messageId
        visible: card.compact
        enabled: card.canSummarize || card.busy
        text: card.busy ? "Summarizing…" : "Summarize"; iconName: "summary"; foreground: Theme.ai
        font.pixelSize: Theme.fontSize(12); implicitHeight: 29; leftPadding: 10; rightPadding: 10
        implicitWidth: Math.min(card.width, 61 + actionMetrics.width + (card.modelName.length ? Math.min(modelMetrics.width, 210) + 7 : 0))
        FmButton {
            objectName: "summaryBusyActions"; visible: card.busy
            anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter
            implicitWidth: 22; implicitHeight: 22; leftPadding: 4; rightPadding: 4
            iconName: "more"; tip: "Summary actions"; onClicked: actions.open()
        }
        TextMetrics { id: actionMetrics; text: "Summarizing…"; font: summarize.font }
        TextMetrics { id: modelMetrics; text: card.modelName; font: summarize.font }
        contentItem: Row {
            spacing: 7
            FmIcon { name: "summary"; size: 12; color: Theme.ai; anchors.verticalCenter: parent.verticalCenter }
            Text { id: summaryLabel; text: summarize.text; color: Theme.ai; font: summarize.font; anchors.verticalCenter: parent.verticalCenter }
            Text {
                id: modelLabel; visible: text.length > 0
                objectName: "summaryActionModel_" + card.messageId
                text: card.modelName
                width: Math.max(0,summarize.availableWidth-summaryLabel.implicitWidth-48)
                elide: Text.ElideRight; color: Theme.subtle; font: summarize.font; anchors.verticalCenter: parent.verticalCenter; textFormat: Text.PlainText
                FmToolTip { visible: summarize.hovered && modelLabel.truncated; text: modelLabel.text }
            }
        }
        Rectangle {
            objectName: "summaryProgress"
            visible: card.busy; x: 6; y: summarize.height - 3; width: summarize.width - 12; height: 1; clip: true
            color: Theme.withAlpha(Theme.ai, 0.15)
            Rectangle {
                width: parent.width * 0.3; height: 1; color: Theme.ai
                SequentialAnimation on x {
                    running: card.busy && card.compact && card.visible; loops: Animation.Infinite
                    NumberAnimation { from: -summarize.width * 0.3; to: summarize.width; duration: 1400; easing.type: Easing.InOutSine }
                }
            }
        }
        background: Rectangle {
            radius: 5; color: summarize.hovered ? Theme.withAlpha(Theme.ai,0.07) : "transparent"
            Shape {
                anchors.fill: parent
                ShapePath {
                    strokeColor: summarize.activeFocus ? Theme.focusBorder : Theme.withAlpha(Theme.ai,0.4)
                    strokeWidth: 1; strokeStyle: ShapePath.DashLine; dashPattern: [2,2]; fillColor: "transparent"
                    PathSvg { path: "M 5 0.5 H " + (summarize.width-5) + " Q " + (summarize.width-0.5) + " 0.5 " + (summarize.width-0.5) + " 5 V " + (summarize.height-5) + " Q " + (summarize.width-0.5) + " " + (summarize.height-0.5) + " " + (summarize.width-5) + " " + (summarize.height-0.5) + " H 5 Q 0.5 " + (summarize.height-0.5) + " 0.5 " + (summarize.height-5) + " V 5 Q 0.5 0.5 5 0.5 Z" }
                }
            }
        }
        Accessible.name: card.busy ? "Summarizing message" : "Summarize message"
        onClicked: if (!card.busy) card.regenerate()
    }

    ColumnLayout {
        id: content
        x: 16; y: 10; width: parent.width - 30; spacing: 5
        visible: !card.compact
        opacity: card.compact ? 0 : 1
        Behavior on opacity { enabled: card.motionReady; NumberAnimation { duration: 180 } }
        RowLayout {
            Layout.fillWidth: true; spacing: 7
            FmIcon { name: "summary"; size: 11; color: Theme.ai }
            Text {
                text: card.busy ? "SUMMARIZING…" : card.title.toUpperCase(); color: Theme.ai
                font { family: Theme.fontFamily; pixelSize: Theme.fontSize(11); letterSpacing: 0.66 }
            }
            Text {
                objectName: "summaryModelName"
                text: card.result ? (card.result.model || {}).name || "" : ""
                Layout.fillWidth: true; horizontalAlignment: Text.AlignRight; elide: Text.ElideRight
                color: Theme.subtle; textFormat: Text.PlainText
                font { family: Theme.fontFamily; pixelSize: Theme.fontSize(11) }
            }
            FmButton {
                id: more; objectName: "summaryActions"
                iconName: "more"; implicitWidth: 22; implicitHeight: 22; leftPadding: 4; rightPadding: 4
                tip: "Summary actions and sources"; onClicked: actions.open()
            }
        }
        Flickable {
            id: summaryScroll
            objectName: "summaryScroll"
            Layout.fillWidth: true
            Layout.preferredHeight: Math.min(summaryBody.implicitHeight, card.maximumBodyHeight)
            visible: card.summaryText.length > 0
            contentWidth: width; contentHeight: summaryBody.implicitHeight
            clip: true; boundsBehavior: Flickable.StopAtBounds
            ScrollBar.vertical: FmScrollBar {}
            TextEdit {
                id: summaryBody; objectName: "summaryBody"
                width: Math.max(0, summaryScroll.width - 8)
                text: Theme.proseMarkup(card.summaryText, 1.5)
                textFormat: TextEdit.RichText; wrapMode: TextEdit.Wrap
                readOnly: true; selectByMouse: true; color: Theme.text
                selectionColor: Theme.selection; selectedTextColor: Theme.selectedText
                font { family: Theme.fontFamily; pixelSize: Theme.fontSize(14.5); letterSpacing: 0 }
                Accessible.name: card.title
            }
        }
        Text {
            objectName: "summaryError"; visible: card.displayedError.length > 0; Layout.fillWidth: true
            text: card.displayedError; color: Theme.error; wrapMode: Text.Wrap; textFormat: Text.PlainText
            font { family: Theme.fontFamily; pixelSize: Theme.fontSize(12) }
        }
        Text {
            objectName: "summaryCoverage"
            visible: !!card.result && (card.result.stale || card.result.partial); Layout.fillWidth: true
            text: [card.result && card.result.stale ? "Out of date · Re-summarize to update" : "",
                   card.result && card.result.partial ? "Partial · Some conversation text was omitted" : ""].filter(Boolean).join("\n")
            color: Theme.muted; wrapMode: Text.Wrap
            font { family: Theme.fontFamily; pixelSize: Theme.fontSize(11) }
        }

    }
    Rectangle {
        visible: card.busy && !card.compact; x: 16; y: 35; width: card.width - 30; height: 1; clip: true
        color: Theme.withAlpha(Theme.ai, 0.15)
        Rectangle {
            width: parent.width * 0.3; height: 1; color: Theme.ai
            SequentialAnimation on x {
                running: card.busy && !card.compact && card.visible; loops: Animation.Infinite
                NumberAnimation { from: -card.width * 0.3; to: card.width; duration: 1400; easing.type: Easing.InOutSine }
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
                    color: Theme.subtle; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(11)
                }
                Repeater {
                    model: card.showSources && card.details ? card.sources : []
                    FmButton {
                        required property string modelData
                        readonly property var member: card.members.find(m => m.id === modelData) || {}
                        objectName: "summarySource_" + modelData
                        text: (member.sender_name || member.sender_email || "Source message") + (member.sent_at ? " · " + Qt.formatDateTime(new Date(member.sent_at), "d MMM, HH:mm") : "")
                        Layout.fillWidth: true; textAlignment: Text.AlignLeft; leftPadding: 0
                        font.pixelSize: Theme.fontSize(11); foreground: Theme.ai
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
