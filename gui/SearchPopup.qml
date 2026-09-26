import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Effects

// Smart search dropdown under the Mail search box. The box holds the whole
// query and Core (fm-core's query parser) parses it. Empty, this offers recent searches and the advertised
// operators. While typing it shows how Core read the text, the result count,
// completion of a partial operator and people after a trailing from: or to:.
// label:, has: and is: still parse but are not advertised.
Popup {
    id: popup
    objectName: "mailSearchPopup"
    // The search field this edits. It keeps focus while the dropdown is open.
    property Item field: null
    property real maxHeight: 560
    signal committed()
    readonly property bool draftScope: backend.folder === "drafts"
    readonly property bool outgoingScope: draftScope || backend.folder === "sent"
    readonly property string query: backend.query
    readonly property bool typing: query.trim().length > 0
    // Core's reading of the current text; null until the search answers. The
    // last reading stays on screen meanwhile, so chips do not blink per key,
    // but only a current one can be edited.
    readonly property var reading: backend.searchTerms.text === query ? backend.searchTerms : null
    readonly property var shownReading: typing && backend.searchTerms.text !== undefined ? backend.searchTerms : null
    readonly property string readingError: reading ? backend.searchError : ""
    readonly property var operators: [
        {op: "subject:", description: "Only in subjects", example: "subject:board deck"},
        {op: "body:", description: "Only in message text", example: "body:invoice attached"},
        {op: "from:", description: "Only the sender", example: "from:maya@example.com"},
        {op: "to:", description: "Recipient name or address", example: "to:finance@example.com"}
    ]
    readonly property string lastToken: { const match = query.match(/(\S*)$/); return match ? match[1].toLowerCase() : "" }
    readonly property var completions: lastToken.length >= 2 && lastToken.indexOf(":") < 0 ? operators.filter(o => o.op.startsWith(lastToken)) : []
    // A trailing from: or to: and its value so far, or null when the text does not end in one.
    readonly property var personPart: { const match = query.match(/(?:^|\s)(from|to):([^:]*)$/i); return match ? {role: match[1].toLowerCase() === "to" ? "recipient" : "sender", op: match[1].toLowerCase(), value: match[2].trim()} : null }
    readonly property string personKey: personPart ? personPart.role + ":" + personPart.value : ""
    readonly property var people: !personPart || backend.searchPeopleQuery !== personKey ? []
        : backend.searchPeople.filter(p => (p.email || "").toLowerCase() !== personPart.value.toLowerCase()).slice(0, 5)
    readonly property var chips: {
        const t = shownReading, out = []
        if (!t) return out
        // Bare addresses lead, as textWithout() writes them back.
        ;(t.people || []).forEach((value, index) => out.push({key: draftScope ? "To" : "From or To", value: value, part: "people", index: index}))
        if (t.words) out.push({key: "Subject or body", value: t.words, part: "words"})
        ;(t.subject || []).forEach((value, index) => out.push({key: "Subject", value: value, part: "subject", index: index}))
        ;(t.body || []).forEach((value, index) => out.push({key: "Body", value: value, part: "body", index: index}))
        ;(t.from || []).forEach((value, index) => out.push({key: draftScope ? "To" : "From", value: value, part: "from", index: index}))
        ;(t.to || []).forEach((value, index) => out.push({key: "To", value: value, part: "to", index: index}))
        if (t.label) out.push({key: "Label", value: t.label, part: "label"})
        if (t.has_attachment) out.push({key: "Has", value: "attachment", part: "has_attachment"})
        if (t.is) out.push({key: "Status", value: ({review: "To review", delayed: "Delayed", done: "Done"})[t.is] || t.is, part: "is"})
        return out
    }
    readonly property string liveCount: {
        const count = backend.querySummary.filtered_conversation_count || 0
        return count + (draftScope ? (count === 1 ? " draft" : " drafts") : (count === 1 ? " conversation" : " conversations"))
    }
    property string settledCount: ""
    Binding on settledCount { value: popup.liveCount; when: popup.reading !== null; restoreMode: Binding.RestoreNone }
    readonly property string countText: reading ? liveCount : settledCount.length ? settledCount : "Searching…"
    // Rebuilds the text from Core's reading without one of its parts.
    function textWithout(chip) {
        const t = JSON.parse(JSON.stringify(reading || {}))
        if (chip.part === "words") t.words = ""
        else if (["subject", "body", "from", "to", "people"].indexOf(chip.part) >= 0) t[chip.part].splice(chip.index, 1)
        else if (chip.part === "label") t.label = null
        else if (chip.part === "has_attachment") t.has_attachment = false
        else if (chip.part === "is") t.is = null
        // Bare addresses first: an operator's value runs to the next operator.
        const parts = (t.people || []).slice()
        if (t.words) parts.push(t.words)
        for (const value of t.subject || []) parts.push("subject:" + value)
        for (const value of t.body || []) parts.push("body:" + value)
        for (const value of t.from || []) parts.push("from:" + value)
        for (const value of t.to || []) parts.push("to:" + value)
        if (t.label) parts.push("label:" + (/\s/.test(t.label) ? "\"" + t.label + "\"" : t.label))
        if (t.has_attachment) parts.push("has:attachment")
        if (t.is) parts.push("is:" + t.is)
        return parts.join(" ")
    }
    // Replaces the text and keeps typing at its end.
    function edit(text) {
        backend.query = text
        if (!field) return
        field.forceActiveFocus()
        Qt.callLater(function() { if (popup.field) popup.field.cursorPosition = popup.field.length })
    }
    // Enter or a recent search: run it, remember it and close.
    function commit(text) {
        const value = (text === undefined ? query : text).trim()
        backend.query = value
        if (value.length) backend.commitSearch(value)
        close()
        committed()
    }
    width: Math.min(440, (Overlay.overlay ? Overlay.overlay.width : 472) - 32)
    height: Math.min(contents.implicitHeight + topPadding + bottomPadding, maxHeight)
    topPadding: 4; bottomPadding: 6; leftPadding: 0; rightPadding: 0
    focus: false
    // The parent is the search box: pressing it keeps the dropdown open.
    closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside | Popup.CloseOnPressOutsideParent
    Connections { target: backend; function onAccountChanged() { popup.close() } }
    Timer { id: peopleTimer; interval: 120; onTriggered: if (popup.opened && popup.personPart) backend.requestSearchPeople(popup.personPart.value, popup.personPart.role) }
    onPersonKeyChanged: if (personPart) peopleTimer.restart()
    onOpened: if (personPart) peopleTimer.restart()

    component SectionLabel: Text {
        Layout.fillWidth: true; leftPadding: 14; rightPadding: 14; topPadding: 10; bottomPadding: 6
        color: Theme.inkAlpha(.4); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(11)
        font.letterSpacing: Theme.fontSize(11) * .05; font.capitalization: Font.AllUppercase
    }
    component Rule: Rectangle { Layout.fillWidth: true; implicitHeight: Theme.ruleWidth; color: Theme.line }
    component SearchRow: Rectangle {
        id: searchRow
        signal clicked()
        default property alias content: rowContent.data
        property string accessibleName
        Layout.fillWidth: true
        implicitHeight: Math.max(32, rowContent.implicitHeight)
        color: rowArea.containsMouse ? Theme.bg4 : "transparent"
        Accessible.role: Accessible.Button; Accessible.name: accessibleName
        Accessible.onPressAction: clicked()
        MouseArea { id: rowArea; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: searchRow.clicked() }
        RowLayout { id: rowContent; anchors.fill: parent; anchors.leftMargin: 14; anchors.rightMargin: 14; spacing: 10 }
    }
    component RemoveButton: Rectangle {
        id: removeButton
        signal clicked()
        property string tip: "Remove"
        property real idleOpacity: .35
        implicitWidth: 20; implicitHeight: 20; radius: 4
        color: removeArea.containsMouse ? Theme.bg6 : "transparent"
        Accessible.role: Accessible.Button; Accessible.name: tip; Accessible.onPressAction: clicked()
        FmIcon { anchors.centerIn: parent; name: "close"; size: 9; color: removeArea.containsMouse ? Theme.text : Theme.inkAlpha(removeButton.idleOpacity) }
        MouseArea { id: removeArea; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: removeButton.clicked() }
        FmToolTip { shown: removeArea.containsMouse; text: removeButton.tip }
    }
    component OperatorChip: Rectangle {
        property alias text: operatorLabel.text
        property real minimumWidth: 0
        implicitWidth: Math.max(minimumWidth, operatorLabel.implicitWidth + 12); implicitHeight: operatorLabel.implicitHeight + 2
        radius: 3; color: Theme.bg4
        Text { id: operatorLabel; x: 6; anchors.verticalCenter: parent.verticalCenter; color: Theme.text; font.family: backend.fixedFontFamily(); font.pointSize: Theme.fontPoints(12); textFormat: Text.PlainText }
    }

    background: Rectangle {
        color: Theme.surface; border.color: Theme.controlBorder; radius: 8
        layer.enabled: GraphicsInfo.api !== GraphicsInfo.Software
        layer.effect: MultiEffect { shadowEnabled: true; shadowColor: Theme.shadowAlpha(.6); shadowBlur: 1; shadowVerticalOffset: 24 }
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
            // While typing: the reading, then completion or people.
            RowLayout {
                visible: popup.typing
                Layout.fillWidth: true; Layout.leftMargin: 14; Layout.rightMargin: 14; Layout.topMargin: 10; Layout.bottomMargin: 10
                spacing: 6
                Flow {
                    id: readingFlow
                    objectName: "mailSearchReading"
                    visible: popup.chips.length > 0
                    Layout.fillWidth: true; spacing: 6
                    Repeater {
                        model: popup.chips
                        Rectangle {
                            id: readChip
                            required property var modelData
                            required property int index
                            objectName: "mailSearchChip_" + index
                            implicitHeight: 24; implicitWidth: Math.min(readingFlow.width, chipLayout.implicitWidth + 12)
                            width: implicitWidth; height: implicitHeight
                            radius: 4; color: "transparent"; border.color: Theme.controlBorder
                            Accessible.role: Accessible.StaticText; Accessible.name: modelData.key + " " + modelData.value
                            RowLayout {
                                id: chipLayout
                                anchors.fill: parent; anchors.leftMargin: 8; anchors.rightMargin: 4; spacing: 6
                                Text { text: readChip.modelData.key; color: Theme.inkAlpha(.45); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12); textFormat: Text.PlainText }
                                Text { text: readChip.modelData.value; color: Theme.text; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12); elide: Text.ElideRight; Layout.fillWidth: true; Layout.maximumWidth: Math.ceil(implicitWidth); textFormat: Text.PlainText }
                                RemoveButton {
                                    objectName: "removeMailSearchChip_" + readChip.index
                                    enabled: popup.reading !== null
                                    implicitWidth: 16; implicitHeight: 16; radius: 3; idleOpacity: .45
                                    tip: "Remove " + readChip.modelData.key.toLowerCase() + " " + readChip.modelData.value
                                    onClicked: if (popup.reading) popup.edit(popup.textWithout(readChip.modelData))
                                }
                            }
                        }
                    }
                }
                Text {
                    objectName: "mailSearchError"
                    visible: popup.readingError.length > 0
                    Layout.fillWidth: true; wrapMode: Text.Wrap
                    text: popup.readingError; color: Theme.warn
                    font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12); textFormat: Text.PlainText
                }
                Item { visible: popup.chips.length === 0 && popup.readingError.length === 0; Layout.fillWidth: true }
                Text {
                    objectName: "mailSearchCount"
                    visible: popup.readingError.length === 0
                    Layout.alignment: Qt.AlignRight | Qt.AlignTop; Layout.topMargin: popup.chips.length ? 4 : 0
                    text: popup.countText; color: Theme.inkAlpha(.45)
                    font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12); font.features: {"tnum": 1}
                }
            }
            ColumnLayout {
                visible: popup.typing && popup.completions.length > 0
                Layout.fillWidth: true; spacing: 0
                Rule {}
                Item { implicitHeight: 4 }
                Repeater {
                    model: popup.completions
                    SearchRow {
                        id: completion
                        required property var modelData
                        objectName: "mailSearchComplete_" + modelData.op.slice(0, -1)
                        accessibleName: "Complete " + modelData.op
                        onClicked: popup.edit(popup.query.slice(0, popup.query.length - popup.lastToken.length) + modelData.op)
                        OperatorChip { text: completion.modelData.op }
                        Text { text: completion.modelData.description; color: Theme.inkAlpha(.6); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(13); Layout.fillWidth: true; elide: Text.ElideRight }
                    }
                }
            }
            ColumnLayout {
                visible: popup.typing && popup.people.length > 0
                Layout.fillWidth: true; spacing: 0
                Rule {}
                SectionLabel { text: popup.personPart && popup.personPart.role === "recipient" ? "Recipients" : "Senders" }
                Repeater {
                    model: popup.people
                    SearchRow {
                        id: person
                        required property var modelData
                        required property int index
                        readonly property string name: modelData.name || ""
                        objectName: "mailSearchPerson_" + index
                        accessibleName: (name || modelData.email) + ", " + modelData.email
                        onClicked: popup.edit(popup.query.replace(/(from|to):([^:]*)$/i, popup.personPart.op + ":" + modelData.email) + " ")
                        Rectangle {
                            implicitWidth: 20; implicitHeight: 20; radius: 10; color: Theme.bg4
                            Text {
                                anchors.centerIn: parent
                                text: (person.name || person.modelData.email).split(/\s+/).filter(w => w.length).slice(0, 2).map(w => w[0]).join("").toUpperCase()
                                color: Theme.inkAlpha(.8); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(9); font.weight: Font.Medium
                            }
                        }
                        Text { text: person.name || person.modelData.email; color: Theme.text; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(13); Layout.maximumWidth: popup.availableWidth * .5; elide: Text.ElideRight; textFormat: Text.PlainText }
                        Text { visible: person.name.length > 0; text: person.modelData.email; color: Theme.inkAlpha(.45); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(13); Layout.fillWidth: true; elide: Text.ElideRight; textFormat: Text.PlainText }
                        Item { visible: person.name.length === 0; Layout.fillWidth: true }
                    }
                }
            }
            // Empty query: recent searches, then the operators.
            ColumnLayout {
                visible: !popup.typing && backend.recentSearches.length > 0
                Layout.fillWidth: true; spacing: 0
                SectionLabel { text: "Recent" }
                Repeater {
                    model: backend.recentSearches
                    SearchRow {
                        id: recent
                        required property string modelData
                        required property int index
                        objectName: "mailRecentSearch_" + index
                        accessibleName: "Search " + modelData
                        onClicked: popup.commit(modelData)
                        FmIcon { name: "clock"; size: 12; color: Theme.inkAlpha(.4) }
                        Text { text: recent.modelData; color: Theme.text; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(13); Layout.fillWidth: true; elide: Text.ElideRight; textFormat: Text.PlainText }
                        RemoveButton { objectName: "removeMailRecentSearch_" + recent.index; tip: "Remove from recent"; onClicked: backend.removeRecentSearch(recent.modelData) }
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
                        objectName: "mailSearchOperator_" + modelData.op.slice(0, -1)
                        accessibleName: modelData.op + " " + modelData.description
                        onClicked: popup.edit((popup.query.trim() ? popup.query.trim() + " " : "") + modelData.op)
                        OperatorChip { text: operatorRow.modelData.op; minimumWidth: 64 }
                        Text { text: operatorRow.modelData.description; color: Theme.inkAlpha(.75); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(13); elide: Text.ElideRight; Layout.fillWidth: true }
                        Text { text: operatorRow.modelData.example; color: Theme.inkAlpha(.35); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12); elide: Text.ElideLeft; Layout.maximumWidth: popup.availableWidth * .42; textFormat: Text.PlainText }
                    }
                }
                Text {
                    objectName: "mailSearchAddressHint"
                    Layout.fillWidth: true; Layout.leftMargin: 14; Layout.rightMargin: 14; Layout.topMargin: 4; Layout.bottomMargin: 6
                    text: popup.draftScope ? "An address on its own finds drafts to it." : "An address on its own finds mail from or to it."
                    color: Theme.inkAlpha(.45); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12); wrapMode: Text.Wrap; textFormat: Text.PlainText
                }
            }
            // Also search the account on the server; its results join the list.
            Rectangle {
                id: remoteRow
                objectName: "mailSearchRemote"
                readonly property var search: backend.remoteSearchState
                readonly property bool on: backend.remoteSearch
                readonly property string note: !on || !popup.typing ? ""
                    : search.busy ? "Searching " + (search.address || "the server") + "…"
                    : search.error ? search.error
                    : search.count > 0 ? search.count + (search.count === 1 ? " more result" : " more results") + " from the server"
                    : "Nothing more on the server"
                visible: !backend.syntheticProfile && backend.folder !== "spam"
                Layout.fillWidth: true; Layout.topMargin: 6
                implicitHeight: remoteContent.implicitHeight + 16
                color: remoteArea.containsMouse ? Theme.bg4 : "transparent"
                Rectangle { width: parent.width; height: Theme.ruleWidth; color: Theme.line }
                Accessible.role: Accessible.CheckBox; Accessible.name: "Search remote mailbox"; Accessible.checkable: true; Accessible.checked: on
                Accessible.onToggleAction: backend.remoteSearch = !on; Accessible.onPressAction: backend.remoteSearch = !on
                MouseArea { id: remoteArea; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: backend.remoteSearch = !remoteRow.on }
                RowLayout {
                    id: remoteContent
                    anchors.left: parent.left; anchors.right: parent.right; anchors.leftMargin: 14; anchors.rightMargin: 14
                    y: 10; spacing: 10
                    FmIcon {
                        name: "cloud"; size: 14; color: remoteRow.on ? Theme.text : Theme.inkAlpha(.45)
                        SequentialAnimation on opacity {
                            running: remoteRow.on && remoteRow.search.busy && popup.typing; loops: Animation.Infinite
                            onRunningChanged: if (!running) parent.opacity = 1
                            NumberAnimation { to: .35; duration: 500; easing.type: Easing.InOutSine }
                            NumberAnimation { to: 1; duration: 500; easing.type: Easing.InOutSine }
                        }
                    }
                    ColumnLayout {
                        Layout.fillWidth: true; spacing: 1
                        Text { text: "Search remote mailbox"; color: Theme.text; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(13) }
                        Text {
                            objectName: "mailSearchRemoteNote"
                            visible: remoteRow.note.length > 0; text: remoteRow.note; Layout.fillWidth: true; elide: Text.ElideRight; textFormat: Text.PlainText
                            color: !remoteRow.search.busy && !remoteRow.search.error && remoteRow.search.count > 0 ? Theme.text : Theme.inkAlpha(.45)
                            font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12)
                        }
                    }
                    FmSwitch {
                        objectName: "mailSearchRemoteSwitch"
                        checked: remoteRow.on; implicitWidth: 28; implicitHeight: 16
                        onClicked: backend.remoteSearch = checked
                        Accessible.name: "Search remote mailbox"
                    }
                }
            }
            Item { visible: remoteRow.visible; implicitHeight: 2 }
        }
    }
}
