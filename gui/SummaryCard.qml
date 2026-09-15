import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Rectangle {
    id: card
    property var result: null
    property var members: []
    property string title: "Summary"
    property bool busy: false
    property bool canSummarize: false
    property bool showSources: true
    property string error: ""
    property bool details: false
    property real maximumBodyHeight: 210
    readonly property string summaryText: result && result.output ? result.output.summary || "" : ""
    readonly property var sources: result ? ((result.output || {}).source_ids || result.source_ids || []) : []
    readonly property bool popupOpen: actions.opened || sourcePopup.opened
    signal regenerate()
    signal cancelRequested()
    signal openSource(string messageId)
    implicitHeight: content.implicitHeight + 21
    color: Theme.withAlpha(Theme.ai, busy ? 0.04 : 0.06)
    radius: 6
    border.width: 0
    visible: !!result || busy || error.length > 0
    readonly property string resultIdentity: result ? [result.conversation_id || "", result.target || "", result.created_at || ""].join("/") : ""
    onResultIdentityChanged: details = false
    Rectangle { x: 0; y: 0; width: 2; height: parent.height; color: Theme.ai }
    ColumnLayout {
        id: content
        x: 16; y: 10; width: parent.width - 30; spacing: 5
        RowLayout {
            Layout.fillWidth: true; spacing: 7
            FmIcon { name: "summary"; size: 11; color: Theme.ai }
            Text {
                text: card.title.toUpperCase(); color: Theme.ai
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
                objectName: "summaryCancel"; visible: card.busy
                iconName: "close"; implicitWidth: 22; implicitHeight: 22; leftPadding: 4; rightPadding: 4
                tip: "Cancel summarization"; onClicked: card.cancelRequested()
            }
            FmButton {
                id: more; objectName: "summaryActions"; visible: !card.busy
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
            ScrollBar.vertical: ScrollBar {}
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
            visible: card.busy; Layout.fillWidth: true
            text: card.summaryText ? "Updating summary…" : "Summarizing on this machine…"
            color: Theme.secondary; wrapMode: Text.Wrap
            font { family: Theme.fontFamily; pixelSize: Theme.fontSize(13) }
        }
        Text {
            objectName: "summaryError"; visible: card.error.length > 0; Layout.fillWidth: true
            text: card.error; color: Theme.error; wrapMode: Text.Wrap; textFormat: Text.PlainText
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
    onDetailsChanged: { if (details) sourcePopup.open(); else sourcePopup.close() }
    Popup {
        id: sourcePopup
        x: Math.max(0, card.width - width); y: 38
        width: Math.min(card.width, 320); padding: 12
        onClosed: card.details = false
        closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
        background: Rectangle { color: Theme.surface; radius: 6; border.color: Theme.controlBorder }
        contentItem: ScrollView {
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
        x: Math.max(0, card.width - width); y: 38
        width: Math.min(card.width, 220); padding: 6
        closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
        background: Rectangle { color: Theme.surface; radius: 6; border.color: Theme.controlBorder }
        contentItem: ColumnLayout {
            spacing: 2
            FmButton {
                objectName: "regenerateSummary"; Layout.fillWidth: true; textAlignment: Text.AlignLeft
                text: card.result ? "Re-summarize" : "Try again"; iconName: "summary"
                enabled: card.canSummarize; onClicked: { actions.close(); card.regenerate() }
            }
            FmButton {
                objectName: "summaryDetails"; Layout.fillWidth: true; textAlignment: Text.AlignLeft
                text: card.details ? "Hide sources and details" : "Sources and details"
                visible: !!card.result; onClicked: { actions.close(); card.details = !card.details }
            }
        }
    }
}
