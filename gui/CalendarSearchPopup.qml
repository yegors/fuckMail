// Calendar search's dropdown under the search box. The box holds the whole
// query and Core reads it (date:, with:, title: and words). Empty, this offers
// recent searches and the operators. While typing it offers a day to go to,
// completion of a partial operator and the next matching events, upcoming
// first and then the recent past. Main.qml positions it.
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Effects

Popup {
    id: popup
    objectName: "calendarSearchPopup"
    property bool combined: true
    // The search field this edits. It keeps focus while the dropdown is open.
    property Item field: null
    signal eventRequested(var event)
    signal committed()
    readonly property string query: backend.calendarSearchQuery
    readonly property bool typing: query.trim().length > 0
    readonly property var result: backend.calendarSearch
    // Core's latest reading. It may trail the text by a keystroke.
    readonly property var terms: backend.calendarSearchTerms
    readonly property bool filtering: typing && ((terms.words || []).length + (terms.title || []).length + (terms["with"] || []).length) > 0
    readonly property bool onlyPeople: filtering && !(terms.words || []).length && !(terms.title || []).length
    // The last finished search stays on screen while the next one runs, so the
    // dropdown never empties between keys. Only a real search carries a query.
    // A handler, not a Binding with `when`: both change on one signal, and the
    // Binding could take the busy result before `when` turns false.
    property var settled: ({})
    onResultChanged: if (!result.busy) settled = result
    Component.onCompleted: if (!result.busy) settled = result
    readonly property var rows: settled.events || []
    readonly property bool answered: settled.query !== undefined
    // A status while busy (opening a match) shows at once; others are held too.
    readonly property string statusText: result.status || settled.status || ""
    // Words to match, read from the text itself so it never trails a key:
    // everything but date: and an operator with nothing after it.
    readonly property bool wordsTyped: typing && query.replace(/(^|\s)(date:\S*|with:|title:)(?=\s|$)/gi, " ").trim().length > 0
    readonly property string dateValue: typing && terms.date ? terms.date : ""
    readonly property string goDate: dateValue ? backend.calendarQueryDate(dateValue) : ""
    readonly property var operators: [
        {op: "date:", description: "Jump to a day", example: "date:2026-10-02 · tomorrow · +7"},
        {op: "with:", description: "Events with someone", example: "with:maya@example.com"},
        {op: "title:", description: "Only in event titles", example: "title:standup"}
    ]
    readonly property string lastToken: { const match = query.match(/(\S*)$/); return match ? match[1].toLowerCase() : "" }
    readonly property var completions: lastToken.length >= 1 && lastToken.indexOf(":") < 0 ? operators.filter(o => o.op.startsWith(lastToken)) : []
    function dayText(iso) {
        const day = new Date(iso + "T12:00:00"), today = new Date(backend.todayDate + "T12:00:00")
        return Qt.formatDate(day, "ddd d MMM") + (day.getFullYear() !== today.getFullYear() ? " " + day.getFullYear() : "")
    }
    function goText(iso) {
        const offset = Math.round((new Date(iso + "T12:00:00") - new Date(backend.todayDate + "T12:00:00")) / 86400000)
        return dayText(iso) + (offset === 0 ? " · Today" : offset === 1 ? " · Tomorrow" : offset === -1 ? " · Yesterday" : "")
    }
    function whenText(row) { return dayText(row.date) + (row.all_day ? " · all day" : " · " + row.time_display) }
    // Replaces the text and keeps typing at its end.
    function edit(text) {
        backend.calendarSearchQuery = text
        if (!field) return
        field.forceActiveFocus()
        Qt.callLater(function() { if (popup.field) popup.field.cursorPosition = popup.field.length })
    }
    // Enter, the Go to row or a recent search: keep it, remember it and close.
    function commit(text) {
        const value = (text === undefined ? query : text).trim()
        if (value.length) backend.commitCalendarSearch(value)
        close()
        committed()
    }
    function pick(row) {
        backend.rememberCalendarSearch(query)
        eventRequested(row)
    }
    width: Math.min(460, (Overlay.overlay ? Overlay.overlay.width : 492) - 32)
    height: Math.min(contents.implicitHeight + topPadding + bottomPadding, parent ? parent.height - y - 12 : 560)
    topPadding: 4; bottomPadding: 6; leftPadding: 0; rightPadding: 0
    focus: false
    closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside

    component SectionLabel: Text {
        Layout.fillWidth: true; leftPadding: 14; rightPadding: 14; topPadding: 10; bottomPadding: 6
        color: Theme.inkAlpha(.4); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(10.5)
        font.letterSpacing: Theme.fontSize(10.5) * .05; font.capitalization: Font.AllUppercase
    }
    component Rule: Rectangle { Layout.fillWidth: true; implicitHeight: Theme.ruleWidth; color: Theme.line }
    component SearchRow: Rectangle {
        id: searchRow
        signal clicked()
        default property alias content: rowContent.data
        property string accessibleName
        property real minimumHeight: 32
        Layout.fillWidth: true
        implicitHeight: Math.max(minimumHeight, rowContent.implicitHeight)
        color: rowArea.containsMouse && enabled ? Theme.bg4 : "transparent"
        Accessible.role: Accessible.Button; Accessible.name: accessibleName
        Accessible.onPressAction: if (enabled) clicked()
        MouseArea { id: rowArea; anchors.fill: parent; hoverEnabled: true; cursorShape: searchRow.enabled ? Qt.PointingHandCursor : Qt.ArrowCursor; onClicked: if (searchRow.enabled) searchRow.clicked() }
        RowLayout { id: rowContent; anchors.fill: parent; anchors.leftMargin: 14; anchors.rightMargin: 14; spacing: 10 }
    }
    component RemoveButton: Rectangle {
        id: removeButton
        signal clicked()
        property string tip: "Remove"
        implicitWidth: 20; implicitHeight: 20; radius: 5
        color: removeArea.containsMouse ? Theme.bg6 : "transparent"
        Accessible.role: Accessible.Button; Accessible.name: tip; Accessible.onPressAction: clicked()
        FmIcon { anchors.centerIn: parent; name: "close"; size: 9; color: removeArea.containsMouse ? Theme.text : Theme.inkAlpha(.35) }
        MouseArea { id: removeArea; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: removeButton.clicked() }
        FmToolTip { shown: removeArea.containsMouse; text: removeButton.tip }
    }
    component OperatorChip: Rectangle {
        property alias text: operatorLabel.text
        property real minimumWidth: 0
        implicitWidth: Math.max(minimumWidth, operatorLabel.implicitWidth + 12); implicitHeight: operatorLabel.implicitHeight + 2
        radius: 5; color: Theme.bg4
        Text { id: operatorLabel; x: 6; anchors.verticalCenter: parent.verticalCenter; color: Theme.text; font.family: backend.fixedFontFamily(); font.pointSize: Theme.fontPoints(11.5); textFormat: Text.PlainText }
    }

    background: Rectangle {
        color: Theme.surface; border.color: Theme.controlBorder; radius: 8
        layer.enabled: GraphicsInfo.api !== GraphicsInfo.Software
        layer.effect: MultiEffect { shadowEnabled: true; shadowColor: Theme.shadowAlpha(.55); shadowBlur: .8; shadowVerticalOffset: 18 }
    }
    contentItem: ScrollView {
        ScrollBar.vertical: FmScrollBar {}
        ScrollBar.horizontal.policy: ScrollBar.AlwaysOff
        clip: true
        contentWidth: availableWidth
        ColumnLayout {
            id: contents
            width: popup.availableWidth
            spacing: 0
            // date: — the day it names, or how to write one.
            SearchRow {
                objectName: "calendarSearchGoTo"
                visible: popup.goDate.length > 0
                minimumHeight: 38
                accessibleName: "Go to " + goLabel.text
                onClicked: popup.commit()
                FmIcon { name: "calendar"; size: 13; color: Theme.amber }
                Text {
                    Layout.fillWidth: true; elide: Text.ElideRight; textFormat: Text.StyledText
                    text: "Go to <font color=\"" + Theme.text + "\">" + goLabel.text + "</font>"
                    color: Theme.inkAlpha(.75); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12)
                    Text { id: goLabel; visible: false; text: popup.goDate ? popup.goText(popup.goDate) : "" }
                }
                Text { text: "↵"; color: Theme.inkAlpha(.4); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(10.5) }
            }
            Text {
                objectName: "calendarSearchDateHint"
                visible: popup.dateValue.length > 0 && !popup.goDate.length
                Layout.fillWidth: true; leftPadding: 14; rightPadding: 14; topPadding: 10; bottomPadding: 10; wrapMode: Text.Wrap
                text: "Use YYYY-MM-DD, MM-DD, today, tomorrow or +7"
                color: Theme.inkAlpha(.5); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12)
            }
            // A partial operator.
            Repeater {
                model: popup.typing ? popup.completions : []
                SearchRow {
                    id: completion
                    required property var modelData
                    objectName: "calendarSearchComplete_" + modelData.op.slice(0, -1)
                    accessibleName: "Complete " + modelData.op
                    onClicked: popup.edit(popup.query.slice(0, popup.query.length - popup.lastToken.length) + modelData.op)
                    OperatorChip { text: completion.modelData.op; minimumWidth: 64 }
                    Text { text: completion.modelData.description; color: Theme.inkAlpha(.6); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12); Layout.fillWidth: true; elide: Text.ElideRight }
                }
            }
            // The next matching events.
            RowLayout {
                visible: popup.wordsTyped && !popup.statusText.length && (popup.rows.length > 0 || !popup.answered)
                Layout.fillWidth: true; Layout.leftMargin: 14; Layout.rightMargin: 14; Layout.topMargin: 10; Layout.bottomMargin: 6
                Text {
                    objectName: "calendarSearchCount"
                    text: !popup.rows.length ? "" : popup.settled.has_more ? "7+ events" : popup.rows.length + (popup.rows.length === 1 ? " event" : " events")
                    color: Theme.inkAlpha(.4); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(10.5)
                    font.letterSpacing: Theme.fontSize(10.5) * .05; font.capitalization: Font.AllUppercase
                }
                Item { Layout.fillWidth: true }
                Text {
                    objectName: "calendarSearchEnterNote"
                    text: popup.onlyPeople ? "↵ shows this month with them" : "↵ keeps the filter on"
                    color: Theme.inkAlpha(.35); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(11)
                }
            }
            Repeater {
                model: popup.wordsTyped ? popup.rows : []
                SearchRow {
                    id: resultRow
                    required property var modelData
                    required property int index
                    objectName: "calendarSearchResult_" + index
                    enabled: !popup.result.busy
                    opacity: modelData.past ? .55 : 1
                    accessibleName: (modelData.title || "Untitled event") + ", " + popup.whenText(modelData)
                    onClicked: popup.pick(modelData)
                    Rectangle {
                        implicitWidth: 16; implicitHeight: 16; radius: 8
                        color: Theme.accountTint(resultRow.modelData.account, .22)
                        Text {
                            anchors.centerIn: parent
                            text: ((resultRow.modelData.account_name || resultRow.modelData.account || "?")[0] || "?").toUpperCase()
                            color: Theme.accountColor(resultRow.modelData.account)
                            font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(10.5); font.weight: Font.Medium
                        }
                    }
                    Text {
                        Layout.fillWidth: true; elide: Text.ElideRight; textFormat: Text.PlainText
                        text: resultRow.modelData.title || "Untitled event"
                        color: Theme.text; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12)
                    }
                    Text {
                        text: popup.whenText(resultRow.modelData); textFormat: Text.PlainText
                        color: Theme.inkAlpha(.45); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(11.5); font.features: {"tnum": 1}
                    }
                }
            }
            Text {
                objectName: "calendarSearchNone"
                visible: popup.wordsTyped && popup.answered && !popup.rows.length && !popup.statusText.length
                Layout.fillWidth: true; leftPadding: 14; rightPadding: 14; topPadding: 12; bottomPadding: 12
                text: "No events match"
                color: Theme.inkAlpha(.45); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12)
            }
            Text {
                objectName: "calendarSearchStatus"
                visible: popup.typing && popup.statusText.length > 0
                Layout.fillWidth: true; leftPadding: 14; rightPadding: 14; topPadding: 10; bottomPadding: 10; wrapMode: Text.Wrap; textFormat: Text.PlainText
                text: popup.statusText
                color: Theme.inkAlpha(.5); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12)
            }
            // Empty query: recent searches, then the operators.
            ColumnLayout {
                visible: !popup.typing && backend.calendarRecentSearches.length > 0
                Layout.fillWidth: true; spacing: 0
                SectionLabel { text: "Recent" }
                Repeater {
                    model: backend.calendarRecentSearches
                    SearchRow {
                        id: recent
                        required property string modelData
                        required property int index
                        objectName: "calendarRecentSearch_" + index
                        accessibleName: "Search " + modelData
                        onClicked: popup.commit(modelData)
                        FmIcon { name: "clock"; size: 12; color: Theme.inkAlpha(.4) }
                        Text { text: recent.modelData; color: Theme.text; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12); Layout.fillWidth: true; elide: Text.ElideRight; textFormat: Text.PlainText }
                        RemoveButton { objectName: "removeCalendarRecentSearch_" + recent.index; tip: "Remove from recent"; onClicked: backend.removeCalendarRecentSearch(recent.modelData) }
                    }
                }
                Item { implicitHeight: 4 }
                Rule {}
            }
            ColumnLayout {
                visible: !popup.typing
                Layout.fillWidth: true; spacing: 0
                SectionLabel { text: "Narrow it down" }
                Repeater {
                    model: popup.operators
                    SearchRow {
                        id: operatorRow
                        required property var modelData
                        objectName: "calendarSearchOperator_" + modelData.op.slice(0, -1)
                        accessibleName: modelData.op + " " + modelData.description
                        onClicked: popup.edit((popup.query.trim() ? popup.query.trim() + " " : "") + modelData.op)
                        OperatorChip { text: operatorRow.modelData.op; minimumWidth: 64 }
                        Text { text: operatorRow.modelData.description; color: Theme.inkAlpha(.75); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12); elide: Text.ElideRight; Layout.fillWidth: true }
                        Text { text: operatorRow.modelData.example; color: Theme.inkAlpha(.35); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(11.5); elide: Text.ElideLeft; Layout.maximumWidth: popup.availableWidth * .42; textFormat: Text.PlainText }
                    }
                }
                Item { implicitHeight: 2 }
            }
        }
    }
}
