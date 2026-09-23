import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Popup {
    id: panel
    objectName: "debugConsole"
    required property var hostWindow
    required property var diagnostics
    property bool searchActive: false
    property bool paused: false
    property bool showState: false
    property var previousFocus: null
    property string displayText: ""
    property string copyStatus: ""
    readonly property bool blocksWorkspaceShortcuts: true
    parent: Overlay.overlay
    width: Math.min(1100, parent.width - 40)
    height: Math.min(680, parent.height - 40)
    x: (parent.width - width) / 2
    y: 20
    padding: 18
    modal: true
    focus: true
    font.family: Theme.fontFamily
    closePolicy: Popup.CloseOnEscape
    function typing(item) {
        while (item) {
            if ((item.cursorPosition !== undefined && item.readOnly !== true) || item.editText !== undefined) return true
            item = item.parent
        }
        return false
    }
    function toggle() {
        if (opened) close()
        else { previousFocus = hostWindow.activeFocusItem; open() }
    }
    function updateView() {
        if (!diagnostics || !opened || paused) return
        const atEnd = scroll.contentItem.contentY >= scroll.contentItem.contentHeight - scroll.height - 24
        displayText = showState ? diagnostics.stateText : diagnostics.events(level.currentText, filter.text)
        if (!showState && atEnd) Qt.callLater(function() {
            scroll.contentItem.contentY = Math.max(0, scroll.contentItem.contentHeight - scroll.height)
        })
    }
    onOpened: { diagnostics.refresh(); updateView(); output.forceActiveFocus() }
    onClosed: if (previousFocus && previousFocus.visible) previousFocus.forceActiveFocus()
    onPausedChanged: if (!paused) updateView()
    onShowStateChanged: { paused = false; updateView(); scroll.contentItem.contentY = 0 }
    Connections { target: panel.diagnostics; function onChanged() { panel.updateView() } }
    Shortcut {
        sequences: ["`", "~", "Shift+~", "Shift+`"]
        autoRepeat: false
        enabled: panel.diagnostics !== null && !panel.searchActive && !panel.typing(panel.hostWindow.activeFocusItem)
        onActivated: panel.toggle()
    }
    background: Rectangle { color: Theme.bg2; radius: 8; border.color: Theme.line2 }
    Overlay.modal: Rectangle { color: Theme.shadowAlpha(0.45) }
    contentItem: ColumnLayout {
        spacing: 10
        RowLayout {
            Layout.fillWidth: true
            Label { text: "Debug console"; color: Theme.text; font.pointSize: Theme.fontPoints(18); font.weight: Font.Medium }
            Item { Layout.fillWidth: true }
            Label { text: panel.copyStatus; color: Theme.muted; font.pointSize: Theme.fontPoints(12) }
            FmButton { objectName: "debugCopy"; text: "Copy report"; onClicked: { panel.diagnostics.refresh(); panel.diagnostics.copyReport(); panel.copyStatus = "Copied full report"; copiedTimer.restart() } }
            FmButton { text: "Close  ~"; onClicked: panel.close() }
        }
        Label {
            Layout.fillWidth: true
            text: "This session · " + (panel.diagnostics ? panel.diagnostics.count : 0) + " / 1000 events · " + (panel.diagnostics ? panel.diagnostics.dropped : 0) + " older events evicted · account aliases, no request payloads"
            color: Theme.muted; font.pointSize: Theme.fontPoints(12); wrapMode: Text.Wrap
        }
        RowLayout {
            Layout.fillWidth: true
            FmButton { text: "Events"; prominent: !panel.showState; onClicked: panel.showState = false }
            FmButton { text: "Runtime state"; prominent: panel.showState; onClicked: panel.showState = true }
            FmComboBox {
                id: level; objectName: "debugLevel"; visible: !panel.showState
                model: ["All", "Warnings + errors", "ERROR", "WARN", "INFO", "DEBUG"]
                Layout.preferredWidth: 175
                onCurrentTextChanged: panel.updateView()
                Accessible.name: "Event severity"
            }
            FmTextField {
                id: filter; objectName: "debugFilter"; visible: !panel.showState
                Layout.fillWidth: true; placeholderText: "Filter events…"
                onTextChanged: panel.updateView()
                Accessible.name: "Filter debug events"
            }
            Item { visible: panel.showState; Layout.fillWidth: true }
            FmButton { objectName: "debugPause"; text: panel.paused ? "Resume view" : "Pause view"; onClicked: panel.paused = !panel.paused }
            FmButton { text: "Clear"; onClicked: { panel.paused = false; panel.diagnostics.clear(); panel.updateView() } }
        }
        ScrollView {
            id: scroll
            Layout.fillWidth: true; Layout.fillHeight: true
            clip: true
            ScrollBar.vertical: FmScrollBar { x: scroll.width - width; height: scroll.height }
            TextArea {
                id: output; objectName: "debugOutput"
                text: panel.displayText
                readOnly: true; selectByMouse: true; textFormat: TextEdit.PlainText
                wrapMode: TextEdit.WrapAnywhere
                rightPadding: 20
                font.family: Qt.platform.os === "windows" ? "Consolas" : "monospace"
                font.pointSize: Theme.fontPoints(12)
                color: Theme.text; selectionColor: Theme.focusBorder
                background: Rectangle { color: Theme.canvas; radius: 4 }
                Accessible.name: panel.showState ? "Runtime application state" : "Runtime events"
            }
        }
        Label {
            text: panel.paused ? "View paused. Capture continues; Copy report includes current state and all retained events." : "Copy report includes current state and all retained events, regardless of filters. Esc or ~ closes."
            color: Theme.muted; font.pointSize: Theme.fontPoints(12); wrapMode: Text.Wrap; Layout.fillWidth: true
        }
    }
    Timer { id: copiedTimer; interval: 2500; onTriggered: panel.copyStatus = "" }
}
