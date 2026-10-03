import QtQuick
import QtQuick.Controls
import QtQuick.Shapes
import QtQuick.Layouts

// A message's local summary: absent until requested (the header's summarize
// action starts it), a neutral progress box while running, the summary in AI
// colours, or a red failure box with Try again. A fresh summary arrives green
// with "Done" and settles to the AI colours.
Rectangle {
    id: card
    property string messageId: ""
    property string modelName: ""
    property var result: null
    property var members: []
    property string title: "Summary"
    property bool busy: false
    property bool canSummarize: false
    property bool showSources: true
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
    readonly property string generatedAt: result && result.created_at ? Qt.formatDateTime(new Date(result.created_at), "MMMM d yyyy HH:mm") : ""
    readonly property bool failed: !busy && error.length > 0
    readonly property bool showing: busy || failed || summaryText.length > 0
    // "Done" follows only a summary this card saw being written, never navigation.
    property bool done: false
    property string startIdentity: ""
    property bool armed: false
    // Try again is its own button, so the engine's closing advice is dropped.
    readonly property string failureReason: error.replace(/\s*(Please )?try again\.?\s*$/i, "")
    readonly property string failureText: /tim(e|ed) ?out|respond|stopped/i.test(error) || !failureReason.length
        ? "Couldn’t summarize — " + (modelName || "the local model") + " stopped responding."
        : "Couldn’t summarize — " + failureReason.charAt(0).toLowerCase() + failureReason.slice(1)
    signal regenerate()
    signal cancelRequested()
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
    color: busy ? Theme.inkAlpha(.03) : failed ? Theme.withAlpha(Theme.red, .07) : done ? Qt.rgba(111/255, 191/255, 138/255, .08) : Theme.withAlpha(Theme.ai, .07)
    border.width: busy ? 0 : 1
    border.color: failed ? Theme.withAlpha(Theme.red, .35) : done ? Qt.rgba(111/255, 191/255, 138/255, .55) : Theme.withAlpha(Theme.ai, .16)
    Behavior on color { enabled: card.fading; ColorAnimation { duration: 600; easing.type: Easing.InOutQuad } }
    Behavior on border.color { enabled: card.fading; ColorAnimation { duration: 600; easing.type: Easing.InOutQuad } }
    Shape {
        anchors.fill: parent; visible: card.busy
        ShapePath {
            strokeColor: Theme.inkAlpha(.18); strokeWidth: 1
            strokeStyle: ShapePath.DashLine; dashPattern: [3,3]; fillColor: "transparent"
            PathSvg { path: "M 8 .5 H " + (card.width-8) + " Q " + (card.width-.5) + " .5 " + (card.width-.5) + " 8 V " + (card.height-8) + " Q " + (card.width-.5) + " " + (card.height-.5) + " " + (card.width-8) + " " + (card.height-.5) + " H 8 Q .5 " + (card.height-.5) + " .5 " + (card.height-8) + " V 8 Q .5 .5 8 .5 Z" }
        }
    }
    RowLayout {
        id: failure; x: 14; y: 12; width: Math.max(0, parent.width-28); spacing: 12; visible: card.failed
        Shape {
            implicitWidth: 13; implicitHeight: 13; Layout.alignment: Qt.AlignVCenter
            ShapePath {
                strokeColor: Theme.red; strokeWidth: 1.3; fillColor: "transparent"; capStyle: ShapePath.RoundCap
                PathAngleArc { centerX: 6.5; centerY: 6.5; radiusX: 5; radiusY: 5; startAngle: 0; sweepAngle: 360 }
            }
            ShapePath {
                strokeColor: Theme.red; strokeWidth: 1.3; fillColor: "transparent"; capStyle: ShapePath.RoundCap
                PathSvg { path: "M 6.5 4 V 7 M 6.5 9.2 V 9.3" }
            }
        }
        Text { objectName: "summaryError"; text: card.failureText; Layout.fillWidth: true; wrapMode: Text.Wrap; color: Theme.text; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(13); textFormat: Text.PlainText }
        AbstractButton {
            objectName: "summaryRetry"; enabled: card.canSummarize; Layout.alignment: Qt.AlignVCenter
            implicitWidth: retryText.implicitWidth; implicitHeight: retryText.implicitHeight + 2
            Accessible.name: "Try again"; onClicked: card.regenerate()
            HoverHandler { cursorShape: Qt.PointingHandCursor }
            contentItem: Item {}
            Text { id: retryText; text: "Try again"; color: Theme.red; opacity: parent.enabled ? 1 : .5; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12.5) }
            Rectangle { y: retryText.implicitHeight + 1; width: retryText.implicitWidth; height: 1; color: Theme.withAlpha(Theme.red, .4) }
        }
    }
    ColumnLayout {
        id: content; x: 14; y: 12; width: Math.max(0, parent.width-28); spacing: card.busy ? 7 : 6; visible: !card.failed
        RowLayout {
            visible: card.busy; Layout.fillWidth: true; spacing: 12
            Text { text: card.elapsedSeconds < 3 ? "Reading the message…" : "Writing the summary…"; color: Theme.inkAlpha(.7); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(13.5); Layout.fillWidth: true }
            Text { text: card.elapsedSeconds + " s"; color: Theme.inkAlpha(.4); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(11); font.features: {"tnum":1} }
        }
        Rectangle {
            objectName: "summaryProgress"; visible: card.busy; Layout.fillWidth: true; implicitHeight: 2; radius: 1
            color: Theme.inkAlpha(.1); clip: true
            Rectangle {
                width: parent.width*.3; height: 2; radius: 1; color: Theme.inkAlpha(.55)
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
            ContextMenu.menu: null
            TextMenuHandler {}
            HoverHandler { cursorShape: Qt.IBeamCursor }
        }
        Text {
            objectName: "summaryCoverage"; visible: !card.busy && !!card.result && (card.result.stale || card.result.partial)
            Layout.fillWidth: true; wrapMode: Text.Wrap; color: Theme.muted; font.pointSize: Theme.fontPoints(11)
            text: [card.result && card.result.stale ? "Out of date · Resummarize to update" : "", card.result && card.result.partial ? "Partial · Some conversation text was omitted" : ""].filter(Boolean).join(" · ")
        }
        RowLayout {
            Layout.fillWidth: true; spacing: 6
            FmIcon {
                name: "summary"; size: 10; color: card.busy ? Theme.inkAlpha(.4) : Theme.withAlpha(Theme.ai,.6)
                RotationAnimator on rotation { running: card.busy && card.visible; from: 0; to: 360; duration: 2400; loops: Animation.Infinite }
                onVisibleChanged: if (!card.busy) rotation = 0
            }
            AbstractButton {
                objectName: "summaryActions"; Accessible.name: card.busy ? "Summary progress and cancel" : "Summary actions"; onClicked: actions.open()
                Layout.fillWidth: true; implicitHeight: contentItem.implicitHeight
                contentItem: Text {
                    objectName: "summaryModelName"; elide: Text.ElideRight
                    text: card.busy ? "Summarizing · " + card.modelName + " · on this machine · keep reading" : [card.captionModel, card.generatedAt].filter(Boolean).join(" · ")
                    color: card.busy ? Theme.inkAlpha(.4) : Theme.withAlpha(Theme.ai,.6); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(11); font.features: {"tnum":1}; textFormat: Text.PlainText
                    HoverHandler { id: captionHover }
                    FmToolTip { shown: captionHover.hovered && parent.truncated; text: parent.text }
                }
            }
            Row {
                objectName: "summaryDone"; visible: card.done && !card.busy; spacing: 4
                FmIcon { anchors.verticalCenter: parent.verticalCenter; name: "check"; size: 10; color: Qt.rgba(111/255, 191/255, 138/255, 1) }
                Text { anchors.verticalCenter: parent.verticalCenter; text: "Done"; color: Qt.rgba(111/255, 191/255, 138/255, 1); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(11) }
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
        x: Math.max(0, card.width - width); y: 38
        width: Math.min(card.width, 220); padding: 6
        closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
        background: Rectangle { color: Theme.surface; radius: 6; border.color: Theme.controlBorder }
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
