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
    property var collapsedGroups: ({})
    property string loadedConversation: ""
    property string pendingReveal: ""
    property string replyRevealIdentity: ""
    readonly property string composerIdentity: backend.account + (backend.activeDraftId.length ? "/draft/" + backend.activeDraftId : "/" + backend.selectedId)
    readonly property bool newMessageAvailable: backend.connected && backend.account.length > 0
    readonly property bool popupOpen: folderPopup.opened || filterPopup.opened || categoryPopup.opened || railCategories.opened || senderCard.opened || morePopup.opened || capturePopup.opened || reply.popupOpen || (mail.ApplicationWindow.window && mail.ApplicationWindow.window.searchPopupOpen)
    readonly property bool canMutate: { const accounts = backend.accounts; return backend.connected && backend.accountCapability(backend.account,"mail_write") }
    readonly property bool singleKeyActionsAllowed: mail.visible && !popupOpen && mail.ApplicationWindow.window && mail.ApplicationWindow.window.singleKeysAllowed
    readonly property bool draftsMode: backend.folder === "drafts"
    readonly property bool anyUnread: members.some(function(m) { return m.unread })
    readonly property bool anyStarred: members.some(function(m) { return m.starred })
    readonly property int rowHeight: Math.ceil((backend.preferences.density === "compact" ? Theme.compactRowHeight : Theme.comfortableRowHeight) * Theme.fontScale)
    readonly property bool filtering: backend.query.length > 0 || Object.keys(backend.filters).length > 0 || backend.category.length > 0
    readonly property var matchingIds: {
        const row = backend.conversations.find(function(c) { return c.id === backend.selectedId })
        return row && row.matching_message_ids ? row.matching_message_ids : []
    }
    readonly property var latest: members.length ? members[members.length - 1] : ({})
    readonly property var popularCategories: backend.categories.slice().sort(function(a, b) { return b.conversation_count - a.conversation_count || a.name.localeCompare(b.name) }).slice(0, 7)
    readonly property string expansionKey: backend.account + "/" + backend.selectedId + "/"
    readonly property string coverageText: backend.querySummary.coverage ? backend.querySummary.coverage.text || "" : ""
    readonly property string filterDescription: {
        const f = backend.filters, labels = []
        if (backend.query.length) labels.push('“' + backend.query + '”')
        if (f.sender) labels.push("From " + f.sender)
        if (f.exact_sender) labels.push("From " + f.exact_sender)
        if (f.kind && f.kind !== "all") labels.push({people:"People",automated:"Automated senders",unknown:"Unknown sender kind"}[f.kind] || f.kind)
        if (f.status && f.status !== "all") labels.push({review:"To review",held:"Delayed",done:"Done"}[f.status] || f.status)
        if (f.has_attachment === true) labels.push("With attachments")
        if (f.has_attachment === false) labels.push("Without attachments")
        if (f.after) labels.push("From " + f.after + " UTC")
        if (f.before) labels.push("Before " + f.before + " UTC")
        return labels.join(" · ")
    }
    onVisibleChanged: if (!visible) { folderPopup.close(); filterPopup.close(); categoryPopup.close(); morePopup.close(); senderCard.close(); railCategories.close() }
    function categoryCount(id) { return members.filter(function(m) { return (m.categories || []).indexOf(id) >= 0 }).length }
    function groupOf(row) { return row ? row.display_group || row.group || "done" : "done" }
    function groupTitle(group) { return {review:"To review",held:"Delayed",done:"Done"}[group] || "Conversations" }
    function groupCount(group) { return (backend.querySummary.filtered_group_counts || {})[group] || 0 }
    function groupKey(group) { return backend.account + "/" + backend.folder + "/" + group }
    function groupCollapsed(group) { return collapsedGroups[groupKey(group)] === true }
    function syncCollapsedGroups() { backend.setCollapsedGroups(backend.folder === "inbox" ? ["review", "held", "done"].filter(function(group) { return mail.groupCollapsed(group) }) : []) }
    function toggleGroup(group) { const next = Object.assign({}, collapsedGroups); next[groupKey(group)] = !groupCollapsed(group); collapsedGroups = next; syncCollapsedGroups() }
    function collapsedBetween(previous, next) {
        if (backend.folder !== "inbox") return []
        const groups = ["review", "held", "done"]
        return groups.filter(function(group, index) { return index > previous && index < next && mail.groupCollapsed(group) && mail.groupCount(group) > 0 })
    }
    function collapsedBefore(index) {
        const rows = backend.conversations, groups = ["review", "held", "done"]
        if (index < 0 || index >= rows.length || !rows[index]) return []
        if (index > 0 && groupOf(rows[index - 1]) === groupOf(rows[index])) return []
        return collapsedBetween(index > 0 ? groups.indexOf(groupOf(rows[index - 1])) : -1, groups.indexOf(groupOf(rows[index])))
    }
    readonly property var trailingCollapsed: collapsedBetween(backend.conversations.length ? ["review", "held", "done"].indexOf(groupOf(backend.conversations[backend.conversations.length - 1])) : -1, 3)
    component ReaderAction: FmButton {
        leftPadding: 14; rightPadding: 14
        font.weight: Font.Medium
    }
    component GroupHeader: ItemDelegate {
        id: heading
        required property string group
        readonly property bool collapsed: mail.groupCollapsed(group)
        objectName: "mailGroup_" + group
        height: 34; leftPadding: 24; rightPadding: 24; topPadding: 0; bottomPadding: 0; hoverEnabled: true
        enabled: backend.folder === "inbox"
        onClicked: mail.toggleGroup(group)
        background: Item {}
        Accessible.name: mail.groupTitle(group) + ", " + mail.groupCount(group) + " conversations, " + (collapsed ? "collapsed" : "expanded")
        contentItem: RowLayout {
            spacing: 12
            FmIcon { name: heading.group === "review" ? "review" : heading.group === "held" ? "clock" : "checkCircle"; size: 12; color: Theme.subtle }
            Text { text: backend.folder === "sent" ? "Sent" : backend.folder === "trash" ? "Trash" : backend.folder === "all" ? "All mail" : mail.groupTitle(heading.group); color: Theme.subtle; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(12) }
            Rectangle { Layout.fillWidth: true; implicitHeight: 1; color: Theme.line }
            Text { visible: backend.folder === "inbox"; text: mail.groupCount(heading.group); color: Theme.subtle; font.pixelSize: Theme.fontSize(12) }
            FmIcon { visible: backend.folder === "inbox"; name: "down"; size: 10; color: Theme.subtle; rotation: heading.collapsed ? -90 : 0; Behavior on rotation { NumberAnimation { duration: 180 } } }
        }
    }
    Component.onCompleted: syncCollapsedGroups()
    function setKind(kind) {
        const next = Object.assign({}, backend.filters)
        if (next.kind === kind) delete next.kind; else next.kind = kind
        backend.applyFilters(next)
    }
    function kindIcon(kind) { return kind === "people" ? "person" : kind === "automated" ? "bot" : "unknown" }
    function kindTitle(kind) { return kind === "people" ? "Person" : kind === "automated" ? "Automated sender" : "Sender kind is unknown" }
    function senderCardFor(member, anchor) { senderCard.showSender(member.sender_email, member.sender_name, anchor) }
    function currentComposerIdentity() { return backend.account + (backend.activeDraftId.length ? "/draft/" + backend.activeDraftId : "/" + backend.selectedId) }
    function newMessage() { backend.beginDraft("new") }
    function openActions() { morePopup.open() }
    function openFilters(anchorItem) {
        const p = anchorItem ? anchorItem.mapToItem(Overlay.overlay,0,anchorItem.height+8) : mail.mapToItem(Overlay.overlay,24,56)
        filterPopup.x = Math.max(12,Math.min(p.x,Overlay.overlay.width-filterPopup.width-12))
        filterPopup.y = Math.min(p.y,Overlay.overlay.height-filterPopup.height-12)
        filterPopup.open()
    }
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
        if (isNaN(date.getTime())) return ""
        const day = Qt.formatDate(date, "yyyy-MM-dd"), now = new Date(backend.clockNow)
        const days = (Date.UTC(now.getFullYear(), now.getMonth(), now.getDate()) - Date.UTC(date.getFullYear(), date.getMonth(), date.getDate())) / 86400000
        return day === backend.todayDate ? Qt.formatTime(date,"HH:mm") : days >= 0 && days < 5 ? Qt.formatDateTime(date,"ddd HH:mm") : Qt.formatDate(date,date.getFullYear() === now.getFullYear() ? "d MMM" : "d MMM yyyy")
    }
    function relativeTime(value) {
        const date = new Date(value), now = new Date(backend.clockNow)
        if (isNaN(date.getTime())) return ""
        const minutes = Math.max(0, (now.getTime() - date.getTime()) / 60000)
        if (minutes < 60) return Math.max(1, Math.round(minutes)) + " min ago"
        if (minutes < 1440) return Math.round(minutes / 60) + " h ago"
        if (minutes < 8 * 1440) return Math.round(minutes / 1440) + " d ago"
        return Qt.formatDate(date,date.getFullYear() === now.getFullYear() ? "d MMM" : "d MMM yyyy")
    }
    function fullTime(value) { const date = new Date(value); return isNaN(date.getTime()) ? "" : Qt.formatDateTime(date, "dddd d MMMM yyyy, HH:mm t") }
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
        if (draftsMode) { backend.openDraft(id); return }
        pendingReveal = id; backend.openConversation(id)
        if (backend.selectedId === id && members.length) { if (matchingIds.length) revealMatch(); pendingReveal = "" }
    }
    function moveSelection(delta) {
        const selected = draftsMode ? backend.activeDraftId : backend.selectedId
        const rows = backend.conversations.filter(function(row) { return backend.folder !== "inbox" || !mail.groupCollapsed(mail.groupOf(row)) })
        const current = rows.findIndex(function(row) { return row.id === selected })
        const next = Math.max(0, Math.min(rows.length - 1, current < 0 ? 0 : current + delta))
        if (!rows.length || next === current) return
        openRow(rows[next].id)
        list.positionViewAtIndex(backend.conversations.findIndex(function(row) { return row.id === rows[next].id }), ListView.Contain)
        backend.remember("listY", list.contentY)
    }
    function setReplyOpen(value) {
        const identity = composerIdentity
        const next = Object.assign({},replyStates); next[identity] = value; replyStates = next
        replyOpen = value; backend.remember("replyOpen",value)
    }
    function openReply() {
        if (backend.draft.mode && backend.draft.mode !== "reply") { backend.beginDraft("reply"); return }
        resumeDraft()
    }
    function resumeDraft() {
        setReplyOpen(true)
        replyRevealIdentity = composerIdentity
        replyReveal.pass = 0; replyReveal.stable = 0; replyReveal.lastTop = -1; replyReveal.lastHeight = -1
        replyReveal.restart()
    }
    Timer {
        id: replyReveal
        objectName: "replyReveal"
        interval: 16; repeat: true
        property int pass: 0
        property int stable: 0
        property real lastTop: -1
        property real lastHeight: -1
        onTriggered: {
            if (!mail.visible || !mail.replyOpen || mail.replyRevealIdentity !== mail.composerIdentity) { stop(); return }
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
        const identity = composerIdentity
        const sameDraft = backend.activeDraftId.length ? view.selectedDraft === backend.activeDraftId : !view.selectedDraft && view.selected === backend.selectedId
        if (sameDraft && view.replyOpen !== undefined) {
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
        function onAccountChanged() { mail.syncCollapsedGroups() }
        function onQueryChanged() { mail.syncCollapsedGroups() }
        function onPresentationChanged() { Qt.callLater(mail.restorePosition) }
        function onConversationsChanged() { Qt.callLater(mail.restoreListPosition) }
        function onConversationChanged() {
            const identity = mail.currentComposerIdentity()
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
            if (backend.activeDraftId.length && Object.keys(backend.draft).length) {
                if (backend.draftStatus === "Draft discarded") mail.replyOpen = false
                else if (mail.hydratedDraftIdentity !== backend.account+"/draft/"+backend.activeDraftId) {
                    const identity = mail.currentComposerIdentity()
                    mail.hydratedDraftIdentity = identity
                    mail.loadedConversation = identity
                    const opened = backend.presentation.selectedDraft !== backend.activeDraftId || backend.presentation.replyOpen !== false
                    const replies = Object.assign({}, mail.replyStates); replies[identity] = opened; mail.replyStates = replies
                    mail.replyOpen = opened
                    mail.replyRevealIdentity = mail.composerIdentity
                    if (mail.replyOpen) { replyReveal.pass=0; replyReveal.stable=0; replyReveal.lastTop=-1; replyReveal.lastHeight=-1; replyReveal.restart() }
                }
                return
            }
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
        handle: Item {
            id: splitHandle
            implicitWidth: 1
            Rectangle { x: -4; width: 9; height: parent.height; color: Theme.inkAlpha(.05); visible: splitHandle.SplitHandle.hovered }
            Rectangle { x: Theme.snapX(parent, 0); width: Theme.ruleWidth; height: parent.height; color: splitHandle.SplitHandle.pressed ? Theme.accent : Theme.controlBorder }
            containmentMask: Item { x: -4; width: 9; height: split.height }
        }
        Item {
            id: leftPane
            SplitView.preferredWidth: mail.width * 0.50; SplitView.minimumWidth: 340
            onWidthChanged: if (!mail.restoring && width > 0) backend.remember("splitWidth",width)
            ColumnLayout {
                anchors.fill: parent; spacing: 0
                ColumnLayout {
                    Layout.fillWidth: true; Layout.leftMargin: 24; Layout.rightMargin: 24; Layout.topMargin: 24; spacing: 16
                    RowLayout {
                        Layout.fillWidth: true; spacing: 14
                        FmButton { objectName: "inboxScopeButton"; text: "Inbox"; selected: backend.folder==="inbox"; font.pixelSize: Theme.fontSize(selected ? 22 : 15); font.weight: Font.Medium; implicitWidth: contentItem.implicitWidth; foreground: selected ? Theme.text : Theme.inkAlpha(.32); leftPadding: 0; rightPadding: 0; textAlignment: Text.AlignLeft; background: Item {} Behavior on font.pixelSize { NumberAnimation { duration: 150 } } onClicked: backend.folder="inbox" }
                        FmButton { objectName: "sentScopeButton"; text: "Sent"; selected: backend.folder==="sent"; font.pixelSize: Theme.fontSize(selected ? 22 : 15); font.weight: Font.Medium; implicitWidth: contentItem.implicitWidth; foreground: selected ? Theme.text : Theme.inkAlpha(.32); leftPadding: 0; rightPadding: 0; textAlignment: Text.AlignLeft; background: Item {} Behavior on font.pixelSize { NumberAnimation { duration: 150 } } onClicked: backend.folder="sent" }
                        FmButton { objectName: "draftsScopeButton"; text: "Drafts"; selected: backend.folder==="drafts"; font.pixelSize: Theme.fontSize(selected ? 22 : 15); font.weight: Font.Medium; implicitWidth: contentItem.implicitWidth; foreground: selected ? Theme.text : Theme.inkAlpha(.32); leftPadding: 0; rightPadding: 0; textAlignment: Text.AlignLeft; background: Item {} Behavior on font.pixelSize { NumberAnimation { duration: 150 } } onClicked: backend.folder="drafts" }
                        FmButton { id: folderButton; objectName: "moreMailFolders"; visible: !backend.syntheticProfile; text: backend.folder === "trash" ? "Trash" : backend.folder === "all" ? "All mail" : ""; iconName: "down"; implicitHeight: 28; leftPadding: 0; rightPadding: 0; tip: "More mail folders"; onClicked: folderPopup.open() }
                        Item { Layout.fillWidth: true }
                        RowLayout {
                            spacing: 6
                            FmIcon { name: "review"; size: 13; color: Theme.muted }
                            Text { text: backend.currentAccount.unread_conversation_count||0; color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(12); font.weight: Font.Medium }
                            Text { visible: leftPane.width>570; text: "to review"; color: Theme.muted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(12) }
                        }
                        RowLayout {
                            spacing: 6
                            FmIcon { name: "clock"; size: 13; color: Theme.muted }
                            Text { text: backend.currentAccount.held_conversation_count||0; color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(12); font.weight: Font.Medium }
                            Text { visible: leftPane.width>570; text: "delayed"; color: Theme.muted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(12) }
                        }
                    }
                    RowLayout {
                    visible: !mail.draftsMode; Layout.fillWidth: true; spacing: 6
                    Flickable {
                        visible: !mail.draftsMode
                        Layout.fillWidth: true; implicitHeight: 37; clip: true
                        contentWidth: categoryRow.width; contentHeight: height; boundsBehavior: Flickable.StopAtBounds; flickableDirection: Flickable.HorizontalFlick
                        ScrollBar.horizontal: ScrollBar { height: 3 }
                        Row {
                            id: categoryRow; spacing: 6
                            FmButton { id: allCategories; text: "All  "+(backend.currentAccount.conversation_count||0); selected: backend.category.length===0; prominent: selected; implicitHeight: 29; font.pixelSize: Theme.fontSize(12); background: Rectangle { radius: 15; color: allCategories.selected ? Theme.text : "transparent"; border.color: allCategories.activeFocus ? Theme.accent : Theme.controlBorder } onClicked: backend.category="" }
                            Repeater {
                                model: mail.popularCategories
                                FmButton {
                                    required property var modelData
                                    id: categoryChip
                                    text: modelData.name+"  "+modelData.conversation_count; selected: backend.category===modelData.id; prominent: selected; implicitHeight: 29; font.pixelSize: Theme.fontSize(12)
                                    tip: modelData.conversation_count+" conversations in this category across the account"
                                    background: Rectangle { radius: 15; color: categoryChip.selected ? Theme.text : "transparent"; border.color: categoryChip.activeFocus ? Theme.accent : Theme.controlBorder }
                                    onClicked: backend.category=backend.category===modelData.id ? "" : modelData.id
                                }
                            }
                            FmButton { id: allLabels; objectName: "allCategoriesButton"; visible: backend.categories.length > 7; iconName: "more"; implicitHeight: 29; implicitWidth: 29; tip: "All categories"; onClicked: railCategories.open() }
                        }
                    }
                    Rectangle {
                        visible: backend.folder === "inbox"; implicitWidth: kindButtons.width + 4; implicitHeight: 28; radius: 14; color: "transparent"; border.color: Theme.line
                        Layout.alignment: Qt.AlignTop
                        Row { id: kindButtons; x: 2; y: 2; spacing: 2
                            Repeater { model: ["people", "automated"]
                                FmButton {
                                    id: kindButton; required property string modelData
                                    objectName: "senderKind_" + modelData; iconName: mail.kindIcon(modelData); iconSize: 12
                                    width: 26; height: 22; implicitWidth: 26; leftPadding: 0; rightPadding: 0
                                    selected: backend.filters.kind === modelData; foreground: selected ? Theme.canvas : Theme.muted
                                    tip: (modelData === "people" ? "People only" : "Automated only") + (selected ? " · click again to show all kinds" : "")
                                    background: Rectangle { radius: 11; color: kindButton.selected ? Theme.text : kindButton.hovered ? Theme.hover : "transparent"; border.color: kindButton.activeFocus ? Theme.accent : "transparent" }
                                    onClicked: mail.setKind(modelData)
                                }
                            }
                        }
                    }
                    }
                }
                ListView {
                    id: list; objectName: "conversationList"
                    Layout.fillWidth: true; Layout.fillHeight: true; Layout.topMargin: 6
                    clip: true; model: backend.conversations; boundsBehavior: Flickable.StopAtBounds
                    ScrollBar.vertical: ScrollBar {}
                    onContentHeightChanged: Qt.callLater(mail.restoreListPosition)
                    onMovementEnded: if (!mail.restoring) backend.remember("listY",contentY)
                    delegate: Item {
                        id: groupedRow
                        required property var modelData
                        required property int index
                        readonly property string group: mail.groupOf(modelData)
                        readonly property bool firstInGroup: index===0 || mail.groupOf(backend.conversations[index-1])!==group
                        readonly property bool doneRow: group==="done" && !mail.draftsMode
                        readonly property bool collapsed: backend.folder === "inbox" && mail.groupCollapsed(group)
                        readonly property int headingHeight: firstInGroup && !mail.draftsMode ? (index === 0 ? 34 : 44) : 0
                        width: list.width; height: collapsedHeaders.height + headingHeight + (collapsed ? 0 : mail.rowHeight)
                        Column {
                            id: collapsedHeaders; width: parent.width
                            Repeater {
                                model: mail.collapsedBefore(groupedRow.index)
                                GroupHeader { required property string modelData; group: modelData; width: collapsedHeaders.width }
                            }
                        }
                        GroupHeader { group: groupedRow.group; visible: groupedRow.headingHeight > 0; width: parent.width; y: collapsedHeaders.height + groupedRow.headingHeight - height }
                        ItemDelegate {
                            id: row
                            objectName: "conversationRow_"+groupedRow.modelData.id
                            visible: !groupedRow.collapsed
                            y: collapsedHeaders.height + groupedRow.headingHeight; width: parent.width; height: parent.height-y
                            leftPadding: 24; rightPadding: 24; topPadding: 0; bottomPadding: 0; hoverEnabled: true
                            Accessible.name: groupedRow.modelData.subject+", "+groupedRow.modelData.sender_name+(groupedRow.modelData.unread ? ", unread" : ", reviewed")+(groupedRow.modelData.starred ? ", action held" : "")
                            onClicked: mail.openRow(groupedRow.modelData.id)
                            background: Rectangle { color: (mail.draftsMode ? backend.activeDraftId : backend.selectedId)===groupedRow.modelData.id ? Theme.selected : row.hovered ? Theme.surface : "transparent"; border.color: row.activeFocus ? Theme.accent : "transparent" }
                            contentItem: RowLayout {
                                spacing: 14
                                Item {
                                    implicitWidth: 6; implicitHeight: 12
                                    FmIcon { anchors.centerIn: parent; visible: mail.draftsMode; name: "edit"; size: 12; color: Theme.muted }
                                    Rectangle { anchors.centerIn: parent; width: 6; height: 6; radius: 3; color: groupedRow.modelData.unread ? Theme.text : "transparent"; border.width: groupedRow.modelData.starred && !groupedRow.modelData.unread ? 1 : 0; border.color: Theme.muted; visible: !mail.draftsMode && (groupedRow.modelData.unread || groupedRow.modelData.starred) }
                                }
                                ColumnLayout {
                                    Layout.fillWidth: true; spacing: 2
                                    RowLayout {
                                        Layout.fillWidth: true; spacing: 7
                                        Text { text: groupedRow.modelData.subject; color: groupedRow.doneRow ? Theme.muted : Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(14); font.weight: groupedRow.modelData.unread ? Font.Medium : Font.Normal; elide: Text.ElideRight; Layout.fillWidth: true; textFormat: Text.PlainText }
                                        Rectangle {
                                            visible: groupedRow.modelData.message_count>1; implicitWidth: memberCount.implicitWidth+10; implicitHeight: 17; radius: 3; color: "transparent"; border.color: Theme.controlBorder
                                            Text { id: memberCount; anchors.centerIn: parent; text: groupedRow.modelData.message_count; color: Theme.muted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(10) }
                                        }
                                    }
                                    RowLayout {
                                        Layout.fillWidth: true; spacing: 5
                                        FmIcon { visible: !mail.draftsMode; name: mail.kindIcon(groupedRow.modelData.kind); size: 11; color: Theme.subtle }
                                        Text { text: groupedRow.modelData.sender_name+(groupedRow.modelData.snippet ? " · "+groupedRow.modelData.snippet.replace(/\s+/g," ") : ""); color: Theme.secondary; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(12); elide: Text.ElideRight; Layout.fillWidth: true; textFormat: Text.PlainText }
                                    }
                                }
                                Rectangle {
                                    visible: (groupedRow.modelData.categories||[]).length>0 && leftPane.width>480
                                    implicitWidth: Math.min(rowCategory.implicitWidth+14,82); implicitHeight: 22; radius: 4; color: "transparent"; border.color: Theme.line
                                    Text { id: rowCategory; anchors.fill: parent; anchors.margins: 3; text: (groupedRow.modelData.categories||[]).length ? groupedRow.modelData.categories[0].name : ""; color: Theme.muted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(12); elide: Text.ElideRight; horizontalAlignment: Text.AlignHCenter }
                                }
                                Text { text: groupedRow.group === "held" || groupedRow.doneRow ? mail.relativeTime(groupedRow.modelData.latest_at) : mail.rowTime(groupedRow.modelData.latest_at); color: groupedRow.group === "held" ? Theme.warn : Theme.subtle; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(12); font.weight: groupedRow.group === "held" ? Font.Medium : Font.Normal; Layout.preferredWidth: Theme.fontSize(68); horizontalAlignment: Text.AlignRight }
                            }
                            FmToolTip { visible: row.hovered; text: groupedRow.modelData.subject + "\n" + mail.kindTitle(groupedRow.modelData.kind) + " · " + mail.fullTime(groupedRow.modelData.latest_at) + (groupedRow.group === "held" ? "\nDelayed without a timer." : "") + (groupedRow.modelData.pinned ? "\nKept in place while open. Current state: "+mail.groupTitle(groupedRow.modelData.group)+"." : "") }
                        }
                    }
                    footer: Column {
                        width: list.width
                        Repeater { model: mail.trailingCollapsed; GroupHeader { required property string modelData; group: modelData; width: list.width } }
                        Item {
                            width: list.width; height: backend.hasMore || (backend.folder==="sent" && backend.captures.length) ? 48 : 0
                            Row { anchors.centerIn: parent; spacing: 12
                                FmButton { visible: backend.hasMore; text: backend.loading ? "Loading…" : "Load more"; enabled: !backend.loading; onClicked: backend.loadMore() }
                                FmButton { objectName: "localCapturesButton"; visible: backend.folder==="sent" && backend.captures.length>0; text: "Local captures"; font.pixelSize: Theme.fontSize(12); onClicked: capturePopup.open() }
                            }
                        }
                    }
                    Text {
                        anchors.horizontalCenter: parent.horizontalCenter; y: 48; width: parent.width-48
                        visible: list.count===0 && mail.trailingCollapsed.length===0
                        text: !backend.connected ? "Connecting to your local mail…" : !backend.accounts.length ? "No accounts connected." : backend.loading ? "Searching…" : mail.filtering ? "No conversations match these filters." : "No conversations in this view."
                        color: Theme.muted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(13); horizontalAlignment: Text.AlignHCenter; wrapMode: Text.Wrap
                    }
                }
            }
        }
        Item {
            id: readerPane
            SplitView.fillWidth: true; SplitView.minimumWidth: Math.max(470, readerButtons.implicitWidth + 48)
            readonly property int margin: width<620 ? 24 : 40
            Rectangle { anchors.fill: parent; color: Theme.bg0 }
            Rectangle {
                width: 28; height: parent.height
                gradient: Gradient { orientation: Gradient.Horizontal; GradientStop { position: 0; color: Theme.shadow } GradientStop { position: 1; color: "transparent" } }
                opacity: .3
            }
            ColumnLayout {
                anchors.fill: parent; spacing: 0
                ColumnLayout {
                    visible: mail.members.length>0
                    Layout.fillWidth: true; Layout.leftMargin: readerPane.margin; Layout.rightMargin: readerPane.margin; Layout.topMargin: 24; spacing: 14
                    Item {
                        id: readerActions
                        objectName: "conversationActionsRow"
                        Layout.fillWidth: true
                        readonly property bool wrapped: width < readerCategoryRow.width + readerButtons.width + 16
                        implicitHeight: wrapped ? readerCategoryRow.height + 12 + readerButtons.height : Math.max(readerCategoryRow.height, readerButtons.height)
                        Row {
                            id: readerCategoryRow; spacing: 6; height: 32
                            FmButton { id: categoriesButton; objectName: "categoryAssignmentButton"; text: mail.thread.categories && mail.thread.categories.length ? mail.thread.categories[0].name : "Label"; font.pixelSize: Theme.fontSize(12); implicitHeight: 24; anchors.verticalCenter: parent.verticalCenter; outlined: true; tip: "Edit labels on the loaded members"; onClicked: categoryPopup.open() }
                            Repeater { model: (mail.thread.categories||[]).slice(1); FmButton { required property var modelData; text: modelData.name; font.pixelSize: Theme.fontSize(12); implicitHeight: 24; anchors.verticalCenter: parent.verticalCenter; outlined: true; onClicked: categoryPopup.open() } }
                        }
                        Row {
                            id: readerButtons
                            x: Math.max(0, readerActions.width - width)
                            y: readerActions.wrapped ? readerCategoryRow.height + 12 : 0
                            height: 32; spacing: 4
                            ReaderAction { objectName: "replyButton"; text: "Reply"; iconName: "reply"; font.pixelSize: Theme.fontSize(13); prominent: true; enabled: !backend.draftBusy; onClicked: mail.openReply() }
                            ReaderAction { objectName: "replyAllButton"; visible: backend.draft.reply_all_available === true; text: "Reply all"; iconName: "replyAll"; font.pixelSize: Theme.fontSize(13); outlined: true; enabled: !backend.draftBusy; onClicked: backend.beginDraft("reply_all") }
                            ReaderAction { objectName: "forwardButton"; text: "Forward"; iconName: "forward"; font.pixelSize: Theme.fontSize(13); outlined: true; enabled: !backend.draftBusy; onClicked: backend.beginDraft("forward") }
                            Item { width: 13; height: 32; Rectangle { x: Theme.snapX(parent, (parent.width-1)/2); y: Theme.snapY(parent, (parent.height-20)/2); width: Theme.ruleWidth; height: 20; color: Theme.controlBorder } }
                            ReaderAction { objectName: "delayConversationButton"; visible: !mail.anyStarred; text: "Delay"; iconName: "clock"; outlined: true; font.pixelSize: Theme.fontSize(13); enabled: !backend.mutationInFlight && mail.canMutate; tip: "Hold this conversation for later, without a timer · D"; onClicked: backend.setStarred(true) }
                            ReaderAction { objectName: "doneConversationButton"; visible: mail.anyStarred; text: "Done"; iconName: "check"; outlined: true; font.pixelSize: Theme.fontSize(13); enabled: !backend.mutationInFlight && mail.canMutate; tip: "Clear unread and stars on the loaded members · E"; onClicked: backend.doneConversation() }
                            FmButton { id: moreButton; objectName: "conversationActionsButton"; visible: backend.folder==="sent" || !backend.syntheticProfile; iconName: "more"; implicitWidth: 30; outlined: true; tip: "More conversation actions"; onClicked: morePopup.open() }
                        }
                    }
                    Text {
                        objectName: "mailDeliveryStatus"; visible: !backend.syntheticProfile && (!mail.canMutate || text.length > 0)
                        text: !mail.canMutate ? "Cached copy · reconnect with Mail access to mark reviewed or change labels"
                            : ({pending:"Saved locally · waiting for Google",unknown:"Saved locally · checking Google's outcome",confirmed:"Synced to Google",failed:"Google did not accept this change · check account access",superseded:"Replaced by a later change"})[backend.mailDelivery.state] || ""
                        color: backend.mailDelivery.state === "failed" ? Theme.error : Theme.subtle
                        font { family: Theme.fontFamily; pixelSize: Theme.fontSize(12) }
                        Layout.fillWidth: true; wrapMode: Text.Wrap; textFormat: Text.PlainText
                    }
                    Text { text: mail.thread.subject||""; color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(28); font.weight: Font.Medium; font.letterSpacing: -0.56; wrapMode: Text.Wrap; Layout.fillWidth: true; Layout.topMargin: 10; textFormat: Text.PlainText }
                    RowLayout {
                        Layout.fillWidth: true; spacing: 10
                        Rectangle {
                            implicitWidth: 24; implicitHeight: 24; color: Theme.selected; radius: 12
                            FmIcon { anchors.centerIn: parent; name: mail.kindIcon(mail.thread.kind); color: Theme.secondary; size: 13 }
                            FmToolTip { visible: kindHover.hovered; text: mail.kindTitle(mail.thread.kind) }
                            HoverHandler { id: kindHover }
                        }
                        Flow {
                            Layout.fillWidth: true; spacing: 4
                            FmButton {
                                id: metadataSender; text: mail.latest.sender_name || ""; foreground: Theme.secondary; font.pixelSize: Theme.fontSize(13)
                                implicitHeight: 24; implicitWidth: Math.min(contentItem.implicitWidth, readerPane.width / 3); leftPadding: 0; rightPadding: 0
                                onHoveredChanged: hovered ? mail.senderCardFor(mail.latest, metadataSender) : senderCard.scheduleClose()
                                onClicked: mail.senderCardFor(mail.latest, metadataSender)
                            }
                            Text {
                                text: "· " + mail.members.length + (mail.members.length === 1 ? " message · " : " messages · ") + mail.relativeTime(mail.latest.sent_at) + " · " + (backend.currentAccount.name || "")
                                height: 24; verticalAlignment: Text.AlignVCenter; color: Theme.muted; font.pixelSize: Theme.fontSize(13)
                                FmToolTip { visible: metadataDateHover.hovered; text: mail.fullTime(mail.latest.sent_at) }
                                HoverHandler { id: metadataDateHover }
                            }
                        }
                        FmButton { visible: mail.matchingIds.length>0; text: "Show match"; font.pixelSize: Theme.fontSize(11); onClicked: mail.revealMatch() }
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
                        x: readerPane.margin; y: 8; width: reader.width-readerPane.margin*2; spacing: 0; visible: mail.members.length>0 || mail.replyOpen
                        RowLayout {
                            Layout.fillWidth: true; visible: mail.members.length>1; spacing: 12; Layout.topMargin: 6; Layout.bottomMargin: 5
                            Rectangle { Layout.fillWidth: true; implicitHeight: 1; color: Theme.line }
                            FmButton { objectName: "earlierMessagesButton"; iconName: mail.expanded ? "up" : "down"; text: (mail.expanded ? "Hide " : "")+(mail.members.length-1)+" earlier messages"; font.pixelSize: Theme.fontSize(12); onClicked: { mail.expanded=!mail.expanded; backend.remember("expanded",mail.expanded) } }
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
                                    onClicked: if (member.index!==mail.members.length-1) mail.toggleMember(member.modelData.id,member.index)
                                    background: Rectangle { color: memberHeading.hovered ? Theme.surface : "transparent"; border.color: memberHeading.activeFocus ? Theme.accent : "transparent"; radius: 4 }
                                    contentItem: ColumnLayout {
                                        spacing: 4
                                        RowLayout {
                                            Layout.fillWidth: true
                                            FmButton {
                                                id: memberSender; objectName: "senderFilter_"+member.modelData.id; text: member.modelData.sender_name; foreground: Theme.text; font.pixelSize: Theme.fontSize(13); font.weight: Font.Medium
                                                Layout.fillWidth: true; leftPadding: 0; rightPadding: 0; textAlignment: Text.AlignLeft
                                                onHoveredChanged: hovered ? mail.senderCardFor(member.modelData, memberSender) : senderCard.scheduleClose()
                                                onClicked: mail.senderCardFor(member.modelData, memberSender)
                                            }
                                            Text { visible: member.isMatch; text: "Match"; color: Theme.accent; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(11) }
                                            FmIcon { visible: member.modelData.starred; name: "star"; size: 12; color: Theme.accent }
                                            Text {
                                                text: member.index === mail.members.length - 1 ? mail.relativeTime(member.modelData.sent_at) : mail.rowTime(member.modelData.sent_at)
                                                color: Theme.subtle; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(12)
                                                FmToolTip { visible: memberDateHover.hovered; text: mail.fullTime(member.modelData.sent_at) }
                                                HoverHandler { id: memberDateHover }
                                            }
                                        }
                                        Text { visible: !member.isExpanded; text: (member.modelData.text||"").replace(/\s+/g," "); color: Theme.subtle; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(12); elide: Text.ElideRight; Layout.fillWidth: true; textFormat: Text.PlainText }
                                    }
                                }
                                TextArea {
                                    visible: member.isExpanded; Layout.fillWidth: true; text: member.modelData.text||"Readable content is unavailable."
                                    readOnly: true; selectByMouse: true; wrapMode: TextEdit.Wrap; textFormat: TextEdit.PlainText
                                    color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(15); leftPadding: 0; rightPadding: 0; topPadding: 0; bottomPadding: 0; background: null
                                    Accessible.name: "Message from "+member.modelData.sender_name
                                }
                                Flow {
                                    visible: member.isExpanded; Layout.fillWidth: true; spacing: 8
                                    Repeater { model: backend.linksInText(member.modelData.text||""); FmButton { required property string modelData; text: modelData.length>60 ? modelData.substring(0,60)+"…" : modelData; implicitWidth: Math.min(contentItem.implicitWidth+22,readerColumn.width); font.pixelSize: Theme.fontSize(11); tip: "Open link: "+modelData; onClicked: backend.openLink(modelData) } }
                                }
                                Flow {
                                    visible: member.isExpanded; Layout.fillWidth: true; spacing: 8
                                    Repeater {
                                        model: member.modelData.attachments||[]
                                        FmButton {
                                            required property var modelData
                                            iconName: "attachment"; text: modelData.name+" · "+(modelData.state==="available" ? "available offline" : modelData.state); implicitWidth: Math.min(contentItem.implicitWidth+22,readerColumn.width)
                                            outlined: true; font.pixelSize: Theme.fontSize(12); enabled: modelData.state==="available"; onClicked: backend.openAttachment(modelData.id)
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
                            Layout.fillWidth: true; Layout.topMargin: visible ? (mail.members.length ? 36 : 24) : 0
                            visible: mail.replyOpen
                            onDismissed: mail.setReplyOpen(false)
                        }
                    }
                    Column {
                        visible: mail.members.length===0 && !mail.replyOpen; anchors.horizontalCenter: parent.horizontalCenter; y: parent.height*0.2; width: parent.width-readerPane.margin*2; spacing: 14
                        Text { text: backend.loading ? "Loading conversation…" : "Select a conversation"; color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(28); font.weight: Font.Medium; width: parent.width; wrapMode: Text.Wrap }
                        Text { text: "Mail stays in "+(backend.currentAccount.name||"the selected account")+". Your calendar remains in view below."; color: Theme.muted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(14); width: parent.width; wrapMode: Text.Wrap; lineHeight: 1.5 }
                    }
                }
            }
        }
    }
    Rectangle {
        objectName: "minimisedDraft"
        visible: !mail.replyOpen && backend.draftStatus!=="Draft discarded" && (backend.activeDraftId.length>0 || (backend.draft.body||"").length>0)
        anchors.right: parent.right; anchors.bottom: parent.bottom; anchors.rightMargin: 24; anchors.bottomMargin: 16
        width: Math.min(420, readerPane.width - 48); height: Math.max(40, draftBar.implicitHeight + 8); z: 4
        color: Theme.surface; radius: 6; border.color: Theme.controlBorder
        RowLayout {
            id: draftBar; anchors.fill: parent; anchors.leftMargin: 14; anchors.rightMargin: 6; spacing: 10
            FmIcon { name: "edit"; size: 13; color: Theme.accent }
            FmButton { objectName: "resumeDraftButton"; text: backend.draft.subject || "New message"; foreground: Theme.text; font.pixelSize: Theme.fontSize(13); Layout.fillWidth: true; leftPadding: 0; rightPadding: 0; textAlignment: Text.AlignLeft; tip: "Open the draft"; onClicked: mail.resumeDraft() }
            Text { text: (backend.draft.to_recipients || []).length ? "to " + (backend.draft.to_recipients[0].name || backend.draft.to_recipients[0].email) : "no recipients yet"; color: Theme.muted; font.pixelSize: Theme.fontSize(12); Layout.maximumWidth: 135; elide: Text.ElideRight }
            FmButton { objectName: "minimisedDraftDiscard"; iconName: "close"; iconSize: 11; implicitWidth: 28; implicitHeight: 28; leftPadding: 0; rightPadding: 0; tip: "Discard draft"; onClicked: reply.confirmDiscard() }
        }
    }
    SenderCard { id: senderCard; formatAge: mail.relativeTime; onConversationOpened: function(id) { backend.openConversation(id) } }
    Popup {
        id: railCategories; objectName: "allCategoriesPopup"; parent: Overlay.overlay; width: 280; padding: 12
        height: Math.min(categoryChoices.implicitHeight + 24, parent.height - 100); focus: true
        onAboutToShow: { const point = allLabels.mapToItem(parent, 0, allLabels.height + 8); x = Math.max(12,Math.min(point.x,parent.width-width-12)); y = Math.max(64,Math.min(point.y,parent.height-height-12)) }
        background: Rectangle { color: Theme.surface; border.color: Theme.controlBorder; radius: 8 }
        contentItem: ScrollView {
            property bool blocksWorkspaceShortcuts: true
            clip: true; contentWidth: availableWidth; ScrollBar.horizontal.policy: ScrollBar.AlwaysOff
            ColumnLayout {
                id: categoryChoices; width: railCategories.availableWidth; spacing: 4
                Text { text: "All categories"; font.pixelSize: Theme.fontSize(13); color: Theme.text }
                Repeater { model: backend.categories
                    FmButton { required property var modelData; Layout.fillWidth: true; textAlignment: Text.AlignLeft; text: modelData.name + "  " + modelData.conversation_count; selected: backend.category === modelData.id; onClicked: { backend.category = selected ? "" : modelData.id; railCategories.close() } }
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
            Text { text: "Categories"; color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(14); font.weight: Font.Medium }
            Text { Layout.fillWidth: true; text: "Apply to the "+mail.members.length+" loaded messages. Later replies keep their own labels."; color: Theme.muted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(12); wrapMode: Text.Wrap }
            Repeater {
                model: backend.categories
                RowLayout {
                    required property var modelData
                    property int memberCount: mail.categoryCount(modelData.id)
                    Layout.fillWidth: true
                    Text { Layout.fillWidth: true; text: modelData.name; color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(12); textFormat: Text.PlainText }
                    Text { text: memberCount===0 ? "None" : memberCount===mail.members.length ? "All" : memberCount+"/"+mail.members.length; color: Theme.subtle; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(11) }
                    FmButton { objectName: "categoryAdd_"+modelData.id; text: memberCount===mail.members.length ? "Clear" : "Apply"; enabled: !backend.mutationInFlight && mail.canMutate; outlined: true; font.pixelSize: Theme.fontSize(11); onClicked: backend.setCategoryMembership(modelData.id,memberCount!==mail.members.length) }
                    FmButton { visible: memberCount>0 && memberCount<mail.members.length; text: "Clear"; font.pixelSize: Theme.fontSize(11); enabled: !backend.mutationInFlight && mail.canMutate; onClicked: backend.setCategoryMembership(modelData.id,false) }
                }
            }
        }
    }
    Popup {
        id: folderPopup; parent: Overlay.overlay; width: 160; padding: 6
        onAboutToShow: { const p=folderButton.mapToItem(parent,0,folderButton.height+6); x=Math.max(12,p.x); y=p.y }
        focus: true; closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
        background: Rectangle { color: Theme.surface; border.color: Theme.controlBorder; radius: 6 }
        contentItem: ColumnLayout {
            spacing: 2
            FmButton { objectName: "allMailScopeButton"; text: "All mail"; Layout.fillWidth: true; textAlignment: Text.AlignLeft; onClicked: { backend.folder="all"; folderPopup.close() } }
            FmButton { objectName: "trashScopeButton"; text: "Trash"; Layout.fillWidth: true; textAlignment: Text.AlignLeft; onClicked: { backend.folder="trash"; folderPopup.close() } }
        }
    }
    Popup {
        id: morePopup; parent: Overlay.overlay; width: 220; padding: 6
        onAboutToShow: { const p=readerActions.mapToItem(parent,readerActions.width,readerActions.height+6); x=Math.max(12,p.x-width); y=p.y }
        focus: true; closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
        background: Rectangle { color: Theme.surface; border.color: Theme.controlBorder; radius: 6 }
        contentItem: ColumnLayout {
            spacing: 2
            FmButton { text: "Mark unread"; Layout.fillWidth: true; textAlignment: Text.AlignLeft; enabled: !backend.mutationInFlight && mail.canMutate; onClicked: { backend.setRead(false); morePopup.close() } }
            FmButton { text: "Mark reviewed"; Layout.fillWidth: true; textAlignment: Text.AlignLeft; enabled: !backend.mutationInFlight && mail.canMutate; onClicked: { backend.setRead(true); morePopup.close() } }
            FmButton { objectName: "unholdConversationButton"; visible: mail.anyStarred; text: "Remove Delay"; Layout.fillWidth: true; textAlignment: Text.AlignLeft; enabled: !backend.mutationInFlight && mail.canMutate; onClicked: { backend.setStarred(false); morePopup.close() } }
            FmButton { text: "Done"; Layout.fillWidth: true; textAlignment: Text.AlignLeft; enabled: !backend.mutationInFlight && mail.canMutate; onClicked: { backend.doneConversation(); morePopup.close() } }
            FmButton { objectName: "archiveConversation"; visible: !backend.syntheticProfile && backend.folder !== "trash"; text: "Archive"; Layout.fillWidth: true; textAlignment: Text.AlignLeft; enabled: !backend.mutationInFlight && mail.canMutate; onClicked: { backend.fileConversation("archive"); morePopup.close() } }
            FmButton { objectName: "trashConversation"; visible: !backend.syntheticProfile && backend.folder !== "trash"; text: "Move to Trash"; Layout.fillWidth: true; textAlignment: Text.AlignLeft; enabled: !backend.mutationInFlight && mail.canMutate; onClicked: { backend.fileConversation("trash"); morePopup.close() } }
            FmButton { objectName: "restoreConversation"; visible: !backend.syntheticProfile && backend.folder === "trash"; text: "Restore from Trash"; Layout.fillWidth: true; textAlignment: Text.AlignLeft; enabled: !backend.mutationInFlight && mail.canMutate; onClicked: { backend.fileConversation("restore"); morePopup.close() } }
            FmButton { visible: backend.syntheticProfile; text: "Local captures"; Layout.fillWidth: true; textAlignment: Text.AlignLeft; onClicked: { morePopup.close(); capturePopup.open() } }
        }
    }
    Popup {
        id: capturePopup; objectName: "captureHistoryPopup"; parent: Overlay.overlay
        width: Math.min(560,parent.width-40); height: Math.min(420,parent.height-80); anchors.centerIn: parent
        padding: 20; focus: true; closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
        background: Rectangle { color: Theme.surface; border.color: Theme.controlBorder; radius: 8 }
        contentItem: ColumnLayout {
            spacing: 14
            RowLayout {
                Layout.fillWidth: true
                Text { text: "Local captures"; color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(16); font.weight: Font.Medium; Layout.fillWidth: true }
                FmButton { iconName: "close"; tip: "Close local captures"; onClicked: capturePopup.close() }
            }
            Text { text: "Saved MIME copies on this machine. No messages were delivered."; color: Theme.muted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(12); wrapMode: Text.Wrap; Layout.fillWidth: true }
            ListView {
                Layout.fillWidth: true; Layout.fillHeight: true; clip: true; model: backend.captures; spacing: 8
                ScrollBar.vertical: ScrollBar {}
                delegate: RowLayout {
                    required property var modelData
                    width: ListView.view.width; height: 52; spacing: 10
                    ColumnLayout {
                        Layout.fillWidth: true; spacing: 4
                        Text { text: modelData.subject || "(No subject)"; color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(13); elide: Text.ElideRight; Layout.fillWidth: true }
                        Text { text: modelData.state+" · "+Qt.formatDateTime(new Date(modelData.created_at*1000),"d MMM, HH:mm"); color: Theme.muted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(12) }
                    }
                    FmButton { text: "Open"; onClicked: { if (modelData.draft_id) backend.openDraft(modelData.draft_id); else backend.selectConversation(modelData.conversation_id); capturePopup.close() } }
                    FmButton { text: "Inspect"; enabled: !backend.draftBusy; onClicked: backend.inspectCapture(modelData.operation_id) }
                }
                footer: FmButton { visible: backend.capturesMore; text: "Load more"; onClicked: backend.loadMoreCaptures() }
            }
            Text { text: backend.captureStatus; visible: text.length>0; color: Theme.muted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(12); wrapMode: Text.Wrap; Layout.fillWidth: true }
        }
    }
    TapHandler { parent: readerActions; acceptedButtons: Qt.RightButton; onTapped: mail.openActions() }
    Shortcut { sequence: "N"; enabled: mail.singleKeyActionsAllowed; onActivated: mail.newMessage() }
    Shortcut { sequence: "R"; enabled: mail.singleKeyActionsAllowed && mail.members.length>0; onActivated: mail.openReply() }
    Shortcut { sequence: "A"; enabled: mail.singleKeyActionsAllowed && mail.members.length>0 && backend.draft.reply_all_available === true; onActivated: backend.beginDraft("reply_all") }
    Shortcut { sequence: "F"; enabled: mail.singleKeyActionsAllowed && mail.members.length>0; onActivated: backend.beginDraft("forward") }
    Shortcut { sequence: "E"; enabled: mail.singleKeyActionsAllowed && mail.members.length>0 && mail.canMutate; onActivated: backend.doneConversation() }
    Shortcut { sequence: "D"; enabled: mail.singleKeyActionsAllowed && mail.members.length>0 && mail.canMutate; onActivated: backend.setStarred(!mail.anyStarred) }
    Shortcut { sequence: "S"; enabled: mail.singleKeyActionsAllowed; onActivated: backend.folder=backend.folder==="inbox" ? "sent" : backend.folder==="sent" ? "drafts" : "inbox" }
    Shortcut { sequence: "J"; enabled: mail.singleKeyActionsAllowed; onActivated: mail.moveSelection(1) }
    Shortcut { sequence: "K"; enabled: mail.singleKeyActionsAllowed; onActivated: mail.moveSelection(-1) }
    Shortcut { sequence: "Escape"; enabled: mail.visible && mail.replyOpen && !mail.popupOpen; onActivated: { backend.saveDraft(); mail.setReplyOpen(false) } }
}
