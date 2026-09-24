import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

// The account tab's hover card: who the account is, what is waiting for
// review, and where its download and local model stages stand. It takes no
// input, so the pointer passes through to the tab underneath.
Rectangle {
    id: card
    objectName: "accountHoverCard"
    property var progress: null
    property string account: ""
    property var row: ({})
    readonly property var pipeline: progress && account.length ? progress.view(account) : ({})
    function show(accountRow, left, top) {
        row = accountRow; account = accountRow.id
        x = Math.max(8, Math.min(left, (parent ? parent.width : 1000) - width - 8)); y = top
        showTimer.restart()
    }
    function close() { showTimer.stop(); account = "" }
    Timer { id: showTimer; interval: 250 }
    visible: account.length > 0 && !showTimer.running && !!progress
    width: 300; height: content.implicitHeight + 26
    z: 60; radius: 8; color: Theme.bg2; border.color: Theme.line2
    component StageRow: ColumnLayout {
        id: stageRow
        property var stage: ({})
        property string iconName: ""
        property color iconColor: Theme.faint
        property bool twoTone: false
        Layout.fillWidth: true; spacing: 6
        RowLayout {
            Layout.fillWidth: true; spacing: 7
            FmIcon { name: stageRow.iconName; size: 12; color: stageRow.iconColor }
            Text { text: stageRow.stage.label || ""; color: Theme.text; font { family: Theme.fontFamily; pointSize: Theme.fontPoints(12.5); weight: Font.Medium } }
            Item { Layout.fillWidth: true }
            Text { text: stageRow.stage.count || ""; color: Theme.inkAlpha(.55); font { family: Theme.fontFamily; pointSize: Theme.fontPoints(12.5); features: {"tnum": 1} } }
        }
        Rectangle {
            Layout.fillWidth: true; implicitHeight: 3; radius: 2; color: Theme.inkAlpha(.07); clip: true
            Rectangle { width: parent.width * Math.min(1, stageRow.stage.frac || 0); height: 3; color: stageRow.stage.fill || Theme.ai }
            Rectangle {
                visible: stageRow.twoTone
                x: parent.width * Math.min(1, stageRow.stage.frac || 0)
                width: parent.width * Math.min(1, stageRow.stage.qfrac || 0); height: 3; color: Theme.withAlpha(Theme.ai, .28)
            }
        }
        Text { text: stageRow.stage.status || ""; Layout.fillWidth: true; elide: Text.ElideRight; color: stageRow.stage.live ? Theme.inkAlpha(.78) : Theme.inkAlpha(.5); font { family: Theme.fontFamily; pointSize: Theme.fontPoints(12) } }
    }
    ColumnLayout {
        id: content
        x: 14; y: 12; width: parent.width - 28; spacing: 12
        RowLayout {
            Layout.fillWidth: true; spacing: 12
            ColumnLayout {
                Layout.fillWidth: true; spacing: 2
                Text { text: card.row.name || ""; Layout.fillWidth: true; elide: Text.ElideRight; color: Theme.text; font { family: Theme.fontFamily; pointSize: Theme.fontPoints(13); weight: Font.Medium } }
                Text { text: (card.row.email || "") + (card.pipeline.err ? " · disconnected" : ""); Layout.fillWidth: true; elide: Text.ElideRight; color: Theme.inkAlpha(.5); font { family: Theme.fontFamily; pointSize: Theme.fontPoints(12) } }
            }
            Text {
                readonly property int review: card.row.unread_conversation_count || 0
                readonly property int held: card.row.held_conversation_count || 0
                Layout.alignment: Qt.AlignTop
                text: (review ? review + " to review" : "All reviewed") + (held ? " · " + held + " delayed" : "")
                color: Theme.inkAlpha(.5); font { family: Theme.fontFamily; pointSize: Theme.fontPoints(12) }
            }
        }
        Rectangle { Layout.fillWidth: true; Layout.topMargin: -2; implicitHeight: 1; color: Theme.line }
        StageRow { stage: card.pipeline.mail || ({}); iconName: "fetch"; iconColor: card.pipeline.err ? Theme.error : (card.pipeline.mail || {}).live ? Theme.amber : Theme.faint }
        StageRow { stage: card.pipeline.ai || ({}); iconName: "summary"; twoTone: true; iconColor: (card.pipeline.ai || {}).live ? Theme.ai : Theme.withAlpha(Theme.ai, .55) }
    }
}
