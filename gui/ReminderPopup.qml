import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Effects

// One full-screen event screen for due reminders and for View on an event
// card. Viewing never acknowledges, snoozes or plays a sound on opening.
Item {
    id: panel
    // Kept for existing hosts; "Open event" is gone, so nothing emits it.
    // View mode: the host closes the screen.
    signal closeRequested()
    property Item backgroundSource
    property bool viewing: false
    property var viewEvent: ({})
    property bool blocksWorkspaceShortcuts: true
    readonly property var event: viewing ? (viewEvent || ({})) : backend.reminders.length ? backend.reminders[0] : ({})
    readonly property string eventIdentity: [event.account || "", event.calendar_id || "", event.id || "", event.start || ""].join("|")
    property string presentedIdentity: ""
    property int soundedLevel: -1
    property int revealedLevel: -1
    // identity -> highest level already sounded. Showing the same reminder
    // again at that level (after a refresh hid it) stays silent; only Snooze
    // or Dismiss forgets it.
    property var heard: ({})
    // Sounds played since this event was shown.
    property int chimes: 0
    property double nowMs: Date.now()
    property bool muted: false
    // All-day dates are local midnights; timed values carry their offset.
    function instant(value) {
        const text = String(value || ""), day = /^(\d{4})-(\d{2})-(\d{2})$/.exec(text)
        return day ? new Date(Number(day[1]), Number(day[2]) - 1, Number(day[3])).getTime() : Date.parse(text)
    }
    readonly property double startMs: instant(event.due_at || event.start)
    readonly property double endMs: {
        const end = instant(event.end)
        if (event.all_day) return isNaN(end) ? startMs + 86400000 : end
        return Math.max(isNaN(end) ? startMs + (event.duration_minutes || 0) * 60000 : end, startMs + 900000)
    }
    readonly property int secondsLeft: isNaN(startMs) ? 0 : Math.ceil((startMs - nowMs) / 1000)
    readonly property bool ended: !isNaN(startMs) && nowMs >= endMs
    readonly property int level: secondsLeft <= 0 ? 2 : secondsLeft <= 60 ? 1 : 0
    // A viewed event that has started is simply in progress, and an ended
    // event is over: neither is an alarm.
    readonly property bool alerting: level > 0 && !ended && !(viewing && secondsLeft <= 0)
    readonly property color alertColor: alerting ? Theme.error : ended ? Theme.inkAlpha(.45) : Theme.accent
    readonly property bool hasPrimary: !ended && !!event.conference_url
    readonly property bool canSnooze: !viewing && !ended
    readonly property string provider: /meet\.google\.com/i.test(event.conference_url || "") ? "Google Meet" : /zoom(gov)?\.(us|com)/i.test(event.conference_url || "") ? "Zoom" : /teams\.(microsoft|live)\.com/i.test(event.conference_url || "") ? "Teams" : /webex\.com/i.test(event.conference_url || "") ? "Webex" : "meeting"
    readonly property string kicker: ended ? "ENDED" : viewing && secondsLeft <= 0 ? "IN PROGRESS" : level === 1 ? "STARTING NOW" : level === 2 ? "STARTED" : viewing ? "UPCOMING" : "REMINDER"
    // m:ss, h:mm:ss, Nd h:mm:ss, Ny Nd h:mm:ss; a year is 365 days.
    function clock(seconds) {
        const value = Math.abs(seconds), years = Math.floor(value / 31536000), days = Math.floor(value % 31536000 / 86400), rest = value % 86400
        const two = function(n) { return String(n).padStart(2, "0") }
        return (years ? years + "y " : "") + (years || days ? days + "d " : "")
            + (rest >= 3600 || years || days ? Math.floor(rest / 3600) + ":" + two(Math.floor(rest % 3600 / 60)) : Math.floor(rest / 60)) + ":" + two(rest % 60)
    }
    readonly property string countdown: ended ? "Ended" : (secondsLeft <= 0 ? "+" : "") + clock(secondsLeft)
    readonly property int countdownSize: {
        const value = Math.abs(secondsLeft)
        return ended || value < 86400 ? 96 : value < 31536000 ? 80 : 64
    }
    readonly property var rows: [{label: "Where", value: event.location}, {label: "Join", value: event.conference_url},
        {label: "With", value: (event.attendees || []).map(person => person.name || person.displayName || person.email || "").filter(Boolean).join(", ")}].filter(row => !!row.value)
    readonly property string notes: Theme.plainNotes(event.description)
    function primary() {
        if (!hasPrimary) return
        backend.joinCalendarEvent(event)
        if (viewing) closeRequested()
    }
    function forget() { const next = Object.assign({}, heard); delete next[eventIdentity]; heard = next }
    function remember(value) { const next = Object.assign({}, heard); next[eventIdentity] = value; heard = next }
    function snooze() { if (canSnooze) { forget(); backend.snoozeCalendarReminder(event) } }
    function dismiss() { if (viewing) closeRequested(); else { forget(); backend.dismissCalendarReminder(event) } }
    function play() { if (!muted && backend.playReminderSound) { backend.playReminderSound(level); ++chimes } }
    function reveal() {
        if (!visible || !event.account || presentedIdentity === eventIdentity) return
        nowMs = Date.now(); presentedIdentity = eventIdentity; forceActiveFocus()
        revealedLevel = level
        const seen = heard[eventIdentity]
        if (seen !== undefined && level <= seen) { soundedLevel = seen; return }
        muted = false; chimes = 0; soundedLevel = level; remember(level)
        if (!viewing && !ended) play()
    }
    onVisibleChanged: { if (visible) reveal(); else { backend.stopReminderSound(); presentedIdentity = ""; soundedLevel = -1 } }
    onMutedChanged: if (muted) backend.stopReminderSound()
    onEventIdentityChanged: reveal()
    // Viewing sounds only when the one-minute or start point passes while open,
    // and never once the event is more than a minute old.
    onLevelChanged: {
        if (!visible || presentedIdentity !== eventIdentity || level <= soundedLevel) return
        soundedLevel = level; remember(level)
        if (!ended && (!viewing || secondsLeft > -60)) play()
    }
    onEndedChanged: if (ended) backend.stopReminderSound()
    Keys.onReturnPressed: primary()
    Keys.onEnterPressed: primary()
    Keys.onEscapePressed: dismiss()
    Keys.onPressed: function(key) {
        if (key.key === Qt.Key_S && canSnooze) { snooze(); key.accepted = true }
        if (key.key === Qt.Key_M) { muted = !muted; key.accepted = true }
    }
    Timer { interval: 1000; repeat: true; running: panel.visible; onTriggered: panel.nowMs = Date.now() }
    Timer {
        interval: panel.level === 2 && panel.secondsLeft > -30 ? 2600 : 8000; repeat: true
        running: panel.visible && !panel.muted && panel.level > 0 && !panel.ended
            && (!panel.viewing || (panel.soundedLevel > panel.revealedLevel && panel.secondsLeft > -60))
        onTriggered: panel.play()
    }
    MultiEffect {
        // The source may be smaller than this screen, e.g. below a window header.
        readonly property rect area: panel.backgroundSource && panel.width > 0 && panel.height > 0
            ? panel.backgroundSource.mapToItem(panel, 0, 0, panel.backgroundSource.width, panel.backgroundSource.height) : Qt.rect(0, 0, panel.width, panel.height)
        x: area.x; y: area.y; width: area.width; height: area.height
        source: panel.backgroundSource; blurEnabled: true; blur: 1; blurMax: 16
    }
    Rectangle { anchors.fill: parent; color: Qt.rgba(Theme.canvas.r,Theme.canvas.g,Theme.canvas.b,0.94); MouseArea { anchors.fill: parent; onClicked: panel.forceActiveFocus() } }
    Rectangle { anchors.fill: parent; color: "transparent"; border.width: panel.alerting ? 2 : 0; border.color: Theme.withAlpha(panel.alertColor,.55) }
    Rectangle { id: redBar; anchors.top: parent.top; width: parent.width; height: panel.alerting ? 3 : 0; color: panel.alertColor; opacity: redBar.on ? 1 : 0.25
        property bool on: true
        Timer { interval: 500; repeat: true; running: panel.visible && panel.alerting; onTriggered: redBar.on = !redBar.on; onRunningChanged: redBar.on = true }
    }
    Flickable {
        id: viewport
        anchors.fill: parent; anchors.margins: 24; anchors.topMargin:48; contentWidth: width; contentHeight: Math.max(height, body.implicitHeight)
        clip: true; boundsBehavior: Flickable.StopAtBounds
        ColumnLayout {
            id: body
            width: Math.min(viewport.width, 720); x: Math.max(0, (viewport.width - width) / 2)
            y: Math.max(0, (viewport.height - implicitHeight) / 2)
            spacing: 0
            RowLayout { Layout.fillWidth: true; spacing: 10
                Rectangle { id: statusDot; Layout.preferredWidth: 8; Layout.preferredHeight: 8; radius: 4; color: panel.alertColor; opacity: statusDot.on ? 1 : 0.45
                    property bool on: true
                    Timer { interval: 600; repeat: true; running: panel.visible && panel.alerting; onTriggered: statusDot.on = !statusDot.on; onRunningChanged: statusDot.on = true }
                }
                Text { objectName: "reminderKicker"; text: panel.kicker; color: panel.alertColor; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12); font.letterSpacing:.96; font.weight: Font.Normal }
                Item { Layout.fillWidth: true }
                Text {font.family:Theme.fontFamily; text: (panel.event.account_name || "") + " / " + (panel.event.calendar_name || ""); color: Theme.muted; font.pointSize: Theme.fontPoints(12.5); elide: Text.ElideRight; Layout.fillWidth:true; horizontalAlignment:Text.AlignRight; textFormat: Text.PlainText }
                FmButton { objectName: "reminderMute"; iconName: panel.muted ? "mute" : "volume"; tip: panel.muted ? "Sound off · M" : "Sound on · M";implicitWidth:28;implicitHeight:28;leftPadding:7;rightPadding:7;outlined:true; onClicked: panel.muted = !panel.muted }
            }
            Text {font.family:Theme.fontFamily; objectName: "reminderRelative"; Layout.topMargin: 36; text: panel.ended ? "This event is over" : panel.secondsLeft <= 0 ? "Started " + panel.clock(panel.secondsLeft) + " ago" : "Starts in"; color: Theme.muted; font.pointSize: Theme.fontPoints(13) }
            Text { Layout.topMargin:4;objectName: "reminderCountdown"; text: panel.countdown; color: panel.ended ? Theme.inkAlpha(.4) : panel.alerting ? panel.alertColor : Theme.text; font.family: Theme.fontFamily; readonly property real size: Math.min(panel.countdownSize, body.width / 4); font.pixelSize: size; font.weight: Font.Medium; font.letterSpacing: -0.045 * size;lineHeightMode:Text.FixedHeight;lineHeight:size; font.features: { "tnum": 1 } }
            Text { Layout.topMargin: 32; Layout.fillWidth: true; text: panel.event.title || "Event"; textFormat: Text.PlainText; wrapMode: Text.Wrap; color: Theme.text; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(36); font.weight: Font.Medium; font.letterSpacing: -0.02 * Theme.fontSize(36); lineHeightMode:Text.FixedHeight; lineHeight: Math.round(Theme.fontSize(36) * 1.15) }
            Text {font.family:Theme.fontFamily; Layout.topMargin: 10; Layout.fillWidth: true; text: backend.calendarReminderWhen(panel.event); textFormat: Text.PlainText; color: Theme.muted; font.pointSize: Theme.fontPoints(16); wrapMode: Text.Wrap }
            ColumnLayout {
                Layout.fillWidth:true;Layout.topMargin:28;spacing:0
                Rectangle {visible:panel.rows.length>0 || panel.notes.length>0;Layout.fillWidth:true;implicitHeight:1;color:Theme.line}
                Repeater {
                    model:panel.rows
                    ColumnLayout {
                        required property var modelData
                        Layout.fillWidth:true;spacing:0
                        RowLayout {
                            Layout.fillWidth:true;Layout.topMargin:14;Layout.bottomMargin:14;spacing:0
                            Text {font.family:Theme.fontFamily;text:modelData.label;color:Theme.inkAlpha(.45);font.pointSize:Theme.fontPoints(14);Layout.preferredWidth:84}
                            Text {font.family:Theme.fontFamily;text:modelData.value;textFormat:Text.PlainText;color:Theme.text;font.pointSize:Theme.fontPoints(15.5);elide:Text.ElideRight;Layout.fillWidth:true}
                        }
                        Rectangle {Layout.fillWidth:true;implicitHeight:1;color:Theme.line}
                    }
                }
                // The full description, with its own line breaks, scrolling when long.
                RowLayout {
                    objectName: "reminderNotes"; visible: panel.notes.length > 0
                    Layout.fillWidth:true;Layout.topMargin:14;Layout.bottomMargin:14;spacing:0
                    Text {font.family:Theme.fontFamily;text:"Notes";color:Theme.inkAlpha(.45);font.pointSize:Theme.fontPoints(14);Layout.preferredWidth:84;Layout.alignment:Qt.AlignTop}
                    ScrollView {
                        id: notesScroll
                        ScrollBar.vertical: FmScrollBar {}
                        Layout.fillWidth: true; Layout.preferredHeight: Math.min(notesText.implicitHeight, Math.max(360, panel.height * .36))
                        clip: true; contentWidth: availableWidth; ScrollBar.horizontal.policy: ScrollBar.AlwaysOff
                        // Selectable, with its links opening in the browser: meeting
                        // details and dial-in numbers get copied from here.
                        TextEdit {
                            id: notesText; objectName: "reminderNotesText"; width: notesScroll.availableWidth
                            readonly property string notes: panel.notes
                            text: "<div style=\"line-height:165%\">" + Theme.linkedNotes(panel.notes, Theme.text) + "</div>"
                            textFormat: TextEdit.RichText; readOnly: true; selectByMouse: true; wrapMode: TextEdit.Wrap
                            color: Theme.inkAlpha(.82); selectionColor: Theme.selection; selectedTextColor: Theme.selectedText
                            font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(15.5)
                            onLinkActivated: function(link) { backend.openLink(link) }
                            HoverHandler { cursorShape: notesText.hoveredLink ? Qt.PointingHandCursor : Qt.IBeamCursor }
                        }
                    }
                }
            }
            Flow {
                id:reminderActions;Layout.fillWidth:true;Layout.topMargin:32;spacing:8
                FmButton {
                    id:primaryButton;objectName:"reminderPrimary";visible:panel.hasPrimary;text:"Join "+panel.provider;shortcutHint:"↵"
                    prominent:true;foreground:panel.alerting ? "white" : Theme.canvas;implicitHeight:36;leftPadding:14;rightPadding:14;font.pointSize:Theme.fontPoints(13);font.weight:Font.Medium
                    onClicked:panel.primary()
                    background:Rectangle {radius:6;color:panel.alerting ? panel.alertColor : Theme.text}
                }
                FmButton {id:snoozeButton;objectName:"reminderSnooze";visible:panel.canSnooze;text:"Snooze "+(backend.preferences.reminder_snooze_minutes || 5)+" min";shortcutHint:"S";implicitHeight:36;outlined:true;leftPadding:14;rightPadding:14;font.weight:Font.Medium;onClicked:panel.snooze()}
                Item {width:Math.max(0,reminderActions.width-(primaryButton.visible?primaryButton.width+8:0)-(snoozeButton.visible?snoozeButton.width+8:0)-dismissButton.width-8);height:36}
                FmButton {id:dismissButton;objectName:"reminderDismiss";text:panel.viewing || panel.ended ? "Close" : "Dismiss";shortcutHint:"Esc";implicitHeight:36;outlined:true;leftPadding:14;rightPadding:14;font.weight:Font.Medium;onClicked:panel.dismiss()}
            }
        }
    }
}
