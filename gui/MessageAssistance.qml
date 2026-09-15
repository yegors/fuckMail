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
    readonly property bool popupOpen: correction.opened || summaryCard.popupOpen
    readonly property var suggestions: classification ? (classification.output.category_ids || []).filter(id => (message.categories || []).indexOf(id) < 0) : []
    spacing: 8
    visible: controller.summariesEnabled || canClassify || !!summary || !!classification
    Flow {
        Layout.fillWidth: true; spacing: 6
        FmButton {
            objectName: "summarizeMessage_" + panel.message.id
            visible: panel.controller.summariesEnabled && !panel.summary && !panel.controller.workingOn("summary", panel.message.id)
            enabled: !panel.controller.busy && backend.connected
            text: "Summarize message"; iconName: "summary"; foreground: Theme.ai
            font.pixelSize: Theme.fontSize(11); leftPadding: 0
            onClicked: panel.controller.start("summary", panel.message.id)
        }
        FmButton {
            id: kind; objectName: "correctSender_" + panel.message.id
            visible: panel.canClassify
            text: ({people: "Human", automated: "Automated", unknown: "Unknown sender kind"})[panel.message.kind] || "Unknown sender kind"
            iconName: "down"; iconSize: 10; font.pixelSize: Theme.fontSize(11)
            tip: "Correct sender kind or classify this message"; onClicked: correction.open()
        }
        FmButton {
            visible: panel.controller.workingOn("classification", panel.message.id)
            text: "Classifying… Cancel"; font.pixelSize: Theme.fontSize(11)
            onClicked: panel.controller.cancel()
        }
        Repeater {
            model: panel.suggestions
            FmButton {
                required property string modelData
                objectName: "acceptSuggestion_" + panel.message.id + "_" + modelData
                text: (backend.categories.find(c => c.id === modelData) || {}).name || modelData
                iconName: "plus"; iconSize: 10; foreground: Theme.ai
                enabled: !panel.classification.stale && backend.connected
                font.pixelSize: Theme.fontSize(11)
                tip: panel.classification.stale ? "Suggestion is out of date. Classify again." : "Apply suggested label to this message"
                background: Rectangle { radius: 4; color: parent.hovered ? Theme.withAlpha(Theme.ai, 0.08) : "transparent"; border.color: Theme.withAlpha(Theme.ai, 0.3) }
                onClicked: panel.controller.accept(panel.message, modelData)
            }
        }
    }
    Text {
        visible: panel.controller.errorFor("classification", panel.message.id).length > 0
        text: panel.controller.errorFor("classification", panel.message.id)
        Layout.fillWidth: true; wrapMode: Text.Wrap; textFormat: Text.PlainText; color: Theme.error
        font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(12)
    }
    SummaryCard {
        id: summaryCard; objectName: "messageSummary_" + panel.message.id
        Layout.fillWidth: true; title: "Message summary"
        result: panel.summary; members: [panel.message]
        busy: panel.controller.workingOn("summary", panel.message.id)
        error: panel.controller.errorFor("summary", panel.message.id)
        canSummarize: panel.controller.summariesEnabled && !panel.controller.busy && backend.connected
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
                model: [{name: "Human", kind: "human"}, {name: "Automated", kind: "automated"}, {name: "Unknown", kind: "unknown"}, {name: "Reset correction", kind: "reset"}]
                FmButton {
                    required property var modelData
                    text: modelData.name; Layout.fillWidth: true; textAlignment: Text.AlignLeft
                    enabled: backend.connected
                    onClicked: { correction.close(); panel.controller.correct(panel.message, modelData.kind) }
                }
            }
            Text {
                visible: !!panel.classification; Layout.fillWidth: true; wrapMode: Text.Wrap; textFormat: Text.PlainText
                text: panel.classification ? (panel.classification.stale ? "Out-of-date suggestion. " : "") + panel.classification.output.reason : ""
                color: Theme.muted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(11)
            }
        }
    }
}
