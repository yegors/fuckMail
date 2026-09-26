// Local-model controls under one message in MailView: the sender-kind
// correction menu (Human / Automated) and the message's SummaryCard. Results
// and jobs belong to the shared MailAssistance passed in as `controller`.
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

ColumnLayout {
    id: panel
    required property var controller
    required property var message
    readonly property var summary: controller.resultFor("summary", message.id)
    readonly property var classification: controller.resultFor("classification", message.id)
    readonly property bool canClassify: controller.classificationEnabled && !message.sent
    property bool showKindControl: true
    function correctSender() { correction.open() }
    // The header's summarize icon shows only with no summary, none running and no failure.
    readonly property bool canSummarize: controller.summariesEnabled && !controller.busy && backend.connected
    readonly property bool showSummarize: !message.sent && controller.summariesEnabled && !summaryCard.showing
    readonly property bool canResummarize: !message.sent && summaryCard.summaryText.length > 0 && !summaryCard.busy
    readonly property string modelName: (controller.settings.model || {}).name || ""
    readonly property bool summaryShowing: summaryCard.showing
    readonly property bool summaryBusy: summaryCard.busy
    readonly property bool summaryFailed: summaryCard.failed
    function summarize() { controller.start("summary", message.id) }
    readonly property bool popupOpen: correction.opened || summaryCard.popupOpen

    spacing: 8
    visible: !message.sent && (controller.summariesEnabled || canClassify || !!summary || !!classification)
    Flow {
        Layout.fillWidth: true; spacing: 6
        visible: panel.canClassify && panel.showKindControl
        FmButton {
            id: kind; objectName: "correctSender_" + panel.message.id
            visible: panel.canClassify && panel.showKindControl
            text: ({people: "Human", automated: "Automated", unknown: "Unknown sender kind"})[panel.message.kind] || "Unknown sender kind"
            iconName: "down"; iconSize: 10; font.pointSize: Theme.fontPoints(11)
            tip: "Correct sender kind or classify this message"; onClicked: correction.open()
        }

    }
    Text {
        objectName: "classificationError_" + panel.message.id
        visible: panel.controller.errorFor("classification", panel.message.id).length > 0 && (!panel.controller.summariesEnabled || panel.controller.errorFor("classification", panel.message.id) !== panel.controller.errorFor("summary", panel.message.id))
        text: panel.controller.errorFor("classification", panel.message.id)
        Layout.fillWidth: true; wrapMode: Text.Wrap; textFormat: Text.PlainText; color: Theme.error
        font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12)
    }
    SummaryCard {
        id: summaryCard; objectName: "messageSummary_" + panel.message.id
        Layout.fillWidth: true; title: "Summary"
        messageId: panel.message.id
        modelName: panel.modelName
        result: panel.summary; members: [panel.message]
        busy: panel.controller.workingOn("summary", panel.message.id)
        error: panel.controller.errorFor("summary", panel.message.id)
        canSummarize: panel.canSummarize
        onRegenerate: panel.controller.start("summary", panel.message.id)
        onCancelRequested: panel.controller.cancel()
        onOpenSource: messageId => panel.controller.openSource(messageId)
    }
    Popup {
        id: correction; y: kind.height; width: Math.min(panel.width, 230); padding: 6
        background: Rectangle { color: Theme.surface; radius: 6; border.color: Theme.controlBorder }
        contentItem: ColumnLayout {
            spacing: 2
            FmButton {
                objectName: "classifyMessage_" + panel.message.id
                text: panel.classification ? "Classify again" : "Classify message"
                Layout.fillWidth: true; textAlignment: Text.AlignLeft
                enabled: !panel.controller.busy && backend.connected
                onClicked: { correction.close(); panel.controller.start("classification", panel.message.id) }
            }
            Rectangle { Layout.fillWidth: true; implicitHeight: 1; color: Theme.line }
            Repeater {
                model: [{name: "Human", kind: "human"}, {name: "Automated", kind: "automated"}, {name: "Reset correction", kind: "reset"}]
                FmButton {
                    required property var modelData
                    text: modelData.name; Layout.fillWidth: true; textAlignment: Text.AlignLeft
                    enabled: backend.connected
                    onClicked: { correction.close(); panel.controller.correct(panel.message, modelData.kind) }
                }
            }
            Text {
                visible: !!panel.classification && panel.classification.stale; Layout.fillWidth: true; wrapMode: Text.Wrap; textFormat: Text.PlainText
                text: "Out-of-date classification."
                color: Theme.muted; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(11)
            }
        }
    }
}
