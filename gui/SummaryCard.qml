import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

// A local summary box, for one message or a whole thread: absent until
// requested, then a progress line while the model works, the summary, or a
// red failure line with Try again. The summary reads as one paragraph led by
// a TLDR tag and the conversation's #label; the model and time sit in the
// box's tooltip, and the tag opens sources, details and Cancel. A fresh
// summary arrives green with a check and settles to the AI colours.
Rectangle {
    id: card
    property string messageId: ""
    property string modelName: ""
    property var result: null
    property var members: []
    property string title: "Summary"
    // The conversation's first label, shown after the tag.
    property string label: ""
    // Progress lines while busy, one after another as time passes.
    property var stages: ["Reading the message…", "Writing the summary…"]
    property bool busy: false
    property bool canSummarize: false
    property bool showSources: true
    // A thread summary can be put away with ×.
    property bool closable: false
    property string error: ""
    property bool details: false
    property double startedAt: Date.now()
    property int elapsedSeconds: 0
    readonly property bool heightAnimating: false
    readonly property string summaryText: result && result.output ? result.output.summary || "" : ""
    readonly property var sources: result ? ((result.output || {}).source_ids || result.source_ids || []) : []
    readonly property bool popupOpen: actions.opened || sourcePopup.opened
    readonly property string resultIdentity: result ? [result.conversation_id || "", result.target || "", result.created_at || ""].join("/") : ""
    readonly property string captionModel: result ? (result.model || {}).name || modelName : modelName
    readonly property string generatedAt: result && result.created_at ? Qt.formatDateTime(new Date(result.created_at), "d MMM yyyy HH:mm") : ""
    readonly property bool failed: !busy && error.length > 0
    readonly property bool showing: busy || failed || summaryText.length > 0
    readonly property string stageText: stages.length ? stages[Math.min(stages.length - 1, Math.floor(elapsedSeconds / 3))] : ""
    readonly property color doneGreen: Qt.rgba(111/255, 191/255, 138/255, 1)
    // "Done" follows only a summary this card saw being written, never navigation.
    property bool done: false
    property string startIdentity: ""
    property bool armed: false
    // Try again is its own button, so the engine's closing advice is dropped.
    readonly property string failureReason: error.replace(/\s*(Please )?try again\.?\s*$/i, "")
    readonly property string failureText: /tim(e|ed) ?out|respond|stopped/i.test(error) || !failureReason.length
        ? "Couldn’t summarize — " + (modelName || "the local model") + " stopped responding."
        : "Couldn’t summarize — " + failureReason.charAt(0).toLowerCase() + failureReason.slice(1)
    function escaped(value) { return String(value || "").replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;") }
    signal regenerate()
    signal cancelRequested()
    signal closeRequested()
    signal openSource(string messageId)
    onResultIdentityChanged: {
        details = false
        if (armed && result && resultIdentity !== startIdentity) { armed = false; done = true; doneTimer.restart() }
    }
    onBusyChanged: {
        if (busy) { startedAt = Date.now(); elapsedSeconds = 0; startIdentity = resultIdentity; armed = true; done = false }
        else if (error.length) armed = false
    }
    onMessageIdChanged: { startedAt = Date.now(); elapsedSeconds = 0; armed = false; done = false; doneTimer.stop() }
    // Green appears at once, like a new box; only the return to AI colours fades.
    property bool fading: false
    Timer { id: doneTimer; interval: 1800; onTriggered: { card.fading = true; card.done = false; fadeTimer.restart() } }
    Timer { id: fadeTimer; interval: 650; onTriggered: card.fading = false }
    Timer { interval: 1000; running: card.busy && card.visible; repeat: true; onTriggered: card.elapsedSeconds = Math.floor((Date.now() - card.startedAt) / 1000) }
    visible: showing
    implicitHeight: showing ? (failed ? failure.implicitHeight + 24 : content.implicitHeight + 24) : 0
    radius: 8
    color: failed ? Theme.withAlpha(Theme.red, .07) : done ? Qt.rgba(111/255, 191/255, 138/255, .08) : Theme.withAlpha(Theme.ai, .07)
    Behavior on color { enabled: card.fading; ColorAnimation { duration: 600; easing.type: Easing.InOutQuad } }
    HoverHandler { id: cardHover }
    FmToolTip { shown: cardHover.hovered && !card.busy && !card.failed && !actions.opened; text: [card.title, card.captionModel, card.generatedAt].filter(Boolean).join(" · ") }
    RowLayout {
        id: failure; x: 16; y: 12; width: Math.max(0, parent.width-32); spacing: 12; visible: card.failed
        Text { objectName: "summaryError"; text: card.failureText; Layout.fillWidth: true; wrapMode: Text.Wrap; color: Theme.text; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12); textFormat: Text.PlainText }
        AbstractButton {
            objectName: "summaryRetry"; enabled: card.canSummarize; Layout.alignment: Qt.AlignVCenter
            implicitWidth: retryText.implicitWidth; implicitHeight: retryText.implicitHeight + 2
            Accessible.name: "Try again"; onClicked: card.regenerate()
            HoverHandler { cursorShape: Qt.PointingHandCursor }
            contentItem: Item {}
            Text { id: retryText; text: "Try again"; color: Theme.red; opacity: parent.enabled ? 1 : .5; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12) }
            Rectangle { y: retryText.implicitHeight + 1; width: retryText.implicitWidth; height: 1; color: Theme.withAlpha(Theme.red, .4) }
        }
        CloseMark { visible: card.closable }
    }
    component CloseMark: AbstractButton {
        objectName: "summaryClose"
        Layout.alignment: Qt.AlignTop
        implicitWidth: 16; implicitHeight: 16
        Accessible.name: "Close summary"; onClicked: card.closeRequested()
        HoverHandler { id: closeHover; cursorShape: Qt.PointingHandCursor }
        contentItem: Item {}
        FmIcon { anchors.centerIn: parent; name: "close"; size: 10; color: closeHover.hovered ? Theme.text : Theme.withAlpha(Theme.ai, .7) }
    }
    ColumnLayout {
        id: content; x: 16; y: 12; width: Math.max(0, parent.width-32); spacing: 8; visible: !card.failed
        RowLayout {
            Layout.fillWidth: true; spacing: 12
            // The tag leads the first line; the text wraps back under it.
            Item {
                Layout.fillWidth: true; Layout.alignment: Qt.AlignTop
                implicitHeight: card.busy ? Math.max(17, busyLine.implicitHeight) : summaryBody.implicitHeight
                readonly property real firstLine: Theme.fontSize(12.5) * Theme.naturalLeading * 1.6
                AbstractButton {
                    id: tag; objectName: "summaryActions"
                    y: card.busy ? (parent.implicitHeight - height) / 2 : Math.max(0, (parent.firstLine - height) / 2)
                    implicitWidth: tagText.implicitWidth + 12; implicitHeight: 17
                    Accessible.name: card.busy ? "Summary progress and cancel" : "Summary sources and details"
                    onClicked: actions.open()
                    HoverHandler { cursorShape: Qt.PointingHandCursor }
                    background: Rectangle { radius: 4; color: card.busy ? Theme.inkAlpha(.1) : Theme.withAlpha(card.done ? card.doneGreen : Theme.ai, tag.hovered ? .26 : .16) }
                    contentItem: Item {}
                    Text {
                        id: tagText; anchors.centerIn: parent; text: "TLDR"
                        color: card.busy ? Theme.inkAlpha(.6) : card.done ? card.doneGreen : Theme.ai
                        font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(10.5); font.weight: Font.Medium; font.letterSpacing: Theme.fontSize(10.5) * .08
                    }
                }
                RowLayout {
                    id: busyLine; visible: card.busy
                    x: tag.width + 10; width: Math.max(0, parent.width - x); anchors.verticalCenter: tag.verticalCenter; spacing: 12
                    Text { text: card.stageText; color: Theme.inkAlpha(.7); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12.5); Layout.fillWidth: true; elide: Text.ElideRight }
                    Text { text: card.elapsedSeconds + " s"; color: Theme.inkAlpha(.4); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(11); font.features: {"tnum":1} }
                }
                TextEdit {
                    id: summaryBody; objectName: "summaryBody"
                    visible: card.summaryText.length > 0 && !card.busy
                    width: parent.width
                    readonly property string labelMarkup: card.label.length ? "<span style=\"color:" + Theme.withAlpha(Theme.ai, .7) + "\">#" + card.escaped(card.label.toLowerCase()) + "</span>&nbsp;&nbsp;" : ""
                    text: "<p style=\"white-space:pre-wrap;line-height:" + Theme.leadingPercent(1.6) + "%;margin:0;text-indent:" + (tag.width + 10) + "px\">" + labelMarkup + card.escaped(card.summaryText).replace(/\n/g, "<br>") + "</p>"
                    textFormat: TextEdit.RichText; wrapMode: TextEdit.Wrap; readOnly: true; selectByMouse: true
                    color: Theme.ai; selectionColor: Theme.selection; selectedTextColor: Theme.selectedText
                    font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12.5)
                    Accessible.name: card.title
                    ContextMenu.menu: null
                    TextMenuHandler {}
                    HoverHandler { cursorShape: Qt.IBeamCursor }
                }
            }
            FmIcon { objectName: "summaryDone"; visible: card.done && !card.busy; Layout.alignment: Qt.AlignTop; Layout.topMargin: 5; name: "check"; size: 11; color: card.doneGreen }
            CloseMark { visible: card.closable && !card.busy; Layout.topMargin: 3 }
        }
        Rectangle {
            objectName: "summaryProgress"; visible: card.busy; Layout.fillWidth: true; implicitHeight: 2; radius: 1
            color: Theme.inkAlpha(.1); clip: true
            Rectangle {
                width: parent.width*.3; height: 2; radius: 1; color: Theme.withAlpha(Theme.ai, .7)
                SequentialAnimation on x { running: card.busy && card.visible; loops: Animation.Infinite
                    NumberAnimation { from: -card.width*.3; to: card.width; duration: 1400; easing.type: Easing.InOutSine }
                }
            }
        }
        Text {
            objectName: "summaryCoverage"; visible: !card.busy && !!card.result && (card.result.stale || card.result.partial)
            Layout.fillWidth: true; wrapMode: Text.Wrap; color: Theme.withAlpha(Theme.ai, .6); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(11)
            text: [card.result && card.result.stale ? "Out of date · Resummarize to update" : "", card.result && card.result.partial ? "Partial · Some conversation text was omitted" : ""].filter(Boolean).join(" · ")
        }
    }
    onDetailsChanged: { if (details) sourcePopup.open(); else sourcePopup.close() }
    Popup {
        id: sourcePopup
        x: Math.max(0, card.width - width); y: 38
        width: Math.min(card.width, 320); padding: 12
        onClosed: card.details = false
        closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
        background: Rectangle { color: Theme.surface; radius: 8; border.color: Theme.controlBorder }
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
                    color: Theme.subtle; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(10.5)
                }
                Repeater {
                    model: card.showSources && card.details ? card.sources : []
                    FmButton {
                        required property string modelData
                        readonly property var member: card.members.find(m => m.id === modelData) || {}
                        objectName: "summarySource_" + modelData
                        text: (member.sender_name || member.sender_email || "Source message") + (member.sent_at ? " · " + Qt.formatDateTime(new Date(member.sent_at), "d MMM, HH:mm") : "")
                        Layout.fillWidth: true; textAlignment: Text.AlignLeft; leftPadding: 0
                        font.pointSize: Theme.fontPoints(10.5); foreground: Theme.ai
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
        background: Rectangle { color: Theme.surface; radius: 8; border.color: Theme.controlBorder }
        contentItem: ColumnLayout {
            spacing: 2
            FmButton {
                objectName: "summaryCancel"; visible: card.busy
                Layout.fillWidth: true; textAlignment: Text.AlignLeft; text: "Cancel summarization"
                onClicked: { actions.close(); card.cancelRequested() }
            }
            FmButton {
                objectName: "summaryDetails"; Layout.fillWidth: true; textAlignment: Text.AlignLeft
                text: card.details ? "Hide sources and details" : "Sources and details"
                visible: !!card.result && !card.busy; onClicked: { actions.close(); card.details = !card.details }
            }
        }
    }
}
