import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    id: mail
    objectName: "mailWorkspace"
    property var thread: backend.conversation
    property var members: thread.messages || []
    property bool expanded: false
    property bool replyOpen: false
    property var replyStates: ({})
    property string hydratedDraftIdentity: ""
    property bool restoring: false
    property var expandedMembers: ({})
    property string loadedConversation: ""
    property string pendingReveal: ""
    property string replyRevealIdentity: ""
    readonly property bool anyUnread: members.some(function(m) { return m.unread })
    readonly property bool anyStarred: members.some(function(m) { return m.starred })
    readonly property int rowHeight: backend.preferences.density === "compact" ? Theme.compactRowHeight : Theme.comfortableRowHeight
    readonly property bool filtering: backend.query.length > 0 || Object.keys(backend.filters).length > 0 || backend.category.length > 0
    readonly property var matchingIds: {
        const row = backend.conversations.find(function(c) { return c.id === backend.selectedId })
        return row && row.matching_message_ids ? row.matching_message_ids : []
    }
    readonly property var latest: members.length ? members[members.length - 1] : ({})
    readonly property string expansionKey: backend.account + "/" + backend.selectedId + "/"
    readonly property string coverageText: backend.querySummary.coverage ? backend.querySummary.coverage.text || "" : ""
    readonly property string filterDescription: {
        const f = backend.filters, labels = []
        if (backend.query.length) labels.push('“' + backend.query + '”')
        if (f.sender) labels.push("From " + f.sender)
        if (f.status && f.status !== "all") labels.push({review:"To review",held:"Delayed",done:"Done"}[f.status] || f.status)
        if (f.has_attachment === true) labels.push("With attachments")
        if (f.has_attachment === false) labels.push("Without attachments")
        if (f.after) labels.push("From " + f.after + " UTC")
        if (f.before) labels.push("Before " + f.before + " UTC")
        return labels.join(" · ")
    }
    onVisibleChanged: if (!visible) { filterPopup.close(); categoryPopup.close(); morePopup.close() }
    function categoryCount(id) { return members.filter(function(m) { return (m.categories || []).indexOf(id) >= 0 }).length }
    function groupOf(row) { return row.display_group || row.group || "done" }
    function groupTitle(group) { return {review:"To review",held:"Delayed",done:"Done"}[group] || "Conversations" }
    function groupCount(group) { return backend.conversations.filter(function(row) { return groupOf(row) === group }).length }
    function restoreListPosition() {
        if (list.moving) return
        list.forceLayout()
        const minimum = list.originY
        const maximum = minimum + Math.max(0,list.contentHeight-list.height)
        // A shorter result set must remain visible. Keep the saved position so
        // clearing the query can restore the longer mailbox's previous offset.
        list.contentY = Math.max(minimum,Math.min(maximum,backend.presentation.listY||0))
    }
    function rowTime(value) {
        const date = new Date(value)
        return isNaN(date.getTime()) ? "" : Qt.formatDate(date, "yyyy-MM-dd") === backend.todayDate ? Qt.formatTime(date,"HH:mm") : Qt.formatDate(date,"d MMM")
    }
    function memberIsExpanded(id, index) { return index === members.length - 1 || expandedMembers[expansionKey + id] === true }
    function toggleMember(id, index) {
        if (index === members.length - 1) return
        const next = Object.assign({}, expandedMembers), key = expansionKey + id
        next[key] = !next[key]; expandedMembers = next
        backend.remember("selectedMember", next[key] ? id : "")
    }
    function scrollToMember(id) {
        expanded = true; backend.remember("expanded",true)
        const next = Object.assign({},expandedMembers); next[expansionKey+id] = true; expandedMembers = next
        backend.remember("selectedMember",id)
        reader.cancelFlick()
        matchReveal.identity = backend.account + "/" + backend.selectedId
        matchReveal.query = backend.query; matchReveal.memberId = id
        matchReveal.pass = 0; matchReveal.stable = 0; matchReveal.lastTop = -1; matchReveal.lastHeight = -1
        matchReveal.restart()
    }
    Timer {
        id: matchReveal
        interval: 16; repeat: true
        property string identity: ""
        property string query: ""
        property string memberId: ""
        property int pass: 0
        property int stable: 0
        property real lastTop: -1
        property real lastHeight: -1
        onTriggered: {
            if (!mail.visible || reader.dragging || identity !== backend.account+"/"+backend.selectedId || query !== backend.query) { stop(); return }
            const index = mail.members.findIndex(function(member) { return member.id === memberId })
            const item = index < 0 ? null : memberRepeater.itemAt(index)
            if (!item || !item.visible) { stop(); return }
            ++pass
            const top = item.mapToItem(reader.contentItem,0,0).y
            const height = reader.contentHeight
            stable = top === lastTop && height === lastHeight ? stable+1 : 0
            lastTop = top; lastHeight = height
            // Expansion changes several nested layouts. Wait for their mapped
            // geometry, then confirm it again after positioning the reader.
            if (item.height>0 && height>=top+item.height-1 && (stable>=1 || pass>=20)) {
                reader.contentY = Math.max(0,Math.min(Math.max(0,height-reader.height),top-12))
                backend.remember("readerY",reader.contentY)
                if (stable>=2 || pass>=20) stop()
            } else if (pass>=20) stop()
        }
    }
    function revealMatch() { if (matchingIds.length) scrollToMember(matchingIds[0]) }
    function openRow(id) {
        pendingReveal = id; backend.openConversation(id)
        if (backend.selectedId === id && members.length) { if (matchingIds.length) revealMatch(); pendingReveal = "" }
    }
    function setReplyOpen(value) {
        const identity = backend.account + "/" + backend.selectedId
        const next = Object.assign({},replyStates); next[identity] = value; replyStates = next
        replyOpen = value; backend.remember("replyOpen",value)
    }
    function openReply() {
        setReplyOpen(true)
        replyRevealIdentity = backend.account + "/" + backend.selectedId
        replyReveal.pass = 0; replyReveal.stable = 0; replyReveal.lastTop = -1; replyReveal.lastHeight = -1
        replyReveal.restart()
    }
    Timer {
        id: replyReveal
        interval: 16; repeat: true
        property int pass: 0
        property int stable: 0
        property real lastTop: -1
        property real lastHeight: -1
        onTriggered: {
            if (!mail.visible || !mail.replyOpen || mail.replyRevealIdentity !== backend.account+"/"+backend.selectedId) { stop(); return }
            ++pass
            const top = reply.mapToItem(reader.contentItem,0,0).y
            const height = reader.contentHeight
            stable = top === lastTop && height === lastHeight ? stable+1 : 0
            lastTop = top; lastHeight = height
            if (reply.visible && reply.height>0 && height>=top+reply.height-1 && (stable>=1 || pass>=12)) {
                reader.cancelFlick()
                reader.contentY = Math.max(0,Math.min(height-reader.height,top-12))
                backend.remember("readerY",reader.contentY)
                reply.focusEditor()
                if (stable>=2 || pass>=12) stop()
            } else if (pass>=12) stop()
        }
    }
    function restorePosition() {
        restoring = true
        const view = backend.presentation
        if (view.splitWidth) leftPane.SplitView.preferredWidth = Math.max(340,Math.min(mail.width-470,view.splitWidth))
        expanded = view.expanded || false
        const identity = backend.account + "/" + backend.selectedId
        if (view.selected === backend.selectedId && view.replyOpen !== undefined) {
            const replies = Object.assign({},replyStates); replies[identity] = view.replyOpen === true; replyStates = replies
        }
        replyOpen = replyStates[identity] === true
        const chosen = view.selectedMember || ""
        if (chosen && members.some(function(m) { return m.id === chosen })) {
            const next = Object.assign({},expandedMembers); next[expansionKey+chosen] = true; expandedMembers = next; expanded = true
        }
        Qt.callLater(function() { mail.restoreListPosition(); if (!replyReveal.running) reader.contentY = Math.max(0,view.readerY||0); restoring = false })
    }
    Connections {
        target: backend
        function onPresentationChanged() { Qt.callLater(mail.restorePosition) }
        function onConversationsChanged() { Qt.callLater(mail.restoreListPosition) }
        function onConversationChanged() {
            const identity = backend.account + "/" + backend.selectedId
            if (identity !== mail.loadedConversation) {
                mail.loadedConversation = identity; mail.expanded = false; mail.hydratedDraftIdentity = ""
                mail.replyOpen = mail.replyStates[identity] === true
                reader.contentY = Math.max(0,backend.presentation.readerY||0)
            }
            if (mail.members.length) {
                const selected = backend.presentation.selectedMember || ""
                if (selected && mail.members.some(function(m) { return m.id === selected })) {
                    const next = Object.assign({},mail.expandedMembers); next[mail.expansionKey+selected] = true; mail.expandedMembers = next; mail.expanded = true
                }
                if (mail.pendingReveal === backend.selectedId) { if (mail.matchingIds.length) mail.revealMatch(); mail.pendingReveal = "" }
            }
        }
        function onDraftChanged() {
            if (!backend.selectedId.length || !Object.keys(backend.draft).length) return
            const identity = backend.account + "/" + backend.selectedId
            if (backend.draftStatus === "Draft discarded") { mail.setReplyOpen(false); return }
            if (mail.hydratedDraftIdentity !== identity) {
                mail.hydratedDraftIdentity = identity
                if (mail.replyStates[identity] === undefined)
                    mail.setReplyOpen((backend.draft.body||"").length>0 || (backend.draft.attachment_ids||[]).length>0)
                else mail.replyOpen = mail.replyStates[identity] === true
            }
        }
    }
    SplitView {
        id: split
        anchors.fill: parent; orientation: Qt.Horizontal
        handle: Rectangle {
            implicitWidth: 1; color: SplitHandle.pressed ? Theme.accent : SplitHandle.hovered ? Theme.subtle : Theme.line
            containmentMask: Item { x: -4; width: 9; height: split.height }
        }
        Item {
            id: leftPane
            SplitView.preferredWidth: mail.width * 0.47; SplitView.minimumWidth: 340
            onWidthChanged: if (!mail.restoring && width > 0) backend.remember("splitWidth",width)
            ColumnLayout {
                anchors.fill: parent; spacing: 0
                ColumnLayout {
                    Layout.fillWidth: true; Layout.leftMargin: 24; Layout.rightMargin: 24; Layout.topMargin: 20; spacing: 13
                    RowLayout {
                        Layout.fillWidth: true; spacing: 14
                        FmButton { objectName: "inboxScopeButton"; text: "Inbox"; selected: backend.folder==="inbox"; font.pixelSize: 22; font.weight: Font.Medium; leftPadding: 0; rightPadding: 0; background: Item {} onClicked: backend.folder="inbox" }
                        FmButton { objectName: "sentScopeButton"; text: "Sent"; selected: backend.folder==="sent"; font.pixelSize: 22; font.weight: Font.Medium; leftPadding: 0; rightPadding: 0; background: Item {} onClicked: backend.folder="sent" }
                        Item { Layout.fillWidth: true }
                        FmButton { text: "All mail"; selected: backend.folder==="all"; font.pixelSize: 12; tip: "Include archived conversations in this account"; onClicked: backend.folder="all" }
                        FmButton {
                            id: filtersButton; objectName: "mailFiltersButton"
                            iconName: "filter"; implicitWidth: 30; selected: Object.keys(backend.filters).length > 0; tip: "Filter sender, review state, attachments and dates"
                            onClicked: { const p=filtersButton.mapToItem(Overlay.overlay,0,filtersButton.height+8); filterPopup.x=Math.max(12,Math.min(p.x-filterPopup.width+filtersButton.width,Overlay.overlay.width-filterPopup.width-12)); filterPopup.y=p.y; filterPopup.open() }
                        }
                    }
                    Text {
                        text: (backend.currentAccount.unread_conversation_count||0) + " to review · " + (backend.currentAccount.held_conversation_count||0) + " held conversations"
                        color: Theme.muted; font.family: Theme.fontFamily; font.pixelSize: 12; Layout.fillWidth: true
                        MouseArea { id: countsHover; anchors.fill: parent; hoverEnabled: true; acceptedButtons: Qt.NoButton }
                        FmToolTip { visible: countsHover.containsMouse; text: (backend.currentAccount.unread_count||0)+" unread messages · "+(backend.currentAccount.starred_count||0)+" held messages.\nAccount-wide totals include archived mail. A conversation can be both unread and held." }
                    }
                    Flickable {
                        Layout.fillWidth: true; implicitHeight: 31; clip: true
                        contentWidth: categoryRow.width; contentHeight: height; boundsBehavior: Flickable.StopAtBounds; flickableDirection: Flickable.HorizontalFlick
                        ScrollBar.horizontal: ScrollBar { height: 3 }
                        Row {
                            id: categoryRow; spacing: 6
                            FmButton { id: allCategories; text: "All"; selected: backend.category.length===0; implicitHeight: 27; font.pixelSize: 12; background: Rectangle { radius: 14; color: allCategories.selected ? Theme.selected : "transparent"; border.color: allCategories.activeFocus ? Theme.accent : Theme.controlBorder } onClicked: backend.category="" }
                            Repeater {
                                model: backend.categories
                                FmButton {
                                    required property var modelData
                                    id: categoryChip
                                    text: modelData.name+"  "+modelData.conversation_count; selected: backend.category===modelData.id; implicitHeight: 27; font.pixelSize: 12
                                    tip: modelData.conversation_count+" conversations in this category across the account"
                                    background: Rectangle { radius: 14; color: categoryChip.selected ? Theme.selected : "transparent"; border.color: categoryChip.activeFocus ? Theme.accent : Theme.controlBorder }
                                    onClicked: backend.category=backend.category===modelData.id ? "" : modelData.id
                                }
                            }
                        }
                    }
                    Rectangle { Layout.fillWidth: true; implicitHeight: 1; color: Theme.line }
                    RowLayout {
                        visible: mail.filterDescription.length>0; Layout.fillWidth: true; spacing: 6
                        Text { text: mail.filterDescription; color: Theme.muted; font.family: Theme.fontFamily; font.pixelSize: 12; wrapMode: Text.Wrap; Layout.fillWidth: true; textFormat: Text.PlainText }
                        FmButton { text: "Clear"; font.pixelSize: 12; onClicked: { backend.applyFilters({}); backend.query="" } }
                    }
                }
                ListView {
                    id: list; objectName: "conversationList"
                    Layout.fillWidth: true; Layout.fillHeight: true; Layout.topMargin: 6
                    clip: true; model: backend.conversations; boundsBehavior: Flickable.StopAtBounds
                    ScrollBar.vertical: ScrollBar {}
                    onContentHeightChanged: Qt.callLater(mail.restoreListPosition)
                    onHeightChanged: Qt.callLater(mail.restoreListPosition)
                    onMovementEnded: if (!mail.restoring) backend.remember("listY",contentY)
                    delegate: Item {
                        id: groupedRow
                        required property var modelData
                        required property int index
                        readonly property string group: mail.groupOf(modelData)
                        readonly property bool firstInGroup: index===0 || mail.groupOf(backend.conversations[index-1])!==group
                        readonly property bool doneRow: group==="done"
                        readonly property int headingHeight: firstInGroup ? 35 : 0
                        width: list.width; height: headingHeight+(doneRow ? 44 : mail.rowHeight)
                        RowLayout {
                            visible: groupedRow.firstInGroup; anchors.top: parent.top; anchors.left: parent.left; anchors.right: parent.right; anchors.leftMargin: 24; anchors.rightMargin: 24
                            height: groupedRow.headingHeight; spacing: 10
                            Text { text: backend.folder==="sent" ? "Outgoing conversations" : mail.groupTitle(groupedRow.group); color: Theme.subtle; font.family: Theme.fontFamily; font.pixelSize: 11 }
                            Text { text: mail.groupCount(groupedRow.group)+" shown"; color: Theme.subtle; font.family: Theme.fontFamily; font.pixelSize: 11 }
                            Rectangle { Layout.fillWidth: true; implicitHeight: 1; color: Theme.line }
                        }
                        ItemDelegate {
                            id: row
                            objectName: "conversationRow_"+groupedRow.modelData.id
                            y: groupedRow.headingHeight; width: parent.width; height: parent.height-y
                            leftPadding: 24; rightPadding: 24; topPadding: 0; bottomPadding: 0; hoverEnabled: true
                            Accessible.name: groupedRow.modelData.subject+", "+groupedRow.modelData.sender_name+(groupedRow.modelData.unread ? ", unread" : ", reviewed")+(groupedRow.modelData.starred ? ", action held" : "")
                            onClicked: mail.openRow(groupedRow.modelData.id)
                            background: Rectangle { color: backend.selectedId===groupedRow.modelData.id ? Theme.selected : row.hovered ? Theme.surface : "transparent"; border.color: row.activeFocus ? Theme.accent : "transparent" }
                            contentItem: RowLayout {
                                spacing: 14
                                Item {
                                    implicitWidth: 6; implicitHeight: 12
                                    Rectangle { anchors.centerIn: parent; width: 6; height: 6; radius: 3; color: groupedRow.modelData.unread ? Theme.text : "transparent"; border.width: groupedRow.modelData.starred && !groupedRow.modelData.unread ? 1 : 0; border.color: Theme.muted; visible: groupedRow.modelData.unread || groupedRow.modelData.starred }
                                }
                                ColumnLayout {
                                    Layout.fillWidth: true; spacing: 2
                                    RowLayout {
                                        Layout.fillWidth: true; spacing: 7
                                        Text { text: groupedRow.modelData.subject; color: groupedRow.doneRow ? Theme.subtle : Theme.text; font.family: Theme.fontFamily; font.pixelSize: groupedRow.doneRow ? 13 : 14; font.weight: groupedRow.modelData.unread ? Font.Medium : Font.Normal; elide: Text.ElideRight; Layout.fillWidth: true; textFormat: Text.PlainText }
                                        Rectangle {
                                            visible: groupedRow.modelData.message_count>1; implicitWidth: memberCount.implicitWidth+10; implicitHeight: 17; radius: 3; color: "transparent"; border.color: Theme.controlBorder
                                            Text { id: memberCount; anchors.centerIn: parent; text: groupedRow.modelData.message_count; color: Theme.muted; font.family: Theme.fontFamily; font.pixelSize: 10 }
                                        }
                                    }
                                    Text { visible: !groupedRow.doneRow; text: groupedRow.modelData.sender_name+(groupedRow.modelData.snippet ? " · "+groupedRow.modelData.snippet.replace(/\s+/g," ") : ""); color: Theme.muted; font.family: Theme.fontFamily; font.pixelSize: 12; elide: Text.ElideRight; Layout.fillWidth: true; textFormat: Text.PlainText }
                                }
                                FmIcon { visible: groupedRow.modelData.starred; name: "star"; size: 12; color: Theme.accent }
                                Text { text: mail.rowTime(groupedRow.modelData.latest_at); color: Theme.subtle; font.family: Theme.fontFamily; font.pixelSize: 12; Layout.preferredWidth: 52; horizontalAlignment: Text.AlignRight }
                            }
                            FmToolTip { visible: row.hovered; text: groupedRow.modelData.subject+(groupedRow.modelData.pinned ? "\nKept in place while open. Current state: "+mail.groupTitle(groupedRow.modelData.group)+"." : "") }
                        }
                    }
                    footer: Column {
                        width: list.width; spacing: 10
                        Item {
                            width: parent.width; height: backend.hasMore ? 48 : 34
                            FmButton { anchors.centerIn: parent; visible: backend.hasMore; text: backend.loading ? "Loading…" : "Load more conversations"; enabled: !backend.loading; onClicked: backend.loadMore() }
                            Text { anchors.centerIn: parent; visible: !backend.hasMore && list.count>0; text: (backend.querySummary.filtered_conversation_count||0)+" matching conversations"; color: Theme.subtle; font.family: Theme.fontFamily; font.pixelSize: 11 }
                        }
                        Column {
                            visible: backend.folder==="sent" && backend.captures.length>0; width: parent.width
                            RowLayout {
                                width: parent.width-48; x: 24; height: 36; spacing: 12
                                Text { text: "Local captures"; color: Theme.muted; font.family: Theme.fontFamily; font.pixelSize: 12 }
                                Rectangle { Layout.fillWidth: true; implicitHeight: 1; color: Theme.line }
                                Text { text: "Not delivered"; color: Theme.subtle; font.family: Theme.fontFamily; font.pixelSize: 11 }
                            }
                            Repeater {
                                model: backend.folder==="sent" ? backend.captures : []
                                ItemDelegate {
                                    required property var modelData
                                    id: captureRow
                                    width: list.width; implicitHeight: 56; leftPadding: 24; rightPadding: 24
                                    onClicked: backend.selectConversation(modelData.conversation_id)
                                    background: Rectangle { color: captureRow.hovered ? Theme.surface : "transparent"; border.color: captureRow.activeFocus ? Theme.accent : "transparent" }
                                    contentItem: RowLayout {
                                        spacing: 8
                                        ColumnLayout {
                                            Layout.fillWidth: true; spacing: 4
                                            Text { text: captureRow.modelData.subject || "Reply · "+captureRow.modelData.conversation_id; color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: 13; elide: Text.ElideRight; Layout.fillWidth: true; textFormat: Text.PlainText }
                                            Text { text: ({accepted:"Captured locally",failed:"Capture failed",acceptance_unknown:"Capture unconfirmed",prepared:"Prepared locally",in_flight:"Capture in progress"}[captureRow.modelData.state]||captureRow.modelData.state)+" · "+Qt.formatDateTime(new Date(captureRow.modelData.created_at*1000),"d MMM, HH:mm"); color: captureRow.modelData.state==="failed" ? Theme.error : Theme.subtle; font.family: Theme.fontFamily; font.pixelSize: 11 }
                                        }
                                        FmButton { text: "Inspect"; font.pixelSize: 11; enabled: !backend.draftBusy; onClicked: backend.inspectCapture(captureRow.modelData.operation_id); tip: "Inspect this existing local operation without creating another capture" }
                                    }
                                    FmToolTip { visible: captureRow.hovered; text: "Local-only operation "+captureRow.modelData.operation_id+"\nNo message was delivered. Open its source conversation." }
                                }
                            }
                            FmButton { x: 24; visible: backend.capturesMore; text: "More local captures"; font.pixelSize: 12; onClicked: backend.loadMoreCaptures() }
                            Text { x: 24; width: parent.width-48; visible: backend.captureStatus.length>0; text: backend.captureStatus; color: Theme.muted; font.family: Theme.fontFamily; font.pixelSize: 11; wrapMode: Text.Wrap }
                        }
                    }
                    Text {
                        anchors.horizontalCenter: parent.horizontalCenter; y: 48; width: parent.width-48
                        visible: list.count===0 && !(backend.folder==="sent" && backend.captures.length>0)
                        text: !backend.connected ? "Connecting to your local mail…" : backend.loading ? "Searching…" : mail.filtering ? "No conversations match these filters." : "No conversations in this view."
                        color: Theme.muted; font.family: Theme.fontFamily; font.pixelSize: 13; horizontalAlignment: Text.AlignHCenter; wrapMode: Text.Wrap
                    }
                }
                Text {
                    text: mail.coverageText; visible: text.length>0
                    Layout.fillWidth: true; Layout.leftMargin: 24; Layout.rightMargin: 24; Layout.bottomMargin: 10; Layout.topMargin: 7
                    color: Theme.subtle; font.family: Theme.fontFamily; font.pixelSize: 10; wrapMode: Text.Wrap; maximumLineCount: 2; elide: Text.ElideRight
                    MouseArea { id: coverageHover; anchors.fill: parent; hoverEnabled: true; acceptedButtons: Qt.NoButton }
                    FmToolTip { visible: coverageHover.containsMouse; text: mail.coverageText }
                }
            }
        }
        Item {
            id: readerPane
            SplitView.fillWidth: true; SplitView.minimumWidth: 470
            readonly property int margin: width<620 ? 24 : 40
            ColumnLayout {
                anchors.fill: parent; spacing: 0
                ColumnLayout {
                    visible: mail.members.length>0
                    Layout.fillWidth: true; Layout.leftMargin: readerPane.margin; Layout.rightMargin: readerPane.margin; Layout.topMargin: 24; spacing: 14
                    RowLayout {
                        Layout.fillWidth: true; spacing: 4
                        FmButton { id: categoriesButton; objectName: "categoryAssignmentButton"; iconName: "filter"; implicitWidth: 30; tip: "Change categories on the loaded conversation members"; outlined: true; onClicked: categoryPopup.open() }
                        Flickable {
                            Layout.fillWidth: true; implicitHeight: 28; clip: true
                            contentWidth: readerCategoryRow.width; contentHeight: height; boundsBehavior: Flickable.StopAtBounds
                            Row {
                                id: readerCategoryRow; spacing: 6
                                Repeater { model: mail.thread.categories||[]; FmButton { required property var modelData; text: modelData.name; font.pixelSize: 11; implicitHeight: 24; outlined: true; onClicked: categoryPopup.open() } }
                            }
                        }
                        FmButton { objectName: "replyButton"; text: "Reply"; iconName: "reply"; prominent: true; enabled: !backend.draftBusy; onClicked: mail.openReply() }
                        FmButton { objectName: "delayConversationButton"; text: mail.anyStarred ? "Unhold" : "Delay"; iconName: "clock"; outlined: true; enabled: !backend.mutationInFlight; tip: mail.anyStarred ? "Clear the held action on these messages" : "Leave action for later. There is no timer."; onClicked: backend.setStarred(!mail.anyStarred) }
                        FmButton { objectName: "doneConversationButton"; text: "Done"; iconName: "check"; outlined: true; enabled: !backend.mutationInFlight; tip: "Clear unread attention and held action on the "+mail.members.length+" loaded members. Later replies are unaffected."; onClicked: backend.doneConversation() }
                        FmButton { id: moreButton; iconName: "more"; implicitWidth: 30; outlined: true; tip: "More conversation actions"; onClicked: morePopup.open() }
                    }
                    Text { text: mail.thread.subject||""; color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: 28; font.weight: Font.Medium; wrapMode: Text.Wrap; Layout.fillWidth: true; Layout.topMargin: 7; textFormat: Text.PlainText }
                    RowLayout {
                        Layout.fillWidth: true; spacing: 10
                        Rectangle {
                            implicitWidth: 24; implicitHeight: 24; color: Theme.selected; radius: 12
                            Text { anchors.centerIn: parent; text: (mail.latest.sender_name||"?").split(" ").map(function(word) { return word.charAt(0) }).slice(0,2).join(""); color: Theme.muted; font.family: Theme.fontFamily; font.pixelSize: 10 }
                        }
                        Text { text: (mail.latest.sender_name||"")+" · "+mail.members.length+" messages · "+(backend.currentAccount.name||""); color: Theme.muted; font.family: Theme.fontFamily; font.pixelSize: 12; elide: Text.ElideRight; Layout.fillWidth: true; textFormat: Text.PlainText }
                        FmButton { visible: mail.matchingIds.length>0; text: "Show match"; font.pixelSize: 11; onClicked: mail.revealMatch() }
                    }
                }
                Flickable {
                    id: reader; objectName: "reader"
                    Layout.fillWidth: true; Layout.fillHeight: true; Layout.topMargin: 8
                    contentWidth: width; contentHeight: readerColumn.implicitHeight+44; clip: true; boundsBehavior: Flickable.StopAtBounds
                    ScrollBar.vertical: ScrollBar {}
                    onMovementEnded: if (!mail.restoring) backend.remember("readerY",contentY)
                    ColumnLayout {
                        id: readerColumn
                        x: readerPane.margin; y: 8; width: reader.width-readerPane.margin*2; spacing: 0; visible: mail.members.length>0
                        RowLayout {
                            Layout.fillWidth: true; visible: mail.members.length>1; spacing: 12; Layout.topMargin: 6; Layout.bottomMargin: 5
                            Rectangle { Layout.fillWidth: true; implicitHeight: 1; color: Theme.line }
                            FmButton { objectName: "earlierMessagesButton"; iconName: "down"; text: (mail.expanded ? "Hide " : "Show ")+(mail.members.length-1)+" earlier messages"; font.pixelSize: 12; onClicked: { mail.expanded=!mail.expanded; backend.remember("expanded",mail.expanded) } }
                            Rectangle { Layout.fillWidth: true; implicitHeight: 1; color: Theme.line }
                        }
                        Repeater {
                            id: memberRepeater
                            model: mail.members
                            delegate: ColumnLayout {
                                id: member
                                required property var modelData
                                required property int index
                                readonly property bool isExpanded: mail.memberIsExpanded(modelData.id,index)
                                readonly property bool isMatch: mail.matchingIds.indexOf(modelData.id)>=0
                                objectName: "messageMember_"+modelData.id
                                Layout.fillWidth: true; spacing: 10; Layout.topMargin: 12; Layout.bottomMargin: 8; visible: mail.expanded || index===mail.members.length-1
                                ItemDelegate {
                                    id: memberHeading
                                    Layout.fillWidth: true; implicitHeight: member.isExpanded ? 30 : 48; leftPadding: 0; rightPadding: 0; topPadding: 0; bottomPadding: 0
                                    enabled: member.index!==mail.members.length-1; onClicked: mail.toggleMember(member.modelData.id,member.index)
                                    background: Rectangle { color: memberHeading.hovered ? Theme.surface : "transparent"; border.color: memberHeading.activeFocus ? Theme.accent : "transparent"; radius: 4 }
                                    contentItem: ColumnLayout {
                                        spacing: 4
                                        RowLayout {
                                            Layout.fillWidth: true
                                            Text { text: member.modelData.sender_name; color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: 13; font.weight: Font.Medium; textFormat: Text.PlainText; elide: Text.ElideRight; Layout.fillWidth: true }
                                            Text { visible: member.isMatch; text: "Match"; color: Theme.accent; font.family: Theme.fontFamily; font.pixelSize: 11 }
                                            FmIcon { visible: member.modelData.starred; name: "star"; size: 12; color: Theme.accent }
                                            Text { text: Qt.formatDateTime(new Date(member.modelData.sent_at),"d MMM, HH:mm"); color: Theme.subtle; font.family: Theme.fontFamily; font.pixelSize: 12 }
                                        }
                                        Text { visible: !member.isExpanded; text: (member.modelData.text||"").replace(/\s+/g," "); color: Theme.subtle; font.family: Theme.fontFamily; font.pixelSize: 12; elide: Text.ElideRight; Layout.fillWidth: true; textFormat: Text.PlainText }
                                    }
                                }
                                RowLayout {
                                    visible: member.isExpanded; Layout.fillWidth: true; spacing: 8
                                    FmButton { text: member.modelData.sender_email; font.pixelSize: 11; implicitHeight: 22; leftPadding: 0; rightPadding: 0; Layout.maximumWidth: readerColumn.width-90; onClicked: backend.applyFilters({sender:member.modelData.sender_email}); tip: "Filter this mailbox by the sender address" }
                                    Text { text: member.modelData.unread ? "Unread" : "Reviewed"; color: Theme.subtle; font.family: Theme.fontFamily; font.pixelSize: 10 }
                                    Item { Layout.fillWidth: true }
                                }
                                TextArea {
                                    visible: member.isExpanded; Layout.fillWidth: true; text: member.modelData.text||"Readable content is unavailable."
                                    readOnly: true; selectByMouse: true; wrapMode: TextEdit.Wrap; textFormat: TextEdit.PlainText
                                    color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: 15; leftPadding: 0; rightPadding: 0; topPadding: 0; bottomPadding: 0; background: null
                                    Accessible.name: "Message from "+member.modelData.sender_name
                                }
                                Flow {
                                    visible: member.isExpanded; Layout.fillWidth: true; spacing: 8
                                    Repeater { model: backend.linksInText(member.modelData.text||""); FmButton { required property string modelData; text: modelData.length>60 ? modelData.substring(0,60)+"…" : modelData; implicitWidth: Math.min(contentItem.implicitWidth+22,readerColumn.width); font.pixelSize: 11; tip: "Open link: "+modelData; onClicked: backend.openLink(modelData) } }
                                }
                                Flow {
                                    visible: member.isExpanded; Layout.fillWidth: true; spacing: 8
                                    Repeater {
                                        model: member.modelData.attachments||[]
                                        FmButton {
                                            required property var modelData
                                            iconName: "attachment"; text: modelData.name+" · "+(modelData.state==="available" ? "available offline" : modelData.state); implicitWidth: Math.min(contentItem.implicitWidth+22,readerColumn.width)
                                            outlined: true; font.pixelSize: 12; enabled: modelData.state==="available"; onClicked: backend.openAttachment(modelData.id)
                                            tip: modelData.state==="available" ? "Verify the retained bytes and open this attachment" : "This attachment is "+modelData.state+" and cannot be opened."
                                        }
                                    }
                                }
                                Rectangle { Layout.fillWidth: true; implicitHeight: 1; color: Theme.line; Layout.topMargin: 10 }
                            }
                        }
                        ReplyComposer {
                            id: reply
                            objectName: "replyPanel"
                            Layout.fillWidth: true; Layout.topMargin: visible ? 22 : 0
                            visible: mail.replyOpen
                            onDismissed: mail.setReplyOpen(false)
                        }
                    }
                    Column {
                        visible: mail.members.length===0; anchors.horizontalCenter: parent.horizontalCenter; y: parent.height*0.2; width: parent.width-readerPane.margin*2; spacing: 14
                        Text { text: backend.loading ? "Loading conversation…" : "Select a conversation"; color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: 28; font.weight: Font.Medium; width: parent.width; wrapMode: Text.Wrap }
                        Text { text: "Mail stays in "+(backend.currentAccount.name||"the selected account")+". Your calendar remains in view below."; color: Theme.muted; font.family: Theme.fontFamily; font.pixelSize: 14; width: parent.width; wrapMode: Text.Wrap; lineHeight: 1.5 }
                    }
                }
            }
        }
    }
    MailFilters { id: filterPopup; parent: Overlay.overlay }
    Popup {
        id: categoryPopup; objectName: "categoryAssignmentPopup"; parent: Overlay.overlay; width: 350; padding: 16
        onAboutToShow: { const p=categoriesButton.mapToItem(parent,0,categoriesButton.height+6); x=Math.min(p.x,parent.width-width-12); y=p.y }
        focus: true; closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
        background: Rectangle { color: Theme.surface; border.color: Theme.controlBorder; radius: 8 }
        contentItem: ColumnLayout {
            spacing: 10
            Text { text: "Categories"; color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: 14; font.weight: Font.Medium }
            Text { Layout.fillWidth: true; text: "Apply to the "+mail.members.length+" loaded messages. Later replies keep their own labels."; color: Theme.muted; font.family: Theme.fontFamily; font.pixelSize: 12; wrapMode: Text.Wrap }
            Repeater {
                model: backend.categories
                RowLayout {
                    required property var modelData
                    property int memberCount: mail.categoryCount(modelData.id)
                    Layout.fillWidth: true
                    Text { Layout.fillWidth: true; text: modelData.name; color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: 12; textFormat: Text.PlainText }
                    Text { text: memberCount===0 ? "None" : memberCount===mail.members.length ? "All" : memberCount+"/"+mail.members.length; color: Theme.subtle; font.family: Theme.fontFamily; font.pixelSize: 11 }
                    FmButton { objectName: "categoryAdd_"+modelData.id; text: memberCount===mail.members.length ? "Clear" : "Apply"; enabled: !backend.mutationInFlight; outlined: true; font.pixelSize: 11; onClicked: backend.setCategoryMembership(modelData.id,memberCount!==mail.members.length) }
                    FmButton { visible: memberCount>0 && memberCount<mail.members.length; text: "Clear"; font.pixelSize: 11; enabled: !backend.mutationInFlight; onClicked: backend.setCategoryMembership(modelData.id,false) }
                }
            }
        }
    }
    Popup {
        id: morePopup; parent: Overlay.overlay; width: 220; padding: 6
        onAboutToShow: { const p=moreButton.mapToItem(parent,moreButton.width,moreButton.height+6); x=p.x-width; y=p.y }
        focus: true; closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
        background: Rectangle { color: Theme.surface; border.color: Theme.controlBorder; radius: 6 }
        contentItem: ColumnLayout {
            spacing: 2
            FmButton { text: "Mark unread"; Layout.fillWidth: true; textAlignment: Text.AlignLeft; enabled: !backend.mutationInFlight; onClicked: { backend.setRead(false); morePopup.close() } }
            FmButton { text: "Mark reviewed"; Layout.fillWidth: true; textAlignment: Text.AlignLeft; enabled: !backend.mutationInFlight; onClicked: { backend.setRead(true); morePopup.close() } }
        }
    }
}
