import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

// The developer console: drops from the top over a dim overlay. ~ or ` toggles
// it outside normal text fields; Esc, `exit` or the overlay closes it. The logs
// tab shows the latest 600 filtered lines of the 5,000 retained; the state tab
// shows a read-only tree of live state.
Popup {
    id: panel
    objectName: "debugConsole"
    required property var hostWindow
    required property var diagnostics
    property bool searchActive: false
    // {view, settings_tab, theme} for the state tab.
    property var uiState: ({})
    property string tab: "logs"
    property bool followTail: true
    property real newMark: 0
    property string selectedSequence: ""
    property string copiedSequence: ""
    property var history: []
    property int historyIndex: 0
    property var stateModel: []
    property var previousFocus: null
    readonly property int newCount: followTail || !diagnostics ? 0 : Math.max(0, diagnostics.appended - newMark)
    readonly property bool paused: !!diagnostics && diagnostics.paused
    readonly property bool blocksWorkspaceShortcuts: true
    readonly property font monoFont: Qt.font({family: diagnostics ? diagnostics.monoFamily : "monospace", pixelSize: Math.round(Theme.fontSize(12))})
    readonly property font smallFont: Qt.font({family: monoFont.family, pixelSize: Math.round(Theme.fontSize(11))})
    readonly property real line: Math.round(Theme.fontSize(19))
    readonly property real lineGap: (line - monoMetrics.height) / 2
    readonly property var levelNames: diagnostics ? diagnostics.levels : []
    // Syntax colours for JSON and state values; the theme has no green of its own.
    readonly property color green: Theme.light ? "#2F7A55" : "#7FC4A4"
    readonly property color stringColor: Theme.light ? "#4A7443" : "#9FC79F"
    readonly property color numberColor: Theme.ai

    parent: Overlay.overlay
    x: 0; y: 0
    width: parent ? parent.width : 0
    height: parent ? Math.min(parent.height, Math.max(360, parent.height * 0.76)) : 0
    padding: 0
    modal: true
    focus: true
    closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
    font: monoFont

    FontMetrics { id: monoMetrics; font: panel.monoFont }

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
    function levelColor(level) {
        return level <= 3 ? Theme.error : level === 4 ? Theme.amber : level === 5 ? Theme.text : level === 6 ? Theme.inkAlpha(.6) : Theme.inkAlpha(.35)
    }
    function tail() { followTail = true; settleTries = 0; settleTail() }
    // Rows have varying heights, so the end can move once they lay out; try again briefly.
    property int settleTries: 0
    function settleTail() {
        if (!opened || !followTail || tab !== "logs") return
        events.forceLayout(); events.positionViewAtEnd()
        // positionViewAtEnd can stop short of bottomMargin once rows leave the top.
        if (!events.atYEnd) events.contentY = Math.max(events.originY - events.topMargin, events.originY + events.contentHeight + events.bottomMargin - events.height)
        if (!events.atYEnd && settleTries++ < 10) settleTimer.restart()
        else if (events.atYEnd) settleTries = 0
    }
    function stopFollowing() {
        if (!followTail) return
        followTail = false
        newMark = diagnostics ? diagnostics.appended : 0
    }
    function toggleTail() { if (followTail) stopFollowing(); else tail() }
    function exportLog() { if (diagnostics) diagnostics.exportLog(); tail() }
    function run(text) {
        const command = text.trim()
        commandField.text = ""
        if (!command.length || !diagnostics) return
        history = history.concat([command]); historyIndex = history.length
        const result = diagnostics.run(command)
        if (result.close) { close(); return }
        if (result.tab) tab = result.tab
        if (result.tail) tail()
    }
    function refreshState() { if (diagnostics && opened && tab === "state") stateModel = diagnostics.stateRows(uiState) }
    function escapeHtml(text) { return String(text).replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;") }
    // Rich text takes opaque colours; blend translucent ones over the console.
    function solid(color) { return String(Qt.tint(Theme.canvas, color)) }
    // Pretty JSON as coloured rich text: keys muted, strings green, numbers blue,
    // booleans green/red, null grey, punctuation faint.
    function jsonHtml(json) {
        let value
        try { value = JSON.parse(json) } catch (e) { return escapeHtml(json) }
        const lines = [], faint = solid(Theme.inkAlpha(.35))
        const span = (color, text) => '<span style="color:' + color + '">' + escapeHtml(text) + '</span>'
        const scalar = v => typeof v === "string" ? span(String(stringColor), JSON.stringify(v))
            : typeof v === "boolean" ? span(String(v ? green : Theme.error), String(v))
            : v === null || v === undefined ? span(solid(Theme.inkAlpha(.4)), "null") : span(String(numberColor), String(v))
        const emit = (key, v, depth, last) => {
            const indent = "&nbsp;".repeat(depth * 2), name = key === null ? "" : span(solid(Theme.inkAlpha(.6)), JSON.stringify(key) + ": ")
            const comma = last ? "" : span(faint, ",")
            if (v !== null && typeof v === "object") {
                const array = Array.isArray(v), keys = Object.keys(v)
                if (!keys.length) { lines.push(indent + name + span(solid(Theme.inkAlpha(.5)), array ? "[]" : "{}") + comma); return }
                lines.push(indent + name + span(faint, array ? "[" : "{"))
                keys.forEach((k, i) => emit(array ? null : k, v[k], depth + 1, i === keys.length - 1))
                lines.push(indent + span(faint, array ? "]" : "}") + comma)
            } else lines.push(indent + name + scalar(v) + comma)
        }
        emit(null, value, 0, true)
        return lines.join("<br>")
    }
    function copyJson(sequence, json) {
        try { diagnostics.copyText(JSON.stringify(JSON.parse(json), null, 2)) } catch (e) { diagnostics.copyText(json) }
        copiedSequence = sequence; copiedTimer.restart()
    }

    onOpened: { tail(); refreshState(); if (diagnostics) diagnostics.requestState(); commandField.forceActiveFocus() }
    onClosed: if (previousFocus && previousFocus.visible) previousFocus.forceActiveFocus()
    onTabChanged: { refreshState(); if (tab === "state" && diagnostics) diagnostics.requestState(); Qt.callLater(settleTail) }
    Connections {
        target: panel.diagnostics
        function onFilterChanged() {
            if (grepField.text !== panel.diagnostics.grep) grepField.text = panel.diagnostics.grep
            panel.tail()
        }
    }
    Shortcut {
        sequences: ["`", "~", "Shift+~", "Shift+`"]
        autoRepeat: false
        enabled: panel.diagnostics !== null && !panel.searchActive && (panel.opened || !panel.typing(panel.hostWindow.activeFocusItem))
        onActivated: panel.toggle()
    }
    Timer { id: settleTimer; interval: 16; onTriggered: panel.settleTail() }
    Timer { id: copiedTimer; interval: 1200; onTriggered: panel.copiedSequence = "" }
    Timer { interval: 1000; repeat: true; running: panel.opened && panel.tab === "state"; onTriggered: panel.refreshState() }

    enter: Transition {
        NumberAnimation { property: "opacity"; from: 0; to: 1; duration: 180; easing.type: Easing.OutCubic }
        NumberAnimation { property: "y"; from: -16; to: 0; duration: 180; easing.type: Easing.OutCubic }
    }
    exit: Transition { NumberAnimation { property: "opacity"; to: 0; duration: 100 } }
    Overlay.modal: Rectangle { color: Theme.shadowAlpha(0.4) }
    background: Rectangle {
        color: Theme.canvas
        Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: Theme.ruleWidth; color: Theme.line2 }
    }

    // One monospace line of console text.
    component Mono: Text {
        font: panel.monoFont; color: Theme.inkAlpha(.85); textFormat: Text.PlainText
        lineHeightMode: Text.FixedHeight; lineHeight: panel.line
        topPadding: panel.lineGap; bottomPadding: -panel.lineGap
    }
    // A text button in the top bar (24 px, hover fill) or the filter row (compact).
    component Chip: Rectangle {
        id: chip
        property alias text: label.text
        property color textColor: Theme.inkAlpha(.65)
        property color hoverColor: Theme.text
        property bool selected: false
        property bool compact: false
        property string tip: ""
        signal clicked()
        implicitWidth: label.implicitWidth + (compact ? 10 : 16); implicitHeight: compact ? panel.line : 24; radius: 3
        color: selected || (area.containsMouse && !compact) ? Theme.bg4 : "transparent"
        Accessible.role: Accessible.Button; Accessible.name: label.text; Accessible.description: tip; Accessible.onPressAction: clicked()
        Mono { id: label; anchors.centerIn: parent; color: area.containsMouse ? chip.hoverColor : chip.textColor }
        MouseArea { id: area; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: chip.clicked() }
        FmToolTip { shown: area.containsMouse && chip.tip.length > 0; text: chip.tip }
    }
    component Rule: Rectangle { Layout.fillWidth: true; implicitHeight: Theme.ruleWidth; color: Theme.line }
    component Status: Text { font: panel.smallFont; textFormat: Text.PlainText; color: Theme.inkAlpha(.5) }

    contentItem: ColumnLayout {
        spacing: 0
        // Top bar
        RowLayout {
            Layout.fillWidth: true; Layout.preferredHeight: 40; Layout.leftMargin: 16; Layout.rightMargin: 10; spacing: 18
            Row {
                spacing: 8
                Mono { text: "~/fuckmail"; color: Theme.amber }
                Mono { text: "$"; color: Theme.inkAlpha(.35) }
                Mono { text: "console" }
            }
            Row {
                spacing: 2
                Repeater {
                    model: ["logs", "state"]
                    delegate: Chip {
                        required property string modelData
                        objectName: "debugTab_" + modelData
                        text: modelData; selected: panel.tab === modelData
                        textColor: selected ? Theme.text : Theme.inkAlpha(.5)
                        onClicked: { panel.tab = modelData; panel.tail() }
                    }
                }
            }
            Item { Layout.fillWidth: true }
            Row {
                spacing: 2
                Rectangle {
                    id: tailToggle
                    objectName: "debugTailToggle"
                    readonly property color tint: panel.followTail ? panel.green : Theme.inkAlpha(.5)
                    width: tailRow.implicitWidth + 16; height: 24; radius: 3
                    color: tailArea.containsMouse ? Theme.bg4 : "transparent"
                    Row {
                        id: tailRow
                        anchors.centerIn: parent; spacing: 6
                        Rectangle {
                            id: tailDot
                            width: 7; height: 7; radius: 3.5; anchors.verticalCenter: parent.verticalCenter
                            color: panel.followTail ? panel.green : Theme.inkAlpha(.3)
                            SequentialAnimation on opacity {
                                running: panel.opened && panel.followTail && !panel.paused
                                loops: Animation.Infinite
                                onRunningChanged: if (!running) tailDot.opacity = 1
                                NumberAnimation { to: 0.3; duration: 700; easing.type: Easing.InOutSine }
                                NumberAnimation { to: 1; duration: 700; easing.type: Easing.InOutSine }
                            }
                        }
                        Mono { text: "tail -f"; color: tailToggle.tint; anchors.verticalCenter: parent.verticalCenter }
                    }
                    MouseArea { id: tailArea; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: panel.toggleTail() }
                    FmToolTip { shown: tailArea.containsMouse; text: panel.followTail ? "Following new lines · click to stop" : "Follow new lines" }
                    Accessible.role: Accessible.CheckBox; Accessible.name: "tail -f"; Accessible.checked: panel.followTail; Accessible.onPressAction: panel.toggleTail()
                }
                Chip {
                    objectName: "debugPause"
                    text: panel.paused ? "resume" : "pause"
                    textColor: panel.paused ? Theme.amber : Theme.inkAlpha(.65); hoverColor: panel.paused ? Theme.amber : Theme.text
                    tip: panel.paused ? "Resume capturing" : "Stop capturing new lines · the log file keeps recording"
                    onClicked: panel.diagnostics.paused = !panel.paused
                }
                Chip { objectName: "debugClear"; text: "clear"; tip: "Clear the view · the log file is kept"; onClicked: { panel.selectedSequence = ""; panel.diagnostics.clear(); panel.tail() } }
                Chip { objectName: "debugExport"; text: "export"; tip: "Save the filtered lines as a .log file in Downloads"; onClicked: panel.exportLog() }
                Item { width: 13; height: 24; Rectangle { anchors.centerIn: parent; width: 1; height: 16; color: Theme.line2 } }
                Rectangle {
                    objectName: "debugClose"
                    width: 27; height: 24; radius: 3; color: closeArea.containsMouse ? Theme.bg4 : "transparent"
                    FmIcon { anchors.centerIn: parent; name: "close"; size: 11; color: closeArea.containsMouse ? Theme.text : Theme.inkAlpha(.65) }
                    MouseArea { id: closeArea; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: panel.close() }
                    FmToolTip { shown: closeArea.containsMouse; text: "Close — ~ or Esc" }
                    Accessible.role: Accessible.Button; Accessible.name: "Close console"; Accessible.onPressAction: panel.close()
                }
            }
        }
        Rule {}
        // Filter row
        RowLayout {
            visible: panel.tab === "logs"
            Layout.fillWidth: true; Layout.minimumHeight: 34; Layout.leftMargin: 16; Layout.rightMargin: 10; spacing: 20
            Row {
                spacing: 1
                Mono { text: "level ≤"; color: Theme.inkAlpha(.35); rightPadding: 5 }
                Repeater {
                    model: panel.levelNames
                    delegate: Chip {
                        required property string modelData
                        required property int index
                        readonly property int chosen: panel.diagnostics ? panel.diagnostics.level : 6
                        objectName: "debugLevel_" + modelData
                        compact: true; text: modelData; selected: index === chosen
                        textColor: index <= chosen ? panel.levelColor(index) : Theme.inkAlpha(.3)
                        hoverColor: index <= chosen ? textColor : Theme.inkAlpha(.6)
                        tip: (index === chosen ? "Showing " : "Show ") + modelData + " and worse"
                        onClicked: panel.diagnostics.level = index
                    }
                }
            }
            Row {
                spacing: 1
                Mono { text: "fac"; color: Theme.inkAlpha(.35); rightPadding: 5 }
                Repeater {
                    model: ["all"].concat(panel.diagnostics ? panel.diagnostics.facilities : [])
                    delegate: Chip {
                        required property string modelData
                        objectName: "debugFac_" + modelData
                        compact: true; text: modelData; selected: (panel.diagnostics ? panel.diagnostics.facility : "all") === modelData
                        textColor: selected ? Theme.text : Theme.inkAlpha(.45)
                        onClicked: panel.diagnostics.facility = modelData
                    }
                }
            }
            Item { Layout.fillWidth: true }
            Rectangle {
                Layout.preferredWidth: 220; Layout.minimumWidth: 140; implicitHeight: 24; radius: 3
                color: "transparent"; border.color: grepField.activeFocus ? Theme.line3 : Theme.line2
                RowLayout {
                    anchors.fill: parent; anchors.leftMargin: 8; anchors.rightMargin: 8; spacing: 6
                    Mono { text: "grep"; color: Theme.inkAlpha(.35) }
                    TextInput {
                        id: grepField
                        objectName: "debugGrep"
                        Layout.fillWidth: true; clip: true
                        font: panel.monoFont; color: Theme.text; selectionColor: Theme.selection; selectByMouse: true
                        onTextEdited: panel.diagnostics.grep = text
                        Keys.onPressed: event => { if (event.text === "`" || event.text === "~") { panel.close(); event.accepted = true } }
                        Accessible.name: "Filter lines"
                    }
                }
            }
        }
        Rule { visible: panel.tab === "logs" }
        // Lines, or the state tree
        Item {
            Layout.fillWidth: true; Layout.fillHeight: true
            ListView {
                id: events
                objectName: "debugEvents"
                anchors.fill: parent; visible: panel.tab === "logs"; clip: true
                topMargin: 8; bottomMargin: 10
                model: panel.diagnostics ? panel.diagnostics.lines : null
                boundsBehavior: Flickable.StopAtBounds
                keyNavigationEnabled: false
                // Only deliberate navigation changes follow mode.
                onMovementStarted: panel.stopFollowing()
                onMovementEnded: if (atYEnd) panel.tail()
                onContentHeightChanged: panel.settleTail()
                onCountChanged: panel.settleTail()
                onHeightChanged: panel.settleTail()
                // Any deliberate scroll away from the end stops following; wheel scrolls move the view too.
                onContentYChanged: if (moving && !atYEnd) panel.stopFollowing()
                Keys.onPressed: event => {
                    if (event.key === Qt.Key_End) { panel.tail(); event.accepted = true }
                    else if ([Qt.Key_Home, Qt.Key_PageUp, Qt.Key_PageDown].indexOf(event.key) >= 0) {
                        panel.stopFollowing()
                        const top = originY - topMargin, bottom = originY + contentHeight + bottomMargin - height
                        contentY = event.key === Qt.Key_Home ? top : Math.max(top, Math.min(bottom, contentY + (event.key === Qt.Key_PageUp ? -height : height)))
                        if (atYEnd) panel.tail()
                        event.accepted = true
                    }
                }
                ScrollBar.vertical: FmScrollBar {
                    id: bar
                    onPressedChanged: {
                        if (pressed) panel.stopFollowing()
                        else if (events.atYEnd) panel.tail()
                    }
                }
                delegate: Rectangle {
                    id: row
                    required property string sequence
                    required property string time
                    required property int level
                    required property string facility
                    required property string message
                    required property string context
                    required property string json
                    required property int jsonLines
                    required property bool typed
                    readonly property bool open: panel.selectedSequence === sequence
                    readonly property bool badge: !typed && level <= 2
                    readonly property real detailIndent: 92 + 62 + 56 + 36
                    width: events.width; height: lineColumn.implicitHeight
                    color: open ? Theme.inkAlpha(.06) : hover.hovered ? Theme.inkAlpha(.035)
                        : !typed && level <= 3 ? Theme.errorAlpha(.07) : !typed && level === 4 ? Theme.withAlpha(Theme.amber, .04) : "transparent"
                    function toggle() { panel.selectedSequence = open ? "" : sequence }
                    function inJson(position) { return jsonBlock.visible && jsonBlock.contains(jsonBlock.mapFromItem(row, position)) }
                    HoverHandler { id: hover; cursorShape: Qt.PointingHandCursor }
                    // A press the JSON text takes for selection leaves the line open.
                    TapHandler { onTapped: (point) => { if (!row.inJson(point.position)) row.toggle() } }
                    Accessible.role: Accessible.ListItem
                    Accessible.name: time + " " + (typed ? "" : (panel.levelNames[level] || "") + " " + facility + " ") + message
                    Accessible.onPressAction: row.toggle()
                    ColumnLayout {
                        id: lineColumn
                        x: 16; width: parent.width - 32; spacing: 0
                        RowLayout {
                            Layout.fillWidth: true; spacing: 12
                            Mono { Layout.preferredWidth: 92; Layout.alignment: Qt.AlignTop; text: row.time; color: Theme.inkAlpha(.3) }
                            Item {
                                Layout.preferredWidth: 62; Layout.alignment: Qt.AlignTop; implicitHeight: panel.line
                                Rectangle { visible: row.badge; x: -4; y: 1; width: levelText.implicitWidth + 8; height: panel.line - 2; radius: 2; color: Theme.error }
                                Mono {
                                    id: levelText
                                    text: row.typed ? "" : panel.levelNames[row.level] || ""
                                    color: row.badge ? Theme.canvas : panel.levelColor(row.level)
                                    font.weight: row.badge ? Font.DemiBold : row.level <= 4 ? Font.Medium : Font.Normal
                                }
                            }
                            Mono { Layout.preferredWidth: 56; Layout.alignment: Qt.AlignTop; text: row.facility; color: Theme.inkAlpha(.45); elide: Text.ElideRight }
                            Mono {
                                Layout.fillWidth: true; Layout.alignment: Qt.AlignTop
                                wrapMode: Text.WrapAtWordBoundaryOrAnywhere; textFormat: Text.StyledText
                                color: row.typed ? Theme.amber : row.level <= 3 ? Theme.error : row.level <= 5 ? Theme.text : row.level === 6 ? Theme.inkAlpha(.78) : Theme.inkAlpha(.45)
                                text: panel.escapeHtml(row.message)
                                    + (row.json && !row.open ? '<font color="' + panel.solid(Theme.inkAlpha(.35)) + '">&nbsp;&nbsp;{…} ' + row.jsonLines + (row.jsonLines === 1 ? " line" : " lines") + '</font>' : "")
                            }
                        }
                        Mono {
                            visible: row.open
                            Layout.fillWidth: true; Layout.leftMargin: row.detailIndent; Layout.bottomMargin: 4; topPadding: panel.lineGap + 1
                            wrapMode: Text.WrapAnywhere; color: Theme.inkAlpha(.45)
                            text: row.open ? "└ " + (row.context ? row.context.split(" ").join("  ") : "(no context)") : ""
                        }
                        Rectangle {
                            id: jsonBlock
                            visible: row.open && row.json.length > 0
                            Layout.fillWidth: true; Layout.leftMargin: row.detailIndent; Layout.topMargin: 2; Layout.bottomMargin: 8
                            implicitHeight: visible ? jsonText.implicitHeight + 16 : 0
                            color: Theme.inkAlpha(.025)
                            Rectangle { width: Theme.ruleWidth; height: parent.height; color: Theme.line2 }
                            TextEdit {
                                id: jsonText
                                objectName: "debugJson_" + row.sequence
                                x: 12; y: 8; width: parent.width - 24 - copyButton.width
                                readOnly: true; selectByMouse: true; wrapMode: TextEdit.WrapAnywhere
                                textFormat: TextEdit.RichText; font: panel.monoFont; color: Theme.text; selectionColor: Theme.selection
                                text: row.open && row.json ? panel.jsonHtml(row.json) : ""
                                Accessible.name: "JSON payload"
                            }
                            Rectangle {
                                id: copyButton
                                objectName: "debugCopyJson_" + row.sequence
                                anchors.top: parent.top; anchors.right: parent.right; anchors.topMargin: 6; anchors.rightMargin: 8
                                width: copyLabel.implicitWidth + 12; height: copyLabel.implicitHeight + 2; radius: 3
                                color: copyArea.containsMouse ? Theme.bg4 : "transparent"
                                Status {
                                    id: copyLabel; anchors.centerIn: parent
                                    text: panel.copiedSequence === row.sequence ? "copied" : "copy"
                                    color: copyArea.containsMouse ? Theme.text : Theme.inkAlpha(.5)
                                }
                                MouseArea { id: copyArea; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: panel.copyJson(row.sequence, row.json) }
                                Accessible.role: Accessible.Button; Accessible.name: "Copy JSON"; Accessible.onPressAction: panel.copyJson(row.sequence, row.json)
                            }
                        }
                    }
                }
            }
            Mono {
                x: 16; y: 8; visible: panel.tab === "logs" && events.count === 0
                text: "-- no lines match --"; color: Theme.inkAlpha(.35)
            }
            ListView {
                id: stateView
                objectName: "debugState"
                anchors.fill: parent; visible: panel.tab === "state"; clip: true
                topMargin: 8; bottomMargin: 10; boundsBehavior: Flickable.StopAtBounds
                model: panel.stateModel
                ScrollBar.vertical: FmScrollBar {}
                delegate: Row {
                    id: stateRow
                    required property var modelData
                    x: 16 + modelData.depth * 18; spacing: 10
                    Mono {
                        width: stateRow.modelData.kind === "section" ? implicitWidth : Math.max(implicitWidth, 170)
                        text: stateRow.modelData.key
                        color: stateRow.modelData.kind !== "section" ? Theme.inkAlpha(.5) : stateRow.modelData.depth ? Theme.inkAlpha(.75) : Theme.amber
                    }
                    Mono {
                        text: stateRow.modelData.value
                        color: stateRow.modelData.kind === "string" ? panel.stringColor : stateRow.modelData.kind === "number" ? panel.numberColor
                            : stateRow.modelData.kind === "null" ? Theme.inkAlpha(.4) : stateRow.modelData.kind === "bool" ? (stateRow.modelData.value === "true" ? panel.green : Theme.error) : Theme.text
                    }
                }
            }
            Rectangle {
                objectName: "debugTail"
                visible: panel.tab === "logs" && panel.newCount > 0
                anchors.right: parent.right; anchors.bottom: parent.bottom; anchors.rightMargin: 20; anchors.bottomMargin: 12
                width: newLabel.implicitWidth + 20; height: 24; radius: 3
                color: Theme.bg4; border.color: Theme.line2
                Mono { id: newLabel; anchors.centerIn: parent; text: "↓ " + panel.newCount + " new"; color: Theme.amber }
                MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: panel.tail() }
                Accessible.role: Accessible.Button; Accessible.name: panel.newCount + " new lines · follow"; Accessible.onPressAction: panel.tail()
            }
        }
        Rule {}
        // Command line
        RowLayout {
            Layout.fillWidth: true; Layout.preferredHeight: 34; Layout.leftMargin: 16; Layout.rightMargin: 16; spacing: 8
            Mono { text: "›"; color: Theme.amber }
            TextInput {
                id: commandField
                objectName: "debugCommand"
                Layout.fillWidth: true; clip: true
                font: panel.monoFont; color: Theme.text; selectionColor: Theme.selection; selectByMouse: true
                cursorDelegate: Rectangle { width: 1.5; color: Theme.amber; visible: commandField.activeFocus && commandField.cursorVisible }
                Text { visible: !commandField.text.length; text: "type help"; font: panel.monoFont; color: Theme.inkAlpha(.3); textFormat: Text.PlainText }
                Keys.onPressed: event => {
                    if (event.text === "`" || event.text === "~") { panel.close(); event.accepted = true }
                    else if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) { panel.run(text); event.accepted = true }
                    else if (event.key === Qt.Key_Up && panel.history.length) {
                        panel.historyIndex = Math.max(0, panel.historyIndex - 1); text = panel.history[panel.historyIndex]; event.accepted = true
                    } else if (event.key === Qt.Key_Down && panel.history.length) {
                        panel.historyIndex = Math.min(panel.history.length, panel.historyIndex + 1); text = panel.history[panel.historyIndex] || ""; event.accepted = true
                    }
                }
                Accessible.name: "Console command"
            }
        }
        // Status bar
        Rectangle {
            Layout.fillWidth: true; Layout.preferredHeight: 22; color: Theme.bg4; clip: true
            RowLayout {
                readonly property var d: panel.diagnostics
                anchors.fill: parent; anchors.leftMargin: 16; anchors.rightMargin: 16; spacing: 14
                Status {
                    objectName: "debugMode"
                    text: panel.paused ? "‖ paused" : panel.followTail ? "● tail" : "○ scrolled"
                    color: panel.followTail ? panel.green : Theme.inkAlpha(.5)
                }
                Status {
                    objectName: "debugCounts"
                    text: parent.d ? parent.d.count.toLocaleString(Qt.locale("en_US"), "f", 0) + " lines · " + parent.d.matched.toLocaleString(Qt.locale("en_US"), "f", 0) + " shown" : ""
                }
                Status {
                    Layout.fillWidth: true; elide: Text.ElideRight
                    text: parent.d ? "level ≤ " + panel.levelNames[parent.d.level] + " · fac " + parent.d.facility + (parent.d.grep ? " · grep \"" + parent.d.grep + "\"" : "") : ""
                }
                Status {
                    objectName: "debugErrors"
                    text: (parent.d ? parent.d.errors : 0) + " err"
                    color: parent.d && parent.d.errors > 0 ? Theme.error : Theme.inkAlpha(.5)
                }
                Status { text: (parent.d ? parent.d.warnings : 0) + " warn · last hour" }
                Status { text: "~ close"; color: Theme.inkAlpha(.35) }
            }
        }
    }
}
