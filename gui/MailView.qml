import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    id: mail
    property var thread: backend.conversation
    property var members: thread.messages || []
    property bool expanded: false
    property bool anyUnread: members.some(function(m) { return m.unread })
    property bool anyStarred: members.some(function(m) { return m.starred })
    property bool restoring: false
    property var matchingIds: {
        let row = backend.conversations.find(function(c) { return c.id === backend.selectedId })
        return row && row.matching_message_ids ? row.matching_message_ids : []
    }
    function categoryCount(id) {
        return members.filter(function(message) { return (message.categories || []).indexOf(id) >= 0 }).length
    }

    function restorePosition() {
        restoring = true
        let view = backend.presentation
        if (view.splitWidth) leftPane.SplitView.preferredWidth = view.splitWidth
        list.contentY = Math.max(0, view.listY || 0)
        reader.contentY = Math.max(0, view.readerY || 0)
        expanded = view.expanded || backend.query.length > 0
        Qt.callLater(function() { restoring = false })
    }
    Connections {
        target: backend
        function onPresentationChanged() { Qt.callLater(mail.restorePosition) }
        function onConversationsChanged() {
            Qt.callLater(function() { list.contentY = Math.max(0, backend.presentation.listY || 0) })
        }
        function onConversationChanged() {
            if (backend.query.length > 0) mail.expanded = true
        }
        function onDraftChanged() {
            let value = backend.draft.body || ""
            if (composer.text !== value) composer.text = value
        }
    }
    SplitView {
        id: split
        anchors.fill: parent; orientation: Qt.Horizontal
        handle: Rectangle {
            implicitWidth: 5
            color: SplitHandle.pressed ? Theme.accent : SplitHandle.hovered ? Theme.subtle : Theme.line
            containmentMask: Item { x: -3; width: 11; height: split.height }
        }
        Item {
            id: leftPane
            SplitView.preferredWidth: mail.width * 0.47
            SplitView.minimumWidth: 330
            onWidthChanged: { if (!mail.restoring && width > 0) backend.remember("splitWidth", width) }
            ColumnLayout {
                anchors.fill: parent; spacing: 0
                ColumnLayout {
                    Layout.fillWidth: true; Layout.margins: 18; spacing: 9
                    RowLayout {
                        spacing: 1
                        FmButton { text: "Inbox"; selected: backend.folder === "inbox"; font.pixelSize: 18; font.bold: true; onClicked: backend.folder = "inbox" }
                        FmButton { text: "Sent"; selected: backend.folder === "sent"; font.pixelSize: 18; onClicked: backend.folder = "sent" }
                        FmButton { text: "All mail"; selected: backend.folder === "all"; onClicked: backend.folder = "all" }
                        Item { Layout.fillWidth: true }
                    }
                    Text {
                        Layout.leftMargin: 10
                        text: (backend.currentAccount.unread_count || 0) + " unread messages  ·  " + (backend.currentAccount.starred_count || 0) + " starred for later"
                        color: Theme.muted; font.pixelSize: 11
                    }
                    Flickable {
                        Layout.fillWidth: true; implicitHeight: 30; clip: true
                        contentWidth: categoryRow.width; contentHeight: 30
                        boundsBehavior: Flickable.StopAtBounds
                        Row {
                            id: categoryRow; spacing: 4
                            FmButton { text: "All categories"; selected: backend.category.length === 0; onClicked: backend.category = ""; font.pixelSize: 10 }
                            Repeater {
                                model: backend.categories
                                FmButton {
                                    required property var modelData
                                    text: modelData.name + "  " + modelData.conversation_count
                                    selected: backend.category === modelData.id; outlined: true; font.pixelSize: 10
                                    tip: modelData.conversation_count + " conversations in this category across the account"
                                    onClicked: backend.category = modelData.id
                                }
                            }
                        }
                    }
                    Rectangle {
                        Layout.fillWidth: true; implicitHeight: queryLabel.implicitHeight + 12
                        visible: backend.query.length > 0; color: Theme.surface; radius: 4
                        Text { id: queryLabel; anchors.fill: parent; anchors.margins: 6; text: "Search: " + backend.query + "  ·  source text in this account"; color: Theme.muted; font.pixelSize: 11; wrapMode: Text.Wrap; textFormat: Text.PlainText }
                    }
                }
                Rectangle { Layout.fillWidth: true; height: 1; color: Theme.line }
                ListView {
                    id: list; objectName: "conversationList"
                    Layout.fillWidth: true; Layout.fillHeight: true
                    clip: true; model: backend.conversations
                    boundsBehavior: Flickable.StopAtBounds
                    ScrollBar.vertical: ScrollBar { }
                    onMovementEnded: backend.remember("listY", contentY)
                    delegate: ItemDelegate {
                        id: row
                        required property var modelData
                        width: list.width; height: 52
                        hoverEnabled: true
                        Accessible.name: modelData.subject + ", " + modelData.sender_name + (modelData.unread ? ", unread" : "") + (modelData.starred ? ", starred" : "")
                        onClicked: { mail.expanded = backend.query.length > 0; backend.selectConversation(modelData.id) }
                        background: Rectangle { color: backend.selectedId === row.modelData.id ? Theme.selected : row.hovered ? Theme.surface : "transparent" }
                        contentItem: RowLayout {
                            spacing: 10
                            Text { text: row.modelData.unread ? "●" : row.modelData.starred ? "☆" : "·"; color: row.modelData.starred ? Theme.accent : Theme.text; font.pixelSize: row.modelData.starred ? 16 : 10; Layout.preferredWidth: 13; horizontalAlignment: Text.AlignHCenter }
                            ColumnLayout {
                                Layout.fillWidth: true; spacing: 4
                                RowLayout {
                                    Text { text: row.modelData.subject; color: Theme.text; font.pixelSize: 13; font.bold: row.modelData.unread; elide: Text.ElideRight; Layout.fillWidth: true; textFormat: Text.PlainText }
                                    Text { text: row.modelData.message_count > 1 ? row.modelData.message_count : ""; color: Theme.subtle; font.pixelSize: 10 }
                                    Text { text: row.modelData.starred ? "☆" : ""; color: Theme.accent; font.pixelSize: 13 }
                                }
                                Text { text: row.modelData.sender_name + " · " + row.modelData.snippet.replace(/\s+/g, " "); color: Theme.muted; font.pixelSize: 11; elide: Text.ElideRight; Layout.fillWidth: true; textFormat: Text.PlainText }
                            }
                            Text {
                                text: row.modelData.latest_at ? Qt.formatDateTime(new Date(row.modelData.latest_at), "ddd hh:mm") : ""
                                color: Theme.subtle; font.pixelSize: 10; Layout.preferredWidth: 65; horizontalAlignment: Text.AlignRight
                            }
                        }
                    }
                    footer: Item {
                        width: list.width; height: backend.hasMore ? 46 : 30
                        FmButton { anchors.centerIn: parent; visible: backend.hasMore; text: backend.loading ? "Loading…" : "Load more conversations"; enabled: !backend.loading; onClicked: backend.loadMore() }
                        Text { anchors.centerIn: parent; visible: !backend.hasMore && list.count > 0; text: list.count + " conversations"; color: Theme.subtle; font.pixelSize: 10 }
                    }
                    Text {
                        anchors.centerIn: parent; width: parent.width - 60
                        visible: list.count === 0
                        text: !backend.connected ? "Connecting to your local mail…" : backend.loading ? "Searching…" : backend.query.length ? "No matching conversations in this account." : "No conversations in this view."
                        color: Theme.muted; horizontalAlignment: Text.AlignHCenter; wrapMode: Text.Wrap
                    }
                }
            }
        }
        Item {
            SplitView.fillWidth: true; SplitView.minimumWidth: 560
            ScrollView {
                anchors.fill: parent
                ScrollBar.horizontal.policy: ScrollBar.AlwaysOff
                Flickable {
                    id: reader; objectName: "reader"
                    contentWidth: width; contentHeight: readerColumn.height + 50
                    clip: true; boundsBehavior: Flickable.StopAtBounds
                    onMovementEnded: backend.remember("readerY", contentY)
                    ColumnLayout {
                        id: readerColumn
                        width: Math.min(reader.width - 54, 820)
                        anchors.horizontalCenter: parent.horizontalCenter
                        y: 22; spacing: 14
                        visible: mail.members.length > 0
                        RowLayout {
                            Layout.fillWidth: true; spacing: 5
                            FmButton {
                                id: categoriesButton; objectName: "categoryAssignmentButton"
                                text: "Categories"; outlined: true
                                onClicked: categoryPopup.open()
                                Popup {
                                    id: categoryPopup; objectName: "categoryAssignmentPopup"
                                    y: categoriesButton.height + 6; width: 350; padding: 14
                                    focus: true; closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
                                    background: Rectangle { color: Theme.surface; border.color: Theme.line; radius: 5 }
                                    contentItem: ColumnLayout {
                                        spacing: 9
                                        Text { text: "Categories"; color: Theme.text; font.pixelSize: 14; font.bold: true }
                                        Text { Layout.fillWidth: true; text: "Apply to the " + mail.members.length + " messages shown."; color: Theme.muted; font.pixelSize: 11 }
                                        Repeater {
                                            model: backend.categories
                                            RowLayout {
                                                required property var modelData
                                                property int memberCount: mail.categoryCount(modelData.id)
                                                Layout.fillWidth: true
                                                Text { Layout.fillWidth: true; text: modelData.name; color: Theme.text; font.pixelSize: 12; textFormat: Text.PlainText }
                                                Text { text: memberCount === 0 ? "None" : memberCount === mail.members.length ? "All" : memberCount + " of " + mail.members.length; color: Theme.muted; font.pixelSize: 11 }
                                                FmButton {
                                                    objectName: "categoryAdd_" + modelData.id
                                                    text: memberCount === mail.members.length ? "Clear" : "Apply"
                                                    enabled: !backend.mutationInFlight
                                                    outlined: true; font.pixelSize: 11
                                                    onClicked: backend.setCategoryMembership(modelData.id, memberCount !== mail.members.length)
                                                }
                                                FmButton {
                                                    visible: memberCount > 0 && memberCount < mail.members.length
                                                    text: "Clear"; font.pixelSize: 11; enabled: !backend.mutationInFlight
                                                    onClicked: backend.setCategoryMembership(modelData.id, false)
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            Item { Layout.fillWidth: true }
                            FmButton { text: "↶  Reply"; prominent: true; onClicked: { composer.forceActiveFocus(); reader.contentY = Math.max(0, composerFrame.y - 180) } }
                            FmButton { text: mail.anyStarred ? "★  Clear star" : "☆  Star"; outlined: true; enabled: !backend.mutationInFlight; tip: "Explicitly " + (mail.anyStarred ? "remove the action hold from" : "leave action for later on") + " these " + mail.members.length + " messages"; onClicked: backend.setStarred(!mail.anyStarred) }
                            FmButton { text: mail.anyUnread ? "✓  Mark read" : "Mark unread"; outlined: true; enabled: !backend.mutationInFlight; tip: "Change read state on the " + mail.members.length + " currently shown conversation members only"; onClicked: backend.setRead(mail.anyUnread) }
                        }
                        Flow {
                            Layout.fillWidth: true; spacing: 5; visible: (mail.thread.categories || []).length > 0
                            Repeater {
                                model: mail.thread.categories || []
                                Rectangle {
                                    required property var modelData
                                    implicitWidth: category.implicitWidth + 14; implicitHeight: 23; radius: 3; color: "transparent"; border.color: Theme.line
                                    Text { id: category; anchors.centerIn: parent; text: modelData.name; color: Theme.muted; font.pixelSize: 10; textFormat: Text.PlainText }
                                }
                            }
                        }
                        Text { text: mail.thread.subject || ""; color: Theme.text; font.pixelSize: 25; font.bold: true; wrapMode: Text.Wrap; Layout.fillWidth: true; textFormat: Text.PlainText }
                        Text { text: mail.members.length + " messages  ·  " + (backend.currentAccount.name || "") + "  ·  stored on this machine"; color: Theme.muted; font.pixelSize: 11 }
                        FmButton {
                            Layout.fillWidth: true; visible: mail.members.length > 1
                            text: (mail.expanded ? "⌃  Collapse " : "⌄  Show ") + (mail.members.length - 1) + " earlier messages"
                            outlined: true
                            onClicked: { mail.expanded = !mail.expanded; backend.remember("expanded", mail.expanded) }
                        }
                        Repeater {
                            model: mail.members
                            delegate: ColumnLayout {
                                id: member
                                required property var modelData
                                required property int index
                                Layout.fillWidth: true; spacing: 12
                                visible: mail.expanded || index === mail.members.length - 1
                                RowLayout {
                                    Layout.fillWidth: true
                                    Text { text: member.modelData.sender_name; color: Theme.text; font.pixelSize: 12; font.bold: true; textFormat: Text.PlainText }
                                    Text { text: member.modelData.unread ? "unread" : "read"; color: Theme.subtle; font.pixelSize: 10 }
                                    Text { text: member.modelData.starred ? "★  action for later" : ""; color: Theme.accent; font.pixelSize: 10 }
                                    Text { text: backend.query.length > 0 && mail.matchingIds.indexOf(member.modelData.id) >= 0 ? "Search match" : ""; color: Theme.accent; font.pixelSize: 10 }
                                    Item { Layout.fillWidth: true }
                                    Text { text: Qt.formatDateTime(new Date(member.modelData.sent_at), "ddd d MMM, hh:mm"); color: Theme.subtle; font.pixelSize: 10 }
                                }
                                FmButton { text: member.modelData.sender_email; font.pixelSize: 10; onClicked: backend.query = member.modelData.sender_email; tip: "Search mail from this address" }
                                TextArea {
                                    Layout.fillWidth: true
                                    text: member.modelData.text || "Readable content is unavailable."
                                    readOnly: true; selectByMouse: true; wrapMode: TextEdit.Wrap
                                    textFormat: TextEdit.PlainText
                                    color: Theme.text; font.pixelSize: 14
                                    leftPadding: 0; rightPadding: 0; topPadding: 0; bottomPadding: 0
                                    background: null
                                    Accessible.name: "Message from " + member.modelData.sender_name
                                }
                                Flow {
                                    Layout.fillWidth: true; spacing: 8
                                    Repeater {
                                        model: backend.linksInText(member.modelData.text || "")
                                        FmButton {
                                            required property string modelData
                                            text: "↗  " + (modelData.length > 60 ? modelData.substring(0, 60) + "…" : modelData)
                                            font.pixelSize: 11; tip: modelData
                                            onClicked: backend.openLink(modelData)
                                        }
                                    }
                                }
                                Flow {
                                    Layout.fillWidth: true; spacing: 8
                                    Repeater {
                                        model: member.modelData.attachments || []
                                        FmButton {
                                            required property var modelData
                                            text: "⌁  " + modelData.name + "  ·  " + (modelData.size / 1024).toFixed(1) + " KB  ·  " + modelData.state
                                            outlined: true; font.pixelSize: 11
                                            onClicked: backend.openAttachment(modelData.id)
                                            tip: "Open the retained attachment in its default application"
                                        }
                                    }
                                }
                                Rectangle { Layout.fillWidth: true; height: 1; color: Theme.line; Layout.topMargin: 8; Layout.bottomMargin: 4 }
                            }
                        }
                        Rectangle {
                            id: composerFrame
                            Layout.fillWidth: true; implicitHeight: composeColumn.implicitHeight + 2
                            color: Theme.surface; border.color: composer.activeFocus ? "#575044" : Theme.line; radius: 5
                            ColumnLayout {
                                id: composeColumn; anchors.fill: parent; anchors.margins: 1; spacing: 0
                                Text {
                                    Layout.fillWidth: true; Layout.margins: 12
                                    text: "To " + (backend.draft.to || "") + "  ·  from " + (backend.draft.from || backend.currentAccount.email || "")
                                    color: Theme.muted; font.pixelSize: 11; elide: Text.ElideRight; textFormat: Text.PlainText
                                }
                                Rectangle { Layout.fillWidth: true; height: 1; color: Theme.line }
                                TextArea {
                                    id: composer; objectName: "replyComposer"
                                    Layout.fillWidth: true; Layout.minimumHeight: 112
                                    placeholderText: "Write your reply…"
                                    placeholderTextColor: Theme.subtle; color: Theme.text
                                    textFormat: TextEdit.PlainText; wrapMode: TextEdit.Wrap; selectByMouse: true
                                    persistentSelection: true
                                    enabled: Object.keys(backend.draft).length > 0
                                    font.pixelSize: 14; padding: 13
                                    onTextChanged: if (activeFocus) backend.editDraft(text)
                                    background: null
                                    Accessible.name: "Human reply draft"
                                }
                                Text {
                                    Layout.fillWidth: true; Layout.leftMargin: 13; Layout.rightMargin: 13; Layout.bottomMargin: visible ? 12 : 0
                                    visible: (backend.currentAccount.signature_text || "").length > 0
                                    text: backend.currentAccount.signature_text || ""
                                    color: Theme.muted; font.pixelSize: 12; wrapMode: Text.Wrap; textFormat: Text.PlainText
                                }
                                Rectangle { Layout.fillWidth: true; height: 1; color: Theme.line }
                                RowLayout {
                                    Layout.fillWidth: true; Layout.margins: 10
                                    FmButton { text: "Save draft"; outlined: true; onClicked: backend.saveDraft() }
                                    Text { text: "Sending comes with Google sync"; color: Theme.subtle; font.pixelSize: 10; Layout.fillWidth: true; wrapMode: Text.Wrap }
                                    Text { objectName: "draftStatus"; text: backend.draftStatus; color: Theme.muted; font.pixelSize: 10; wrapMode: Text.Wrap; Layout.maximumWidth: 155 }
                                }
                            }
                        }
                    }
                    Text { anchors.centerIn: parent; visible: mail.members.length === 0; text: "Select a conversation"; color: Theme.subtle; font.pixelSize: 16 }
                }
            }
        }
    }
}
