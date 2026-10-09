import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Effects

// The account tab's hover card: who the account is, what is waiting for
// review, and where its download and local model stages stand, as a
// label column (inbox · mail · model) beside the values. It takes no
// input, so the pointer passes through to the tab underneath.
Rectangle {
    id: card
    objectName: "accountHoverCard"
    property var progress: null
    property string account: ""
    property var row: ({})
    readonly property var pipeline: progress && account.length ? progress.view(account, true) : ({})
    function show(accountRow, left, top) {
        row = accountRow; account = accountRow.id
        x = Math.max(8, Math.min(left, (parent ? parent.width : 1000) - width - 8)); y = top
        showTimer.restart()
    }
    function close() { showTimer.stop(); account = "" }
    // "about 9 min left" reads "~9m left" here.
    function terse(text) { return String(text || "").replace(/about /g, "~").replace(/ min left/, "m left").replace(/ h left/, "h left") }
    Timer { id: showTimer; interval: 250 }
    visible: account.length > 0 && !showTimer.running && !!progress
    width: 384; height: content.implicitHeight + 24
    z: 60; radius: 8; color: Theme.bg2; border.color: Theme.line2
    layer.enabled: visible && GraphicsInfo.api !== GraphicsInfo.Software
    layer.effect: MultiEffect { shadowEnabled: true; shadowColor: Theme.shadowAlpha(.55); shadowBlur: .8; shadowVerticalOffset: 18 }
    component Caption: Text {
        Layout.preferredWidth: 48; Layout.alignment: Qt.AlignTop
        lineHeightMode: Text.FixedHeight; lineHeight: 16
        color: Theme.inkAlpha(.5); font { family: Theme.fontFamily; pointSize: Theme.fontPoints(11) }
    }
    component StageRow: ColumnLayout {
        id: stageRow
        property var stage: ({})
        property color tone: Theme.faint
        property bool twoTone: false
        // While the stage works, its activity word pulses.
        property real pulse: 1
        Layout.fillWidth: true; spacing: 5
        SequentialAnimation on pulse {
            running: !!stageRow.stage.live && card.visible; loops: Animation.Infinite
            NumberAnimation { to: .3; duration: 700; easing.type: Easing.InOutSine }
            NumberAnimation { to: 1; duration: 700; easing.type: Easing.InOutSine }
            onRunningChanged: if (!running) stageRow.pulse = 1
        }
        RowLayout {
            Layout.fillWidth: true; spacing: 12
            Text { text: stageRow.stage.count || ""; color: Theme.text; font { family: Theme.fontFamily; pointSize: Theme.fontPoints(12); features: {"tnum": 1} } }
            Text {
                Layout.fillWidth: true; horizontalAlignment: Text.AlignRight; elide: Text.ElideRight
                text: String((stageRow.stage.live ? stageRow.stage.label : stageRow.stage.short) || "").toLowerCase()
                color: stageRow.tone; opacity: stageRow.pulse
                font { family: Theme.fontFamily; pointSize: Theme.fontPoints(11) }
            }
        }
        Rectangle {
            Layout.fillWidth: true; implicitHeight: 2; color: Theme.inkAlpha(.08); clip: true
            Rectangle { width: parent.width * Math.min(1, stageRow.stage.frac || 0); height: 2; color: stageRow.stage.fill || Theme.ai }
            Rectangle {
                visible: stageRow.twoTone
                x: parent.width * Math.min(1, stageRow.stage.frac || 0)
                width: parent.width * Math.min(1, stageRow.stage.qfrac || 0); height: 2; color: Theme.withAlpha(Theme.ai, .28)
            }
        }
        Text {
            text: card.terse(stageRow.stage.status); Layout.fillWidth: true; elide: Text.ElideRight
            lineHeightMode: Text.FixedHeight; lineHeight: 16
            color: stageRow.stage.live ? Theme.inkAlpha(.78) : Theme.inkAlpha(.5); font { family: Theme.fontFamily; pointSize: Theme.fontPoints(11) }
        }
    }
    ColumnLayout {
        id: content
        x: 14; y: 12; width: parent.width - 28; spacing: 0
        Text { text: String(card.row.name || "").toLowerCase(); Layout.fillWidth: true; elide: Text.ElideRight; color: Theme.text; font { family: Theme.fontFamily; pointSize: Theme.fontPoints(12) } }
        Text { Layout.topMargin: 2; text: (card.row.email || "") + (card.pipeline.err ? " · disconnected" : ""); Layout.fillWidth: true; elide: Text.ElideRight; color: Theme.inkAlpha(.5); font { family: Theme.fontFamily; pointSize: Theme.fontPoints(11) } }
        Rectangle { Layout.fillWidth: true; Layout.topMargin: 10; Layout.bottomMargin: 12; implicitHeight: 1; color: Theme.line }
        GridLayout {
            Layout.fillWidth: true; columns: 2; columnSpacing: 12; rowSpacing: 14
            Caption { text: "inbox" }
            Text {
                readonly property int review: (card.row.inbox_review_count ?? card.row.unread_conversation_count) || 0
                readonly property int held: (card.row.inbox_held_count ?? card.row.held_conversation_count) || 0
                Layout.fillWidth: true; elide: Text.ElideRight
                lineHeightMode: Text.FixedHeight; lineHeight: 16
                text: (review ? review + " to review" : "All reviewed") + (held ? " · " + held + " delayed" : "")
                color: Theme.text; font { family: Theme.fontFamily; pointSize: Theme.fontPoints(12) }
            }
            Caption { text: "mail" }
            StageRow { stage: card.pipeline.mail || ({}); tone: card.pipeline.err ? Theme.error : (card.pipeline.mail || {}).live ? Theme.amber : Theme.faint }
            Caption { text: "model" }
            StageRow { stage: card.pipeline.ai || ({}); twoTone: true; tone: (card.pipeline.ai || {}).live ? Theme.ai : Theme.withAlpha(Theme.ai, .55) }
        }
    }
}
