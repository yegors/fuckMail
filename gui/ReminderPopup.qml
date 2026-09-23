import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Effects

Item {
    id: panel
    signal openEventRequested(var event)
    property Item backgroundSource
    readonly property var event: backend.reminders.length ? backend.reminders[0] : ({})
    readonly property string eventIdentity: [event.account || "", event.calendar_id || "", event.id || "", event.start || ""].join("|")
    property string presentedIdentity: ""
    property int soundedLevel: -1
    property double nowMs: Date.now()
    property bool muted: false
    readonly property double startMs: Date.parse(event.due_at || event.start || "")
    readonly property int secondsLeft: isNaN(startMs) ? 0 : Math.ceil((startMs - nowMs) / 1000)
    readonly property int level: secondsLeft <= 0 ? 2 : secondsLeft <= 60 ? 1 : 0
    readonly property color alertColor: level > 0 ? "#e0645a" : Theme.accent
    readonly property string provider: /meet\.google\.com/i.test(event.conference_url || "") ? "Google Meet" : /zoom\.us/i.test(event.conference_url || "") ? "Zoom" : /teams\.microsoft\.com/i.test(event.conference_url || "") ? "Teams" : "meeting"
    function elapsed() {
        const value = Math.abs(secondsLeft), h = Math.floor(value / 3600), m = Math.floor((value % 3600) / 60), s = value % 60
        return (secondsLeft <= 0 ? "+" : "") + (h ? h + ":" + String(m).padStart(2,"0") : String(m)) + ":" + String(s).padStart(2,"0")
    }
    function primary() {
        if (event.conference_url) backend.joinCalendarEvent(event)
        else { backend.dismissCalendarReminder(event); openEventRequested(event) }
    }
    function play() { if (!muted && backend.playReminderSound) backend.playReminderSound(level) }
    function reveal() {
        if (!visible || !event.account || presentedIdentity === eventIdentity) return
        nowMs = Date.now(); presentedIdentity = eventIdentity; muted = false; forceActiveFocus()
        soundedLevel = level; play()
    }
    onVisibleChanged: { if (visible) reveal(); else { backend.stopReminderSound(); presentedIdentity = ""; soundedLevel = -1 } }
    onMutedChanged: if (muted) backend.stopReminderSound()
    onEventIdentityChanged: reveal()
    onLevelChanged: { if (visible && presentedIdentity === eventIdentity && level > soundedLevel) { soundedLevel = level; play() } }
    Keys.onReturnPressed: primary()
    Keys.onEnterPressed: primary()
    Keys.onEscapePressed: backend.dismissCalendarReminder(event)
    Keys.onPressed: function(key) {
        if (key.key === Qt.Key_S) { backend.snoozeCalendarReminder(event); key.accepted = true }
        if (key.key === Qt.Key_M) { muted = !muted; key.accepted = true }
    }
    Timer { interval: 1000; repeat: true; running: panel.visible; onTriggered: panel.nowMs = Date.now() }
    Timer { interval: panel.level === 2 && panel.secondsLeft > -30 ? 2600 : 8000; repeat: true; running: panel.visible && !panel.muted && panel.level > 0; onTriggered: panel.play() }
    MultiEffect { anchors.fill: parent; source: panel.backgroundSource; blurEnabled: true; blur: 1; blurMax: 16 }
    Rectangle { anchors.fill: parent; color: Qt.rgba(Theme.canvas.r,Theme.canvas.g,Theme.canvas.b,0.94); MouseArea { anchors.fill: parent; onClicked: panel.forceActiveFocus() } }
    Rectangle { anchors.fill: parent; color: "transparent"; border.width: panel.level > 0 ? 2 : 0; border.color: Theme.withAlpha(panel.alertColor,.55) }
    Rectangle { id: redBar; anchors.top: parent.top; width: parent.width; height: panel.level > 0 ? 3 : 0; color: panel.alertColor; opacity: redBar.on ? 1 : 0.25
        property bool on: true
        Timer { interval: 500; repeat: true; running: panel.visible && panel.level > 0; onTriggered: redBar.on = !redBar.on }
    }
    Flickable {
        id: viewport
        anchors.fill: parent; anchors.margins: 24; anchors.topMargin:48; contentWidth: width; contentHeight: Math.max(height, body.implicitHeight)
        clip: true; boundsBehavior: Flickable.StopAtBounds
        ColumnLayout {
            id: body
            width: Math.min(viewport.width, 560); x: Math.max(0, (viewport.width - width) / 2)
            y: Math.max(0, (viewport.height - implicitHeight) / 2)
            spacing: 0
            RowLayout { Layout.fillWidth: true; spacing: 10
                Rectangle { id: statusDot; Layout.preferredWidth: 8; Layout.preferredHeight: 8; radius: 4; color: panel.alertColor; opacity: statusDot.on ? 1 : 0.45
                    property bool on: true
                    Timer { interval: 600; repeat: true; running: panel.visible && panel.level > 0; onTriggered: statusDot.on = !statusDot.on }
                }
                Text { text: panel.level === 0 ? "REMINDER" : panel.level === 1 ? "STARTING NOW" : "STARTED"; color: panel.alertColor; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12); font.letterSpacing:.96; font.weight: Font.Normal }
                Item { Layout.fillWidth: true }
                Text {font.family:Theme.fontFamily; text: (panel.event.account_name || "") + " / " + (panel.event.calendar_name || ""); color: Theme.muted; font.pointSize: Theme.fontPoints(12.5); elide: Text.ElideRight; Layout.fillWidth:true; horizontalAlignment:Text.AlignRight; textFormat: Text.PlainText }
                FmButton { objectName: "reminderMute"; iconName: panel.muted ? "mute" : "volume"; tip: panel.muted ? "Sound off · M" : "Sound on · M";implicitWidth:28;implicitHeight:28;leftPadding:7;rightPadding:7;outlined:true; onClicked: panel.muted = !panel.muted }
            }
            Text {font.family:Theme.fontFamily; Layout.topMargin: 36; text: panel.secondsLeft <= 0 ? "Started " + panel.elapsed().slice(1) + " ago" : "Starts in"; color: Theme.muted; font.pointSize: Theme.fontPoints(13) }
            Text { Layout.topMargin:4;objectName: "reminderCountdown"; text: panel.elapsed(); color: panel.level > 0 ? panel.alertColor : Theme.text; font.family: Theme.fontFamily; font.pixelSize: Math.min(96, body.width / 4); font.weight: Font.Medium; font.letterSpacing:-4.32;lineHeightMode:Text.FixedHeight;lineHeight:font.pixelSize; font.features: { "tnum": 1 } }
            Text { Layout.topMargin: 32; Layout.fillWidth: true; text: panel.event.title || "Event"; textFormat: Text.PlainText; wrapMode: Text.Wrap; color: Theme.text; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(30); font.weight: Font.Medium; font.letterSpacing:-.6;lineHeightMode:Text.FixedHeight;lineHeight:34.5 }
            Text {font.family:Theme.fontFamily; Layout.topMargin: 8; Layout.fillWidth: true; text: backend.calendarReminderWhen(panel.event); textFormat: Text.PlainText; color: Theme.muted; font.pointSize: Theme.fontPoints(14); wrapMode: Text.Wrap }
            ColumnLayout {
                Layout.fillWidth:true;Layout.topMargin:24;spacing:0
                readonly property var rows:[{label:"Where",value:panel.event.location},{label:"Join",value:panel.event.conference_url},{label:"With",value:(panel.event.attendees || []).map(person=>person.displayName || person.email || "").filter(Boolean).join(", ")}].filter(row=>!!row.value)
                Rectangle {visible:parent.rows.length>0;Layout.fillWidth:true;implicitHeight:1;color:Theme.line}
                Repeater {
                    model:parent.rows
                    ColumnLayout {
                        required property var modelData
                        Layout.fillWidth:true;spacing:0
                        RowLayout {
                            Layout.fillWidth:true;Layout.topMargin:11;Layout.bottomMargin:11;spacing:0
                            Text {font.family:Theme.fontFamily;text:modelData.label;color:Theme.inkAlpha(.45);font.pointSize:Theme.fontPoints(12.5);Layout.preferredWidth:64}
                            Text {font.family:Theme.fontFamily;text:modelData.value;textFormat:Text.PlainText;color:Theme.text;font.pointSize:Theme.fontPoints(13.5);elide:Text.ElideRight;Layout.fillWidth:true}
                        }
                        Rectangle {Layout.fillWidth:true;implicitHeight:1;color:Theme.line}
                    }
                }
            }
            Flow {
                id:reminderActions;Layout.fillWidth:true;Layout.topMargin:32;spacing:8
                FmButton {
                    id:primaryButton;objectName:"reminderPrimary";text:panel.event.conference_url ? "Join "+panel.provider : "Open event";shortcutHint:"↵"
                    prominent:true;foreground:panel.level>0 ? "white" : Theme.canvas;implicitHeight:36;leftPadding:14;rightPadding:14;font.pointSize:Theme.fontPoints(13);font.weight:Font.Medium
                    onClicked:panel.primary()
                    background:Rectangle {radius:6;color:panel.level>0 ? panel.alertColor : Theme.text}
                }
                FmButton {id:snoozeButton;objectName:"reminderSnooze";text:"Snooze "+(backend.preferences.reminder_snooze_minutes || 5)+" min";shortcutHint:"S";implicitHeight:36;outlined:true;leftPadding:14;rightPadding:14;font.weight:Font.Medium;onClicked:backend.snoozeCalendarReminder(panel.event)}
                Item {width:Math.max(0,reminderActions.width-primaryButton.width-snoozeButton.width-dismissButton.width-24);height:36}
                FmButton {id:dismissButton;objectName:"reminderDismiss";text:"Dismiss";shortcutHint:"Esc";implicitHeight:36;outlined:true;leftPadding:14;rightPadding:14;font.weight:Font.Medium;onClicked:backend.dismissCalendarReminder(panel.event)}
            }
        }
    }
}
