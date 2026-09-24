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
    property bool followTail: true
    property bool updating: false
    property var previousFocus: null
    property string selectedScope: "All"
    property string selectedSequence: ""
    property string selectedText: ""
    property string displayText: ""
    property string copyStatus: ""
    readonly property string mono: Qt.platform.os === "windows" ? "Consolas" : "monospace"
    readonly property int rowHeight: Math.ceil(Theme.fontSize(34))
    readonly property bool blocksWorkspaceShortcuts: true
    parent: Overlay.overlay
    width: Math.max(0, parent.width - 64)
    height: Math.max(0, parent.height - 64)
    x: (parent.width - width) / 2
    y: (parent.height - height) / 2
    padding: 0
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
    function tail() { followTail = true; paused = false; settleTail() }
    function settleTail() {
        if (opened && followTail && !showState && !paused) {
            events.forceLayout()
            events.positionViewAtEnd()
        }
    }
    function inspect(row) {
        selectedSequence = row.sequence
        selectedText = row.at + "   " + row.level + " / " + row.scope + " / " + row.source
            + "\n" + row.message + (row.details ? "\n\n" + JSON.stringify(JSON.parse(row.details), null, 2) : "")
    }
    function updateView(reset) {
        if (!diagnostics || !opened || (paused && !reset)) return
        if (showState) { displayText = diagnostics.stateText; return }
        const rows = diagnostics.eventRows(level.currentText, filter.text, selectedScope)
        if (!reset && rows.length === entries.count && (!rows.length
            || (rows[0].sequence === entries.get(0).sequence && rows[rows.length - 1].sequence === entries.get(entries.count - 1).sequence))) return
        displayText = rows.map(row => row.line).join("\n")
        // Anchor an event and its viewport offset across head eviction. New
        // events append to the model; they never replace a text document.
        const oldIndex = events.indexAt(1, events.contentY + 1)
        const anchor = oldIndex >= 0 ? entries.get(oldIndex).sequence : ""
        const item = oldIndex >= 0 ? events.itemAtIndex(oldIndex) : null
        const offset = item ? events.contentY - item.y : 0
        updating = true
        if (reset) { entries.clear(); followTail = true }
        else {
            let remove = 0
            while (remove < entries.count && (!rows.length || Number(entries.get(remove).sequence) < Number(rows[0].sequence))) ++remove
            if (remove) entries.remove(0, remove)
        }
        for (let i = entries.count; i < rows.length; ++i) entries.append(rows[i])
        events.forceLayout()
        if (!followTail && anchor) {
            let index = 0
            while (index < entries.count && entries.get(index).sequence !== anchor) ++index
            // Expired anchors fall back to the oldest retained event.
            index = index < entries.count ? index : 0
            if (entries.count) {
                events.positionViewAtIndex(index, ListView.Beginning)
                const retained = events.itemAtIndex(index)
                if (retained) events.contentY = retained.y + offset
            }
        }
        updating = false
        settleTail()
    }
    onOpened: { followTail = true; paused = false; diagnostics.refresh(); updateView(true); events.forceActiveFocus() }
    onClosed: if (previousFocus && previousFocus.visible) previousFocus.forceActiveFocus()
    onPausedChanged: if (!paused) updateView()
    onShowStateChanged: { paused = false; updateView(true) }
    onSelectedScopeChanged: updateView(true)
    Connections { target: panel.diagnostics; function onChanged() { panel.updateView() } }
    Shortcut {
        sequences: ["`", "~", "Shift+~", "Shift+`"]
        autoRepeat: false
        enabled: panel.diagnostics !== null && !panel.searchActive && !panel.typing(panel.hostWindow.activeFocusItem)
        onActivated: panel.toggle()
    }
    background: Rectangle { color: Theme.bg1; radius: 10; border.color: Theme.line2 }
    Overlay.modal: Rectangle { color: Theme.shadowAlpha(0.65) }
    contentItem: ColumnLayout {
        spacing: 0
        RowLayout {
            Layout.fillWidth: true; Layout.margins: 20; Layout.bottomMargin: 14; spacing: 12
            Rectangle {
                width: 32; height: 32; radius: 7; color: Theme.accentAlpha(0.1); border.color: Theme.accentAlpha(0.25)
                Label { anchors.centerIn: parent; text: ">_"; font.family: panel.mono; font.pointSize: Theme.fontPoints(16); color: Theme.accent }
            }
            ColumnLayout {
                spacing: 1
                Label { text: "Developer console"; color: Theme.text; font.pointSize: Theme.fontPoints(17); font.weight: Font.Medium }
                Label { text: "fuckMail / runtime diagnostics"; color: Theme.muted; font.family: panel.mono; font.pointSize: Theme.fontPoints(11) }
            }
            Item { Layout.fillWidth: true }
            Label { text: panel.copyStatus; color: Theme.accent; font.pointSize: Theme.fontPoints(12) }
            FmButton { objectName: "debugCopy"; text: "Copy report"; onClicked: { panel.diagnostics.refresh(); panel.diagnostics.copyReport(); panel.copyStatus = "Copied"; copiedTimer.restart() } }
            FmButton { text: "Close  Esc"; onClicked: panel.close() }
        }
        RowLayout {
            Layout.fillWidth: true; Layout.leftMargin: 20; Layout.rightMargin: 20; Layout.bottomMargin: 14; spacing: 6
            FmButton { text: "Events"; prominent: !panel.showState; onClicked: panel.showState = false }
            FmButton { text: "State"; prominent: panel.showState; onClicked: panel.showState = true }
            Rectangle { Layout.leftMargin: 8; Layout.rightMargin: 8; width: 1; height: 20; color: Theme.line2 }
            FmTextField {
                id: filter; objectName: "debugFilter"; enabled: !panel.showState
                Layout.fillWidth: true; placeholderText: "Filter messages, methods, metadata…"
                onTextChanged: panel.updateView(true)
                Accessible.name: "Filter debug events"
            }
            FmComboBox {
                id: level; objectName: "debugLevel"; enabled: !panel.showState
                model: ["All", "Warnings + errors", "ERROR", "WARN", "INFO", "DEBUG"]
                Layout.preferredWidth: 166
                onCurrentTextChanged: panel.updateView(true)
                Accessible.name: "Event severity"
            }
        }
        RowLayout {
            visible: !panel.showState
            Layout.fillWidth: true; Layout.leftMargin: 20; Layout.rightMargin: 20; Layout.bottomMargin: 12; spacing: 4
            Repeater {
                model: ["All", "Mail", "Calendar", "Sync", "Model", "App"]
                delegate: FmButton {
                    required property string modelData
                    objectName: "debugScope" + modelData
                    text: modelData; prominent: panel.selectedScope === modelData
                    onClicked: panel.selectedScope = modelData
                    Accessible.name: modelData === "All" ? "All subsystem scopes" : modelData + " scope"
                }
            }
            Item { Layout.fillWidth: true }
            FmButton { objectName: "debugPause"; text: panel.paused ? "Resume" : "Pause"; onClicked: panel.paused = !panel.paused }
            FmButton { text: "Clear"; onClicked: { panel.paused = false; panel.selectedText = ""; panel.diagnostics.clear(); panel.updateView(true) } }
        }
        Rectangle { Layout.fillWidth: true; height: 1; color: Theme.line2 }
        Rectangle {
            visible: !panel.showState; Layout.fillWidth: true; height: 30; color: Theme.bg2
            Row {
                anchors.fill: parent; anchors.leftMargin: 20; anchors.rightMargin: 20; spacing: 12
                Label { width: 102; height: parent.height; verticalAlignment: Text.AlignVCenter; text: "TIME · UTC"; color: Theme.muted; font.family: panel.mono; font.pointSize: Theme.fontPoints(10) }
                Label { width: 46; height: parent.height; verticalAlignment: Text.AlignVCenter; text: "LEVEL"; color: Theme.muted; font.family: panel.mono; font.pointSize: Theme.fontPoints(10) }
                Label { width: 68; height: parent.height; verticalAlignment: Text.AlignVCenter; text: "SCOPE"; color: Theme.muted; font.family: panel.mono; font.pointSize: Theme.fontPoints(10) }
                Label { height: parent.height; verticalAlignment: Text.AlignVCenter; text: "EVENT / METADATA"; color: Theme.muted; font.family: panel.mono; font.pointSize: Theme.fontPoints(10) }
            }
        }
        Item {
            Layout.fillWidth: true; Layout.fillHeight: true
            Rectangle { anchors.fill: parent; color: Theme.bg0 }
            ListModel { id: entries }
            ListView {
                id: events; objectName: "debugEvents"
                anchors.fill: parent; visible: !panel.showState; clip: true
                model: entries; reuseItems: true; boundsBehavior: Flickable.StopAtBounds
                keyNavigationEnabled: false
                // Only deliberate navigation changes follow mode. Layout and
                // model changes to contentY cannot unexpectedly turn it off.
                onMovementStarted: panel.followTail = false
                onMovementEnded: if (!panel.updating) panel.followTail = atYEnd
                onContentYChanged: if (!panel.updating && (dragging || flicking || bar.pressed)) panel.followTail = atYEnd
                onContentHeightChanged: if (!panel.updating) panel.settleTail()
                onHeightChanged: panel.settleTail()
                WheelHandler {
                    target: null; blocking: false
                    acceptedDevices: PointerDevice.Mouse | PointerDevice.TouchPad
                    onWheel: event => { panel.followTail = false; event.accepted = false }
                    onActiveChanged: if (!active && !panel.updating) panel.followTail = events.atYEnd
                }
                Keys.onPressed: event => {
                    if (event.key === Qt.Key_End) { panel.tail(); event.accepted = true }
                    else if ([Qt.Key_Home, Qt.Key_Up, Qt.Key_Down, Qt.Key_PageUp, Qt.Key_PageDown].indexOf(event.key) >= 0) {
                        panel.followTail = false
                        const step = event.key === Qt.Key_Up ? -panel.rowHeight : event.key === Qt.Key_Down ? panel.rowHeight : event.key === Qt.Key_PageUp ? -height : height
                        contentY = event.key === Qt.Key_Home ? originY : Math.max(originY, Math.min(originY + contentHeight - height, contentY + step))
                        if (atYEnd) panel.followTail = true
                        event.accepted = true
                    }
                }
                ScrollBar.vertical: FmScrollBar {
                    id: bar
                    onPressedChanged: {
                        if (pressed) panel.followTail = false
                        else if (!panel.updating) panel.followTail = events.atYEnd
                    }
                }
                delegate: Rectangle {
                    id: row
                    required property int index
                    required property string sequence
                    required property string at
                    required property string level
                    required property string scope
                    required property string source
                    required property string message
                    required property string details
                    required property string line
                    width: events.width; height: panel.rowHeight
                    color: panel.selectedSequence === sequence ? Theme.accentAlpha(0.1) : hover.hovered ? Theme.inkAlpha(0.035) : "transparent"
                    readonly property color severityColor: level === "ERROR" ? Theme.error : level === "WARN" ? Theme.warn : level === "DEBUG" ? Theme.subtle : Theme.secondary
                    Rectangle { width: 2; height: parent.height; color: row.severityColor; visible: row.level === "ERROR" || row.level === "WARN" }
                    RowLayout {
                        anchors.fill: parent; anchors.leftMargin: 20; anchors.rightMargin: 22; spacing: 12
                        Label { Layout.preferredWidth: 102; text: row.at.slice(11, 23); color: Theme.subtle; font.family: panel.mono; font.pointSize: Theme.fontPoints(12); textFormat: Text.PlainText }
                        Label { Layout.preferredWidth: 46; text: row.level; color: row.severityColor; font.family: panel.mono; font.pointSize: Theme.fontPoints(11); textFormat: Text.PlainText }
                        Label { Layout.preferredWidth: 68; text: row.scope; color: row.scope === "Model" ? Theme.ai : Theme.secondary; font.pointSize: Theme.fontPoints(12); textFormat: Text.PlainText }
                        Label { Layout.fillWidth: true; text: row.message + (row.details ? "   " + row.details : ""); color: Theme.text; elide: Text.ElideRight; font.family: panel.mono; font.pointSize: Theme.fontPoints(12); textFormat: Text.PlainText }
                    }
                    HoverHandler { id: hover }
                    TapHandler { onTapped: { panel.inspect(row); events.forceActiveFocus() } }
                    Accessible.role: Accessible.ListItem
                    Accessible.name: line
                    Accessible.onPressAction: panel.inspect(row)
                }
            }
            Label { anchors.centerIn: parent; visible: !panel.showState && entries.count === 0; text: "No matching events"; color: Theme.muted; font.pointSize: Theme.fontPoints(14) }
            ScrollView {
                anchors.fill: parent; visible: panel.showState; clip: true
                ScrollBar.vertical: FmScrollBar {}
                TextArea {
                    objectName: "debugState"; text: panel.showState ? panel.displayText : ""
                    padding: 20; rightPadding: 28; readOnly: true; selectByMouse: true; textFormat: TextEdit.PlainText
                    wrapMode: TextEdit.WrapAnywhere; color: Theme.secondary; selectionColor: Theme.selection
                    font.family: panel.mono; font.pointSize: Theme.fontPoints(12); background: null
                    Accessible.name: "Runtime application state"
                }
            }
        }
        Rectangle { visible: panel.selectedText !== "" && !panel.showState; Layout.fillWidth: true; height: 1; color: Theme.line2 }
        RowLayout {
            visible: panel.selectedText !== "" && !panel.showState
            Layout.fillWidth: true; Layout.preferredHeight: Math.min(170, panel.height * 0.25); spacing: 0
            ScrollView {
                Layout.fillWidth: true; Layout.fillHeight: true; clip: true
                ScrollBar.vertical: FmScrollBar {}
                TextArea {
                    objectName: "debugDetail"; text: panel.selectedText; padding: 16; rightPadding: 24
                    readOnly: true; selectByMouse: true; textFormat: TextEdit.PlainText; wrapMode: TextEdit.WrapAnywhere
                    font.family: panel.mono; font.pointSize: Theme.fontPoints(12); color: Theme.secondary; selectionColor: Theme.selection; background: null
                    Accessible.name: "Selected event details"
                }
            }
            FmButton { Layout.alignment: Qt.AlignTop; Layout.margins: 8; text: "Hide details"; onClicked: panel.selectedText = "" }
        }
        Rectangle { Layout.fillWidth: true; height: 1; color: Theme.line2 }
        RowLayout {
            Layout.fillWidth: true; Layout.margins: 12; Layout.leftMargin: 20; Layout.rightMargin: 20; spacing: 8
            Rectangle { width: 6; height: 6; radius: 3; color: panel.paused ? Theme.warn : panel.followTail ? Theme.accent : Theme.muted }
            Label {
                text: panel.paused ? "View paused · capture continues" : panel.showState ? "Runtime state" : panel.followTail ? "Following latest" : "Reading history"
                color: panel.followTail && !panel.paused ? Theme.accent : Theme.muted; font.pointSize: Theme.fontPoints(12)
            }
            FmButton { objectName: "debugTail"; visible: !panel.showState && (!panel.followTail || panel.paused); text: "Jump to latest ↓"; onClicked: panel.tail() }
            Item { Layout.fillWidth: true }
            Label {
                text: entries.count + " / " + (panel.diagnostics ? panel.diagnostics.count : 0) + " events"
                color: Theme.muted; font.family: panel.mono; font.pointSize: Theme.fontPoints(11)
                HoverHandler { id: countHover }
                FmToolTip { shown: countHover.hovered; text: "Latest 1,000 events · " + (panel.diagnostics ? panel.diagnostics.dropped : 0) + " older events evicted. Select a row to inspect its metadata." }
            }
            Label {
                text: "·  " + (panel.diagnostics && panel.diagnostics.logDirectory ? "Saved to disk · 10 MiB" : "Disk log unavailable")
                color: Theme.muted; font.pointSize: Theme.fontPoints(11)
                HoverHandler { id: diskHover }
                FmToolTip { shown: diskHover.hovered; text: panel.diagnostics ? panel.diagnostics.logDirectory : "" }
            }
        }
    }
    Timer { id: copiedTimer; interval: 2500; onTriggered: panel.copyStatus = "" }
}
