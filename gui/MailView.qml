import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    id: mail
    objectName: "mailWorkspace"
    property var selectionRows: []
    property string selectionAnchor: ""
    property string selectionCursor: ""
    property var menuRows: []
    property bool rowMenu: false
    property var messageMenuSource: null
    readonly property bool hasSelection: selectionRows.length > 0
    readonly property bool selectionMutable: !backend.mutationInFlight && canMutate && !(morePopup.opened ? menuRows : hasSelection ? selectionRows : [thread]).some(function(row) { return row.is_outgoing })
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
    readonly property bool newMessageAvailable: backend.connected && backend.account.length > 0 && !backend.accountArchived && !backend.unsavedAccountBuffer
    readonly property bool popupOpen: assistancePopupOpen() || labelMenu.opened || discardSelection.opened || folderPopup.opened || filterPopup.opened || categoryPopup.opened || railCategories.opened || senderCard.opened || morePopup.opened || capturePopup.opened || reply.popupOpen || (mail.ApplicationWindow.window && mail.ApplicationWindow.window.searchPopupOpen)
    readonly property bool canMutate: { const accounts = backend.accounts; return backend.connected && backend.accountCapability(backend.account,"mail_write") }
    readonly property bool singleKeyActionsAllowed: mail.visible && !popupOpen && mail.ApplicationWindow.window && mail.ApplicationWindow.window.singleKeysAllowed
    readonly property bool dateGroups: (backend.folder === "sent" || backend.folder === "drafts") && backend.preferences.mail_date_grouping !== "plain"
    readonly property bool draftsMode: backend.folder === "drafts"
    readonly property bool collapsedDraftVisible: !replyOpen && !backend.sendQueued && !backend.googleSend.sent_current && backend.draftStatus !== "Draft discarded"
        && (backend.activeDraftId.length > 0 || (backend.draft.body || "").length > 0)
    readonly property bool anyUnread: members.some(function(m) { return m.unread })
    readonly property bool anyStarred: members.some(function(m) { return m.starred })
    readonly property int rowHeight: Math.ceil((backend.preferences.density === "compact" ? Theme.compactRowHeight : Theme.comfortableRowHeight) * Theme.fontScale)
    readonly property bool filtering: backend.query.length > 0 || Object.keys(backend.filters).length > 0 || backend.category.length > 0
    readonly property var matchingIds: {
        const row = backend.conversations.find(function(c) { return c.id === backend.selectedId })
        return row && row.matching_message_ids ? row.matching_message_ids : []
    }
    readonly property var latest: members.length ? members[members.length - 1] : ({})
    property var participantMetadata: ({})
    onExpansionKeyChanged: participantMetadata = ({})
    function recordMetadata(id, rows) { const next=Object.assign({},participantMetadata); next[id]=rows; participantMetadata=next }
    readonly property var participants: {
        const seen = {}, people = []
        for (const message of members) {
            const key = (message.sender_email || message.sender_name || "").trim().toLowerCase()
            if (seen[key]) continue
            seen[key] = true
            people.push({name: message.sent ? "You" : message.sender_name || message.sender_email, message: message})
        }
        for (const message of members) {
            for (const row of message.metadata || participantMetadata[message.id] || []) {
                if (row.label.toLowerCase() !== "cc") continue
                for (const person of row.people) {
                    const key=(person.email || "").trim().toLowerCase()
                    if (!key || seen[key]) continue
                    seen[key]=true
                    people.push({name:person.name || person.email,message:{id:"cc_"+key,sender_email:person.email,sender_name:person.name,sent:false}})
                }
            }
        }
        return people
    }
    function initials(name) { return (name || "?").split(/\s+/).slice(0,2).map(function(part) { return part.charAt(0).toUpperCase() }).join("") }
    readonly property string conversationSpan: members.length ? (members.length > 1 && members[0].sent_at.slice(0,10) !== latest.sent_at.slice(0,10) ? mail.relativeTime(members[0].sent_at) + " → " : "") + mail.relativeTime(latest.sent_at) : ""

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
    function clearSelection() { selectionRows = []; selectionCursor = "" }
    function listHasFocus() {
        let item = mail.ApplicationWindow.window ? mail.ApplicationWindow.window.activeFocusItem : null
        while (item) { if (item === list) return true; item = item.parent }
        return false
    }
    function visibleRows() { return backend.conversations.filter(function(row) { return (backend.folder !== "inbox" && !mail.dateGroups) || !mail.groupCollapsed(mail.groupOf(row)) }) }
    function isSelected(id) { return selectionRows.some(function(row) { return row.id === id }) }
    function frozenRows(ids) {
        const snapshots = backend.snapshotMailRows(ids)
        return snapshots.map(function(row) { return mail.selectionRows.find(function(old) { return old.id === row.id }) || row })
    }
    function selectRange(id) {
        const rows = visibleRows(), anchor = selectionAnchor || backend.selectedId || id
        let a = rows.findIndex(function(row) { return row.id === anchor }), b = rows.findIndex(function(row) { return row.id === id })
        if (b < 0) return
        if (a < 0) a = b
        selectionAnchor = anchor
        selectionRows = frozenRows(rows.slice(Math.min(a,b), Math.max(a,b)+1).map(function(row) { return row.id }))
        selectionCursor = id
    }
    function rowClick(id, modifiers) {
        morePopup.close()
        if (modifiers & Qt.ShiftModifier) { selectRange(id); return }
        if (modifiers & (Qt.ControlModifier | Qt.MetaModifier)) {
            let ids = selectionRows.map(function(row) { return row.id })
            const current = draftsMode ? backend.activeDraftId : backend.selectedId
            if (!ids.length && current && current !== id && visibleRows().some(function(row) { return row.id === current })) ids.push(current)
            if (ids.indexOf(id) >= 0) ids = ids.filter(function(key) { return key !== id }); else ids.push(id)
            selectionRows = frozenRows(ids); selectionAnchor = id; selectionCursor = id; return
        }
        clearSelection(); selectionAnchor = id; openRow(id)
    }
    function showRowMenu(id, anchor, x, y) {
        if (!isSelected(id)) clearSelection()
        menuRows = isSelected(id) ? selectionRows.slice() : backend.snapshotMailRows([id])
        if (!menuRows.length) return
        rowMenu = true
        const p = anchor.mapToItem(Overlay.overlay, x, y)
        morePopup.x = Math.max(8, Math.min(p.x, Overlay.overlay.width-morePopup.width-8))
        morePopup.y = Math.max(8, Math.min(p.y, Overlay.overlay.height-morePopup.implicitHeight-8))
        morePopup.open()
    }
    function selectionAction(action, category) {
        const rows = (morePopup.opened || labelMenu.opened) ? menuRows : selectionRows.length ? selectionRows : backend.snapshotMailRows([backend.selectedId])
        if (!rows.length) return
        morePopup.close(); labelMenu.close()
        backend.applyMailSelection(rows, action, category || "")
        clearSelection()
    }
    function composeMenu(mode) {
        if (menuRows.length !== 1) return
        const id = menuRows[0].id
        morePopup.close(); clearSelection()
        // Wait for this exact reader before creating a reply/forward.
        pendingComposeId = id; pendingComposeMode = mode; backend.openConversation(id)
        completeMenuCompose()
    }
    property string pendingComposeId: ""
    property string pendingComposeMode: ""
    function completeMenuCompose() {
        if (!pendingComposeId || backend.selectedId !== pendingComposeId || thread.id !== pendingComposeId) return
        const mode = pendingComposeMode; pendingComposeId = ""; pendingComposeMode = ""
        backend.beginDraft(mode)
    }
    function categoryCount(id) { return members.filter(function(m) { return (m.categories || []).indexOf(id) >= 0 }).length }
    function dateGroup(row) {
        const date = new Date(row.latest_at), today = new Date()
        if (isNaN(date.getTime())) return "Earlier"
        const days = Math.round((Date.UTC(today.getFullYear(),today.getMonth(),today.getDate()) - Date.UTC(date.getFullYear(),date.getMonth(),date.getDate())) / 86400000)
        if (days === 0) return "Today"
        if (days === 1) return "Yesterday"
        if (days > 1 && days < 7) return Qt.formatDate(date,"dddd")
        return Qt.formatDate(date,"MMMM yyyy")
    }
    function groupOf(row) { return dateGroups && row ? dateGroup(row) : row ? row.display_group || row.group || "done" : "done" }
    function groupTitle(group) { return {review:"To review",held:"Delayed",done:"Done"}[group] || group }
    function groupCount(group) { return dateGroups ? backend.conversations.filter(function(row) { return mail.groupOf(row) === group }).length : (backend.querySummary.filtered_group_counts || {})[group] || 0 }
    function groupKey(group) { return backend.account + "/" + backend.folder + "/" + group }
    function groupCollapsed(group) { return collapsedGroups[groupKey(group)] === true }
    function syncCollapsedGroups() { backend.setCollapsedGroups(backend.folder === "inbox" ? ["review", "held", "done"].filter(function(group) { return mail.groupCollapsed(group) }) : []) }
    function toggleGroup(group) { const next = Object.assign({}, collapsedGroups); next[groupKey(group)] = !groupCollapsed(group); collapsedGroups = next; syncCollapsedGroups() }
    function collapsedBetween(previous, next) {
        if (backend.folder !== "inbox") return []
        const groups = ["review", "held", "done"]
        return groups.filter(function(group, index) { return index > previous && index < next && mail.groupCollapsed(group) && mail.groupCount(group) > 0 })
    }
    function collapsedBefore(index, previous, current) {
        const groups = ["review", "held", "done"]
        if (index < 0 || (index > 0 && previous === current)) return []
        return collapsedBetween(index > 0 ? groups.indexOf(previous) : -1, groups.indexOf(current))
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
        enabled: backend.folder === "inbox" || mail.dateGroups
        onClicked: mail.toggleGroup(group)
        background: Item {}
        Accessible.name: mail.groupTitle(group) + ", " + mail.groupCount(group) + " conversations, " + (collapsed ? "collapsed" : "expanded")
        contentItem: RowLayout {
            spacing: 12
            FmIcon { name: mail.dateGroups ? "calendar" : heading.group === "review" ? "review" : heading.group === "held" ? "clock" : "checkCircle"; size: 12; color: Theme.subtle }
            Text { text: mail.dateGroups ? mail.groupTitle(heading.group) : backend.folder === "sent" ? "Sent" : backend.folder === "trash" ? "Trash" : backend.folder === "all" ? "All mail" : mail.groupTitle(heading.group); color: Theme.subtle; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(12) }
            Rectangle { Layout.fillWidth: true; implicitHeight: 1; color: Theme.line }
            Text { visible: backend.folder === "inbox" || mail.dateGroups; text: mail.groupCount(heading.group); color: Theme.subtle; font.pixelSize: Theme.fontSize(12) }
            FmIcon { visible: backend.folder === "inbox" || mail.dateGroups; name: "down"; size: 10; color: Theme.subtle; rotation: heading.collapsed ? -90 : 0; Behavior on rotation { NumberAnimation { duration: 180 } } }
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
    component ContactName: FmButton {
        id: contact
        property string email: ""
        leftPadding: 0; rightPadding: 0; topPadding: 0; bottomPadding: 0
        implicitHeight: contentItem.implicitHeight
        textAlignment: Text.AlignLeft
        foreground: Theme.inkAlpha(0.85)
        onHoveredChanged: hovered ? senderCard.showSender(email, text, contact) : senderCard.scheduleClose()
        onClicked: senderCard.showSender(email, text, contact)
        background: Item {
            Row {
                anchors.bottom: parent.bottom; spacing: 2
                visible: contact.hovered || contact.activeFocus
                Repeater { model: Math.floor(contact.width / 3); Rectangle { width: 1; height: 1; color: contact.foreground } }
            }
        }
    }
    component ContactAvatar: FmButton {
        id: avatar
        property string email: ""
        property string personName: ""
        property bool own: false
        property int diameter: 34
        implicitWidth: diameter; implicitHeight: diameter
        leftPadding: 0; rightPadding: 0; topPadding: 0; bottomPadding: 0
        Accessible.name: "Contact: " + personName
        onHoveredChanged: hovered ? senderCard.showSender(email, personName, avatar) : senderCard.scheduleClose()
        onClicked: senderCard.showSender(email, personName, avatar)
        contentItem: Text { text: mail.initials(avatar.personName); color: avatar.own ? Theme.accent : Theme.secondary; font.pixelSize: Theme.fontSize(avatar.diameter > 26 ? 12 : 10); font.weight: Font.Medium; horizontalAlignment: Text.AlignHCenter; verticalAlignment: Text.AlignVCenter }
        background: Rectangle { radius: avatar.diameter / 2; color: avatar.own ? Theme.accentAlpha(0.18) : Theme.bg4; border.width: avatar.activeFocus ? 1 : 0; border.color: Theme.accent }
    }
    function visibleMetadata(message, source) {
        const rows = message.metadata || (source.length ? source : message.to ? [{label:"To",people:[{name:"",email:message.to}]}] : [])
        const own = (backend.currentAccount.email || "").toLowerCase()
        return rows.filter(function(row) {
            const label = row.label.toLowerCase().replace(/[-_]/g," ")
            if (label === "reply to" && row.people.every(function(person) { return person.email.toLowerCase() === (message.sender_email || "").toLowerCase() })) return false
            return row.people.length && (message.sent || label !== "to" || !row.people.every(function(person) { return person.email.toLowerCase() === own }))
        })
    }
    function senderCardFor(member, anchor) { senderCard.showSender(member.sender_email, member.sent ? "You" : member.sender_name, anchor) }
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
    function fullTime(value) { const date = new Date(value); return isNaN(date.getTime()) ? "" : Qt.formatDateTime(date, "dddd d MMMM yyyy, HH:mm") }
    MailAssistance {
        id: assistance
        objectName: "mailAssistanceController"
        onOpenSource: messageId => mail.scrollToMember(messageId)
    }
    function assistancePopupOpen() {
        for (let i = 0; i < memberRepeater.count; ++i) {
            const item = memberRepeater.itemAt(i)
            if (item && item.assistancePopupOpen) return true
        }
        return false
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
        if (draftsMode) { backend.openDraft(id); return }
        pendingReveal = id; backend.openConversation(id)
        if (backend.selectedId === id && members.length) { if (matchingIds.length) revealMatch(); pendingReveal = "" }
    }
    function moveSelection(delta, extend) {
        const selected = extend && selectionCursor ? selectionCursor : draftsMode ? backend.activeDraftId : backend.selectedId
        const rows = backend.conversations.filter(function(row) { return (backend.folder !== "inbox" && !mail.dateGroups) || !mail.groupCollapsed(mail.groupOf(row)) })
        const current = rows.findIndex(function(row) { return row.id === selected })
        const next = Math.max(0, Math.min(rows.length - 1, current < 0 ? 0 : current + delta))
        if (!rows.length || next === current) return
        if (extend) selectRange(rows[next].id); else rowClick(rows[next].id, 0)
        list.positionViewAtIndex(backend.conversations.findIndex(function(row) { return row.id === rows[next].id }), ListView.Contain)
        backend.remember("listY", list.contentY)
    }
    function setReplyOpen(value) {
        const identity = composerIdentity
        const next = Object.assign({},replyStates); next[identity] = value; replyStates = next
        replyOpen = value; backend.remember("replyOpen",value)
    }
    function openReply() {
        if (backend.accountArchived || backend.unsavedAccountBuffer) return
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
        function onAccountChanged() { mail.clearSelection(); morePopup.close(); mail.pendingComposeId = ""; mail.syncCollapsedGroups() }
        function onQueryChanged() { mail.clearSelection(); morePopup.close(); mail.syncCollapsedGroups() }
        function onPresentationChanged() { Qt.callLater(mail.restorePosition) }
        function onConversationsChanged() { Qt.callLater(mail.restoreListPosition) }
        function onConversationChanged() {
            mail.completeMenuCompose()
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
            if (backend.googleSend.sent_current) { mail.setReplyOpen(false); return }
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
                    Text {
                        objectName: "accountArchiveNotice"; visible: backend.accountArchived || backend.unsavedAccountBuffer
                        Layout.fillWidth: true; wrapMode: Text.Wrap; textFormat: Text.PlainText
                        text: (backend.currentAccount.email || "") + (backend.unsavedAccountBuffer ? " · unsaved edits retained in this editor" : " · read-only account archive")
                        color: backend.unsavedAccountBuffer ? Theme.warn : Theme.subtle; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(12)
                    }
                    RowLayout {
                        Layout.fillWidth: true; spacing: 12
                        FmButton { objectName: "inboxScopeButton"; text: "Inbox"; selected: backend.folder==="inbox"; font.pixelSize: Theme.fontSize(20); font.weight: Font.Medium; implicitWidth: contentItem.implicitWidth; foreground: selected ? Theme.text : Theme.inkAlpha(.32); leftPadding: 0; rightPadding: 0; textAlignment: Text.AlignLeft; background: Item {} onClicked: backend.folder="inbox" }
                        Rectangle { implicitWidth: 3; implicitHeight: 3; radius: 2; color: Theme.inkAlpha(.3) }
                        FmButton { objectName: "sentScopeButton"; text: "Sent"; selected: backend.folder==="sent"; font.pixelSize: Theme.fontSize(20); font.weight: Font.Medium; implicitWidth: contentItem.implicitWidth; foreground: selected ? Theme.text : Theme.inkAlpha(.32); leftPadding: 0; rightPadding: 0; textAlignment: Text.AlignLeft; background: Item {} onClicked: backend.folder="sent" }
                        Rectangle { implicitWidth: 3; implicitHeight: 3; radius: 2; color: Theme.inkAlpha(.3) }
                        FmButton { objectName: "draftsScopeButton"; text: "Drafts"; selected: backend.folder==="drafts"; font.pixelSize: Theme.fontSize(20); font.weight: Font.Medium; implicitWidth: contentItem.implicitWidth; foreground: selected ? Theme.text : Theme.inkAlpha(.32); leftPadding: 0; rightPadding: 0; textAlignment: Text.AlignLeft; background: Item {} onClicked: backend.folder="drafts" }
                        Text { objectName: "sendingCount"; visible: backend.sendingCount > 0; text: backend.sendingCount + " sending"; color: Theme.accent; font.pixelSize: Theme.fontSize(12) }
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
                    visible: mail.draftsMode && !backend.syntheticProfile
                    Layout.fillWidth: true; spacing: 8
                    Text {
                        objectName: "googleDraftInventoryStatus"
                        Layout.fillWidth: true
                        text: backend.googleDraftInventory.status || "Cached Google drafts"
                        textFormat: Text.PlainText; wrapMode: Text.Wrap
                        color: Theme.subtle; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(12)
                    }
                    FmButton {
                        objectName: "refreshGoogleDrafts"; text: "Refresh"; outlined: true
                        enabled: backend.googleDraftInventory.can_refresh || false
                        tip: "Check Google drafts using this account's active sync session"
                        onClicked: backend.refreshGoogleDraftInventory()
                    }
                }
                RowLayout {
                    Layout.fillWidth: true; spacing: 6
                    Flickable {
                        enabled: !mail.draftsMode; opacity: enabled ? 1 : .3
                        Layout.fillWidth: true; implicitHeight: 37; clip: true
                        contentWidth: categoryRow.width; contentHeight: height; boundsBehavior: Flickable.StopAtBounds; flickableDirection: Flickable.HorizontalFlick
                        ScrollBar.horizontal: ScrollBar { height: 3 }
                        Row {
                            id: categoryRow; spacing: 6
                            FmButton { id: allCategories; text: "All  "+backend.mailScopeCount; selected: backend.category.length===0; prominent: selected; implicitHeight: 29; font.pixelSize: Theme.fontSize(12); background: Rectangle { radius: 15; color: allCategories.selected ? Theme.text : "transparent"; border.color: allCategories.activeFocus ? Theme.accent : Theme.controlBorder } onClicked: backend.category="" }
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
                        enabled: backend.folder === "inbox"; opacity: enabled ? 1 : .3; implicitWidth: kindButtons.width + 4; implicitHeight: 28; radius: 14; color: "transparent"; border.color: Theme.line
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
                RowLayout {
                    visible: mail.hasSelection
                    Layout.fillWidth: true; Layout.leftMargin: 24; Layout.rightMargin: 24; spacing: 6
                    Text { text: mail.selectionRows.length + " selected"; color: Theme.accent; font.pixelSize: Theme.fontSize(12); Layout.fillWidth: true }
                    FmButton { text: "Done"; visible: !mail.draftsMode && backend.folder !== "sent"; enabled: mail.selectionMutable; onClicked: mail.selectionAction("done") }
                    FmButton { text: "Delay"; visible: !mail.draftsMode && backend.folder !== "sent"; enabled: mail.selectionMutable; onClicked: mail.selectionAction("delay") }
                    FmButton { text: "More"; onClicked: mail.showRowMenu(mail.selectionRows[0].id, this, 0, height) }
                    FmButton { iconName: "close"; tip: "Clear selection · Esc"; onClicked: mail.clearSelection() }
                }
                Text {
                    visible: text.length > 0; text: backend.mailSelectionStatus
                    Layout.fillWidth: true; Layout.leftMargin: 24; Layout.rightMargin: 24
                    wrapMode: Text.Wrap; color: Theme.subtle; font.pixelSize: Theme.fontSize(12)
                }
                ListView {
                    id: list; objectName: "conversationList"
                    Layout.fillWidth: true; Layout.fillHeight: true; Layout.topMargin: 6
                    clip: true; model: backend.conversationModel; boundsBehavior: Flickable.StopAtBounds
                    ScrollBar.vertical: ScrollBar {}
                    // Consume each structural batch before the next can reuse
                    // delegate indexes. Retaining an old layout across several
                    // remove/move batches leaves retired rows over replacements.
                    Connections {
                        target: backend.conversationModel
                        function onRowsRemoved() { list.forceLayout() }
                        function onRowsMoved() { list.forceLayout() }
                        function onRowsInserted() { list.forceLayout() }
                    }
                    onContentHeightChanged: Qt.callLater(mail.restoreListPosition)
                    onMovementEnded: if (!mail.restoring) backend.remember("listY",contentY)
                    delegate: Item {
                        id: groupedRow
                        required property var rowData
                        required property string previousGroup
                        readonly property var modelData: rowData
                        required property int index
                        // Qt can retain a removed delegate with index -1 while
                        // replacements are laid out. Retired rows must neither
                        // paint over the new page nor remain clickable.
                        visible: index >= 0
                        enabled: index >= 0
                        readonly property string group: mail.groupOf(modelData)
                        readonly property bool firstInGroup: index===0 || (mail.dateGroups ? mail.groupOf(backend.conversations[index-1]) : previousGroup)!==group
                        readonly property bool doneRow: group==="done" && backend.folder === "inbox"
                        readonly property bool collapsed: (backend.folder === "inbox" || mail.dateGroups) && mail.groupCollapsed(group)
                        readonly property int headingHeight: firstInGroup && (mail.dateGroups || (backend.folder !== "sent" && !mail.draftsMode)) ? (index === 0 ? 34 : 44) : 0
                        width: list.width; height: collapsedHeaders.height + headingHeight + (collapsed ? 0 : mail.rowHeight)
                        Column {
                            id: collapsedHeaders; width: parent.width
                            Repeater {
                                model: mail.collapsedBefore(groupedRow.index, groupedRow.previousGroup, groupedRow.group)
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
                            onClicked: mail.rowClick(groupedRow.modelData.id, 0)
                            Accessible.selected: mail.isSelected(groupedRow.modelData.id)
                            MouseArea {
                                id: rowMouse; anchors.fill: parent; acceptedButtons: Qt.LeftButton | Qt.RightButton; hoverEnabled: true
                                onClicked: function(mouse) {
                                    row.forceActiveFocus()
                                    if (mouse.button === Qt.RightButton) mail.showRowMenu(groupedRow.modelData.id, row, mouse.x, mouse.y)
                                    else mail.rowClick(groupedRow.modelData.id, mouse.modifiers)
                                }
                            }
                            Keys.onPressed: function(event) {
                                if (event.key === Qt.Key_Menu || (event.key === Qt.Key_F10 && event.modifiers & Qt.ShiftModifier)) {
                                    mail.showRowMenu(groupedRow.modelData.id,row,24,row.height); event.accepted = true
                                }
                            }
                            background: Rectangle {
                                color: mail.isSelected(groupedRow.modelData.id) || (!mail.hasSelection && (mail.draftsMode ? backend.activeDraftId : backend.selectedId)===groupedRow.modelData.id) ? Theme.selected : rowMouse.containsMouse ? Theme.surface : "transparent"
                                border.color: row.activeFocus ? Theme.accent : "transparent"
                                Rectangle { width: 2; height: parent.height; color: Theme.accent; visible: mail.isSelected(groupedRow.modelData.id) }
                            }
                            contentItem: RowLayout {
                                spacing: 14
                                Item {
                                    implicitWidth: 6; implicitHeight: 12
                                    FmIcon { anchors.centerIn: parent; visible: mail.draftsMode; name: "edit"; size: 12; color: Theme.muted }
                                    Rectangle { anchors.centerIn: parent; width: 6; height: 6; radius: 3; color: groupedRow.modelData.unread ? Theme.text : "transparent"; border.width: groupedRow.modelData.starred && !groupedRow.modelData.unread ? 1 : 0; border.color: Theme.muted; visible: !mail.draftsMode && (groupedRow.modelData.unread || groupedRow.modelData.starred) }
                                }
                                ColumnLayout {
                                    Layout.fillWidth: true; spacing: 2
                                    Item {
                                        Layout.fillWidth: true; implicitHeight: Math.max(subjectText.implicitHeight,17)
                                        Text {
                                            id: subjectText; anchors.verticalCenter: parent.verticalCenter
                                            width: Math.max(0,Math.min(implicitWidth,parent.width-(countBadge.visible ? countBadge.width+7 : 0)))
                                            text: groupedRow.modelData.subject; color: groupedRow.doneRow ? Theme.muted : Theme.text
                                            font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(14); font.weight: groupedRow.modelData.unread ? Font.Medium : Font.Normal
                                            elide: Text.ElideRight; textFormat: Text.PlainText
                                        }
                                        Rectangle {
                                            id: countBadge; x: subjectText.width+7; anchors.verticalCenter: parent.verticalCenter
                                            visible: groupedRow.modelData.message_count>1; width: memberCount.implicitWidth+10; height: 17; radius: 3; color: "transparent"; border.color: Theme.controlBorder
                                            Text { id: memberCount; anchors.centerIn: parent; text: groupedRow.modelData.message_count; color: Theme.muted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(10) }
                                        }
                                    }
                                    RowLayout {
                                        Layout.fillWidth: true; spacing: 5
                                        FmIcon { visible: !mail.draftsMode; name: groupedRow.modelData.is_outgoing ? "send" : mail.kindIcon(groupedRow.modelData.kind); size: 11; color: Theme.subtle }
                                        FmIcon { objectName:"summaryPreviewIcon_"+groupedRow.modelData.id; visible:!!groupedRow.modelData.summary; name:"summary"; size:11; color:Theme.ai }
                                        Text { objectName:"mailPreview_"+groupedRow.modelData.id; text: groupedRow.modelData.summary || (groupedRow.modelData.sender_name+(groupedRow.modelData.snippet ? " · "+groupedRow.modelData.snippet.replace(/\s+/g," ") : "")); color: Theme.secondary; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(12); elide: Text.ElideRight; Layout.fillWidth: true; textFormat: Text.PlainText }
                                    }
                                }
                                Rectangle {
                                    visible: (groupedRow.modelData.categories||[]).length>0
                                    implicitWidth: Math.min(rowCategory.implicitWidth+14, leftPane.width > 480 ? 100 : 68); implicitHeight: 22; radius: 4; color: "transparent"; border.color: Theme.line
                                    Text { id: rowCategory; anchors.fill: parent; anchors.margins: 3; text: (groupedRow.modelData.categories||[]).map(function(c) { return c.name }).join(" · "); color: Theme.muted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(12); elide: Text.ElideRight; horizontalAlignment: Text.AlignHCenter }
                                }
                                Text { text: groupedRow.modelData.is_outgoing ? groupedRow.modelData.send_status : groupedRow.group === "held" || groupedRow.doneRow ? mail.relativeTime(groupedRow.modelData.latest_at) : mail.rowTime(groupedRow.modelData.latest_at); color: groupedRow.group === "held" ? Theme.warn : Theme.subtle; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(12); font.weight: groupedRow.group === "held" ? Font.Medium : Font.Normal; Layout.preferredWidth: Theme.fontSize(68); horizontalAlignment: Text.AlignRight }
                            }
                            FmToolTip { visible: rowMouse.containsMouse; text: groupedRow.modelData.subject + "\n" + mail.kindTitle(groupedRow.modelData.kind) + " · " + mail.fullTime(groupedRow.modelData.latest_at) + ((groupedRow.modelData.categories || []).length ? "\n" + groupedRow.modelData.categories.map(function(c) { return c.name }).join(" · ") : "") + (groupedRow.group === "held" ? "\nDelayed without a timer." : "") + (groupedRow.modelData.pinned ? "\nKept in place while open. Current state: "+mail.groupTitle(groupedRow.modelData.group)+"." : "") }
                        }
                    }
                    footer: Column {
                        width: list.width
                        Repeater { model: mail.trailingCollapsed; GroupHeader { required property string modelData; group: modelData; width: list.width } }
                        Item {
                            width: list.width; height: backend.hasMore || (backend.folder==="sent" && backend.captures.length) ? 48 : 0
                            Row { anchors.centerIn: parent; spacing: 12
                                FmButton { objectName: "loadMoreConversations"; visible: backend.hasMore; text: backend.loading ? "Loading…" : "Load more"; enabled: !backend.loading; onClicked: backend.loadMore() }
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
            SplitView.fillWidth: true; SplitView.minimumWidth: 470
            readonly property int margin: width<620 ? 24 : 40
            Rectangle { anchors.fill: parent; color: Theme.bg0 }
            ColumnLayout {
                anchors.fill: parent; spacing: 0
                ColumnLayout {
                    visible: mail.members.length>0
                    Layout.fillWidth: true; Layout.leftMargin: readerPane.margin; Layout.rightMargin: readerPane.margin; Layout.topMargin: 24; spacing: 14
                    Flow {
                        id: readerHeading
                        objectName: "conversationHeadingRow"
                        Layout.fillWidth: true; spacing: 12
                        FmButton { id: categoriesButton; objectName: "categoryAssignmentButton"; visible: (mail.thread.categories || []).length > 0; text: visible ? mail.thread.categories[0].name : "Label"; font.pixelSize: Theme.fontSize(11.5); implicitHeight: 26; y: 3; outlined: true; onClicked: categoryPopup.open() }
                        Repeater { model: (mail.thread.categories||[]).slice(1); FmButton { required property var modelData; text: modelData.name; font.pixelSize: Theme.fontSize(11.5); implicitHeight: 26; y: 3; outlined: true; onClicked: categoryPopup.open() } }
                        Text { objectName: "readerSubject"; text: mail.thread.subject||""; width: Math.min(implicitWidth, readerHeading.width); color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(28); font.weight: Font.Medium; font.letterSpacing: -0.56; wrapMode: Text.Wrap; lineHeight: 1.15; textFormat: Text.PlainText }
                    }
                    RowLayout {
                        Layout.fillWidth: true; spacing: 12; Layout.topMargin: 2
                        Row {
                            spacing: -6
                            Repeater {
                                model: mail.participants.slice(0,6)
                                ContactAvatar {
                                    required property var modelData
                                    objectName: "participantAvatar_" + modelData.message.id
                                    diameter: 22; own: !!modelData.message.sent
                                    email: modelData.message.sender_email || ""; personName: modelData.name
                                }
                            }
                        }
                        Flow {
                            id: participantNames
                            Layout.fillWidth: true; spacing: 4
                            Text { text: mail.members.length + (mail.members.length === 1 ? " message · " : " messages · ") + mail.conversationSpan + " ·"; color: Theme.inkAlpha(0.62); font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(13) }
                            Repeater {
                                model: mail.participants
                                Row {
                                    required property var modelData
                                    required property int index
                                    ContactName { objectName: "participantName_" + parent.modelData.message.id; text: parent.modelData.name; email: parent.modelData.message.sender_email || ""; width: Math.min(implicitWidth, participantNames.width); foreground: parent.modelData.message.sent ? Theme.accent : Theme.inkAlpha(0.85) }
                                    Text { visible: parent.index < mail.participants.length - 1; text: ","; color: Theme.subtle; font.pixelSize: Theme.fontSize(13) }
                                }
                            }
                        }
                        FmButton { visible: mail.matchingIds.length>0; text: "Show match"; font.pixelSize: Theme.fontSize(11); onClicked: mail.revealMatch() }
                    }
                    Text {
                        objectName: "mailDeliveryStatus"; visible: !backend.syntheticProfile && text.length > 0
                        text: mail.thread.is_outgoing ? "" : backend.accountArchived ? "Read-only account archive" : !mail.canMutate ? "Cached copy · reconnect with Mail access to make changes"
                            : ({pending:"Saved locally · waiting for Google",unknown:"Saved locally · checking Google's outcome",failed:"Google did not accept this change · check account access"})[backend.mailDelivery.state] || ""
                        color: backend.mailDelivery.state === "failed" ? Theme.error : Theme.subtle
                        font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(12)
                        Layout.fillWidth: true; wrapMode: Text.Wrap; textFormat: Text.PlainText
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
                        x: readerPane.margin; y: 8; width: reader.width-readerPane.margin*2; spacing: 0; visible: mail.members.length>0 || mail.replyOpen || mail.collapsedDraftVisible
                        RowLayout {
                            Layout.fillWidth: true; visible: mail.members.length>1 && !mail.expanded; spacing: 12; Layout.topMargin: 6; Layout.bottomMargin: 5
                            Rectangle { Layout.fillWidth: true; implicitHeight: 1; color: Theme.line }
                            FmButton { objectName: "earlierMessagesButton"; iconName: mail.expanded ? "up" : "down"; text: (mail.expanded ? "Hide " : "")+(mail.members.length-1)+" earlier messages"; font.pixelSize: Theme.fontSize(12); onClicked: { mail.expanded=!mail.expanded; backend.remember("expanded",mail.expanded) } }
                            Rectangle { Layout.fillWidth: true; implicitHeight: 1; color: Theme.line }
                        }
                        Repeater {
                            id: memberRepeater
                            model: backend.messageModel
                            delegate: ColumnLayout {
                                id: member
                                required property var rowData
                                property var modelData: rowData
                                required property int index
                                readonly property bool isExpanded: mail.memberIsExpanded(modelData.id,index)
                                readonly property bool isMatch: mail.matchingIds.indexOf(modelData.id)>=0
                                readonly property bool assistancePopupOpen: messageAssistance.popupOpen || messageSource.popupOpen || attachmentPopupOpen()
                                function attachmentPopupOpen() {
                                    for (let i = 0; i < attachmentRepeater.count; ++i) {
                                        const tile = attachmentRepeater.itemAt(i)
                                        if (tile && tile.popupOpen) return true
                                    }
                                    return false
                                }
                                objectName: "messageMember_"+modelData.id
                                Layout.fillWidth: true; spacing: member.isExpanded ? 14 : 0; Layout.topMargin: member.isExpanded ? 24 : 0; Layout.bottomMargin: member.isExpanded ? 28 : 0; visible: mail.expanded || index===mail.members.length-1
                                MessageSource { id: messageSource; message: member.modelData; active: member.isExpanded && member.visible; visible: false; onMetadataChanged: mail.recordMetadata(member.modelData.id, metadata) }
                                ItemDelegate {
                                    id: memberHeading
                                    Layout.fillWidth: true; implicitHeight: member.isExpanded ? Math.max(34,contentItem.implicitHeight) : 44; leftPadding: 0; rightPadding: 0; topPadding: 0; bottomPadding: 0
                                    objectName: "messageHeading_" + member.modelData.id
                                    focusPolicy: member.isExpanded ? Qt.NoFocus : Qt.StrongFocus
                                    onClicked: if (!member.isExpanded) { memberSender.forceActiveFocus(); mail.toggleMember(member.modelData.id,member.index) }
                                    background: Rectangle { color: !member.isExpanded && memberHeading.hovered ? Theme.surface : "transparent"; border.color: memberHeading.activeFocus ? Theme.accent : "transparent"; radius: 4 }
                                    contentItem: ColumnLayout {
                                        spacing: 4
                                        RowLayout {
                                            Layout.fillWidth: true; spacing: 14
                                            ContactAvatar {
                                                objectName: "senderAvatar_" + member.modelData.id
                                                diameter: member.isExpanded ? 34 : 26; own: !!member.modelData.sent
                                                personName: member.modelData.sent ? "You" : member.modelData.sender_name || member.modelData.sender_email
                                                email: member.modelData.sender_email || ""
                                                Layout.alignment: Qt.AlignTop
                                            }
                                            Item {
                                                id: senderIdentity
                                                Layout.fillWidth: member.isExpanded; Layout.maximumWidth: member.isExpanded ? Number.POSITIVE_INFINITY : readerColumn.width * 0.32
                                                implicitWidth: memberSender.implicitWidth + (senderKind.visible ? 20 : 0)
                                                implicitHeight: identityContent.implicitHeight
                                                Column {
                                                    id: identityContent; width: parent.width; spacing: 2
                                                    Row {
                                                        width: parent.width; spacing: 8
                                                        ContactName {
                                                            id: memberSender; objectName: "senderFilter_"+member.modelData.id
                                                            text: member.modelData.sent ? "You" : member.modelData.sender_name || member.modelData.sender_email
                                                            email: member.modelData.sender_email || ""
                                                            foreground: member.modelData.sent ? Theme.accent : Theme.text
                                                            font.pixelSize: Theme.fontSize(member.isExpanded ? 14 : 13); font.weight: Font.Medium
                                                            width: Math.max(0, Math.min(implicitWidth, senderIdentity.width - (senderKind.visible ? 20 : 0)))
                                                        }
                                                        FmButton {
                                                            id: senderKind; objectName: "messageSenderKind_" + member.modelData.id
                                                            visible: member.isExpanded && !member.modelData.sent && assistance.classificationEnabled
                                                            iconName: mail.kindIcon(member.modelData.kind); iconSize: 12
                                                            implicitWidth: 12; implicitHeight: 18; leftPadding: 0; rightPadding: 0; topPadding: 0; bottomPadding: 0
                                                            tip: mail.kindTitle(member.modelData.kind) + " · correct sender kind"
                                                            onClicked: messageAssistance.correctSender()
                                                        }
                                                    }
                                                    Text { visible: member.isExpanded; text: member.modelData.sender_email || ""; color: Theme.subtle; font.pixelSize: Theme.fontSize(12.5); elide: Text.ElideRight; width: parent.width; textFormat: Text.PlainText }
                                                }
                                            }
                                            Text { visible: !member.isExpanded; text: (member.modelData.text || "").replace(/\s+/g," "); Layout.fillWidth: true; elide: Text.ElideRight; color: Theme.inkAlpha(0.45); font.pixelSize: Theme.fontSize(13); textFormat: Text.PlainText }
                                            Text { visible: member.isMatch; text: "Match"; color: Theme.accent; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(11) }
                                            RowLayout {
                                                Layout.alignment: Qt.AlignRight | Qt.AlignTop
                                                Layout.fillWidth: false
                                                Layout.maximumWidth: readerColumn.width * 0.45
                                                spacing: 10
                                                ColumnLayout {
                                                    objectName: "messageDateBlock_" + member.modelData.id
                                                    Layout.fillWidth: true; spacing: 2
                                                    RowLayout {
                                                        Layout.alignment: Qt.AlignRight; Layout.fillWidth: false; spacing: 8
                                                        FmIcon { objectName: "messageStar_" + member.modelData.id; visible: !!member.modelData.starred; name: "star"; size: 12; color: Theme.accent }
                                                        Text {
                                                            objectName: "messageShortDate_" + member.modelData.id
                                                            text: (member.index === mail.members.length - 1 ? mail.relativeTime(member.modelData.sent_at) : mail.rowTime(member.modelData.sent_at)) + (member.modelData.is_outgoing ? " · " + member.modelData.send_status : "")
                                                            color: member.isExpanded ? Theme.inkAlpha(0.85) : Theme.subtle; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(member.isExpanded ? 13 : 12)
                                                            FmToolTip { visible: memberDateHover.hovered; text: mail.fullTime(member.modelData.sent_at) }
                                                            HoverHandler { id: memberDateHover }
                                                        }
                                                    }
                                                    Text {
                                                        objectName: "messageFullDate_" + member.modelData.id
                                                        visible: member.isExpanded && !member.modelData.sent && member.index === mail.members.length - 1
                                                        Layout.fillWidth: true; horizontalAlignment: Text.AlignRight; wrapMode: Text.Wrap; text: mail.fullTime(member.modelData.sent_at)
                                                        color: Theme.subtle; font.pixelSize: Theme.fontSize(12)
                                                    }
                                                }
                                                FmButton {
                                                    id: memberMore; objectName: "messageMore_" + member.modelData.id; visible: member.isExpanded
                                                    Layout.alignment: Qt.AlignVCenter
                                                    iconName: "more"; implicitWidth: 28; implicitHeight: 28; leftPadding: 6; rightPadding: 6; outlined: true; tip: "Message options"
                                                    onClicked: {
                                                        mail.menuRows = backend.snapshotMailRows([member.modelData.is_outgoing ? member.modelData.id : backend.selectedId]); mail.rowMenu = true; mail.messageMenuSource = member.modelData.is_outgoing ? null : messageSource
                                                        const point = memberMore.mapToItem(Overlay.overlay, width, height)
                                                        morePopup.x = Math.max(8, point.x - morePopup.width); morePopup.y = point.y + 6; morePopup.open()
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                                ColumnLayout {
                                    id: recipients
                                    readonly property var rows: mail.visibleMetadata(member.modelData, messageSource.metadata)
                                    visible: member.isExpanded && rows.length > 0; Layout.fillWidth: true; Layout.leftMargin: 48; spacing: 6
                                    Repeater {
                                        model: recipients.rows
                                        RowLayout {
                                            required property var modelData; Layout.fillWidth: true; spacing: 14
                                            Text { text: modelData.label.toUpperCase(); Layout.preferredWidth: 48; Layout.alignment: Qt.AlignTop; color: Theme.inkAlpha(0.4); font.pixelSize: Theme.fontSize(11); font.letterSpacing: 0.66 }
                                            Flow {
                                                id: recipientPeople
                                                Layout.fillWidth: true; spacing: 14
                                                Repeater {
                                                    model: parent.parent.modelData.people
                                                    Flow {
                                                        required property var modelData
                                                        width: Math.min(recipientContact.implicitWidth + (recipientAddress.visible ? recipientAddress.implicitWidth + 5 : 0), recipientPeople.width); spacing: 5
                                                        ContactName { id: recipientContact; objectName: "recipientName_" + member.modelData.id + "_" + parent.modelData.email; email: parent.modelData.email; text: parent.modelData.name || parent.modelData.email; width: Math.min(implicitWidth, recipientPeople.width) }
                                                        Text { id: recipientAddress; visible: !!parent.modelData.name; text: parent.modelData.email; width: Math.min(implicitWidth, recipientPeople.width); elide: Text.ElideRight; color: Theme.subtle; font.pixelSize: Theme.fontSize(12); textFormat: Text.PlainText }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                                RowLayout {
                                    visible: member.isExpanded && !!member.modelData.text_truncated; Layout.fillWidth: true; Layout.leftMargin: 48; spacing: 12
                                    Text { visible: !!member.modelData.text_truncated; text: "This preview is shortened."; color: Theme.subtle; font.pixelSize: Theme.fontSize(12) }
                                }
                                MessageAssistance {
                                    id: messageAssistance
                                    Layout.fillWidth: true; Layout.leftMargin: 48
                                    controller: assistance; message: member.modelData
                                    showKindControl: false
                                    visible: member.isExpanded && !member.modelData.sent && (assistance.summariesEnabled || (assistance.classificationEnabled && !member.modelData.sent) || !!summary || !!classification)
                                }
                                Text { visible: member.isExpanded && !!member.modelData.content_error; Layout.fillWidth: true; Layout.leftMargin: 48; text: member.modelData.content_error || ""; color: Theme.error; wrapMode: Text.Wrap; font.pixelSize: Theme.fontSize(13) }
                                ColumnLayout {
                                    id: messageBody
                                    visible: member.isExpanded && !member.modelData.content_error
                                    Layout.fillWidth: true; Layout.leftMargin: 48; spacing: 7
                                    readonly property var rows: backend.receivedBody(messageSource.hasHtml && !messageSource.plain ? messageSource.result.body_html : "", member.modelData.content_pending ? "Loading sent message…" : member.modelData.text || "(No text)")
                                    readonly property string projectionKey: messageSource.identity + "/" + messageSource.plain + "/" + messageSource.hasHtml
                                    property var openQuotes: ({})
                                    onProjectionKeyChanged: openQuotes = ({})
                                    function toggle(key) { const next=Object.assign({},openQuotes); next[key]=!next[key]; openQuotes=next }
                                    Repeater {
                                        model: messageBody.rows
                                        ColumnLayout {
                                            required property var modelData
                                            required property int index
                                            Layout.fillWidth: true; Layout.leftMargin: Math.min(24,modelData.parents.length*10); spacing: 0
                                            visible: modelData.parents.every(function(key) { return !!messageBody.openQuotes[key] })
                                            FmButton {
                                                objectName: "quotedReply_" + member.modelData.id + "_" + index
                                                visible: !!modelData.quote
                                                text: messageBody.openQuotes[modelData.key] ? "Hide quoted reply" : "Show quoted reply"
                                                iconName: messageBody.openQuotes[modelData.key] ? "up" : "more"
                                                font.pixelSize: Theme.fontSize(11.5); implicitHeight: 25; leftPadding: 7; rightPadding: 9
                                                outlined: true; foreground: Theme.subtle
                                                onClicked: messageBody.toggle(modelData.key)
                                            }
                                            TextArea {
                                                objectName: index === 0 ? "messageBody_" + member.modelData.id : "messageBodyPart_" + member.modelData.id + "_" + index
                                                visible: !modelData.quote; Layout.fillWidth: true; text: modelData.html || ""
                                                readOnly: true; selectByMouse: true; wrapMode: TextEdit.Wrap; textFormat: TextEdit.RichText
                                                color: modelData.parents.length ? Theme.secondary : Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(15)
                                                leftPadding: 0; rightPadding: 0; topPadding: 0; bottomPadding: 0; background: null
                                                Accessible.name: "Message from " + member.modelData.sender_name
                                                onLinkActivated: function(link) { backend.openLink(link) }
                                            }
                                        }
                                    }
                                }
                                Flow {
                                    id: attachmentFlow
                                    visible: member.isExpanded && (member.modelData.attachments||[]).length > 0; Layout.fillWidth: true; Layout.leftMargin: 48; spacing: 8
                                    Repeater {
                                        id: attachmentRepeater
                                        model: member.modelData.attachments||[]
                                        AttachmentTile {
                                            required property var modelData
                                            attachment: modelData; account: backend.account
                                            maximumWidth: attachmentFlow.width
                                            available: !member.modelData.is_outgoing && modelData.state === "available"
                                        }
                                    }
                                }
                                Rectangle { Layout.fillWidth: true; implicitHeight: 1; color: Theme.line; Layout.topMargin: 10 }
                            }
                        }
                        Item {
                            id: readerActions; objectName: "conversationActionsRow"
                            visible: mail.members.length > 0 && !mail.replyOpen && !mail.thread.is_outgoing
                            Layout.fillWidth: true; Layout.topMargin: visible ? 24 : 0; Layout.bottomMargin: visible ? 8 : 0
                            implicitHeight: readerButtons.implicitHeight
                            Flow {
                                id: readerButtons; width: parent.width; spacing: 6
                                ReaderAction { objectName: "replyButton"; text: "Reply"; iconName: "reply"; prominent: true; visible: backend.folder !== "sent"; enabled: mail.newMessageAvailable && !backend.draftBusy; onClicked: mail.openReply() }
                                ReaderAction { objectName: "replyAllButton"; visible: backend.draft.reply_all_available === true && backend.folder !== "sent"; text: "Reply all"; iconName: "replyAll"; outlined: true; enabled: mail.newMessageAvailable && !backend.draftBusy; onClicked: backend.beginDraft("reply_all") }
                                ReaderAction { objectName: "forwardButton"; text: "Forward"; iconName: "forward"; outlined: true; enabled: mail.newMessageAvailable && !backend.draftBusy; onClicked: backend.beginDraft("forward") }
                                Item { width: 13; height: 32; Rectangle { x: 6; y: 6; width: 1; height: 20; color: Theme.line2 } }
                                ReaderAction { objectName: "delayConversationButton"; visible: !mail.anyStarred && backend.folder !== "sent"; text: "Delay"; iconName: "clock"; outlined: true; enabled: !backend.mutationInFlight && mail.canMutate; tip: "Hold this conversation for later · D"; onClicked: backend.setStarred(true) }
                                ReaderAction { objectName: "doneConversationButton"; visible: mail.anyStarred && backend.folder !== "sent"; text: "Done"; iconName: "check"; outlined: true; enabled: !backend.mutationInFlight && mail.canMutate; tip: "Clear unread and stars · E"; onClicked: backend.doneConversation() }
                                FmButton { id: moreButton; objectName: "conversationActionsButton"; iconName: "more"; implicitWidth: 32; outlined: true; tip: "More conversation actions"; onClicked: morePopup.open() }
                            }
                        }
                        Item {
                            objectName: "minimisedDraft"
                            visible: mail.collapsedDraftVisible
                            Layout.fillWidth: true; Layout.topMargin: visible ? 36 : 0
                            implicitHeight: 32
                            Rectangle { width: 2; height: parent.height; color: Theme.accentAlpha(.35) }
                            RowLayout {
                                anchors.fill: parent; anchors.leftMargin: 20; spacing: 10
                                FmIcon { name: "edit"; size: 13; color: Theme.accent }
                                FmButton { objectName: "resumeDraftButton"; text: backend.draft.subject || "New message"; foreground: Theme.text; font.pixelSize: Theme.fontSize(13); Layout.fillWidth: true; leftPadding: 0; rightPadding: 0; textAlignment: Text.AlignLeft; tip: "Open the draft"; onClicked: mail.resumeDraft() }
                                Text { text: (backend.draft.to_recipients || []).length ? "to " + (backend.draft.to_recipients[0].name || backend.draft.to_recipients[0].email) : "no recipients yet"; color: Theme.muted; font.pixelSize: Theme.fontSize(12); Layout.maximumWidth: 135; elide: Text.ElideRight }
                                FmButton { objectName: "minimisedDraftDiscard"; visible: !reply.readOnly; iconName: "close"; iconSize: 11; implicitWidth: 28; implicitHeight: 28; leftPadding: 0; rightPadding: 0; tip: "Discard draft"; enabled: !backend.draftLocked && !backend.attachmentImport.pending; onClicked: reply.confirmDiscard() }
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
                        visible: mail.members.length===0 && !mail.replyOpen && !mail.collapsedDraftVisible; anchors.horizontalCenter: parent.horizontalCenter; y: parent.height*0.2; width: parent.width-readerPane.margin*2; spacing: 14
                        Text { text: backend.loading ? "Loading conversation…" : "Select a conversation"; color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(28); font.weight: Font.Medium; width: parent.width; wrapMode: Text.Wrap }
                        Text { text: "Mail stays in "+(backend.currentAccount.name||"the selected account")+". Your calendar remains in view below."; color: Theme.muted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(14); width: parent.width; wrapMode: Text.Wrap; lineHeight: 1.5 }
                    }
                }
            }
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
        id: morePopup; readonly property bool outgoing: mail.menuRows.some(function(row) { return row.is_outgoing }); objectName: "mailContextMenu"; parent: Overlay.overlay; width: 220; padding: 6
        onAboutToShow: {
            if (!mail.rowMenu) {
                mail.menuRows = backend.snapshotMailRows([backend.selectedId])
                const p=readerActions.mapToItem(parent,readerActions.width,readerActions.height+6)
                x=Math.max(8,Math.min(p.x-width,parent.width-width-8)); y=Math.max(8,Math.min(p.y,parent.height-implicitHeight-8))
            }
        }
        onOpened: { x = Math.max(8, Math.min(x, parent.width - width - 8)); y = Math.max(8, Math.min(y, parent.height - height - 8)) }
        onClosed: { mail.rowMenu = false; labelMenu.close(); formatMenu.close(); mail.messageMenuSource = null }
        readonly property bool drafts: mail.menuRows.length > 0 && mail.menuRows.every(function(row) { return row.is_draft === true })
        readonly property bool allUnread: mail.menuRows.length > 0 && mail.menuRows.every(function(row) { return row.unread === true })
        focus: true; closePolicy: labelMenu.opened || formatMenu.opened ? Popup.CloseOnEscape : Popup.CloseOnEscape | Popup.CloseOnPressOutside
        background: Rectangle { color: Theme.surface; border.color: Theme.controlBorder; radius: 6 }
        contentItem: ColumnLayout {
            FmButton {
                objectName: "classifyConversation"; text: "Classify thread"; iconName: "summary"
                visible: !mail.rowMenu && assistance.classificationEnabled && mail.members.some(m => !m.sent)
                enabled: !assistance.busy && backend.connected
                Layout.fillWidth: true; textAlignment: Text.AlignLeft
                onClicked: { morePopup.close(); assistance.start("classification", "") }
            }

            spacing: 2
            Text { visible: mail.menuRows.length > 1; text: mail.menuRows.length + " SELECTED"; color: Theme.subtle; font.pixelSize: Theme.fontSize(11); Layout.margins: 8 }
            FmButton { objectName: "contextOutgoing"; visible: morePopup.outgoing && mail.menuRows.length === 1; text: mail.menuRows[0] && mail.menuRows[0].outgoing_state === "queued" ? "Undo send" : "Send status"; Layout.fillWidth: true; textAlignment: Text.AlignLeft; onClicked: { const row = mail.menuRows[0]; morePopup.close(); backend.outgoingAction(row.id, row.outgoing_state === "queued") } }
            FmButton { objectName: "contextReply"; visible: !morePopup.drafts && !morePopup.outgoing && backend.folder !== "sent" && mail.menuRows.length === 1; text: "Reply"; Layout.fillWidth: true; textAlignment: Text.AlignLeft; enabled: mail.newMessageAvailable && !backend.draftBusy; onClicked: mail.composeMenu("reply") }
            FmButton { objectName: "contextForward"; visible: !morePopup.drafts && !morePopup.outgoing && mail.menuRows.length === 1; text: "Forward"; Layout.fillWidth: true; textAlignment: Text.AlignLeft; enabled: mail.newMessageAvailable && !backend.draftBusy; onClicked: mail.composeMenu("forward") }
            Rectangle { visible: !morePopup.drafts && !morePopup.outgoing && mail.menuRows.length === 1; Layout.fillWidth: true; height: 1; color: Theme.line }
            FmButton { objectName: "contextRead"; visible: !morePopup.drafts && !morePopup.outgoing && backend.folder !== "sent"; text: morePopup.allUnread ? "Mark as read" : "Mark as unread"; Layout.fillWidth: true; textAlignment: Text.AlignLeft; enabled: mail.selectionMutable; onClicked: mail.selectionAction(morePopup.allUnread ? "read" : "unread") }
            FmButton { objectName: "contextDelay"; visible: !morePopup.drafts && !morePopup.outgoing && backend.folder !== "sent"; text: "Delay"; Layout.fillWidth: true; textAlignment: Text.AlignLeft; enabled: mail.selectionMutable; onClicked: mail.selectionAction("delay") }
            FmButton { objectName: "unholdConversationButton"; visible: !morePopup.drafts && !morePopup.outgoing && mail.menuRows.some(function(row) { return row.starred }); text: "Remove Delay"; Layout.fillWidth: true; textAlignment: Text.AlignLeft; enabled: mail.selectionMutable; onClicked: mail.selectionAction("undelay") }
            FmButton { objectName: "contextDone"; visible: !morePopup.drafts && !morePopup.outgoing && backend.folder !== "sent"; text: "Mark done"; Layout.fillWidth: true; textAlignment: Text.AlignLeft; enabled: mail.selectionMutable; onClicked: mail.selectionAction("done") }
            FmButton { id: labelsMenuButton; objectName: "contextLabels"; visible: !morePopup.drafts && !morePopup.outgoing && backend.folder !== "sent"; text: "Labels   ›"; Layout.fillWidth: true; textAlignment: Text.AlignLeft; enabled: mail.selectionMutable && backend.categories.length > 0; onClicked: labelMenu.open() }
            FmButton { objectName: "contextFormat"; visible: !!mail.messageMenuSource && mail.messageMenuSource.hasHtml; text: "Format   ‹"; Layout.fillWidth: true; textAlignment: Text.AlignLeft; onClicked: formatMenu.open() }
            FmButton { objectName: "contextMessageSource"; visible: !!mail.messageMenuSource; text: "View source"; Layout.fillWidth: true; textAlignment: Text.AlignLeft; onClicked: { const source = mail.messageMenuSource; morePopup.close(); source.show() } }
            Rectangle { visible: !morePopup.drafts && !morePopup.outgoing; Layout.fillWidth: true; height: 1; color: Theme.line }
            FmButton { objectName: "archiveConversation"; visible: !morePopup.drafts && !morePopup.outgoing && !backend.syntheticProfile && backend.folder !== "trash"; text: "Archive in Gmail"; Layout.fillWidth: true; textAlignment: Text.AlignLeft; enabled: mail.selectionMutable; onClicked: mail.selectionAction("archive") }
            FmButton { objectName: "trashConversation"; visible: !morePopup.drafts && !morePopup.outgoing && !backend.syntheticProfile && backend.folder !== "trash"; text: "Move to Trash"; foreground: Theme.error; Layout.fillWidth: true; textAlignment: Text.AlignLeft; enabled: mail.selectionMutable; onClicked: mail.selectionAction("trash") }
            FmButton { objectName: "restoreConversation"; visible: !morePopup.drafts && !backend.syntheticProfile && backend.folder === "trash"; text: "Restore from Trash"; Layout.fillWidth: true; textAlignment: Text.AlignLeft; enabled: mail.selectionMutable; onClicked: mail.selectionAction("restore") }
            FmButton { objectName: "contextDiscard"; visible: morePopup.drafts; text: mail.menuRows.length > 1 ? "Discard drafts…" : "Discard draft…"; foreground: Theme.error; Layout.fillWidth: true; textAlignment: Text.AlignLeft; enabled: mail.selectionMutable; onClicked: { discardSelection.rows = mail.menuRows.slice(); morePopup.close(); discardSelection.open() } }
            FmButton { visible: backend.syntheticProfile && !morePopup.drafts; text: "Local captures"; Layout.fillWidth: true; textAlignment: Text.AlignLeft; onClicked: { morePopup.close(); capturePopup.open() } }
        }
    }
    Popup {
        id: formatMenu; objectName: "messageFormatMenu"; parent: morePopup.contentItem
        width: 154; padding: 6; focus: true; x: -width - 8; y: Math.max(0, morePopup.height - height - 48)
        background: Rectangle { color: Theme.bg2; border.color: Theme.line2; radius: 7 }
        contentItem: ColumnLayout {
            spacing: 2
            FmButton { objectName: "messageFormatHtml"; text: "HTML"; iconName: mail.messageMenuSource && !mail.messageMenuSource.plain ? "check" : ""; Layout.fillWidth: true; textAlignment: Text.AlignLeft; onClicked: { if (mail.messageMenuSource) mail.messageMenuSource.plain = false; morePopup.close() } }
            FmButton { objectName: "messageFormatPlain"; text: "Plain text"; iconName: mail.messageMenuSource && mail.messageMenuSource.plain ? "check" : ""; Layout.fillWidth: true; textAlignment: Text.AlignLeft; onClicked: { if (mail.messageMenuSource) mail.messageMenuSource.plain = true; morePopup.close() } }
        }
    }
    Popup {
        id: labelMenu; objectName: "mailLabelSubmenu"; parent: morePopup.contentItem; width: 310; padding: 8; focus: true
        height: Math.min(implicitHeight, (mail.Overlay.overlay ? mail.Overlay.overlay.height : mail.height)-24)
        closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutsideParent
        onAboutToShow: { x = morePopup.x + morePopup.width + width < (mail.Overlay.overlay ? mail.Overlay.overlay.width : mail.width)-8 ? morePopup.width-6 : -width-6; y = Math.max(0,Math.min(labelsMenuButton.y,(mail.Overlay.overlay ? mail.Overlay.overlay.height : mail.height)-morePopup.y-height-8)) }
        background: Rectangle { color: Theme.surface; border.color: Theme.controlBorder; radius: 6 }
        contentItem: ScrollView {
            implicitHeight: labelOptions.implicitHeight; clip: true; contentWidth: availableWidth
            ColumnLayout { id: labelOptions; width: labelMenu.availableWidth; spacing: 4
                Repeater { model: backend.categories
                    RowLayout { required property var modelData; Layout.fillWidth: true
                        Text { text: modelData.name; textFormat: Text.PlainText; Layout.fillWidth: true; elide: Text.ElideRight; color: Theme.text; font.pixelSize: Theme.fontSize(12) }
                        FmButton { objectName: "applyLabel_" + modelData.id; text: "Apply"; enabled: mail.selectionMutable; onClicked: mail.selectionAction("label_add",modelData.id) }
                        FmButton { objectName: "clearLabel_" + modelData.id; text: "Clear"; enabled: mail.selectionMutable; onClicked: mail.selectionAction("label_remove",modelData.id) }
                    }
                }
            }
        }
    }
    FmConfirmDialog {
        id: discardSelection
        property var rows: []
        title: "Discard " + rows.length + (rows.length === 1 ? " draft?" : " drafts?")
        message: "The selected drafts will be discarded. You can undo for a few seconds afterwards."
        actionText: rows.length === 1 ? "Discard" : "Discard all"
        destructive: true
        onAccepted: { backend.applyMailSelection(rows,"discard"); mail.clearSelection() }
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
    Shortcut { sequences: [StandardKey.SelectAll]; enabled: mail.singleKeyActionsAllowed && mail.listHasFocus(); onActivated: { mail.selectionRows = mail.frozenRows(mail.visibleRows().slice(0,200).map(function(row) { return row.id })); } }
    Shortcut { sequence: "Escape"; enabled: mail.visible && mail.hasSelection && !mail.popupOpen; onActivated: mail.clearSelection() }
    Shortcut { sequence: "Down"; enabled: mail.singleKeyActionsAllowed && mail.listHasFocus(); onActivated: mail.moveSelection(1,false) }
    Shortcut { sequence: "Up"; enabled: mail.singleKeyActionsAllowed && mail.listHasFocus(); onActivated: mail.moveSelection(-1,false) }
    Shortcut { sequences: ["Delete", "Backspace"]; enabled: mail.singleKeyActionsAllowed && mail.listHasFocus() && mail.selectionMutable; onActivated: {
        if (mail.draftsMode) { discardSelection.rows = mail.hasSelection ? mail.selectionRows : backend.snapshotMailRows([backend.activeDraftId]); if (discardSelection.rows.length) discardSelection.open() }
        else if (!backend.syntheticProfile && backend.folder !== "trash") mail.selectionAction("trash")
    } }
    Shortcut { sequence: "Shift+Down"; enabled: mail.singleKeyActionsAllowed && mail.listHasFocus(); onActivated: mail.moveSelection(1,true) }
    Shortcut { sequence: "Shift+Up"; enabled: mail.singleKeyActionsAllowed && mail.listHasFocus(); onActivated: mail.moveSelection(-1,true) }
    Shortcut { sequence: "U"; enabled: mail.singleKeyActionsAllowed && !mail.draftsMode && mail.canMutate; onActivated: mail.selectionAction(mail.hasSelection ? (mail.selectionRows.every(function(row) { return row.unread }) ? "read" : "unread") : mail.anyUnread ? "read" : "unread") }
    Shortcut { sequence: "N"; enabled: mail.singleKeyActionsAllowed; onActivated: mail.newMessage() }
    Shortcut { sequence: "R"; enabled: mail.singleKeyActionsAllowed && !mail.hasSelection && mail.members.length>0 && !mail.thread.is_outgoing; onActivated: mail.openReply() }
    Shortcut { sequence: "A"; enabled: mail.singleKeyActionsAllowed && !mail.hasSelection && mail.members.length>0 && backend.draft.reply_all_available === true; onActivated: backend.beginDraft("reply_all") }
    Shortcut { sequence: "F"; enabled: mail.singleKeyActionsAllowed && !mail.hasSelection && mail.members.length>0 && !mail.thread.is_outgoing; onActivated: backend.beginDraft("forward") }
    Shortcut { sequence: "E"; enabled: mail.singleKeyActionsAllowed && !mail.draftsMode && (mail.hasSelection || mail.members.length>0) && mail.selectionMutable; onActivated: mail.selectionAction("done") }
    Shortcut { sequence: "D"; enabled: mail.singleKeyActionsAllowed && !mail.draftsMode && (mail.hasSelection || mail.members.length>0) && mail.selectionMutable; onActivated: mail.selectionAction(mail.hasSelection || !mail.anyStarred ? "delay" : "undelay") }
    Shortcut { sequence: "S"; enabled: mail.singleKeyActionsAllowed; onActivated: backend.folder=backend.folder==="inbox" ? "sent" : backend.folder==="sent" ? "drafts" : "inbox" }
    Shortcut { sequence: "J"; enabled: mail.singleKeyActionsAllowed; onActivated: mail.moveSelection(1) }
    Shortcut { sequence: "K"; enabled: mail.singleKeyActionsAllowed; onActivated: mail.moveSelection(-1) }
    Shortcut { sequence: "Escape"; enabled: mail.visible && mail.replyOpen && !mail.hasSelection && !mail.popupOpen; onActivated: { if (backend.accountNavigationAllowed()) { backend.saveDraft(); mail.setReplyOpen(false) } } }
}
