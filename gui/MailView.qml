import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQml.Models

Item {
    id: mail
    objectName: "mailWorkspace"
    property var selectionRows: []
    property string selectionAnchor: ""
    property string selectionCursor: ""
    property string pointerRow: ""
    property bool keyboardNavigation: false
    property point pointerPosition: Qt.point(-100000,-100000)
    readonly property string highlightedRow: !keyboardNavigation && pointerRow ? pointerRow : selectionCursor || (draftsMode ? backend.activeDraftId : backend.selectedId)
    property var menuRows: []
    property bool rowMenu: false
    property var messageMenuSource: null
    // The message whose own menu is open; its Reply, Reply all and Forward
    // answer that message rather than the thread.
    property var menuMember: null
    // The opened message menu's summary controls, for Resummarize.
    property var messageMenuAssistance: null
    readonly property bool hasSelection: selectionRows.length > 0
    readonly property bool readerMatchesCursor: !keyboardNavigation || hasSelection || !selectionCursor || selectionCursor === (draftsMode ? backend.activeDraftId : backend.selectedId)
    readonly property bool selectionMutable: readerMatchesCursor && !backend.mutationInFlight && canMutate && !(morePopup.visible ? menuRows : hasSelection ? selectionRows : [thread]).some(function(row) { return row.is_outgoing })
    property var thread: backend.conversation
    // Drafts open in the composer; a conversation from another scope stays hidden.
    property var members: draftsMode ? [] : thread.messages || []
    // A change Google refused, or why this thread can't change, shown under the
    // reader's buttons. Changes waiting for or being checked with Google say nothing.
    readonly property string failureNote: "Google did not accept this change · check account access"
    readonly property string readerNote: backend.syntheticProfile ? ""
        : thread.is_outgoing ? "" : thread.remote ? "Read from the server · not on this machine" : backend.accountArchived ? "Read-only account archive" : !canMutate ? "Cached copy · reconnect with Mail access to make changes"
        : backend.mailDelivery.state === "failed" ? failureNote : ""
    property bool replyOpen: false
    property var replyStates: ({})
    property string hydratedDraftIdentity: ""
    property bool restoring: false
    // Window layout kept outside the engine (WindowPlacement); absent in tests.
    readonly property QtObject layoutStore: typeof windowLayout !== "undefined" ? windowLayout : null
    property var expandedMembers: ({})
    property var collapsedGroups: ({})
    // One JS copy of the list per change. Each read of backend.conversations
    // converts the whole list again, and rows read it once per delegate.
    readonly property var rows: backend.conversations
    property string loadedConversation: ""
    property string loadedThread: ""
    property string pendingReveal: ""
    property string replyRevealIdentity: ""
    readonly property string composerIdentity: backend.account + (backend.activeDraftId.length ? "/draft/" + backend.activeDraftId : "/" + backend.selectedId)
    readonly property bool newMessageAvailable: backend.connected && backend.account.length > 0 && !backend.accountArchived && !backend.unsavedAccountBuffer
    readonly property bool popupOpen: mediaViewer.visible || assistancePopupOpen() || labelMenu.visible || textMenu.visible || discardSelection.visible || binConfirm.visible || reconnectPrompt.visible || railCategories.visible || senderCard.visible || morePopup.visible || capturePopup.visible || binMenu.visible || keepDialog.visible || reply.popupOpen || (mail.ApplicationWindow.window && mail.ApplicationWindow.window.searchPopupOpen)
    readonly property bool canMutate: { const accounts = backend.accounts; return backend.connected && backend.accountCapability(backend.account,"mail_write") }
    readonly property bool singleKeyActionsAllowed: mail.visible && !popupOpen && mail.ApplicationWindow.window && mail.ApplicationWindow.window.singleKeysAllowed
    // Spam is listed and read live from Gmail; none of it is stored. Spam and
    // Trash share one list style, a strip to empty them and their own actions.
    readonly property bool spamMode: backend.folder === "spam"
    readonly property bool binMode: spamMode || backend.folder === "trash"
    // Spam counts Gmail's conversations once known; Trash counts what is here.
    readonly property int binCount: spamMode ? (backend.binCounts.folder === "spam" ? backend.binCounts.conversations : mail.rows.length) : backend.mailScopeCount
    readonly property int binMessageCount: backend.binCounts.folder === backend.folder && backend.binCounts.messages !== undefined ? backend.binCounts.messages : binCount
    // Trash copies stored here, which outnumber Gmail's while sync catches up
    // on mail deleted in Gmail.
    readonly property int binStoredCount: backend.binCounts.folder === backend.folder && backend.binCounts.stored !== undefined ? backend.binCounts.stored : 0
    function reconnectForFullAccess() {
        const window = mail.ApplicationWindow.window, row = backend.currentAccount
        if (!window || !row || !row.id) return
        window.navigate("settings")
        Qt.callLater(function() { if (window.settingsView) { window.settingsView.page = "Accounts"; window.settingsView.reconnect(row) } })
    }
    readonly property bool readerKeyActionsAllowed: singleKeyActionsAllowed && readerMatchesCursor
    readonly property bool dateGroups: backend.folder === "sent" || backend.folder === "drafts"
    readonly property bool draftsMode: backend.folder === "drafts"
    // A reply that has gone out, with no follow-up written in it since.
    readonly property bool sentReplyDone: {
        const accepted = (backend.draft.google_send || {}).last_accepted_draft_revision || 0
        return accepted > 0 && !((backend.draft.revision || 0) > accepted && String(backend.draft.body || "").trim().length > 0)
    }
    readonly property bool collapsedDraftVisible: !replyOpen && !backend.sendQueued && !backend.googleSend.sent_current && !sentReplyDone && backend.draftStatus !== "Draft discarded"
        && (backend.activeDraftId.length > 0 || draftAuthored(backend.draft))
    // A draft starts with the account signature; only text beyond it, or a
    // file, makes it one worth opening or showing.
    function draftAuthored(draft) {
        const body = (draft.body || "").trim()
        const row = (backend.allAccounts || []).find(function(a) { return a.id === backend.account }) || {}
        return (draft.attachment_ids || []).length > 0 || (body.length > 0 && body !== (row.signature_text || "").trim())
    }
    readonly property bool anyUnread: members.some(function(m) { return m.unread })
    readonly property bool anyStarred: members.some(function(m) { return m.starred })
    readonly property int rowHeight: Math.ceil((backend.preferences.density === "compact" ? Theme.compactRowHeight : Theme.comfortableRowHeight) * Theme.fontScale)
    readonly property bool filtering: backend.query.length > 0 || Object.keys(backend.filters).length > 0 || backend.category.length > 0
    readonly property var matchingIds: {
        const row = mail.rows.find(function(c) { return c.id === backend.selectedId })
        return row && row.matching_message_ids ? row.matching_message_ids : []
    }
    readonly property var latest: members.length ? members[members.length - 1] : ({})
    property var participantMetadata: ({})
    onExpansionKeyChanged: { participantMetadata = ({});mediaViewer.close() }
    function recordMetadata(id, rows) { const next=Object.assign({},participantMetadata); next[id]=rows; participantMetadata=next }
    readonly property var participants: {
        const seen = {}, people = []
        for (const message of members) {
            const key = (message.sender_email || message.sender_name || "").trim().toLowerCase()
            if (seen[key]) continue
            seen[key] = true
            if (message.sent) continue
            people.push({name: message.sender_name || message.sender_email, message: message})
        }
        if (!people.length && members.length) people.push({name: "you", message: members[0]})
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
    readonly property string conversationSpan: members.length ? (members.length > 1 && (members[0].sent_at || "").slice(0,10) !== (latest.sent_at || "").slice(0,10) ? mail.relativeTime(members[0].sent_at) + " → " : "") + mail.relativeTime(latest.sent_at) : ""

    // Sent and Drafts rails offer Today / Yesterday / Earlier this week.
    // Sent uses the shared UTC date filter; local drafts are filtered here.
    property string draftDay: ""
    onDraftsModeChanged: draftDay = ""
    readonly property var dayOptions: [{key:"0",name:"Today"},{key:"1",name:"Yesterday"},{key:"older",name:"Earlier this week"}]
    // Day-level labels follow backend.clockDay, which changes only at midnight.
    function dayKey(value, utc) {
        const date = new Date(value), now = new Date(backend.clockDay)
        if (isNaN(date.getTime())) return ""
        const days = utc ? Math.round((Date.UTC(now.getUTCFullYear(), now.getUTCMonth(), now.getUTCDate()) - Date.UTC(date.getUTCFullYear(), date.getUTCMonth(), date.getUTCDate())) / 86400000)
            : Math.round((Date.UTC(now.getFullYear(), now.getMonth(), now.getDate()) - Date.UTC(date.getFullYear(), date.getMonth(), date.getDate())) / 86400000)
        return days === 0 ? "0" : days === 1 ? "1" : days >= 2 && days < 7 ? "older" : ""
    }
    function sentDayBounds(key) {
        const now = new Date(backend.clockDay), day = function(offset) { return new Date(Date.UTC(now.getUTCFullYear(), now.getUTCMonth(), now.getUTCDate() - offset)).toISOString().slice(0,10) }
        return key === "0" ? {after: day(0), before: day(-1)} : key === "1" ? {after: day(1), before: day(0)} : {after: day(6), before: day(1)}
    }
    readonly property string sentDay: {
        if (backend.folder !== "sent") return ""
        for (const option of dayOptions) { const b = sentDayBounds(option.key); if ((backend.filters.after || "") === b.after && (backend.filters.before || "") === b.before) return option.key }
        return ""
    }
    readonly property string activeDay: draftsMode ? draftDay : sentDay
    function selectDay(key) {
        if (draftsMode) { draftDay = draftDay === key ? "" : key; return }
        backend.applyFilters(dayFilters(key))
    }
    // The filters a click on day chip `key` applies: that day, or none when
    // it is already chosen.
    function dayFilters(key) {
        const filters = Object.assign({}, backend.filters); delete filters.after; delete filters.before
        if (key && sentDay !== key) { const b = sentDayBounds(key); filters.after = b.after; filters.before = b.before }
        return filters
    }
    function countDays(rows, utc) {
        const counts = {"0": 0, "1": 0, "older": 0}
        for (const row of rows) { const key = dayKey(row.latest_at, utc); if (key) ++counts[key] }
        return counts
    }
    property var unfilteredDayCounts: ({})
    readonly property string dayCountKey: backend.account + "/" + backend.folder
    readonly property bool sentDateFiltered: backend.folder === "sent" && (!!backend.filters.after || !!backend.filters.before)
    readonly property var dayCounts: {
        const live = countDays(mail.rows, backend.folder === "sent")
        return sentDateFiltered ? (unfilteredDayCounts[dayCountKey] || live) : live
    }
    Connections {
        target: backend
        function onConversationsChanged() {
            if (mail.draftsMode || mail.sentDateFiltered || backend.folder !== "sent") return
            const next = Object.assign({}, mail.unfilteredDayCounts); next[mail.dayCountKey] = mail.countDays(backend.conversations, true); mail.unfilteredDayCounts = next
        }
    }
    function draftHidden(row) { return draftsMode && draftDay !== "" && dayKey(row.latest_at, false) !== draftDay }
    readonly property int sentThisWeek: {
        const now = new Date(backend.clockNow).getTime()
        return backend.folder !== "sent" ? 0 : mail.rows.filter(function(row) { const at = new Date(row.latest_at).getTime(); return at <= now && now - at < 7 * 86400000 }).length
    }
    // The phrases the current search matched, for highlighting:
    // bare phrases in subjects and text, subject: and body: in their own.
    readonly property var searchPhrases: backend.searchTerms.phrases || []
    function phrasesFor(field) { return searchPhrases.filter(function(p) { return p.field === "any" || p.field === field }).map(function(p) { return p.text }) }
    readonly property var subjectPhrases: phrasesFor("subject")
    readonly property var bodyPhrases: phrasesFor("body")
    // Rich text with the matches tinted, or "" when nothing matches.
    function escapeHtml(s) { return String(s).replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;") }
    function markedHtml(text, phrases) {
        const ranges = phrases.length && text ? backend.matchRanges(text, phrases) : []
        if (!ranges.length) return ""
        const escape = escapeHtml
        const tint = String(Qt.tint(Theme.canvas, Theme.matchTint))
        let html = "", at = 0
        for (const range of ranges) {
            html += escape(text.slice(at, range[0])) + "<span style=\"background-color:" + tint + "\">" + escape(text.slice(range[0], range[1])) + "</span>"
            at = range[1]
        }
        return "<span style=\"white-space:pre-wrap\">" + html + escape(text.slice(at)) + "</span>"
    }
    // Labels read alphabetically, ignoring case. The rail shows the seven most
    // used; "All labels" lists every one.
    function byName(a, b) { const x = a.name.toLowerCase(), y = b.name.toLowerCase(); return x < y ? -1 : x > y ? 1 : a.name < b.name ? -1 : a.name > b.name ? 1 : 0 }
    readonly property var categoriesByName: backend.categories.slice().sort(byName)
    readonly property var popularCategories: backend.categories.length <= 7 ? categoriesByName
        : backend.categories.slice().sort(function(a, b) { return b.conversation_count - a.conversation_count || mail.byName(a, b) }).slice(0, 7).sort(byName)
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
    onVisibleChanged: { if (visible) { reviewArrivals(); return } morePopup.close(); senderCard.close(); railCategories.close(); binMenu.close(); if (composingNew) { backend.leaveNewDraft(); setReplyOpen(false) } }
    function clearSelection() { selectionRows = []; selectionCursor = "" }
    function cancelKeyboardOpen() { keyboardOpen.stop(); keyboardOpen.target = "" }
    function listHasFocus() {
        let item = mail.ApplicationWindow.window ? mail.ApplicationWindow.window.activeFocusItem : null
        while (item) { if (item === list) return true; item = item.parent }
        return false
    }
    readonly property var navigationRows: mail.rows.filter(function(row) { return !mail.draftHidden(row) && !row.leaving && ((backend.folder !== "inbox" && !mail.dateGroups) || !mail.groupCollapsed(mail.groupOf(row))) })
    // Each row's place within its group: every row but a group's first draws
    // a separator above it.
    readonly property var groupPositions: {
        const out = {}
        let group = null, place = 0
        for (const row of mail.rows) {
            if (mail.draftHidden(row)) continue
            const current = mail.groupOf(row)
            if (current !== group) { group = current; place = 0 }
            out[row.id] = place++
        }
        return out
    }
    function visibleRows() { return navigationRows }
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
        if (id !== (draftsMode ? backend.activeDraftId : backend.selectedId)) backend.dismissMailUndo()
        cancelKeyboardOpen()
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
    function pointerMoved(id, item, x, y) {
        const position = item.mapToGlobal(x,y)
        // Scrolling can move a delegate under a stationary mouse. Only actual
        // pointer movement takes the cursor back from keyboard navigation.
        if (position.x !== pointerPosition.x || position.y !== pointerPosition.y) {
            pointerPosition = position; keyboardNavigation = false; pointerRow = id
        }
    }
    function showRowMenu(id, anchor, x, y) {
        cancelKeyboardOpen()
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
        if (!readerMatchesCursor) return
        const rows = (morePopup.visible || labelMenu.visible) ? menuRows : selectionRows.length ? selectionRows : backend.snapshotMailRows([backend.selectedId])
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
    function composeFrom(mode, message) {
        morePopup.close()
        if (!backend.accountArchived && !backend.unsavedAccountBuffer) backend.beginDraftFrom(mode, message.id)
    }
    // Whether Reply all to `message` reaches someone Reply doesn't: To and Cc
    // besides you and the sender, or Cc alone for your own message, whose
    // reply already goes to its To.
    function replyAllFor(message) {
        const own = String(backend.currentAccount.email || "").toLowerCase(), sender = String(message.sender_email || "").toLowerCase()
        for (const row of participantMetadata[message.id] || []) {
            if (row.label !== "Cc" && (row.label !== "To" || message.sent)) continue
            for (const person of row.people) {
                const email = String(person.email || "").toLowerCase()
                if (email && email !== own && email !== sender) return true
            }
        }
        return false
    }
    property string pendingComposeId: ""
    property string pendingComposeMode: ""
    function completeMenuCompose() {
        if (!pendingComposeId || backend.selectedId !== pendingComposeId || thread.id !== pendingComposeId) return
        const mode = pendingComposeMode; pendingComposeId = ""; pendingComposeMode = ""
        backend.beginDraft(mode)
    }
    function dateGroup(row) {
        const date = new Date(row.latest_at), today = new Date(backend.clockDay)
        if (isNaN(date.getTime())) return "Earlier"
        const days = Math.round((Date.UTC(today.getFullYear(),today.getMonth(),today.getDate()) - Date.UTC(date.getFullYear(),date.getMonth(),date.getDate())) / 86400000)
        if (days === 0) return "Today"
        if (days === 1) return "Yesterday"
        if (days > 1 && days < 7) return Qt.formatDate(date,"dddd")
        return Qt.formatDate(date,"MMMM yyyy")
    }
    // Sent and Drafts group by day. Each row's group and each group's size are
    // worked out once per list, not again for every header and row.
    readonly property var dateGrouping: {
        const of = {}, counts = {}
        if (dateGroups) for (const row of mail.rows) { const group = dateGroup(row); of[row.id] = group; counts[group] = (counts[group] || 0) + 1 }
        return {of: of, counts: counts}
    }
    function groupOf(row) { return dateGroups && row ? dateGrouping.of[row.id] || dateGroup(row) : row ? row.section || row.display_group || row.group || "done" : "done" }
    function groupTitle(group) {
        const fixed = {review:"To review",held:"Delayed",done:"Done","done:week":"This Week","done:month":"This Month","done:undated":"Undated"}[group]
        if (fixed) return fixed
        // A Done month; the year appears only outside the current year.
        if (group.indexOf("done:") === 0) {
            const month = new Date(Number(group.slice(5, 9)), Number(group.slice(10, 12)) - 1, 1, 12)
            return Qt.formatDate(month, month.getFullYear() === new Date(backend.clockDay).getFullYear() ? "MMMM" : "MMMM yyyy")
        }
        return group
    }
    // Inbox sections in list order with their counts, from the backend: To
    // review and Delayed (while browsing, even when empty), This Week, This
    // Month, then one per older month. The open conversation kept in To review
    // counts there, not in the section Core counts it in.
    readonly property var inboxSections: backend.folder === "inbox" ? backend.inboxSections : []
    function sectionCount(key) { const section = inboxSections.find(function(s) { return s.key === key }); return section ? section.count : 0 }
    function groupCount(group) { return dateGroups ? dateGrouping.counts[group] || 0 : backend.folder === "inbox" ? sectionCount(group) : (backend.querySummary.filtered_group_counts || {})[group] || 0 }
    function groupKey(group) { return backend.account + "/" + backend.folder + "/" + group }
    // Inbox sections keep their collapsed state in the account's saved view.
    // Older Done months start collapsed; only the opened ones are listed.
    function olderMonth(group) { return group.indexOf("done:") === 0 && group !== "done:week" && group !== "done:month" }
    // Read from the backend once per change, not once per header.
    readonly property var sectionFolds: ({collapsed: backend.collapsedGroups, open: backend.openSections, all: backend.allSectionsOpen})
    function groupCollapsed(group) {
        if (backend.folder !== "inbox") return collapsedGroups[groupKey(group)] === true
        return sectionFolds.collapsed.indexOf(group) >= 0 || (!sectionFolds.all && olderMonth(group) && sectionFolds.open.indexOf(group) < 0)
    }
    function toggleGroup(group, header) {
        if (header) { anchoredSection = group; anchoredOffset = header.mapToItem(list, 0, 0).y; anchorExpiry.restart() }
        if (backend.folder === "inbox") {
            const collapse = !groupCollapsed(group)
            // Rows in memory hide or show at once, just after this click
            // returns: the row drawing the clicked header may leave the list.
            // The header is put back before that frame paints, and again once
            // the refresh lands.
            Qt.callLater(function() { backend.setSectionCollapsed(group, collapse); mail.anchorAfterChange(false) })
            return
        }
        const next = Object.assign({}, collapsedGroups); next[groupKey(group)] = !groupCollapsed(group); collapsedGroups = next
        anchorAfterChange(true)
    }
    // A clicked section header stays where it was on screen while its rows
    // collapse or expand, as on a page. Inbox rows reload after the click; the
    // header holds until that refresh lands, however long sync delays it, and
    // lets go as soon as the list is scrolled.
    property string anchoredSection: ""
    property real anchoredOffset: 0
    // Rows changed under the anchor: it is restored after bindings settle and
    // before the next frame paints. A release restores it one last time.
    property bool anchorPending: false
    property bool anchorRelease: false
    function anchorAfterChange(release) {
        anchorPending = true; if (release) anchorRelease = true
        if (Window.window) Window.window.update()
    }
    Connections {
        target: mail.Window.window
        function onAfterAnimating() {
            if (!mail.anchorPending) return
            const release = mail.anchorRelease
            mail.anchorPending = false; mail.anchorRelease = false
            if (mail.anchoredSection) mail.restoreSectionAnchor(!release)
        }
    }
    Timer { id: anchorExpiry; interval: 15000; onTriggered: mail.anchoredSection = "" }
    function findNamed(item, name) {
        if (!item) return null
        if (item.objectName === name && item.visible) return item
        for (let i = 0; i < item.children.length; ++i) { const found = findNamed(item.children[i], name); if (found) return found }
        return null
    }
    function sectionHeader(name) { return findNamed(list.contentItem, name) }
    // The list's scroll range, from its top margin to its end.
    function listTop() { return list.originY - list.topMargin }
    function listBottom() { return Math.max(listTop(), list.originY + list.contentHeight + list.bottomMargin - list.height) }
    function restoreSectionAnchor(keep) {
        if (!anchoredSection || (backend.loading && !keep)) return
        const name = "mailGroup_" + anchoredSection
        list.forceLayout()
        let header = sectionHeader(name)
        if (!header && backend.folder === "inbox") {
            const index = backend.listIndex("section:" + anchoredSection)
            if (index >= 0) { list.positionViewAtIndex(index, ListView.Beginning); list.forceLayout(); header = sectionHeader(name) }
        }
        if (!keep) anchoredSection = ""
        if (!header) return
        list.contentY = Math.max(listTop(), Math.min(listBottom(), list.contentY + header.mapToItem(list, 0, 0).y - anchoredOffset))
    }
    // The height the rows not loaded yet will take, from Core's counts, so the
    // list is its full length from the start and its scrollbar never jumps as
    // older pages arrive. Inbox items are all one row high, so its count is
    // exact; Sent's day headers are estimated from the rows already shown.
    readonly property real unloadedHeight: {
        if (!backend.hasMore) return 0
        if (backend.folder === "inbox") return backend.unloadedItems * mail.rowHeight
        let rowCount = 0
        for (const row of mail.rows) if (!row.leaving) ++rowCount
        const left = Math.max(0, Number(backend.querySummary.filtered_conversation_count || 0) - rowCount)
        const headers = mail.dateGroups && rowCount ? Object.keys(mail.dateGrouping.counts).length / rowCount : 0
        return left * (mail.rowHeight + headers * 44)
    }
    // How many rows reach the bottom of the view, for a scroll past the loaded ones.
    function rowsThrough(viewBottom, loadedBottom) {
        return mail.rows.length + Math.ceil(Math.max(0, viewBottom - loadedBottom) / mail.rowHeight) + 100
    }
    // Loading shows only on the first read of an account's folder; refreshes
    // keep what is on screen.
    property string settledScope: ""
    readonly property string listScope: backend.account + "\n" + backend.folder
    readonly property bool firstLoading: backend.loading && settledScope !== listScope
    // A chosen thread still on its way: the reader waits blank rather than
    // saying nothing is open.
    readonly property bool readerLoading: !draftsMode && backend.selectedId.length > 0 && members.length === 0 && !replyOpen
    component ReaderAction: FmButton {
        leftPadding: 14; rightPadding: 14
        font.pointSize: Theme.fontPoints(12); font.weight: Font.Medium
    }
    // Folder tab: a 15px lowercase name, dimmed until chosen.
    component ScopeTab: FmButton {
        property string folder: ""
        selected: backend.folder === folder
        font.pointSize: Theme.fontPoints(15); font.weight: Font.Medium
        implicitWidth: contentItem.implicitWidth; implicitHeight: 22; topPadding: 0; bottomPadding: 0; leftPadding: 0; rightPadding: 0
        hoverBrightness: 1
        foreground: selected || hovered ? Theme.text : Theme.inkAlpha(.32); textAlignment: Text.AlignLeft; background: Item {}
        onHoveredChanged: if (hovered && !selected) backend.prefetchFolder(folder)
        onClicked: mail.leaveComposerFor(function() { backend.folder = folder })
    }
    // A folder's count right after its name, at the same size; hidden at zero.
    component ScopeCount: Item {
        id: scopeCount
        property int count: 0
        property color tone: Theme.text
        property string folder: ""
        property string tip: ""
        visible: count > 0
        implicitWidth: countText.implicitWidth; implicitHeight: 22
        Text { id: countText; anchors.verticalCenter: parent.verticalCenter; text: scopeCount.count; color: scopeCount.tone; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(15); font.weight: Font.Medium; font.features: {"tnum": 1} }
        HoverHandler { id: countHover; cursorShape: Qt.PointingHandCursor }
        TapHandler { onTapped: mail.leaveComposerFor(function() { backend.folder = scopeCount.folder }) }
        FmToolTip { shown: countHover.hovered; text: scopeCount.tip }
    }
    // Filter toggle as plain text: #, the lowercase name and a small count;
    // an amber # and rule under it while on.
    component RailToggle: AbstractButton {
        id: railToggle
        property string label: ""
        property var count: ""
        property bool selected: false
        property string tip: ""
        hoverEnabled: true
        implicitWidth: toggleRow.implicitWidth; implicitHeight: 22
        topPadding: 0; bottomPadding: 0; leftPadding: 0; rightPadding: 0
        Accessible.name: label + ", " + count + (selected ? ", on" : ", off")
        FmToolTip { shown: railToggle.hovered && railToggle.tip.length > 0; text: railToggle.tip }
        contentItem: Item {
            Row {
                id: toggleRow; spacing: 4; y: 2
                Text { id: toggleName; textFormat: Text.StyledText; text: "<font color=\"" + (railToggle.selected ? Theme.amber : Theme.inkAlpha(.28)) + "\">#</font>" + mail.escapeHtml(String(railToggle.label).toLowerCase()); color: railToggle.selected || railToggle.hovered ? Theme.text : Theme.inkAlpha(.55); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12) }
                Text { anchors.baseline: toggleName.baseline; text: railToggle.count; color: railToggle.selected ? Theme.accentAlpha(.85) : Theme.inkAlpha(.3); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(11); font.features: {"tnum": 1} }
            }
            Rectangle { y: toggleRow.y + toggleRow.height + 2; width: toggleRow.width; height: Theme.ruleWidth; color: railToggle.selected ? Theme.amber : "transparent" }
        }
        background: Rectangle { color: "transparent"; radius: 5; border.width: railToggle.visualFocus ? 1 : 0; border.color: Theme.focusBorder }
    }
    // To review and Delayed title their sections in full ink; Done months and
    // Sent and Drafts days stay dimmed.
    function primaryGroup(group) { return backend.folder === "inbox" && !dateGroups && (group === "review" || group === "held") }
    function headerHeight(group) { return 32 }
    // Every section header: a 32px row with the 11px uppercase title, its count
    // right after it (amber for To review), a hairline running to the chevron.
    // They scroll with the list.
    component GroupHeader: ItemDelegate {
        id: heading
        required property string group
        readonly property bool collapsed: mail.groupCollapsed(group)
        readonly property bool primary: mail.primaryGroup(group)
        objectName: "mailGroup_" + group
        height: mail.headerHeight(group); leftPadding: 16; rightPadding: 16; topPadding: 0; bottomPadding: 0; hoverEnabled: true
        enabled: backend.folder === "inbox" || mail.dateGroups
        onClicked: mail.toggleGroup(group, heading)
        // A collapsed section's rows load on hover, so its click shows them at once.
        onHoveredChanged: if (hovered && collapsed && backend.folder === "inbox") backend.prefetchSection(group)
        background: Item {}
        Accessible.name: mail.groupTitle(group) + ", " + mail.groupCount(group) + " conversations, " + (collapsed ? "collapsed" : "expanded")
        readonly property color tone: hovered ? Theme.text : heading.primary ? Theme.inkAlpha(.5) : Theme.inkAlpha(.45)
        contentItem: RowLayout {
            spacing: heading.primary ? 10 : 12
            Text {
                objectName: "mailGroupTitle_" + heading.group
                text: mail.dateGroups ? mail.groupTitle(heading.group) : backend.folder === "sent" ? "Sent" : backend.folder === "trash" ? "Trash" : backend.folder === "all" ? "All mail" : mail.groupTitle(heading.group)
                color: heading.primary ? Theme.text : heading.tone; font.family: Theme.fontFamily
                font.pointSize: Theme.fontPoints(11); font.capitalization: Font.AllUppercase; font.letterSpacing: Theme.fontSize(11) * .08
            }
            Text {
                objectName: "mailGroupCount_" + heading.group
                visible: backend.folder === "inbox" || mail.dateGroups
                text: mail.groupCount(heading.group)
                color: heading.group === "review" && heading.primary ? Theme.amber : heading.tone
                font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(11); font.features: {"tnum":1}
            }
            Rectangle { Layout.fillWidth: true; implicitHeight: 1; color: Theme.line }
            FmIcon { visible: backend.folder === "inbox" || mail.dateGroups; name: "down"; size: 10; color: heading.tone; rotation: heading.collapsed ? -90 : 0; Behavior on rotation { NumberAnimation { duration: Theme.ms(180); easing.type: Theme.enterEasing } } }
        }
    }
    // The Inbox list's items besides rows. Each is one row high, as a row
    // is, so the list's length and scrollbar are exact however its sections
    // fold: a section's header, drawn at the foot of its item, and the line
    // saying an open section is empty.
    component SectionItem: Item {
        id: sectionItem
        required property var rowData
        required property int index
        readonly property bool header: true
        readonly property string group: rowData ? rowData.section || "" : ""
        // Room above the header, after the section before.
        readonly property real gap: height - mail.headerHeight(group)
        visible: index >= 0; enabled: index >= 0
        width: list.width; height: mail.rowHeight
        GroupHeader { group: sectionItem.group; width: parent.width; y: sectionItem.gap }
    }
    component SectionEmpty: Item {
        id: emptyItem
        required property var rowData
        required property int index
        readonly property string group: rowData ? rowData.section || "" : ""
        objectName: "mailSectionEmpty_" + group
        visible: index >= 0
        width: list.width; height: mail.rowHeight
        EmptyLine { group: emptyItem.group }
    }
    // The line an open, empty section shows. A last row folding out of its
    // section fades into the same line, so the swap moves nothing.
    component EmptyLine: Row {
        property string group
        x: 16; anchors.verticalCenter: parent.verticalCenter; spacing: 8
        FmIcon { anchors.verticalCenter: parent.verticalCenter; name: "check"; size: 12; color: Theme.accent }
        Text {
            anchors.verticalCenter: parent.verticalCenter
            text: parent.group === "review" ? (mail.sectionCount("held") > 0 ? "All reviewed. Nice work." : "Inbox zero. Enjoy it.") : "Nothing waiting on later."
            color: Theme.inkAlpha(.55); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12)
        }
    }
    // Inbox label toggles travel to Core as comma-separated ids; mail with any
    // of them matches. A label: in the search decides instead.
    readonly property var categoryIds: backend.category.length ? backend.category.split(",") : []
    function categoryOn(id) { return categoryIds.indexOf(id) >= 0 }
    function toggleCategory(id) { backend.category = toggledCategory(id) }
    // The label choice a click on label `id` makes.
    function toggledCategory(id) {
        const next = categoryOn(id) ? categoryIds.filter(function(key) { return key !== id }) : categoryIds.concat([id])
        return next.join(",")
    }
    function setKind(kind) { backend.applyFilters(kindFilters(kind)) }
    // The filters a click on sender kind `kind` applies: that kind, or every
    // kind when it is already chosen.
    function kindFilters(kind) {
        const next = Object.assign({}, backend.filters)
        if (next.kind === kind) delete next.kind; else next.kind = kind
        return next
    }
    // Unknown shows nothing: only mail the model has not read yet lacks a kind.
    function kindKnown(kind) { return kind === "people" || kind === "automated" }
    function kindIcon(kind) { return kind === "people" ? "person" : kind === "automated" ? "bot" : "unknown" }
    // This account's starred contacts, by lowercase address: their mail shows
    // an amber star by the sender's name.
    readonly property var starredSenders: {
        const out = {}
        for (const entry of backend.preferences.starred_senders || []) if (entry.account === backend.account) out[entry.email] = true
        return out
    }
    function senderStarred(email) { return !!starredSenders[String(email || "").trim().toLowerCase()] }
    function kindTitle(kind) { return kind === "people" ? "Person" : kind === "automated" ? "Automated sender" : "Sender kind is unknown" }
    component ContactName: FmButton {
        id: contact
        property string email: ""
        property bool underline: true
        leftPadding: 0; rightPadding: 0; topPadding: 0; bottomPadding: 0
        implicitHeight: contentItem.implicitHeight
        textAlignment: Text.AlignLeft
        foreground: Theme.inkAlpha(0.85)
        onHoveredChanged: hovered ? senderCard.hoverSender(email, text, contact) : senderCard.leaveSender(contact)
        onClicked: senderCard.showSender(email, text, contact)
        background: Item {
            Row {
                anchors.bottom: parent.bottom; spacing: 2
                visible: contact.underline && (contact.hovered || contact.activeFocus)
                Repeater { model: Math.floor(contact.width / 3); Rectangle { width: 1; height: 1; color: contact.foreground } }
            }
        }
    }
    // An 18px avatar and the sender's name as one hover element: a rounded pill
    // whose hover fill covers both, opening the contact card. Folded rows keep
    // the card but not the fill.
    // Read-only text that selects with the mouse and copies with the copy key
    // or the app's text menu. A click gives it focus, as the message body's
    // does; single-key shortcuts still work, since it is read-only.
    component CopyText: TextEdit {
        id: copyText
        readOnly: true; selectByMouse: true; textFormat: TextEdit.PlainText
        selectionColor: Theme.selection; selectedTextColor: Theme.text
        font.family: Theme.fontFamily
        ContextMenu.menu: null
        TapHandler { acceptedButtons: Qt.RightButton; onTapped: function(point) { textMenu.show(copyText, point.position.x, point.position.y) } }
        HoverHandler { cursorShape: Qt.IBeamCursor }
    }
    component SenderPill: AbstractButton {
        id: pill
        property string email: ""
        property string personName: ""
        property string kind: "unknown"
        property bool own: false
        property bool open: true
        property string avatarName: ""
        property string nameObjectName: ""
        hoverEnabled: true
        implicitHeight: 24; leftPadding: 3; rightPadding: 8; topPadding: 0; bottomPadding: 0
        implicitWidth: leftPadding + 25 + Math.ceil(nameMetrics.advanceWidth) + rightPadding
        Accessible.name: "Contact: " + personName
        onHoveredChanged: hovered ? senderCard.hoverSender(email, personName, pill) : senderCard.leaveSender(pill)
        onClicked: senderCard.showSender(email, personName, pill)
        background: Rectangle {
            radius: 5; color: pill.open && pill.hovered ? Theme.bg4 : "transparent"
            border.width: pill.visualFocus ? 1 : 0; border.color: Theme.focusBorder
            Behavior on color { ColorAnimation { duration: Theme.ms(120) } }
        }
        contentItem: Item {
            implicitHeight: 18
            SenderAvatar {
                objectName: pill.avatarName
                anchors.verticalCenter: parent.verticalCenter
                diameter: 18; brandSize: 12
                own: pill.own; name: pill.personName; email: pill.email; kind: pill.kind
                discColor: pill.own ? Theme.accentAlpha(0.18) : Theme.oklch(.36, .06, hue)
                initialsColor: pill.own ? Theme.accent : Theme.oklch(.9, .05, hue)
                initials: mail.initials(pill.own && backend.currentAccount.name ? backend.currentAccount.name : pill.personName)
                fontPoints: Theme.fontPoints(10.5); letterSpacing: 0
            }
            // Selectable while the message is open; a plain click still opens
            // the contact card. A folded message's row takes the click instead.
            CopyText {
                id: pillName; objectName: pill.nameObjectName
                x: 25; width: Math.max(0, parent.width - 25); anchors.verticalCenter: parent.verticalCenter
                enabled: pill.open
                // Elision measures a little wider than the advance the pill is
                // sized to, so a name that fits is shown whole.
                text: nameMetrics.advanceWidth <= width ? pill.personName : nameMetrics.elidedText
                color: pill.own ? Theme.accent : Theme.text
                font.pointSize: Theme.fontPoints(12); font.weight: Font.Medium
                TapHandler { onTapped: if (!pillName.selectedText) pill.clicked() }
            }
            TextMetrics { id: nameMetrics; font: pillName.font; text: pill.personName; elide: Text.ElideRight; elideWidth: pillName.width }
        }
    }
    component ContactAvatar: FmButton {
        id: avatar
        property string email: ""
        property string personName: ""
        property string kind: "unknown"
        property bool own: false
        property int diameter: 34
        property bool ring: false
        implicitWidth: diameter; implicitHeight: diameter
        leftPadding: 0; rightPadding: 0; topPadding: 0; bottomPadding: 0
        Accessible.name: "Contact: " + personName
        onHoveredChanged: hovered ? senderCard.hoverSender(email, personName, avatar) : senderCard.leaveSender(avatar)
        onClicked: senderCard.showSender(email, personName, avatar)
        contentItem: Item {
            SenderAvatar {
                diameter: avatar.diameter; brandSize: avatar.diameter - 6
                own: avatar.own; name: avatar.personName; email: avatar.email; kind: avatar.kind
                discColor: avatar.own ? Theme.accentAlpha(0.18) : Theme.oklch(.36, .06, hue)
                initialsColor: avatar.own ? Theme.accent : Theme.oklch(.9, .05, hue)
                initials: mail.initials(avatar.own && backend.currentAccount.name ? backend.currentAccount.name : avatar.personName)
                fontPoints: Theme.fontPoints(avatar.diameter > 26 ? 11.5 : avatar.diameter > 22 ? 10.5 : 10.5); fontWeight: Font.Medium; letterSpacing: 0
            }
            Rectangle { visible: avatar.activeFocus; anchors.fill: parent; radius: avatar.diameter / 2; color: "transparent"; border.width: 1; border.color: Theme.accent }
        }
        background: Item {
            Rectangle { visible: avatar.ring; anchors.centerIn: parent; width: avatar.diameter + 4; height: width; radius: width / 2; color: Theme.bg3 }
        }
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
    function newMessage() {
        cancelKeyboardOpen(); clearSelection(); backend.beginDraft("new")
    }
    // A new message being written here (not one reopened from Drafts).
    readonly property bool composingNew: replyOpen && backend.draft.mode === "new" && !draftsMode && !backend.accountArchived
    // Leaving a new message keeps it in Drafts without asking; an empty one is
    // dropped. Drafts itself keeps the composer open.
    function leaveComposerFor(action, toDrafts) {
        if (composingNew && !toDrafts) { backend.leaveNewDraft(); setReplyOpen(false) }
        action()
    }
    // Save and close the composer without asking.
    function minimizeComposer() { if (backend.accountNavigationAllowed()) { backend.saveDraft(); setReplyOpen(false) } }
    // Esc on a new message with something written asks; replies and drafts
    // reopened from Drafts save back and close.
    function escapeComposer() {
        if (!backend.accountNavigationAllowed()) return
        if (composingNew && backend.draftHasContent()) { keepDialog.open(); return }
        if (composingNew) { backend.leaveNewDraft(); setReplyOpen(false); return }
        backend.saveDraft(); setReplyOpen(false)
    }
    // Chips open a reply draft in its thread, a new message in Drafts.
    function openDraftChip(row) {
        if (row.mode !== "new" && row.conversation_id) {
            if (backend.folder === "drafts" || mail.binMode) backend.folder = "inbox"
            backend.openDraft(row.draft_id)
        } else {
            backend.folder = "drafts"
            backend.openDraft(row.draft_id)
        }
    }
    function chipSubject(row) {
        const text = String(row.subject || "").trim() || String(row.snippet || "").trim() || "Untitled"
        return text.replace(/\s+/g, " ")
    }
    function chipTip(row) {
        const to = (row.to_recipients || []).map(function(r) { return r.name || r.email })
        const who = to.length ? (row.mode !== "new" ? "Reply to " : "To ") + to.join(", ") : "No recipient yet"
        const body = String(row.snippet || "").trim()
        return (backend.currentAccount.name || backend.account) + " · " + who + (body ? " — " + body.slice(0, 80) : "")
    }
    function showEventInCalendar(event) { const window = mail.ApplicationWindow.window; if (window) window.showEventInCalendar(event) }
    readonly property var chipRows: backend.draftChips.account === backend.account ? backend.draftChips.rows || [] : []
    readonly property bool chipsVisible: backend.preferences.show_draft_chips !== false && chipRows.length > 0 && !replyOpen && !binMode
    function openActions() { morePopup.open() }
    function restoreListPosition() {
        if (list.moving) return
        list.forceLayout()
        // The saved offset is from the list's origin, which moves as rows above
        // change. A shorter result set must remain visible. Keep the saved
        // offset so clearing the query can restore the longer mailbox's place.
        list.contentY = Math.max(listTop(),Math.min(listBottom(),list.originY + (backend.presentation.listY||0)))
    }
    // One time format for rows, the reader and sender cards: "22 min ago" and
    // "3 hrs ago" within a day, then "Yday 14:02", the weekday earlier this
    // week (weeks start on Monday) and "21 Sep" before that.
    function relativeTime(value) {
        const date = new Date(value), now = new Date(backend.clockNow)
        if (isNaN(date.getTime())) return ""
        const minutes = Math.max(0, (now.getTime() - date.getTime()) / 60000)
        if (minutes < 1) return "just now"
        if (minutes < 60) return Math.round(minutes) + " min ago"
        if (minutes < 1440) { const hours = Math.round(minutes / 60); return hours + (hours === 1 ? " hr ago" : " hrs ago") }
        const days = (Date.UTC(now.getFullYear(), now.getMonth(), now.getDate()) - Date.UTC(date.getFullYear(), date.getMonth(), date.getDate())) / 86400000
        if (days === 1) return "Yday " + Qt.formatTime(date, "HH:mm")
        if (days >= 2 && days <= (now.getDay() + 6) % 7) return Qt.formatDateTime(date, "ddd HH:mm")
        return Qt.formatDate(date, date.getFullYear() === now.getFullYear() ? "d MMM" : "d MMM yyyy")
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
    // Threads whose summary box is shown, for this session.
    property var threadSummaryShown: ({})
    readonly property var threadSummaryResult: assistance.resultFor("thread", backend.selectedId)
    readonly property bool threadSummaryBusy: assistance.workingOn("thread", backend.selectedId)
    readonly property bool threadSummaryOpen: threadSummaryShown[expansionKey] === true
    function showThreadSummary(shown) { const next = Object.assign({}, threadSummaryShown); next[expansionKey] = shown; threadSummaryShown = next }
    // The first click writes the summary; later clicks show or hide it. A
    // summary the thread has moved past is rewritten when it is shown again.
    function toggleThreadSummary() {
        if (threadSummaryBusy) return
        const result = threadSummaryResult
        if (!result || (result.stale && !threadSummaryOpen)) { showThreadSummary(true); assistance.start("thread", "") }
        else showThreadSummary(!threadSummaryOpen)
    }
    // Attachment cards a click folded away, by message, for this session.
    property var collapsedAttachments: ({})
    function attachmentsCollapsed(id) { return collapsedAttachments[expansionKey + id] === true }
    function toggleAttachments(id) {
        const next = Object.assign({}, collapsedAttachments), key = expansionKey + id
        next[key] = !attachmentsCollapsed(id); collapsedAttachments = next
    }
    // Every message in a thread is listed in date order, open or as a header
    // row. One visit opens the messages since your last reply, from the first
    // still to review, or the latest alone when none are; the rest start
    // closed. The layout follows the thread until its current copy has
    // arrived (a cached copy may lack the newest mail), or until the user
    // opens or closes anything. The message that was latest when a reply or
    // new mail landed after it stays as it was.
    property string lastMemberKey: ""
    property string visitThread: ""
    property var visitUnread: ({})
    property bool visitSettled: false
    property bool visitTouched: false
    property var visitSeen: ({})
    // A thread opened with unread mail scrolls to its first unread message,
    // once per visit and again if a fuller copy of the thread moves it, until
    // the user scrolls. A position restored after a restart wins.
    // New mail that arrives in the open thread is read once it is on screen
    // with the app in front, as opening it would have; a message marked
    // unread by hand is not new and stays unread.
    property bool arrivalsUnread: false
    function reviewArrivals() {
        if (!arrivalsUnread || !mail.visible || Qt.application.state !== Qt.ApplicationActive || draftsMode) return
        arrivalsUnread = false
        backend.reviewOpenConversation()
    }
    Connections { target: Qt.application; function onStateChanged() { mail.reviewArrivals() } }
    property string visitRevealed: ""
    property bool visitScrollTouched: false
    function revealFirstUnread(messages) {
        if (visitScrollTouched || replyOpen || replyReveal.running) return
        const first = messages.find(function(m) { return visitUnread[m.id] === true })
        if (!first || first.id === visitRevealed) return
        if (!visitRevealed && (backend.presentation.readerY || 0) > 0) { visitScrollTouched = true; return }
        visitRevealed = first.id
        revealMember(first.id)
    }
    function openThread(messages) {
        const open = Object.assign({}, expandedMembers)
        for (const key of Object.keys(open)) if (key.startsWith(expansionKey)) delete open[key]
        const firstUnread = messages.findIndex(function(m) { return visitUnread[m.id] === true })
        let tail = messages.length - 1
        if (firstUnread >= 0) {
            let lastSent = -1
            messages.forEach(function(m, i) { if (m.sent) lastSent = i })
            tail = Math.min(tail, Math.max(lastSent + 1, firstUnread))
        }
        messages.forEach(function(m, i) { if (i >= tail) open[expansionKey + m.id] = true })
        expandedMembers = open
    }
    function followThread(thread) {
        const messages = draftsMode ? [] : (backend.conversation.messages || [])
        if (thread !== visitThread) { visitThread = thread; visitUnread = ({}); visitSettled = false; visitTouched = false; lastMemberKey = ""; visitArrivedSent = ({}); visitSeen = ({}); visitRevealed = ""; visitScrollTouched = false; arrivalsUnread = false }
        // Unread stays remembered for the visit, so reading on open changes nothing.
        const unread = Object.assign({}, visitUnread), arrived = []
        for (const m of messages) if (m.unread && unread[m.id] !== true) { unread[m.id] = true; arrived.push(m.id) }
        visitUnread = unread
        // Your own messages that land after the thread was first shown.
        const seen = Object.assign({}, visitSeen), shown = Object.keys(seen).length > 0; let sentArrived = null, newMail = false
        for (const m of messages) {
            if (seen[m.id] === true) continue
            seen[m.id] = true
            if (shown && m.sent && !m.is_outgoing) { sentArrived = sentArrived || Object.assign({}, visitArrivedSent); sentArrived[m.id] = true }
            if (shown && m.unread && !m.sent) newMail = true
        }
        visitSeen = seen
        if (sentArrived) visitArrivedSent = sentArrived
        if (newMail) { arrivalsUnread = true; reviewArrivals() }
        if (!messages.length) return
        if (!visitSettled) {
            if (!visitTouched) { openThread(messages); revealFirstUnread(messages) }
            const row = rows.find(function(r) { return r.id === backend.selectedId })
            visitSettled = !row || (backend.conversation.revision || 0) >= (row.revision || 0)
            lastMemberKey = expansionKey + messages[messages.length - 1].id
            return
        }
        keepLatestOpen(messages)
        // Several new messages at once all open, not only the last.
        if (arrived.length > 1) {
            const open = Object.assign({}, expandedMembers)
            for (const id of arrived) open[expansionKey + id] = true
            expandedMembers = open
        }
    }
    function keepLatestOpen(messages) {
        if (lastMemberKey.startsWith(expansionKey)) {
            const id = lastMemberKey.slice(expansionKey.length)
            const at = messages.findIndex(function(m) { return m.id === id })
            if (at >= 0 && at < messages.length - 1 && expandedMembers[lastMemberKey] === undefined) {
                const open = Object.assign({}, expandedMembers); open[lastMemberKey] = true; expandedMembers = open
            }
        }
        lastMemberKey = messages.length ? expansionKey + messages[messages.length - 1].id : ""
    }
    // While a reply still shows as sent from here, Gmail's own copy arriving
    // in the thread waits behind it, so the reply never shows twice.
    property var visitArrivedSent: ({})
    readonly property bool replyShownFromHere: members.some(function(m) { return m.is_outgoing })
    function awaitingOwnCopy(row) { return !!row && !!row.sent && !row.is_outgoing && replyShownFromHere && visitArrivedSent[row.id] === true }
    function memberIsExpanded(id, index) {
        const state = expandedMembers[expansionKey + id]
        return state === undefined ? index === members.length - 1 : state === true
    }
    function toggleMember(id, index) {
        visitTouched = true
        const next = Object.assign({}, expandedMembers), key = expansionKey + id
        next[key] = !memberIsExpanded(id,index); expandedMembers = next
        backend.remember("selectedMember", next[key] ? id : "")
    }
    function scrollToMember(id) {
        visitTouched = true; replyPinned = false
        const next = Object.assign({},expandedMembers); next[expansionKey+id] = true; expandedMembers = next
        backend.remember("selectedMember",id)
        revealMember(id)
    }
    // Brings a message's top into view once the thread's layout settles,
    // without opening or closing anything.
    function revealMember(id) {
        reader.cancelFlick()
        matchReveal.identity = backend.account + "/" + backend.selectedId
        matchReveal.query = backend.query; matchReveal.memberId = id; matchReveal.toMatch = false
        matchReveal.pass = 0; matchReveal.stable = 0; matchReveal.lastTop = -1; matchReveal.lastHeight = -1
        matchReveal.restart()
    }
    Timer {
        id: matchReveal
        interval: 16; repeat: true
        property string identity: ""
        property string query: ""
        property string memberId: ""
        // Also brings the first match in the body into view, a third down.
        property bool toMatch: false
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
                const match = toMatch ? mail.firstMatchTop(item, memberId) : -1
                reader.contentY = Math.max(0,Math.min(Math.max(0,height-reader.height),match >= 0 ? match - Math.round(reader.height / 3) : top-12))
                backend.remember("readerY",reader.contentY)
                if (stable>=2 || pass>=20) stop()
            } else if (pass>=20) stop()
        }
    }
    function revealMatch(toMatch) { if (!matchingIds.length) return; scrollToMember(matchingIds[0]); matchReveal.toMatch = !!toMatch }
    // Whether a matching message's text holds a match, so Show match has
    // somewhere to go; a match in the subject alone does not.
    readonly property bool bodyMatches: members.some(function(m) { return matchingIds.indexOf(m.id) >= 0 && backend.matchRanges(m.text || "", bodyPhrases).length > 0 })
    // The top of the first search match in a message's body, in the reader's
    // content coordinates, or -1: the HTML view's first tinted range, or the
    // first match in its text rows.
    function firstMatchTop(memberItem, id) {
        let top = -1
        const visit = function(item) {
            if (!item || !item.visible) return
            const name = item.objectName || ""
            if (name === "messageBody_" + id || name.indexOf("messageBodyPart_" + id + "_") === 0) {
                let box = null
                if (typeof item.highlightRect === "function") { const r = item.highlightRect(0); if (r.width > 0) box = r }
                else if (typeof item.positionToRectangle === "function") {
                    const ranges = backend.matchRanges(item.getText(0, item.length), mail.bodyPhrases)
                    if (ranges.length) box = item.positionToRectangle(ranges[0][0])
                }
                if (box) { const y = item.mapToItem(reader.contentItem, 0, box.y).y; if (top < 0 || y < top) top = y }
                return
            }
            for (let i = 0; i < item.children.length; ++i) visit(item.children[i])
        }
        visit(memberItem)
        return top
    }
    function openRow(id) {
        if (draftsMode) { backend.openDraft(id); return }
        pendingReveal = id; backend.openConversation(id)
        if (backend.selectedId === id && members.length) { if (matchingIds.length) revealMatch(); pendingReveal = "" }
    }
    function moveSelection(delta, extend) {
        keyboardNavigation = true
        pointerRow = ""
        const selected = selectionCursor || (draftsMode ? backend.activeDraftId : backend.selectedId)
        const rows = navigationRows
        const current = rows.findIndex(function(row) { return row.id === selected })
        let next = Math.max(0, Math.min(rows.length - 1, current < 0 ? 0 : current + delta))
        // After Done, Delay or filing moved the open conversation, Up and Down
        // go on from the place it left, not from where it went.
        if (!extend && Math.abs(delta) === 1) {
            const anchor = backend.navigationAnchor()
            if (anchor.id && anchor.id === selected) {
                const target = rows.findIndex(function(row) { return row.id === (delta > 0 ? anchor.after : anchor.before) })
                if (target >= 0) next = target
            }
        }
        if (!rows.length) return
        if (next === current) { if (delta > 0 && backend.hasMore && !backend.loading) backend.loadMore(); return }
        if (extend) selectRange(rows[next].id)
        else {
            morePopup.close()
            selectionRows = []; selectionAnchor = rows[next].id; selectionCursor = rows[next].id
            // Accumulate every key against the input cursor, but open only the
            // latest target once per frame. A queued burst must not construct
            // and discard a reader (or review receipt) for every intermediate row.
        }
        keyboardOpen.target = rows[next].id
        keyboardOpen.openReader = !extend
        if (!keyboardOpen.running) keyboardOpen.start()
        list.forceActiveFocus(Qt.OtherFocusReason)
    }
    function pageSelection(direction, extend) { moveSelection(direction * Math.max(1,Math.floor((list.height - 34) / rowHeight)),extend) }
    function edgeSelection(last, extend) { moveSelection(last ? mail.rows.length : -mail.rows.length,extend) }
    Timer {
        id: keyboardOpen
        interval: 16
        property string target: ""
        property bool openReader: false
        onTriggered: {
            const id = target; target = ""
            const index = backend.listIndex(id)
            if (!mail.visible || !id || index < 0) return
            // The first row of an Inbox section brings its header into view too.
            const row = mail.rows.find(function(row) { return row.id === id })
            const header = backend.folder === "inbox" && row ? backend.listIndex("section:" + mail.groupOf(row)) : -1
            if (header >= 0 && header === index - 1) list.positionViewAtIndex(header, ListView.Contain)
            list.positionViewAtIndex(index, ListView.Contain)
            backend.remember("listY", list.contentY - list.originY)
            if (openReader) mail.openRow(id)
        }
    }
    function keyboardRowMenu() {
        const id = selectionCursor || (draftsMode ? backend.activeDraftId : backend.selectedId)
        const index = backend.listIndex(id)
        if (index < 0) return
        list.positionViewAtIndex(index,ListView.Contain)
        const item = list.itemAtIndex(index)
        if (item) showRowMenu(id,item,24,item.height)
    }
    function setReplyOpen(value) {
        const identity = composerIdentity
        const next = Object.assign({},replyStates); next[identity] = value; replyStates = next
        replyOpen = value; backend.remember("replyOpen",value)
    }
    // This conversation's reply that is on its way to Gmail or already sent.
    function replyOnItsWay() {
        const draft = backend.draft.draft_id || ""
        return !!draft && members.some(function(m) { return m.is_outgoing && m.draft_id === draft })
    }
    // A discard closes the reply once. Reply may then reopen the discarded draft,
    // which typing or a file revives; later refreshes of it leave it open, and
    // only a new discard (a new revision) closes it again.
    property string closedDiscard: ""
    function discardClosesReply() {
        const key = backend.draftStatus === "Draft discarded" ? backend.account + "/" + (backend.draft.draft_id || backend.selectedId) + "/" + (backend.draft.revision || 0) : ""
        const fresh = key.length > 0 && key !== closedDiscard
        closedDiscard = key
        return fresh
    }
    // The kind of message being written under this thread, if any.
    readonly property string composingMode: replyOpen && members.length > 0 ? (backend.draft.mode || "reply") : ""
    // Reply all shows while the thread's reply offers it; writing a forward,
    // whose draft cannot say, does not hide it.
    property var replyAllThreads: ({})
    readonly property bool replyAllAvailable: {
        const mode = backend.draft.mode || "reply"
        if (backend.draft.reply_all_available !== undefined && (mode === "reply" || mode === "reply_all")) return backend.draft.reply_all_available === true
        return replyAllThreads[backend.account + "/" + backend.selectedId] === true
    }
    function noteReplyAll() {
        const mode = backend.draft.mode || "reply", key = backend.account + "/" + backend.selectedId
        if (backend.draft.reply_all_available === undefined || (mode !== "reply" && mode !== "reply_all") || replyAllThreads[key] === (backend.draft.reply_all_available === true)) return
        const next = Object.assign({}, replyAllThreads); next[key] = backend.draft.reply_all_available === true; replyAllThreads = next
    }
    // Reply, Reply all and Forward stay under the thread while one is being
    // written. The one being written brings it into view; another turns it
    // into that kind, keeping the words written so far. With the box closed,
    // each starts its own draft. A save under way finishes first.
    function compose(mode) {
        if (backend.accountArchived || backend.unsavedAccountBuffer) return
        if (composingMode === mode && !sentReplyDone && !replyOnItsWay()) { revealReply(); return }
        if (mode === "reply") openReply(); else if (replyOpen) backend.switchDraft(mode); else backend.beginDraft(mode)
    }
    function openReply() {
        if (backend.accountArchived || backend.unsavedAccountBuffer) return
        // Once a reply has gone out, or while it is still going, Reply writes
        // a new one. The sent draft reopens only if a follow-up was being
        // written in it.
        if ((backend.draft.mode && backend.draft.mode !== "reply") || mail.sentReplyDone || replyOnItsWay()) { if (mail.replyOpen) backend.switchDraft("reply"); else backend.beginDraft("reply"); return }
        resumeDraft()
    }
    function resumeDraft() {
        setReplyOpen(true)
        revealReply()
    }
    function revealReply() {
        // A reply turned into a forward, or back, fades in as it changes.
        if (replyRevealIdentity !== composerIdentity && reply.visible) replyOpening.restart()
        replyRevealIdentity = composerIdentity
        replyReveal.pass = 0; replyReveal.stable = 0; replyReveal.lastHeight = -1; replyReveal.focused = false; replyReveal.scrolled = false
        replyPinned = true
        replyReveal.restart()
    }
    // The first move to the reply glides; later ones, as the thread lays out, follow at once.
    NumberAnimation { id: revealScroll; target: reader; property: "contentY"; duration: Theme.ms(200); easing.type: Easing.OutCubic }
    // An opened reply shows whole, at the end of the thread. The reader keeps
    // it there while the thread above finishes loading and laying out, until
    // the height has held for 250 ms (3 s at most) or the reader is scrolled.
    Timer {
        id: replyReveal
        objectName: "replyReveal"
        interval: 16; repeat: true
        property int pass: 0
        property int stable: 0
        property real lastHeight: -1
        property bool focused: false
        property bool scrolled: false
        onTriggered: {
            if (!mail.visible || !mail.replyOpen || mail.replyRevealIdentity !== mail.composerIdentity) { stop(); return }
            ++pass
            const height = reader.contentHeight
            stable = height === lastHeight ? stable+1 : 0
            lastHeight = height
            if (reply.visible && reply.height > 0) {
                const bottom = Math.max(0, height - reader.height)
                // Where the form settles, without its opening slide.
                const top = reply.mapToItem(reader.contentItem,0,0).y - replyShift.y
                // The thread scrolls down until the form's footer is in view
                // with 24px to spare. A form already that far up stays put,
                // and a new message does not scroll.
                const target = reply.mode === "new" ? reader.contentY
                    : Math.max(reader.contentY, Math.min(bottom, top + reply.height + 24 - reader.height))
                if (revealScroll.running) {
                    if (Math.abs(revealScroll.to - target) > .5) { revealScroll.stop(); revealScroll.from = reader.contentY; revealScroll.to = target; revealScroll.start() }
                } else if (Math.abs(reader.contentY - target) > .5) {
                    reader.cancelFlick()
                    if (scrolled) reader.contentY = target
                    else { revealScroll.from = reader.contentY; revealScroll.to = target; revealScroll.start() }
                }
                scrolled = true
                backend.remember("readerY", target)
                if (!focused) { reply.focusEditor(); focused = true }
            }
            if (((stable >= 15 && mail.members.length > 0) || pass >= 190) && !revealScroll.running) stop()
        }
    }
    // While the keyboard is in the reply, the form stays whole in view as it
    // grows with its text, as the reader changes size and as the thread above
    // it lays out: the reader scrolls just enough to show its top and footer.
    // Writing follows the form while any of it shows; other changes only
    // while it was whole in view, so reading back up the thread stays put.
    property bool replyPinned: false
    property bool replyAdjusting: false
    function replyBounds() {
        const top = reply.mapToItem(reader.contentItem, 0, 0).y - replyShift.y - 12
        return {top: top, bottom: top + 12 + reply.height + 24}
    }
    function replyWhollyShown() {
        if (!replyOpen || !reply.visible || reply.height <= 0) return false
        const box = replyBounds()
        return box.top >= reader.contentY - 1 && box.bottom <= reader.contentY + reader.height + 1
    }
    function keepReplyInView(writing) {
        if (!replyOpen || !reply.visible || reply.height <= 0 || replyReveal.running || revealScroll.running || reader.moving || !replyHasFocus()) return
        const box = replyBounds(), top = box.top, bottom = box.bottom
        if (!(replyPinned || (writing && top < reader.contentY + reader.height && bottom > reader.contentY))) return
        replyPinned = true
        let target = reader.contentY
        if (bottom > target + reader.height) target = bottom - reader.height
        if (top < target) target = top
        target = Math.max(0, target)
        if (Math.abs(target - reader.contentY) <= .5) return
        replyAdjusting = true; reader.cancelFlick(); reader.contentY = target; replyAdjusting = false
        backend.remember("readerY", target)
    }
    function replyHasFocus() {
        let item = mail.ApplicationWindow.window ? mail.ApplicationWindow.window.activeFocusItem : null
        while (item) { if (item === reply) return true; item = item.parent }
        return false
    }
    // After Send the sent message takes the box's place and the thread gets
    // shorter. The reader never rests past its end and shows the sent message
    // whole, until the height has held for 250 ms (1.5 s at most) or the
    // reader is scrolled.
    Timer {
        id: sentReveal
        objectName: "sentReveal"
        interval: 16; repeat: true
        property int pass: 0
        property int stable: 0
        property real lastHeight: -1
        function begin() { replyReveal.stop(); revealScroll.stop(); pass = 0; stable = 0; lastHeight = -1; restart() }
        function follow() {
            const bottom = Math.max(0, reader.contentHeight - reader.height)
            let target = Math.min(reader.contentY, bottom)
            const loader = memberRepeater.itemAt(mail.members.length - 1), sent = loader ? loader.item : null
            if (sent && sent.visible && sent.height > 0)
                target = Math.min(bottom, Math.max(target, sent.mapToItem(reader.contentItem, 0, sent.height).y + 24 - reader.height))
            if (Math.abs(reader.contentY - target) > .5) { reader.cancelFlick(); reader.contentY = target }
        }
        onTriggered: {
            ++pass
            const height = reader.contentHeight
            stable = height === lastHeight ? stable + 1 : 0
            lastHeight = height
            follow()
            if (stable >= 15 || pass >= 90) { backend.remember("readerY", reader.contentY); stop() }
        }
    }
    function restorePosition() {
        restoring = true
        const view = backend.presentation
        const identity = composerIdentity
        const sameDraft = backend.activeDraftId.length ? view.selectedDraft === backend.activeDraftId : !view.selectedDraft && view.selected === backend.selectedId
        if (sameDraft && view.replyOpen !== undefined) {
            const replies = Object.assign({},replyStates); replies[identity] = view.replyOpen === true; replyStates = replies
        }
        replyOpen = replyStates[identity] === true
        Qt.callLater(function() { mail.restoreListPosition(); if (!replyReveal.running) reader.contentY = Math.max(0,view.readerY||0); restoring = false })
    }
    Connections {
        target: backend
        function onAccountChanged() { mail.cancelKeyboardOpen(); mail.clearSelection(); morePopup.close(); mail.pendingComposeId = "" }
        function onQueryChanged() { mail.cancelKeyboardOpen(); mail.clearSelection(); morePopup.close() }
        function onPresentationChanged() { Qt.callLater(mail.restorePosition) }
        // The header is put back before the changed rows paint, never a frame later.
        function onConversationsChanged() { if (mail.restoring) Qt.callLater(mail.restoreListPosition); prefetchTimer.restart(); if (mail.anchoredSection) mail.anchorAfterChange(false) }
        function onLoadingChanged() {
            if (!backend.loading) mail.settledScope = mail.listScope
            if (!backend.loading && mail.anchoredSection) mail.anchorAfterChange(true)
        }
        function onConversationChanged() {
            mail.completeMenuCompose()
            const identity = mail.currentComposerIdentity(), thread = backend.account + "/" + backend.selectedId
            if (identity !== mail.loadedConversation) {
                // A reply or forward started in the thread on screen keeps its
                // place and unfolded messages; only another thread starts over.
                const sameThread = thread === mail.loadedThread && backend.selectedId.length > 0 && mail.members.length > 0
                mail.loadedConversation = identity; mail.hydratedDraftIdentity = ""
                mail.replyOpen = mail.replyStates[identity] === true
                if (!replyReveal.running && !sameThread) reader.contentY = Math.max(0,backend.presentation.readerY||0)
            }
            mail.followThread(thread)
            mail.loadedThread = thread
            if (mail.members.length) {
                if (mail.pendingReveal === backend.selectedId) { if (mail.matchingIds.length) mail.revealMatch(); mail.pendingReveal = "" }
            }
        }
        function onDraftChanged() {
            mail.noteReplyAll()
            const discarded = backend.draftStatus === "Draft discarded", closing = mail.discardClosesReply()
            // Once Send is under way the box gives way to the sent message,
            // which the thread already shows; the reader keeps it in view.
            if (backend.googleSend.sent_current || mail.sentReplyDone || backend.sendQueued || mail.replyOnItsWay()) {
                if (mail.replyOpen && mail.members.length) sentReveal.begin()
                mail.setReplyOpen(false); return
            }
            if (backend.activeDraftId.length && Object.keys(backend.draft).length) {
                if (discarded) { if (closing) mail.setReplyOpen(false) }
                else if (mail.hydratedDraftIdentity !== backend.account+"/draft/"+backend.activeDraftId) {
                    const identity = mail.currentComposerIdentity()
                    mail.hydratedDraftIdentity = identity
                    mail.loadedConversation = identity
                    const opened = backend.presentation.selectedDraft !== backend.activeDraftId || backend.presentation.replyOpen !== false
                    const replies = Object.assign({}, mail.replyStates); replies[identity] = opened; mail.replyStates = replies
                    mail.replyOpen = opened
                    if (mail.replyOpen) mail.revealReply()
                }
                return
            }
            if (!backend.selectedId.length || !Object.keys(backend.draft).length) return
            const identity = backend.account + "/" + backend.selectedId
            if (discarded) { if (closing) mail.setReplyOpen(false); return }
            if (mail.hydratedDraftIdentity !== identity) {
                mail.hydratedDraftIdentity = identity
                if (mail.replyStates[identity] === undefined)
                    mail.setReplyOpen(mail.draftAuthored(backend.draft))
                else mail.replyOpen = mail.replyStates[identity] === true
            }
        }
    }
    SplitView {
        id: split
        anchors.fill: parent; orientation: Qt.Horizontal
        onResizingChanged: if (!resizing && mail.layoutStore) mail.layoutStore.mailSplitWidth = leftPane.width
        handle: Item {
            id: splitHandle
            implicitWidth: 1
            Rectangle { x: -4; width: 9; height: parent.height; color: Theme.inkAlpha(.05); visible: splitHandle.SplitHandle.hovered }
            Rectangle { x: Theme.snapX(parent, 0); width: Theme.ruleWidth; height: parent.height; color: splitHandle.SplitHandle.pressed ? Theme.accent : Theme.controlBorder }
            containmentMask: Item { x: -4; width: 9; height: split.height }
        }
        Item {
            id: leftPane
            objectName: "mailListPane"
            // One divider position for the whole app, set once at startup and saved
            // when a drag ends, so switching accounts never relays out the panes.
            SplitView.preferredWidth: mail.width * 0.50; SplitView.minimumWidth: 340
            Component.onCompleted: {
                const saved = mail.layoutStore ? mail.layoutStore.mailSplitWidth : 0
                if (saved > 0) SplitView.preferredWidth = mail.width > 0 ? Math.max(340, Math.min(mail.width - 470, saved)) : saved
            }
            ColumnLayout {
                anchors.fill: parent; spacing: 0
                // The list's head: a 40px row of folders (inbox · sent · drafts,
                // Spam and Trash behind ▾), the stats and New, then a 30px row
                // of label filters and the people / automated toggle, over a rule.
                Item {
                    id: listHead
                    Layout.fillWidth: true
                    implicitHeight: listHeadColumn.implicitHeight + 1
                    Rectangle { x: Theme.snapX(parent, 0); y: Theme.snapY(parent, parent.height - 1); width: parent.width; height: Theme.ruleWidth; color: Theme.line }
                    ColumnLayout {
                        id: listHeadColumn
                        x: 16; width: parent.width - 32; spacing: 0
                        Text {
                            objectName: "accountArchiveNotice"; visible: backend.accountArchived || backend.unsavedAccountBuffer
                            Layout.fillWidth: true; Layout.topMargin: 12; wrapMode: Text.Wrap; textFormat: Text.PlainText
                            text: (backend.currentAccount.email || "") + (backend.unsavedAccountBuffer ? " · unsaved edits retained in this editor" : " · read-only account archive")
                            color: backend.unsavedAccountBuffer ? Theme.warn : Theme.subtle; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(11.5)
                        }
                        RowLayout {
                            id: scopeHeader
                            objectName: "scopeHeader"
                            Layout.fillWidth: true; Layout.preferredHeight: 40; spacing: 16
                            // The stats give way first when the pane is narrow.
                            readonly property bool roomy: width >= scopeTabs.implicitWidth + scopeStats.implicitWidth + newMessageButton.implicitWidth + 48
                            RowLayout {
                                id: scopeTabs
                                spacing: 16
                                RowLayout {
                                    spacing: 6
                                    ScopeTab { objectName: "inboxScopeButton"; text: "inbox"; folder: "inbox" }
                                    ScopeCount {
                                        objectName: "inboxCount"; folder: "inbox"; tone: Theme.amber
                                        count: (backend.currentAccount.inbox_review_count ?? backend.currentAccount.unread_conversation_count) || 0
                                        tip: count + " to review in " + (backend.currentAccount.name || "this account")
                                    }
                                }
                                ScopeTab { objectName: "sentScopeButton"; text: "sent"; folder: "sent" }
                                RowLayout {
                                    spacing: 6
                                    ScopeTab { objectName: "draftsScopeButton"; text: "drafts"; folder: "drafts" }
                                    ScopeCount {
                                        objectName: "draftsCount"; folder: "drafts"; tone: Theme.inkAlpha(.5)
                                        count: backend.draftChips.account === backend.account ? backend.draftChips.total || 0 : 0
                                        tip: count + (count === 1 ? " unsent draft" : " unsent drafts") + " in " + (backend.currentAccount.name || "this account")
                                    }
                                }
                                FmButton {
                                    id: binMenuButton; objectName: "binMenuButton"
                                    visible: !backend.syntheticProfile
                                    text: mail.spamMode ? "spam" : backend.folder === "trash" ? "trash" : ""
                                    iconName: "chevronDown"; iconSize: 10; iconTrailing: true; iconSpacing: 5
                                    implicitHeight: 22; leftPadding: 6; rightPadding: 6; topPadding: 0; bottomPadding: 0; radius: 5
                                    font.pointSize: Theme.fontPoints(12); font.weight: Font.Normal; hoverBrightness: 1
                                    foreground: mail.binMode || hovered ? Theme.text : Theme.inkAlpha(.4)
                                    tip: "Spam and Trash"
                                    background: Rectangle { radius: 5; color: binMenu.opened || binMenuButton.hovered ? Theme.bg4 : "transparent"; border.width: binMenuButton.visualFocus ? 1 : 0; border.color: Theme.focusBorder }
                                    // The press that reached this button already closed the menu.
                                    property bool pressClosedMenu: false
                                    onPressed: pressClosedMenu = !binMenu.opened && Date.now() - binMenu.closedAt < 50
                                    onClicked: { if (binMenu.opened) binMenu.close(); else if (!pressClosedMenu) binMenu.open(); pressClosedMenu = false }
                                }
                                Text { objectName: "sendingCount"; visible: backend.sendingCount > 0; text: backend.sendingCount + " sending"; color: Theme.accent; font.pointSize: Theme.fontPoints(12) }
                            }
                            Item { Layout.fillWidth: true }
                            // "6 to review · 6 delayed": the numbers in ink, the words dimmed.
                            RowLayout {
                                id: scopeStats
                                visible: scopeHeader.roomy
                                spacing: 14
                                Text {
                                    objectName: "scopeStat"
                                    readonly property int count: mail.binMode ? mail.binCount : mail.draftsMode ? mail.rows.length : backend.folder === "sent" ? mail.sentThisWeek : (backend.currentAccount.inbox_review_count ?? backend.currentAccount.unread_conversation_count) || 0
                                    readonly property string label: mail.spamMode ? "in spam" : mail.binMode ? "in trash" : mail.draftsMode ? (count === 1 ? "draft" : "drafts") : backend.folder === "sent" ? "sent this week" : "to review"
                                    textFormat: Text.StyledText
                                    text: "<font color=\"" + Theme.text + "\">" + count + "</font> " + label
                                    color: Theme.inkAlpha(.5); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12); font.features: {"tnum": 1}
                                    HoverHandler { id: scopeStatHover }
                                    FmToolTip {
                                        shown: scopeStatHover.hovered
                                        text: mail.spamMode ? "Messages Google marked as spam · auto-deleted after 30 days" : mail.binMode ? "Deleted messages · auto-deleted after 30 days"
                                            : mail.draftsMode ? "Unsent messages from " + (backend.currentAccount.name || "this account") : backend.folder === "sent" ? "Sent from " + (backend.currentAccount.name || "this account") + " this week"
                                            : "Unread, or open and not yet decided on"
                                    }
                                }
                                Text {
                                    objectName: "scopeStatHeld"
                                    visible: backend.folder === "inbox"
                                    textFormat: Text.StyledText
                                    text: "<font color=\"" + Theme.text + "\">" + ((backend.currentAccount.inbox_held_count ?? backend.currentAccount.held_conversation_count) || 0) + "</font> delayed"
                                    color: Theme.inkAlpha(.5); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12); font.features: {"tnum": 1}
                                    HoverHandler { id: heldStatHover }
                                    FmToolTip { shown: heldStatHover.hovered; text: "Delayed · stays until you mark it Done" }
                                }
                            }
                            FmButton {
                                id: newMessageButton; objectName: "newMessageButton"
                                text: "New"; iconName: "plus"; iconSize: 12; iconSpacing: 7; prominent: true; hoverBrightness: 1
                                implicitHeight: 28; leftPadding: 10; rightPadding: 12; topPadding: 0; bottomPadding: 0; radius: 6
                                font.pointSize: Theme.fontPoints(12); font.weight: Font.Medium
                                opacity: hovered ? .88 : 1
                                enabled: mail.newMessageAvailable
                                tip: "New message — " + ShortcutRegistry.label(ShortcutRegistry.sequence("compose", backend.preferences)).toLowerCase()
                                onClicked: mail.newMessage()
                            }
                        }
                        RowLayout {
                            visible: !mail.binMode
                            Layout.fillWidth: true; Layout.preferredHeight: 30; Layout.topMargin: -4; Layout.bottomMargin: 4; spacing: 16
                            Flickable {
                                Layout.fillWidth: true; Layout.fillHeight: true; clip: true
                                contentWidth: categoryRow.width; contentHeight: height; boundsBehavior: Flickable.StopAtBounds; flickableDirection: Flickable.HorizontalFlick
                                // Plain-text toggles. Inbox labels match any chosen label
                                // (none chosen shows all mail); Sent and Drafts filter by day.
                                Row {
                                    id: categoryRow; spacing: 14; anchors.verticalCenter: parent.verticalCenter
                                    Repeater {
                                        model: backend.folder === "sent" || mail.draftsMode ? mail.dayOptions : []
                                        RailToggle {
                                            required property var modelData
                                            objectName: "railDay_" + modelData.key
                                            label: modelData.name; count: mail.dayCounts[modelData.key] || 0; selected: mail.activeDay === modelData.key
                                            tip: (mail.draftsMode ? "Drafts saved " : "Sent ") + modelData.name.toLowerCase() + (mail.draftsMode ? "" : " · UTC")
                                            onHoveredChanged: if (hovered && !mail.draftsMode) backend.prefetchFilters(mail.dayFilters(modelData.key))
                                            onClicked: mail.selectDay(modelData.key)
                                        }
                                    }
                                    Repeater {
                                        model: backend.folder === "sent" || mail.draftsMode ? [] : mail.popularCategories
                                        RailToggle {
                                            required property var modelData
                                            objectName: "railLabel_" + modelData.id
                                            label: modelData.name; count: modelData.conversation_count; selected: mail.categoryOn(modelData.id)
                                            onHoveredChanged: if (hovered) backend.prefetchCategory(mail.toggledCategory(modelData.id))
                                            onClicked: mail.toggleCategory(modelData.id)
                                        }
                                    }
                                    FmButton { id: allLabels; objectName: "allCategoriesButton"; visible: backend.folder !== "sent" && !mail.draftsMode && backend.categories.length > 7; iconName: "more"; implicitHeight: 20; implicitWidth: 22; tip: "All labels"; onClicked: railCategories.open() }
                                }
                            }
                            // People or automated senders: two bare 22×20 toggles.
                            Row {
                                id: kindButtons; spacing: 2
                                enabled: backend.folder === "inbox"; opacity: enabled ? 1 : 0
                                Behavior on opacity { NumberAnimation { duration: Theme.ms(150); easing.type: Theme.enterEasing } }
                                Repeater { model: ["people", "automated"]
                                    FmButton {
                                        id: kindButton; required property string modelData
                                        objectName: "senderKind_" + modelData; iconName: mail.kindIcon(modelData); iconSize: 11
                                        width: 22; height: 20; implicitWidth: 22; implicitHeight: 20; leftPadding: 0; rightPadding: 0; topPadding: 0; bottomPadding: 0
                                        selected: backend.filters.kind === modelData; foreground: selected ? Theme.canvas : Theme.inkAlpha(.55)
                                        tip: (modelData === "people" ? "People only" : "Automated only") + (selected ? " · click again to show all kinds" : "")
                                        background: Rectangle { radius: 5; color: kindButton.selected ? Theme.text : kindButton.hovered ? Theme.hover : "transparent"; border.width: kindButton.visualFocus ? 1 : 0; border.color: Theme.focusBorder }
                                        onHoveredChanged: if (hovered) backend.prefetchFilters(mail.kindFilters(modelData))
                                        onClicked: mail.setKind(modelData)
                                    }
                                }
                            }
                        }
                    }
                }
                RowLayout {
                    visible: mail.hasSelection
                    Layout.fillWidth: true; Layout.leftMargin: 16; Layout.rightMargin: 16; spacing: 6
                    Text { text: mail.selectionRows.length + " selected"; color: Theme.accent; font.pointSize: Theme.fontPoints(11.5); Layout.fillWidth: true }
                    FmButton { text: "Done"; visible: !mail.draftsMode && backend.folder !== "sent"; enabled: mail.selectionMutable; onClicked: mail.selectionAction("done") }
                    FmButton { text: "Delay"; visible: !mail.draftsMode && backend.folder !== "sent"; enabled: mail.selectionMutable; onClicked: mail.selectionAction("delay") }
                    FmButton { text: "More"; onClicked: mail.showRowMenu(mail.selectionRows[0].id, this, 0, height) }
                    FmButton { iconName: "close"; tip: "Clear selection · Esc"; onClicked: mail.clearSelection() }
                }
                Text {
                    visible: text.length > 0; text: backend.mailSelectionStatus
                    Layout.fillWidth: true; Layout.leftMargin: 16; Layout.rightMargin: 16
                    wrapMode: Text.Wrap; color: Theme.subtle; font.pointSize: Theme.fontPoints(11.5)
                }
                RowLayout {
                    objectName: "binStrip"
                    visible: mail.binMode && list.count > 0
                    Layout.fillWidth: true; Layout.leftMargin: 16; Layout.rightMargin: 16; Layout.topMargin: 14; Layout.preferredHeight: 32; spacing: 12
                    Text { text: "Auto-deleted after 30 days"; color: Theme.inkAlpha(.45); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(11) }
                    Rectangle { Layout.fillWidth: true; implicitHeight: Theme.ruleWidth; color: Theme.line }
                    FmButton {
                        objectName: "emptyBin"; text: mail.spamMode ? "Empty spam" : "Empty trash"
                        enabled: !backend.binBusy; implicitHeight: 20; topPadding: 0; bottomPadding: 0; leftPadding: 0; rightPadding: 0
                        font.pointSize: Theme.fontPoints(11); foreground: hovered ? Theme.error : Theme.inkAlpha(.7); background: Item {}
                        onClicked: binConfirm.ask("empty")
                    }
                }
                ListView {
                    id: list; objectName: "conversationList"
                    Layout.fillWidth: true; Layout.fillHeight: true; Layout.topMargin: 6
                    clip: true; model: backend.conversationModel; boundsBehavior: Flickable.StopAtBounds
                    // The first Inbox header sits at the top, without the room
                    // its item keeps above it after another section.
                    topMargin: backend.folder === "inbox" && mail.inboxSections.length ? mail.headerHeight(mail.inboxSections[0].key) - mail.rowHeight : 0
                    ScrollBar.vertical: FmScrollBar { id: listBar; autoHide: true }
                    HoverHandler { onPointChanged: listBar.pointerMoved(point.scenePosition) }
                    // When a new search, filter or label's mail is still on the
                    // way after 80 ms, the rows still shown fade back to say so;
                    // the answer brings them up again.
                    property bool dimmed: false
                    opacity: dimmed ? .55 : 1
                    Behavior on opacity { NumberAnimation { duration: Theme.ms(90); easing.type: Easing.OutQuad } }
                    Timer { interval: 80; running: backend.listPending; onTriggered: list.dimmed = true }
                    Connections {
                        target: backend
                        function onLoadingChanged() { if (!backend.listPending) list.dimmed = false }
                    }
                    // A new list for the same account (another folder or search)
                    // starts at its top; the saved place follows it, as it does
                    // whenever the list moves. Another account restores its own.
                    property string resetAccount: ""
                    Connections {
                        target: backend.conversationModel
                        function onModelReset() {
                            if (backend.account === list.resetAccount && !mail.restoring && !list.moving) backend.remember("listY", list.contentY - list.originY)
                            list.resetAccount = backend.account
                        }
                    }
                    // Consume each structural batch before the next can reuse
                    // delegate indexes. Retaining an old layout across several
                    // remove/move batches leaves retired rows over replacements.
                    Connections {
                        target: backend.conversationModel
                        function onRowsRemoved() { list.forceLayout() }
                        function onRowsMoved() { list.forceLayout() }
                        function onRowsInserted() { list.forceLayout() }
                    }
                    onContentHeightChanged: { if (mail.restoring) Qt.callLater(mail.restoreListPosition); prefetchTimer.restart() }
                    onContentYChanged: {
                        backend.setBrowsingOlder(contentY - originY > height)
                        if (!mail.restoring && !list.moving) backend.remember("listY", contentY - originY)
                        prefetchTimer.restart()
                    }
                    // Rows merged in above the visible ones move the origin, not
                    // the rows on screen; the saved place follows what is shown.
                    onOriginYChanged: if (!mail.restoring && !list.moving) backend.remember("listY", contentY - originY)
                    Timer {
                        id: prefetchTimer; interval: 40
                        // Older rows load while the view is within four screens of
                        // the loaded ones; a jump past them loads through the view.
                        onTriggered: {
                            if (!mail.visible || list.count === 0 || !backend.hasMore || backend.loading || list.contentY + list.height * 4 < list.loadedBottom) return
                            if (list.contentY + list.height > list.loadedBottom) backend.loadThrough(mail.rowsThrough(list.contentY + list.height, list.loadedBottom))
                            else backend.prefetchMore()
                        }
                    }
                    Connections { target: backend; function onLoadingChanged() { if (!backend.loading) prefetchTimer.restart() } }
                    onMovementEnded: if (!mail.restoring) backend.remember("listY",contentY - originY)
                    onMovementStarted: mail.anchoredSection = ""
                    // Headers and empty lines are light items of their own; only
                    // conversation rows build a row's content.
                    delegate: DelegateChooser {
                        role: "itemType"
                        DelegateChoice { roleValue: "header"; SectionItem {} }
                        DelegateChoice { roleValue: "empty"; SectionEmpty {} }
                        DelegateChoice {
                            roleValue: "row"
                            Item {
                                id: groupedRow
                                required property var rowData
                                required property string previousGroup
                                readonly property var modelData: rowData || ({id:"",subject:"",sender_name:"",latest_at:"",message_count:0,unread:false,starred:false})
                                required property int index
                                // Qt can retain a removed delegate with index -1 while
                                // replacements are laid out. Retired rows must neither
                                // paint over the new page nor remain clickable.
                                readonly property bool dayHidden: mail.draftHidden(modelData)
                                // A starred contact's mail: a star takes the sender kind's place.
                                readonly property bool starred: !modelData.is_outgoing && mail.senderStarred(modelData.sender_email)
                                visible: index >= 0 && !dayHidden
                                enabled: index >= 0 && !dayHidden
                                readonly property string group: mail.groupOf(modelData)
                                readonly property bool firstInGroup: index===0 || (mail.dateGroups ? mail.groupOf(mail.rows[index-1]) : previousGroup)!==group
                                // Inbox styling follows the section: To review, Delayed or Done.
                                readonly property string tier: group === "review" || group === "held" ? group : "done"
                                // Inbox sections fold by leaving the list; Sent and Drafts
                                // days fold their rows in place.
                                readonly property bool collapsed: mail.dateGroups && mail.groupCollapsed(group)
                                // Inbox headers are items of their own; elsewhere the first
                                // row of a group draws its heading.
                                readonly property int headingHeight: firstInGroup && !mail.binMode && backend.folder !== "inbox" && (mail.dateGroups || (backend.folder !== "sent" && !mail.draftsMode)) ? (index === 0 ? 0 : 10) + mail.headerHeight(group) : 0
                                // A row read in To review folds away once another opens: 170 ms,
                                // faded by 70% of the way. The last row of its section keeps its
                                // height and fades into the section's empty line instead.
                                // Done, Delay and filing fold the open row the same way, and
                                // it drifts toward where it goes: Done and filing to the left,
                                // Delay to the right ("later").
                                readonly property bool leaving: !!modelData.leaving
                                readonly property real leaveDrift: modelData.leave_kind === "done" || modelData.leave_kind === "filed" ? -18 : modelData.leave_kind === "delay" ? 18 : 0
                                readonly property bool leavesEmpty: leaving && !!modelData.leaves_empty
                                property real leave: 0
                                NumberAnimation on leave { id: leaveAnimation; running: false; from: 0; to: 1; duration: Theme.ms(170); easing.type: Easing.InQuad }
                                onLeavingChanged: { if (leaving) leaveAnimation.restart(); else { leaveAnimation.stop(); leave = 0 } }
                                Component.onCompleted: if (leaving) leaveAnimation.restart()
                                width: list.width; height: dayHidden ? 0 : headingHeight + (collapsed ? 0 : mail.rowHeight * (leavesEmpty ? 1 : 1 - leave))
                                GroupHeader { group: groupedRow.group; visible: groupedRow.headingHeight > 0; width: parent.width; y: groupedRow.headingHeight - height }
                                Loader {
                                    active: groupedRow.leavesEmpty
                                    y: groupedRow.headingHeight; width: parent.width; height: mail.rowHeight
                                    opacity: groupedRow.leave
                                    sourceComponent: Item { EmptyLine { group: groupedRow.group } }
                                }
                                ItemDelegate {
                                    id: row
                                    objectName: "conversationRow_"+groupedRow.modelData.id
                                    visible: !groupedRow.collapsed
                                    y: groupedRow.headingHeight; width: parent.width; height: parent.height-y
                                    opacity: Math.max(0, 1 - groupedRow.leave / .7); clip: groupedRow.leaving
                                    transform: Translate { x: groupedRow.leave * groupedRow.leaveDrift }
                                    leftPadding: 16; rightPadding: 16; topPadding: 0; bottomPadding: 0; hoverEnabled: true
                                    Accessible.name: groupedRow.modelData.subject+", "+groupedRow.modelData.sender_name+(groupedRow.modelData.unread ? ", unread" : ", reviewed")+(groupedRow.modelData.starred ? ", action held" : "")
                                    onClicked: mail.rowClick(groupedRow.modelData.id, 0)
                                    Accessible.selected: mail.isSelected(groupedRow.modelData.id) || (!mail.hasSelection && (mail.draftsMode ? backend.activeDraftId : backend.selectedId) === groupedRow.modelData.id)
                                    MouseArea {
                                        id: rowMouse; anchors.fill: parent; acceptedButtons: Qt.LeftButton | Qt.RightButton; hoverEnabled: true
                                        onEntered: { if (!mail.keyboardNavigation) mail.pointerRow = groupedRow.modelData.id; if (!mail.draftsMode) backend.prefetchConversation(groupedRow.modelData.id) }
                                        onPositionChanged: function(mouse) { mail.pointerMoved(groupedRow.modelData.id,rowMouse,mouse.x,mouse.y) }
                                        onExited: if (mail.pointerRow === groupedRow.modelData.id) mail.pointerRow = ""
                                        onClicked: function(mouse) {
                                            list.forceActiveFocus(Qt.MouseFocusReason)
                                            mail.pointerPosition = rowMouse.mapToGlobal(mouse.x,mouse.y)
                                            mail.keyboardNavigation = false; mail.pointerRow = groupedRow.modelData.id
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
                                        objectName: "rowHighlight_" + groupedRow.modelData.id
                                        readonly property bool multi: mail.isSelected(groupedRow.modelData.id)
                                        readonly property bool opened: !mail.hasSelection && (mail.draftsMode ? backend.activeDraftId : backend.selectedId) === groupedRow.modelData.id
                                        color: multi ? Theme.bg4 : opened ? Theme.accentAlpha(.08) : mail.highlightedRow === groupedRow.modelData.id ? Theme.surface : "transparent"
                                        Rectangle { width: parent.multi ? 2 : 3; height: parent.height; color: parent.multi ? Theme.accentAlpha(.55) : Theme.accent; visible: parent.multi || parent.opened }
                                        // A hairline above every row but a group's first, inset to the text.
                                        Rectangle {
                                            objectName: "rowSeparator_" + groupedRow.modelData.id
                                            x: 16; y: Theme.snapY(parent, 0); width: parent.width - 32; height: Theme.ruleWidth
                                            color: (mail.groupPositions[groupedRow.modelData.id] || 0) > 0 ? Theme.inkAlpha(.07) : "transparent"
                                        }
                                    }
                                    // Inbox rows: avatar, sender, subject over preview, first label
                                    // and time on a 16px-gapped grid. Sent and Drafts rows lead with
                                    // an arrow or pencil and carry the recipient instead.
                                    readonly property bool outgoingScope: mail.draftsMode || backend.folder === "sent"
                                    readonly property bool narrow: leftPane.width < 520
                                    contentItem: RowLayout {
                                        spacing: row.outgoingScope ? 14 : 16
                                        Item {
                                            visible: row.outgoingScope
                                            implicitWidth: 12; implicitHeight: 12
                                            FmIcon { anchors.centerIn: parent; name: mail.draftsMode ? "edit" : "arrowRight"; size: 12; color: Theme.inkAlpha(.4) }
                                        }
                                        SenderAvatar {
                                            objectName: "mailAvatar_" + groupedRow.modelData.id
                                            visible: !row.outgoingScope
                                            Layout.alignment: Qt.AlignTop; Layout.topMargin: 7
                                            diameter: 24; tipEnabled: true
                                            own: !!groupedRow.modelData.is_outgoing
                                            name: groupedRow.modelData.sender_name || groupedRow.modelData.sender_email || ""
                                            email: groupedRow.modelData.sender_email || ""; kind: groupedRow.modelData.kind || "unknown"
                                            discColor: own ? Theme.accentAlpha(0.18) : Theme.oklch(.36, .06, hue)
                                            initialsColor: own ? Theme.accent : Theme.oklch(.9, .05, hue)
                                            initials: own ? mail.initials(backend.currentAccount.name || "") : Theme.initialsOf(name)
                                        }
                                        ColumnLayout {
                                            objectName: "mailSenderColumn_" + groupedRow.modelData.id
                                            visible: !row.outgoingScope && !row.narrow
                                            readonly property real columnWidth: Math.max(150, row.availableWidth * .24)
                                            Layout.minimumWidth: columnWidth; Layout.preferredWidth: columnWidth; Layout.maximumWidth: columnWidth; spacing: 3
                                            // The name elides before its kind glyph, which follows it. Its cap is
                                            // rounded up: fractional (macOS) text widths otherwise lose a sub-pixel
                                            // to layout rounding and elide a name that fits.
                                            RowLayout {
                                                Layout.fillWidth: true; spacing: 6
                                                Text { objectName: "mailSender_" + groupedRow.modelData.id; text: Theme.oneLine(groupedRow.modelData.sender_name || groupedRow.modelData.sender_email) || "Unknown sender"; maximumLineCount: 1; color: Theme.text; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(13); Layout.fillWidth: true; Layout.maximumWidth: Math.ceil(implicitWidth); elide: Text.ElideRight; textFormat: Text.PlainText }
                                                Text {
                                                    objectName: "mailOthers_" + groupedRow.modelData.id
                                                    readonly property int others: groupedRow.modelData.others || 0
                                                    readonly property var names: groupedRow.modelData.other_names || []
                                                    visible: others > 0
                                                    text: "+" + others; color: Theme.inkAlpha(.5); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(11); font.features: {"tnum":1}; textFormat: Text.PlainText
                                                    HoverHandler { id: othersHover }
                                                    FmToolTip { shown: othersHover.hovered; text: "Also on this thread: " + parent.names.join(", ") + (parent.others > parent.names.length ? " and " + (parent.others - parent.names.length) + " more" : "") }
                                                }
                                                FmIcon { objectName: "mailSenderKind_" + groupedRow.modelData.id; visible: groupedRow.modelData.is_outgoing || groupedRow.starred || mail.kindKnown(groupedRow.modelData.kind); name: groupedRow.modelData.is_outgoing ? "send" : groupedRow.starred ? "contactStarFilled" : mail.kindIcon(groupedRow.modelData.kind); size: 10; color: groupedRow.starred ? Theme.amber : Theme.inkAlpha(.3) }
                                                Item { Layout.fillWidth: true }
                                            }
                                            Text { objectName: "mailSenderAddress_" + groupedRow.modelData.id; text: groupedRow.modelData.sender_email || ""; color: Theme.inkAlpha(.45); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(11); Layout.fillWidth: true; elide: Text.ElideRight; textFormat: Text.PlainText }
                                        }
                                        ColumnLayout {
                                            Layout.fillWidth: true; Layout.minimumWidth: 0; spacing: row.outgoingScope ? 2 : 3
                                            Item {
                                                Layout.fillWidth: true; implicitHeight: subjectText.implicitHeight
                                                // A result from the server that is not on this machine.
                                                Text {
                                                    id: remoteMark; objectName: "mailRemoteMark_"+groupedRow.modelData.id
                                                    visible: !!groupedRow.modelData.remote && !groupedRow.modelData.spam
                                                    width: visible ? implicitWidth : 0; anchors.baseline: subjectText.baseline
                                                    text: "remote"; color: Theme.inkAlpha(.5); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(11)
                                                    HoverHandler { id: remoteMarkHover }
                                                    FmToolTip { shown: remoteMarkHover.hovered; text: "Not on this machine yet · fetched from the server" }
                                                }
                                                Text {
                                                    id: subjectText
                                                    x: remoteMark.visible ? remoteMark.width+8 : 0
                                                    width: Math.max(0,Math.min(implicitWidth,parent.width-x-(threadCount.visible ? threadCount.width+8 : 0)-(attachmentCount.visible ? attachmentCount.width+8 : 0)))
                                                    text: Theme.oneLine(groupedRow.modelData.subject); maximumLineCount: 1
                                                    color: row.outgoingScope ? Theme.inkAlpha(groupedRow.modelData.subject === "(No subject)" ? .5 : .9) : Theme.text
                                                    font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(13); font.weight: row.outgoingScope ? Font.Medium : Font.Normal
                                                    elide: Text.ElideRight; textFormat: Text.PlainText
                                                    MatchMarks { objectName: "mailSubjectMatch_" + groupedRow.modelData.id; phrases: mail.subjectPhrases }
                                                }
                                                // "(3)" for the thread, then a paperclip and count.
                                                Text {
                                                    id: threadCount; objectName: "mailThreadCount_"+groupedRow.modelData.id
                                                    x: subjectText.x+subjectText.width+8; anchors.baseline: subjectText.baseline
                                                    visible: groupedRow.modelData.message_count>1
                                                    text: "(" + groupedRow.modelData.message_count + ")"; color: Theme.inkAlpha(.5); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(11); font.features: {"tnum":1}
                                                    HoverHandler {id:threadCountHover}
                                                    FmToolTip {shown:threadCountHover.hovered;text:groupedRow.modelData.message_count+" messages in this thread"}
                                                }
                                                Row {
                                                    id: attachmentCount; objectName: "mailAttachmentCount_"+groupedRow.modelData.id
                                                    x: subjectText.x+subjectText.width+(threadCount.visible ? threadCount.width+16 : 8); anchors.verticalCenter: subjectText.verticalCenter; spacing: 2
                                                    visible: (groupedRow.modelData.attachment_count || 0)>0
                                                    FmIcon {name:"attachment";size:10;color:Theme.inkAlpha(.5);anchors.verticalCenter:parent.verticalCenter}
                                                    Text {text:groupedRow.modelData.attachment_count || 0;color:Theme.inkAlpha(.5);font.family:Theme.fontFamily;font.pointSize:Theme.fontPoints(11);font.features:{"tnum":1};anchors.verticalCenter:parent.verticalCenter}
                                                    HoverHandler {id:attachmentCountHover}
                                                    FmToolTip {shown:attachmentCountHover.hovered;text:(groupedRow.modelData.attachment_count === 1 ? "1 attachment" : (groupedRow.modelData.attachment_count || 0)+" attachments")+" in this thread"}
                                                }
                                            }
                                            RowLayout {
                                                Layout.fillWidth: true; spacing: 0
                                                FmIcon { visible: !row.outgoingScope && row.narrow && (groupedRow.modelData.is_outgoing || groupedRow.starred || mail.kindKnown(groupedRow.modelData.kind)); name: groupedRow.modelData.is_outgoing ? "send" : groupedRow.starred ? "contactStarFilled" : mail.kindIcon(groupedRow.modelData.kind); size: 10; color: groupedRow.starred ? Theme.amber : Theme.inkAlpha(.3); Layout.rightMargin: 6 }
                                                Text {
                                                    objectName: "mailRecipient_" + groupedRow.modelData.id
                                                    visible: row.outgoingScope; Layout.maximumWidth: Math.max(60, row.availableWidth * .45); Layout.rightMargin: 4
                                                    text: backend.folder === "sent" ? "To " + (groupedRow.modelData.recipient_names || groupedRow.modelData.recipient || "recipient") : groupedRow.modelData.sender_name === "No recipients" ? "No recipient" : groupedRow.modelData.sender_name || ""
                                                    color: Theme.inkAlpha(.72); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(11.5); elide: Text.ElideRight; textFormat: Text.PlainText
                                                }
                                                FmIcon { objectName:"summaryPreviewIcon_"+groupedRow.modelData.id; visible:!!groupedRow.modelData.summary; name:"summary"; size:10; color:Theme.ai; Layout.rightMargin: 6 }
                                                Text { objectName:"mailPreview_"+groupedRow.modelData.id; text: (row.outgoingScope ? "· " : row.narrow ? Theme.oneLine(groupedRow.modelData.sender_name) + " · " : "") + Theme.oneLine(groupedRow.modelData.summary || groupedRow.modelData.snippet); maximumLineCount: 1; color: row.outgoingScope ? Theme.inkAlpha(.5) : Theme.inkAlpha(.55); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(row.outgoingScope ? 11.5 : 12); elide: Text.ElideRight; Layout.fillWidth: true; Layout.minimumWidth: 0; textFormat: Text.PlainText
                                                    MatchMarks { objectName: "mailPreviewMatch_" + groupedRow.modelData.id; phrases: mail.bodyPhrases }
                                                }
                                            }
                                        }
                                        Text {
                                            objectName: "mailLabel_" + groupedRow.modelData.id
                                            visible: !row.outgoingScope && (groupedRow.modelData.categories||[]).length>0
                                            Layout.alignment: Qt.AlignTop; Layout.topMargin: 10; Layout.maximumWidth: row.narrow ? 68 : 120
                                            text: "#" + String(((groupedRow.modelData.categories||[])[0] || {}).name || "").toLowerCase()
                                            color: Theme.inkAlpha(.5); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(11); elide: Text.ElideRight; textFormat: Text.PlainText
                                        }
                                        // Outgoing mail on its way shows its state ("sending") before the time.
                                        Text {
                                            objectName: "mailSendState_" + groupedRow.modelData.id
                                            visible: row.outgoingScope && !!groupedRow.modelData.is_outgoing && !!groupedRow.modelData.send_status
                                            text: String(groupedRow.modelData.send_status || "").toLowerCase()
                                            color: Theme.inkAlpha(.75); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(11); textFormat: Text.PlainText
                                        }
                                        Text {
                                            objectName: "mailTime_" + groupedRow.modelData.id
                                            Layout.alignment: row.outgoingScope ? Qt.AlignVCenter : Qt.AlignTop; Layout.topMargin: row.outgoingScope ? 0 : 10
                                            Layout.preferredWidth: row.outgoingScope ? 68 : 76
                                            text: mail.relativeTime(groupedRow.modelData.latest_at)
                                            color: Theme.inkAlpha(row.outgoingScope ? .4 : .5); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(row.outgoingScope ? 11.5 : 11); font.features: {"tnum":1}
                                            horizontalAlignment: Text.AlignRight; elide: Text.ElideLeft
                                        }
                                    }
                                }
                            }
                        }
                    }
                    // Where the loaded rows end, in content coordinates.
                    readonly property real loadedBottom: footerItem ? footerItem.y + footerItem.unloadedTop : contentHeight
                    footer: Column {
                        readonly property real unloadedTop: unloadedRows.y
                        width: list.width
                        // Room for the rows still to load. Where the view reaches
                        // it, faint placeholder rows stand in until they arrive.
                        Item {
                            id: unloadedRows; objectName: "unloadedRows"
                            width: list.width; height: mail.unloadedHeight
                            readonly property real loadedTop: list.loadedBottom
                            readonly property int first: Math.max(0, Math.floor((list.contentY - loadedTop) / mail.rowHeight))
                            Repeater {
                                model: unloadedRows.height > 0 && list.contentY + list.height > unloadedRows.loadedTop ? Math.ceil(list.height / mail.rowHeight) + 1 : 0
                                Item {
                                    required property int index
                                    readonly property int place: unloadedRows.first + index
                                    readonly property real nameWidth: Math.min(160, width * .2)
                                    visible: (place + 1) * mail.rowHeight <= unloadedRows.height
                                    y: place * mail.rowHeight; width: unloadedRows.width; height: mail.rowHeight
                                    Rectangle { x: 16; y: 7; width: 24; height: 24; radius: 12; color: Theme.inkAlpha(.04) }
                                    Rectangle { x: 56; anchors.verticalCenter: parent.verticalCenter; width: parent.nameWidth; height: 8; radius: 4; color: Theme.inkAlpha(.05) }
                                    Rectangle { x: 72 + parent.nameWidth; anchors.verticalCenter: parent.verticalCenter; width: Math.max(0, parent.width - x - 108); height: 8; radius: 4; color: Theme.inkAlpha(.035) }
                                }
                            }
                            FmButton { objectName: "retryLoadingConversations"; visible: !!backend.listError; y: 8; anchors.horizontalCenter: parent.horizontalCenter; text: "Retry loading"; tip: backend.listError; onClicked: backend.loadMore() }
                        }
                        Item {
                            width: list.width; height: backend.folder==="sent" && backend.captures.length ? 48 : 0
                            FmButton { objectName: "localCapturesButton"; anchors.centerIn: parent; visible: parent.height > 0; text: "Local captures"; font.pointSize: Theme.fontPoints(11.5); onClicked: capturePopup.open() }
                        }
                    }
                    Text {
                        objectName: "binEmptyText"
                        visible: mail.binMode && list.count === 0 && backend.connected && (!backend.loading || !!backend.listError)
                        x: 24; y: 22; width: parent.width - 48; wrapMode: Text.Wrap
                        text: backend.listError ? backend.listError : mail.spamMode ? "No spam. Nice." : "Trash is empty."
                        color: backend.listError ? Theme.error : Theme.inkAlpha(.5); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12)
                    }
                    ColumnLayout {
                        anchors.horizontalCenter: parent.horizontalCenter; y: 48; width: parent.width-48;spacing:9
                        visible:list.count===0 && !(mail.binMode && (!backend.loading || !!backend.listError))
                        Text {
                            Layout.fillWidth:true;horizontalAlignment:Text.AlignHCenter;wrapMode:Text.Wrap
                            text:!backend.connected ? (backend.error.length > 0 ? "Your local mail isn’t connected." : "Connecting to your local mail…") : !backend.accounts.length ? "No accounts connected." : mail.firstLoading ? "" : mail.filtering ? "No matches" : "No conversations in this view."
                            color:mail.filtering ? Theme.text : Theme.muted;font.family:Theme.fontFamily;font.pointSize:Theme.fontPoints(mail.filtering ? 14 : 12);font.weight:mail.filtering ? Font.Medium : Font.Normal
                        }
                        Text {visible:mail.filtering && !mail.firstLoading;Layout.fillWidth:true;horizontalAlignment:Text.AlignHCenter;wrapMode:Text.Wrap;text:"Nothing in "+(backend.currentAccount.name || "this account")+" for "+(mail.filterDescription || backend.query || "these filters");color:Theme.muted;font.pointSize:Theme.fontPoints(12)}
                        FmButton {visible:mail.filtering && !mail.firstLoading;Layout.alignment:Qt.AlignHCenter;text:"Clear filters";foreground:Theme.accent;onClicked:{backend.query="";backend.category="";backend.applyFilters({})}}
                        // The engine stopped and was not started again on its own.
                        Text {visible:!backend.connected && backend.error.length > 0;Layout.fillWidth:true;horizontalAlignment:Text.AlignHCenter;wrapMode:Text.Wrap;text:backend.error;color:Theme.subtle;font.pointSize:Theme.fontPoints(11.5)}
                        FmButton {objectName:"mailReconnect";visible:!backend.connected && backend.error.length > 0;Layout.alignment:Qt.AlignHCenter;text:"Reconnect";prominent:true;onClicked:backend.start()}
                        Text {visible:mail.filtering && backend.folder === "inbox" && !(backend.currentAccount.coverage || {}).inventory_complete;Layout.fillWidth:true;horizontalAlignment:Text.AlignHCenter;wrapMode:Text.Wrap;text:"Older mail is still being indexed.";color:Theme.subtle;font.pointSize:Theme.fontPoints(11.5)}
                    }
                }
            }
        }
        Item {
            id: readerPane
            enabled: mail.readerMatchesCursor || mail.spamMode
            SplitView.fillWidth: true; SplitView.minimumWidth: 470
            readonly property int margin: width<620 ? 24 : 40
            Rectangle { anchors.fill: parent; color: Theme.bg0 }
            ColumnLayout {
                anchors.fill: parent; spacing: 0
                ColumnLayout {
                    visible: mail.members.length>0
                    Layout.fillWidth: true; Layout.leftMargin: readerPane.margin; Layout.rightMargin: readerPane.margin; Layout.topMargin: 24; spacing: 12
                    Flow {
                        id: readerHeading
                        objectName: "conversationHeadingRow"
                        Layout.fillWidth: true; spacing: 10
                        readonly property int titleLine: Math.round(Theme.fontSize(16) * 1.25)
                        Item {
                            objectName: "readerSubjectLine"
                            // Whole title lines, from the text's laid-out height: Qt's
                            // lineCount can stay stale once a search tints the subject.
                            width: Math.min(readerSubject.implicitWidth, readerHeading.width); height: Math.max(1, Math.round(readerSubject.contentHeight / readerHeading.titleLine)) * readerHeading.titleLine
                            // Rich text for the heading's fixed line height; a search tints its matches.
                            CopyText {
                                id: readerSubject; objectName: "readerSubject"
                                y: -Math.max(0, Math.round((Theme.fontSize(16) * Theme.naturalLeading - readerHeading.titleLine) / 2))
                                readonly property string subject: Theme.oneLine(mail.thread.subject)
                                readonly property string marked: mail.markedHtml(subject, mail.subjectPhrases)
                                text: "<p style=\"margin:0;line-height:" + readerHeading.titleLine + "px;-qt-line-height-type:fixed\">" + (marked || mail.escapeHtml(subject)) + "</p>"
                                textFormat: TextEdit.RichText; wrapMode: TextEdit.Wrap; width: parent.width
                                color: Theme.text; font.pointSize: Theme.fontPoints(16); font.weight: Font.Medium; font.letterSpacing: 0
                            }
                        }
                    }
                    RowLayout {
                        objectName: "conversationMetaRow"
                        // The message header already names the sender and time
                        // of a single-message thread.
                        readonly property bool threadLine: mail.members.length > 1
                        visible: threadLine || (mail.matchingIds.length > 0 && mail.bodyMatches)
                        Layout.fillWidth: true; spacing: 12
                        Row {
                            visible: parent.threadLine
                            spacing: 3
                            Repeater {
                                model: mail.participants.slice(0,6)
                                ContactAvatar {
                                    required property var modelData
                                    objectName: "participantAvatar_" + modelData.message.id
                                    diameter: 20; own: !!modelData.message.sent
                                    email: modelData.message.sender_email || ""; personName: modelData.name; kind: modelData.message.kind || "unknown"
                                }
                            }
                        }
                        Flow {
                            id: participantNames
                            visible: parent.threadLine
                            Layout.fillWidth: true; spacing: 0
                            Text { text: mail.members.length + (mail.members.length === 1 ? " message · " : " messages · "); color: Theme.inkAlpha(0.62); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12) }
                            Text { text: mail.conversationSpan; color: Theme.inkAlpha(0.85); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12); FmToolTip { shown: spanHover.hovered; text: "First message → latest message" } HoverHandler { id: spanHover } }
                            Text { text: " · "; color: Theme.inkAlpha(0.62); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12) }
                            Repeater {
                                model: mail.participants
                                Row {
                                    required property var modelData
                                    required property int index
                                    ContactName { objectName: "participantName_" + parent.modelData.message.id; text: parent.modelData.name; email: parent.modelData.message.sender_email || ""; width: Math.min(implicitWidth, participantNames.width); foreground: parent.modelData.message.sent ? Theme.accent : Theme.inkAlpha(0.85) }
                                    Text { visible: parent.index < mail.participants.length - 1; text: parent.index === mail.participants.length - 2 ? " and " : ", "; color: Theme.inkAlpha(0.62); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12) }
                                }
                            }
                        }
                        Item { visible: !parent.threadLine; Layout.fillWidth: true }
                        FmButton { objectName: "showMatch"; visible: mail.matchingIds.length>0 && (parent.threadLine || mail.bodyMatches); text: "Show match"; font.pointSize: Theme.fontPoints(10.5); implicitHeight: 22; onClicked: mail.revealMatch(true) }
                    }
                }
                Flickable {
                    id: reader; objectName: "reader"
                    Layout.fillWidth: true; Layout.fillHeight: true
                    contentWidth: width; contentHeight: readerColumn.implicitHeight + (mail.chipsVisible ? 64 : 40); clip: true; boundsBehavior: Flickable.StopAtBounds
                    ScrollBar.vertical: FmScrollBar { id: readerBar; autoHide: true }
                    HoverHandler { onPointChanged: readerBar.pointerMoved(point.scenePosition) }
                    onMovementEnded: { if (!mail.restoring) backend.remember("readerY",contentY); mail.replyPinned = mail.replyWhollyShown() }
                    // Scrolling takes over from a reply being brought into view.
                    onMovementStarted: { replyReveal.stop(); revealScroll.stop(); sentReveal.stop(); matchReveal.stop(); mail.replyPinned = false; mail.visitScrollTouched = true }
                    onHeightChanged: mail.keepReplyInView(false)
                    // A shorter thread never leaves the reader past its end.
                    onContentHeightChanged: { if (sentReveal.running) sentReveal.follow(); mail.keepReplyInView(false) }
                    // Scrolled by any means but the reply's own following: it
                    // stays followed only while it shows whole.
                    onContentYChanged: if (!mail.replyAdjusting && !replyReveal.running && !revealScroll.running) mail.replyPinned = mail.replyWhollyShown()
                    ColumnLayout {
                        id: readerColumn
                        x: readerPane.margin; y: 8; width: reader.width-readerPane.margin*2; spacing: 0; visible: mail.members.length>0 || mail.replyOpen
                        // The messages sit 8px below the heading; a new message has none.
                        Item { implicitHeight: 8; visible: mail.members.length > 0 }
                        Repeater {
                            id: memberRepeater
                            model: backend.messageModel
                            delegate: Loader {
                                id: memberLoader
                                required property var rowData
                                required property int index
                                Layout.fillWidth:true
                                Layout.bottomMargin:0
                                active:false
                                visible:!mail.draftsMode && !mail.awaitingOwnCopy(rowData)
                                onVisibleChanged:if(visible)active=true
                                Component.onCompleted:if(visible)active=true
                                readonly property bool assistancePopupOpen:item ? item.assistancePopupOpen : false
                                sourceComponent:Item {
                                id: member
                                property var modelData: memberLoader.rowData
                                property int index:memberLoader.index
                                readonly property bool isExpanded: mail.memberIsExpanded(modelData.id,index)
                                readonly property bool isMatch: mail.matchingIds.indexOf(modelData.id)>=0
                                readonly property bool assistancePopupOpen: messageAssistance.popupOpen || messageSource.popupOpen || attachmentPopupOpen()
                                readonly property bool isLast: index === mail.members.length - 1
                                readonly property int attachmentCount: (modelData.attachments || []).length
                                function attachmentPopupOpen() {
                                    for (let i = 0; i < attachmentRepeater.count; ++i) {
                                        const tile = attachmentRepeater.itemAt(i)
                                        if (tile && tile.popupOpen) return true
                                    }
                                    return false
                                }
                                objectName: "messageMember_"+modelData.id
                                // A message on its way out fades in where the reply box was.
                                transform: Translate { id: memberShift }
                                Component.onCompleted: if (modelData.is_outgoing && ["queued","preparing","in_flight"].indexOf(modelData.outgoing_state) >= 0) memberArrival.start()
                                ParallelAnimation {
                                    id: memberArrival
                                    NumberAnimation { target: member; property: "opacity"; from: 0; to: 1; duration: Theme.ms(200); easing.type: Easing.OutCubic }
                                    NumberAnimation { target: memberShift; property: "y"; from: 10; to: 0; duration: Theme.ms(200); easing.type: Easing.OutCubic }
                                }
                                implicitHeight: memberColumn.implicitHeight + (isExpanded ? 18 : 0)
                                // Messages are divided by a hairline, none above the first.
                                Rectangle {
                                    objectName: "messageRule_" + member.modelData.id
                                    visible: member.index > 0
                                    y: Theme.snapY(parent, 0); width: parent.width; height: Theme.ruleWidth; color: Theme.line
                                }
                                MessageSource { id: messageSource; message: member.modelData; active: member.isExpanded && member.visible; visible: false; onMetadataChanged: mail.recordMetadata(member.modelData.id, metadata) }
                                // A reply on its way to Gmail: a hairline along the bottom of the
                                // reply that fills as it uploads, sweeping until the size is known.
                                Item {
                                    id: sendTrack
                                    objectName: "messageSendProgress_" + member.modelData.id
                                    readonly property var share: backend.sendProgress[member.modelData.request_id || ""]
                                    visible: !!member.modelData.is_outgoing && ["queued","preparing","in_flight"].indexOf(member.modelData.outgoing_state) >= 0
                                    y: member.height - (member.isExpanded ? 12 : 2); width: memberColumn.width; height: 2; clip: true; z: 1
                                    Rectangle { anchors.fill: parent; radius: 1; color: Theme.inkAlpha(.06) }
                                    Rectangle {
                                        id: sendFill
                                        readonly property bool known: sendTrack.share !== undefined
                                        height: parent.height; radius: 1; color: Theme.accent
                                        width: known ? sendTrack.width * Math.max(.03, sendTrack.share) : sendTrack.width * .2
                                        onKnownChanged: if (known) x = 0
                                        Behavior on width { enabled: sendFill.known; NumberAnimation { duration: Theme.ms(140); easing.type: Theme.enterEasing } }
                                        NumberAnimation on x {
                                            running: sendTrack.visible && !sendFill.known; loops: Animation.Infinite
                                            from: -sendTrack.width * .2; to: sendTrack.width; duration: 1100; easing.type: Easing.InOutQuad
                                        }
                                    }
                                }
                                ColumnLayout {
                                id: memberColumn
                                width: parent.width; spacing: 0
                                ItemDelegate {
                                    id: memberHeading
                                    Layout.fillWidth: true; implicitHeight: 40; leftPadding: 0; rightPadding: 0; topPadding: 0; bottomPadding: 0
                                    objectName: "messageHeading_" + member.modelData.id
                                    focusPolicy: Qt.StrongFocus
                                    // One row whether the message is open or closed, so nothing
                                    // moves; a click anywhere on it but its controls toggles it.
                                    hoverEnabled: true
                                    Accessible.name: (member.isExpanded ? "Collapse message from " : "Expand message from ") + (member.modelData.sender_name || member.modelData.sender_email)
                                    onClicked: mail.toggleMember(member.modelData.id,member.index)
                                    background: Rectangle {
                                        objectName: "messageHover_" + member.modelData.id
                                        x: -12; width: memberHeading.width + 24; height: memberHeading.height; radius: 5
                                        color: memberHeading.hovered ? Theme.inkAlpha(.025) : "transparent"
                                        border.color: memberHeading.visualFocus ? Theme.accent : "transparent"
                                        // The caret sits in the left gutter and turns as the message opens.
                                        Item {
                                            objectName: "toggleMessage_" + member.modelData.id
                                            x: -12; width: 24; height: parent.height
                                            HoverHandler { id: caretHover; cursorShape: Qt.PointingHandCursor }
                                            TapHandler { onTapped: mail.toggleMember(member.modelData.id,member.index) }
                                            FmIcon {
                                                anchors.centerIn: parent
                                                name: "chevronDown"; size: 10; color: memberHeading.hovered || caretHover.hovered ? Theme.text : Theme.inkAlpha(.4)
                                                rotation: member.isExpanded ? 0 : -90
                                                Behavior on rotation { NumberAnimation { duration: Theme.ms(120); easing.type: Theme.enterEasing } }
                                            }
                                        }
                                    }
                                    contentItem: RowLayout {
                                        spacing: 10
                                        RowLayout {
                                            id: senderIdentity
                                            // The pill's 3px inset keeps the avatar flush with the text column.
                                            transform: Translate { x: -3 }
                                            Layout.fillWidth:false
                                            Layout.maximumWidth: Math.max(0, memberHeading.width - headerActions.implicitWidth - 20) * .6
                                            spacing: 6
                                            SenderPill {
                                                id: memberSender; objectName: "senderFilter_"+member.modelData.id
                                                avatarName: "senderAvatar_" + member.modelData.id; nameObjectName: "messageSenderName_" + member.modelData.id
                                                own: !!member.modelData.sent; open: member.isExpanded
                                                personName: member.modelData.sent ? "You" : member.modelData.sender_name || member.modelData.sender_email
                                                email: member.modelData.sender_email || ""; kind: member.modelData.kind || "unknown"
                                                Layout.fillWidth: true; Layout.minimumWidth: 0
                                            }
                                            FmIcon {
                                                objectName: "messageSenderStar_" + member.modelData.id
                                                visible: !member.modelData.sent && mail.senderStarred(member.modelData.sender_email)
                                                name: "contactStarFilled"; size: 12; color: Theme.amber
                                                HoverHandler { id: senderStarHover }
                                                FmToolTip { shown: senderStarHover.hovered; text: "Starred contact" }
                                            }
                                            FmButton {
                                                id: senderKind; objectName: "messageSenderKind_" + member.modelData.id
                                                visible: member.isExpanded && !member.modelData.sent && assistance.classificationEnabled && mail.kindKnown(member.modelData.kind)
                                                iconName: mail.kindIcon(member.modelData.kind); iconSize: 12; opacity: .8
                                                implicitWidth: 12; implicitHeight: 18; leftPadding: 0; rightPadding: 0; topPadding: 0; bottomPadding: 0
                                                tip: mail.kindTitle(member.modelData.kind) + " · correct sender kind"
                                                background: Item {}
                                                onClicked: messageAssistance.correctSender()
                                            }
                                        }
                                        Text {
                                            objectName: "messageInlineDetail_" + member.modelData.id
                                            visible: !member.isExpanded
                                            text: (member.modelData.text || "").replace(/\s+/g," ")
                                            Layout.fillWidth: true; Layout.minimumWidth: 0; elide: Text.ElideRight
                                            color: Theme.inkAlpha(.45); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12); textFormat: Text.PlainText
                                        }
                                        // The address takes only its own width, so the rest of the row toggles.
                                        CopyText {
                                            objectName: "messageSenderAddress_" + member.modelData.id
                                            visible: member.isExpanded
                                            text: member.modelData.sender_email || ""
                                            Layout.maximumWidth: implicitWidth; Layout.fillWidth: true; Layout.minimumWidth: 0; clip: true
                                            color: Theme.inkAlpha(.45); font.pointSize: Theme.fontPoints(12)
                                        }
                                        Item { visible: member.isExpanded; Layout.fillWidth: true }
                                        RowLayout {
                                            id: headerActions; Layout.fillWidth: false; spacing: 2
                                            FmIcon { visible: !member.isExpanded && member.attachmentCount > 0; name: "attachment"; size: 10; color: Theme.inkAlpha(.4); Layout.rightMargin: 8 }
                                            // Gmail accepted this reply; its own copy replaces it once synced.
                                            FmIcon {
                                                objectName: "messageSent_" + member.modelData.id
                                                visible: opacity > 0; opacity: member.modelData.is_outgoing && member.modelData.outgoing_state === "accepted" ? 1 : 0
                                                Behavior on opacity { NumberAnimation { duration: Theme.ms(120); easing.type: Theme.enterEasing } }
                                                name: "check"; size: 11; color: Theme.accent; Layout.rightMargin: 4
                                            }
                                            MessageHeaderButton {
                                                objectName: "messageFormatToggle_" + member.modelData.id
                                                visible: member.isExpanded && messageSource.hasHtml && messageSource.canChoose
                                                text: messageSource.plain ? "TEXT" : "HTML"
                                                tip: (messageSource.plain ? "Showing plain text — click to show HTML" : "Showing HTML — click to show plain text") + " for all mail from " + messageSource.sender
                                                onClicked: messageSource.setPlain(!messageSource.plain)
                                            }
                                            SummarizeAction {
                                                objectName: "summarizeMessage_" + member.modelData.id
                                                visible: member.isExpanded && messageAssistance.showSummarize
                                                enabled: messageAssistance.canSummarize; modelName: messageAssistance.modelName
                                                onClicked: messageAssistance.summarize()
                                            }
                                            MessageHeaderButton {
                                                id: memberMore; objectName: "messageMore_" + member.modelData.id; visible: member.isExpanded
                                                iconName: "more"; iconSize: 14; tip: "More"
                                                onClicked: {
                                                    mail.menuRows = backend.snapshotMailRows([member.modelData.is_outgoing ? member.modelData.id : backend.selectedId]); mail.rowMenu = true; mail.messageMenuSource = member.modelData.is_outgoing ? null : messageSource; mail.menuMember = member.modelData.is_outgoing ? null : member.modelData; if (mail.messageMenuSource) mail.messageMenuSource.loadRaw()
                                                    mail.messageMenuAssistance = messageAssistance
                                                    const point = memberMore.mapToItem(Overlay.overlay, width, height)
                                                    morePopup.x = Math.max(8, point.x - morePopup.width); morePopup.y = point.y + 6; morePopup.open()
                                                }
                                            }
                                            // The time keeps the far right, open or closed.
                                            Text {
                                                objectName: "messageShortDate_" + member.modelData.id
                                                text: mail.relativeTime(member.modelData.sent_at)
                                                color: Theme.inkAlpha(.5); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(11); font.features: {"tnum":1}
                                                Layout.leftMargin: 8
                                                FmToolTip { shown: memberDateHover.hovered; text: mail.fullTime(member.modelData.sent_at) }
                                                HoverHandler { id: memberDateHover }
                                            }
                                        }
                                    }
                                }
                                // Envelope: one quiet line per recipient field, names only;
                                // the address shows on hover and each name opens its card.
                                ColumnLayout {
                                    id: recipients
                                    readonly property var rows: mail.visibleMetadata(member.modelData, messageSource.metadata)
                                    visible: member.isExpanded && rows.length > 0; Layout.fillWidth: true; Layout.leftMargin: 28; Layout.topMargin: -6; Layout.bottomMargin: 2; spacing: 2
                                    Repeater {
                                        model: recipients.rows
                                        Flow {
                                            id: recipientLine
                                            required property var modelData
                                            Layout.fillWidth: true; spacing: 0
                                            Text { text: recipientLine.modelData.label.toLowerCase(); width: Math.max(18, implicitWidth) + 6; color: Theme.inkAlpha(.35); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12); textFormat: Text.PlainText }
                                            Repeater {
                                                model: recipientLine.modelData.people
                                                ContactName {
                                                    required property var modelData
                                                    required property int index
                                                    objectName: "recipientName_" + member.modelData.id + "_" + modelData.email
                                                    email: modelData.email
                                                    text: (modelData.name || modelData.email) + (index < recipientLine.modelData.people.length - 1 ? "," : "")
                                                    rightPadding: index < recipientLine.modelData.people.length - 1 ? 6 : 0
                                                    width: Math.min(implicitWidth, recipientLine.width)
                                                    underline: false; foreground: hovered ? Theme.text : Theme.inkAlpha(.55)
                                                    font.pointSize: Theme.fontPoints(12)
                                                }
                                            }
                                        }
                                    }
                                }
                                RowLayout {
                                    visible: member.isExpanded && !!member.modelData.text_truncated; Layout.fillWidth: true; Layout.topMargin: 6; spacing: 12
                                    Text { visible: !!member.modelData.text_truncated; text: "This preview is shortened."; color: Theme.subtle; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(11.5) }
                                }
                                MessageAssistance {
                                    id: messageAssistance
                                    Layout.fillWidth: true; Layout.topMargin: summaryShowing ? 12 : 0
                                    controller: assistance; message: member.modelData
                                    showKindControl: false; label: ((mail.thread.categories || [])[0] || {}).name || ""
                                    visible: member.isExpanded && !member.modelData.sent && (assistance.summariesEnabled || (assistance.classificationEnabled && !member.modelData.sent) || !!summary || !!classification)
                                }
                                Text { visible: member.isExpanded && !!member.modelData.content_error; Layout.fillWidth: true; Layout.topMargin: 10; text: member.modelData.content_error || ""; color: Theme.error; wrapMode: Text.Wrap; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12) }
                                ColumnLayout {
                                    id: messageBody
                                    opacity: visible ? 1 : 0
                                    Behavior on opacity { NumberAnimation {duration: Theme.ms(140); easing.type: Theme.enterEasing} }
                                    visible: member.isExpanded && !member.modelData.content_error
                                    // 16px after a summary or its progress, 10px after the recipients line, else 2px.
                                    Layout.fillWidth: true; Layout.topMargin: messageAssistance.visible && messageAssistance.summaryShowing && !messageAssistance.summaryFailed ? 16 : recipients.visible ? 10 : 2
                                    spacing: 7
                                    property var rows: []
                                    readonly property string plainText: member.modelData.content_pending ? "Loading sent message…" : member.modelData.text || "(No text)"
                                    // The source's text is this build's reading of the message; the
                                    // stored copy may come from an older one. The source can arrive
                                    // while a projection lays out, so it projects again afterwards.
                                    readonly property string sourceText: member.modelData.content_pending ? "" : messageSource.result.body_text || ""
                                    onSourceTextChanged: reproject.restart()
                                    // Not Qt.callLater: a timer goes with this message's view,
                                    // so a projection put off as the view closes never runs.
                                    Timer { id: reproject; interval: 0; onTriggered: messageBody.project() }
                                    // Quoted reply history the reader leaves out, shown on request:
                                    // the whole text from the source, or the engine's HTML with it kept.
                                    property bool showQuotes: false
                                    readonly property string historyText: member.modelData.content_pending ? "" : messageSource.result.body_full || ""
                                    onHistoryTextChanged: reproject.restart()
                                    readonly property bool historyHidden: htmlShown ? !!htmlBody.item.result.quote_removed : !!historyText
                                    // Quoted lines the earlier messages here do not have, highlighted
                                    // as answers written inside the quote.
                                    readonly property var quoteAdditions: showQuotes && historyText
                                        ? backend.quoteAdditions(historyText, mail.members.slice(0, member.index).reverse().filter(function(m) { return !m.content_pending }).map(function(m) { return m.text || "" }))
                                        : []
                                    onQuoteAdditionsChanged: reproject.restart()
                                    readonly property string projectionKey: messageSource.identity + "/" + messageSource.plain + "/" + messageSource.hasHtml + "/" + member.isExpanded + "/" + showQuotes
                                    function project() {
                                        if (!member.isExpanded) { rows = []; return }
                                        if (showQuotes && historyText) { rows = backend.historyBody(historyText, quoteAdditions); return }
                                        const prepared = messageSource.plain ? member.modelData.reader_plain_sections : member.modelData.reader_sections
                                        rows = (prepared || backend.receivedBody(messageSource.hasHtml && !messageSource.plain ? messageSource.result.body_html : "", sourceText || plainText)).filter(function(row) { return !row.quote && !(row.parents || []).length })
                                    }
                                    onPlainTextChanged: project()
                                    onProjectionKeyChanged: project()
                                    Component.onCompleted: project()
                                    // HTML mode renders through the HTML reader; its failure falls back to text.
                                    readonly property bool htmlMode: messageSource.hasHtml && !messageSource.plain && !!backend.htmlContent
                                    readonly property bool htmlShown: htmlMode && htmlBody.item !== null && !htmlBody.item.failed
                                    // Mailing list (latest received message only) and hidden images; 16px above the text.
                                    MessageNotices {
                                        Layout.fillWidth: true; Layout.bottomMargin: visible ? 9 : 0
                                        message: member.modelData
                                        hasInvite: !member.modelData.sent && !!messageSource.result.has_invite
                                        onOpenInvite: function(event, declined) {
                                            if (declined && backend.preferences.calendar_show_declined !== true) backend.setPreferences({calendar_show_declined: true})
                                            mail.showEventInCalendar(event)
                                        }
                                        mailingList: member.isLast && !member.modelData.sent ? mail.thread.mailing_list || null : null
                                        unsubscribe: member.isLast && !member.modelData.sent ? messageSource.result.unsubscribe || null : null
                                        htmlBody: messageBody.htmlShown ? htmlBody.item : null
                                    }
                                    Loader {
                                        id: htmlBody
                                        active: messageBody.htmlMode && member.isExpanded
                                        visible: messageBody.htmlShown; Layout.fillWidth: true
                                        sourceComponent: HtmlMessageBody {
                                            message: member.modelData; account: backend.account; conversation: backend.selectedId
                                            quotes: messageBody.showQuotes
                                            addedPhrases: messageBody.quoteAdditions
                                            matchPhrases: mail.bodyPhrases
                                            active: member.isExpanded && member.visible
                                            onLinkHovered: function(href, box) { const p = mapToItem(Overlay.overlay, box.x, box.y); linkCard.hover(href, Qt.rect(p.x, p.y, box.width, box.height), backend.selectedId) }
                                            onLinkExited: linkCard.scheduleHide()
                                        }
                                    }
                                    Text {
                                        visible: messageBody.htmlMode && htmlBody.item !== null && htmlBody.item.failed && !!htmlBody.item.failure
                                        Layout.fillWidth: true; wrapMode: Text.Wrap; textFormat: Text.PlainText
                                        text: htmlBody.item ? htmlBody.item.failure || "" : ""
                                        color: Theme.inkAlpha(.5); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(11.5)
                                    }
                                    Repeater {
                                        model: messageBody.htmlShown ? [] : messageBody.rows
                                        TextArea {
                                            id: bodyText
                                            required property var modelData
                                            required property int index
                                            // A forward's rows: its label, header block, text, and the
                                            // history it quotes, one indent and bar per level.
                                            readonly property string kind: modelData.kind || ""
                                            readonly property int level: modelData.depth || 0
                                            // A quoted line the original does not have: an answer inside the quote.
                                            readonly property bool added: !!modelData.added
                                            objectName: index === 0 ? "messageBody_" + member.modelData.id : "messageBodyPart_" + member.modelData.id + "_" + index
                                            // Search matches tinted in the document.
                                            readonly property var matchKey: [mail.bodyPhrases, Theme.matchTint]
                                            onMatchKeyChanged: backend.markDocument(bodyText, mail.bodyPhrases, Theme.matchTint)
                                            Component.onCompleted: { backend.drawWholeText(bodyText); backend.markDocument(bodyText, mail.bodyPhrases, Theme.matchTint) }
                                            onTextChanged: backend.drawWholeText(bodyText)
                                            Layout.fillWidth: true; text: mail.presentLinks(modelData.html)
                                            readOnly: true; selectByMouse: true; wrapMode: TextEdit.Wrap; textFormat: TextEdit.RichText
                                            palette.link: Theme.text
                                            color: kind === "forward" ? Theme.inkAlpha(.5) : kind === "headers" ? Theme.inkAlpha(.72)
                                                : added ? Theme.text : kind === "quote" || kind === "attribution" ? Theme.inkAlpha(.55) : member.isLast ? Theme.text : Theme.inkAlpha(.78)
                                            font.family: Theme.fontFamily
                                            font.pointSize: Theme.fontPoints(kind === "forward" ? 10.5 : kind === "headers" || kind === "attribution" ? 12 : kind === "quote" ? 13 : 14)
                                            font.capitalization: kind === "forward" ? Font.AllUppercase : Font.MixedCase
                                            font.letterSpacing: kind === "forward" ? 0.6 : 0
                                            leftPadding: level * 16; rightPadding: 0
                                            topPadding: kind === "forward" && index > 0 ? 14 : 0
                                            bottomPadding: kind === "headers" ? 10 : kind === "quote" ? 12 : kind === "attribution" ? 6 : 0
                                            background: null
                                            // An added line sits on an accent tint, its bar in the accent.
                                            Rectangle {
                                                visible: bodyText.added; z: -1; radius: 3; color: Theme.accentAlpha(.1)
                                                x: bodyText.level * 16 - 6; y: -3; width: bodyText.width - x; height: bodyText.height - bodyText.bottomPadding + 6
                                            }
                                            // A rule above the label sets the forward apart from the note before it.
                                            Rectangle { visible: bodyText.kind === "forward" && bodyText.index > 0; width: parent.width; height: 1; color: Theme.inkAlpha(.1) }
                                            // A level's bar runs on into the next row while the quote does.
                                            readonly property var following: messageBody.rows[index + 1] || ({})
                                            Repeater {
                                                model: bodyText.kind === "quote" ? bodyText.level : 0
                                                Rectangle {
                                                    required property int index
                                                    readonly property bool continues: bodyText.following.kind === "quote" && bodyText.following.depth > index
                                                    x: index * 16; width: 2; radius: continues ? 0 : 1; color: bodyText.added && index === bodyText.level - 1 ? Theme.accentAlpha(.7) : Theme.inkAlpha(.16)
                                                    height: continues ? bodyText.height + messageBody.spacing : bodyText.height - bodyText.bottomPadding
                                                }
                                            }
                                            Accessible.name: "Message from " + member.modelData.sender_name
                                            onLinkActivated: function(link) { mail.openBodyLink(link) }
                                            // The app's own text menu, not Qt's.
                                            ContextMenu.menu: null
                                            TapHandler { acceptedButtons: Qt.RightButton; onTapped: function(point) { textMenu.show(bodyText, point.position.x, point.position.y) } }
                                            HoverHandler { id: bodyHover }
                                            onHoveredLinkChanged: {
                                                if (!hoveredLink) { linkCard.scheduleHide(); return }
                                                const r = positionToRectangle(positionAt(bodyHover.point.position.x, bodyHover.point.position.y))
                                                const p = mapToItem(Overlay.overlay, r.x, r.y)
                                                linkCard.hover(hoveredLink, Qt.rect(p.x, p.y, Math.max(1, r.width), r.height), backend.selectedId)
                                            }
                                        }
                                    }
                                    // "···" under the message: shows or hides the quoted history left
                                    // out above, for replies written inside the quote.
                                    AbstractButton {
                                        id: quotesToggle
                                        objectName: "messageQuotesToggle_" + member.modelData.id
                                        visible: messageBody.showQuotes || messageBody.historyHidden
                                        Layout.topMargin: 3
                                        implicitWidth: 28; implicitHeight: 14; padding: 0; hoverEnabled: true
                                        Accessible.name: messageBody.showQuotes ? "Hide quoted text" : "Show quoted text"
                                        onClicked: messageBody.showQuotes = !messageBody.showQuotes
                                        background: Rectangle {
                                            radius: 4
                                            color: quotesToggle.hovered || messageBody.showQuotes ? Theme.inkAlpha(.14) : Theme.inkAlpha(.07)
                                            Behavior on color { ColorAnimation { duration: Theme.ms(120); easing.type: Theme.enterEasing } }
                                            border.width: quotesToggle.visualFocus ? 1 : 0; border.color: Theme.focusBorder
                                        }
                                        contentItem: Item { FmIcon { anchors.centerIn: parent; name: "more"; size: 14; color: quotesToggle.hovered ? Theme.text : Theme.inkAlpha(.55) } }
                                        FmToolTip { shown: quotesToggle.hovered; text: quotesToggle.Accessible.name }
                                    }
                                }
                                // "4 ATTACHMENTS", a hairline to the edge and a chevron, 24px above
                                // the cards. Like a month header, a click folds the cards away.
                                AbstractButton {
                                    id: attachmentsHeading
                                    objectName: "messageAttachmentsCaption_" + member.modelData.id
                                    readonly property bool collapsed: mail.attachmentsCollapsed(member.modelData.id)
                                    readonly property color tone: hovered ? Theme.text : Theme.inkAlpha(.4)
                                    visible: member.isExpanded && member.attachmentCount > 0; Layout.fillWidth: true; Layout.topMargin: 24; Layout.bottomMargin: collapsed ? 0 : 10
                                    implicitHeight: 20; hoverEnabled: true; padding: 0
                                    Accessible.name: (member.attachmentCount === 1 ? "1 attachment, " : member.attachmentCount + " attachments, ") + (collapsed ? "collapsed" : "expanded")
                                    onClicked: mail.toggleAttachments(member.modelData.id)
                                    background: Item {}
                                    contentItem: RowLayout {
                                        spacing: 10
                                        Text { text: (member.attachmentCount === 1 ? "1 attachment" : member.attachmentCount + " attachments").toUpperCase(); color: attachmentsHeading.tone; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(10.5); font.letterSpacing: .55 }
                                        Rectangle { Layout.fillWidth: true; implicitHeight: 1; color: Theme.line }
                                        FmIcon { name: "down"; size: 10; color: attachmentsHeading.tone; rotation: attachmentsHeading.collapsed ? -90 : 0; Behavior on rotation { NumberAnimation { duration: Theme.ms(180); easing.type: Theme.enterEasing } } }
                                    }
                                }
                                Flow {
                                    id: attachmentFlow
                                    objectName: "messageAttachments_" + member.modelData.id
                                    opacity: visible ? 1 : 0
                                    Behavior on opacity { NumberAnimation {duration: Theme.ms(140); easing.type: Theme.enterEasing} }
                                    visible: member.isExpanded && member.attachmentCount > 0 && !attachmentsHeading.collapsed; Layout.fillWidth: true; Layout.maximumWidth: 798; spacing: 10
                                    Repeater {
                                        id: attachmentRepeater
                                        model: member.modelData.attachments||[]
                                        AttachmentTile {
                                            required property var modelData
                                            attachment: modelData; account: backend.account
                                            // A server-only attachment is fetched when opened, not to preview.
                                            // Re-checked on scrolling and on any layout change above, so a card
                                            // placed before the thread settled still loads once it is on screen.
                                            previewActive: { const scroll=reader.contentY, extent=reader.contentHeight, frame=reader.height, placed=y+attachmentFlow.y+memberLoader.y; const pos=mapToItem(reader,0,0);return mail.visible && member.isExpanded && attachmentFlow.visible && modelData.state !== "remote" && pos.y+height>0 && pos.y<reader.height }
                                            maximumWidth: (attachmentFlow.width - (Math.max(1,Math.floor((attachmentFlow.width+10)/210))-1)*10)/Math.max(1,Math.floor((attachmentFlow.width+10)/210))
                                            onOpenMedia: function(attachment) { mail.showAttachment(attachment) }
                                            // A message still on its way to Gmail previews the copies it was sent from.
                                            available: modelData.state === "available" || modelData.state === "remote"
                                        }
                                    }
                                }
                                }
                                }
                            }
                        }
                        Item {
                            id: readerActions; objectName: "conversationActionsRow"
                            visible: mail.members.length > 0 && !mail.thread.is_outgoing && !mail.thread.remote && !mail.binMode
                            Layout.fillWidth: true; Layout.bottomMargin: visible ? 8 : 0
                            implicitHeight: readerButtons.implicitHeight + 25
                            Rectangle { width: parent.width; height: 1; color: Theme.inkAlpha(.07) }
                            Flow {
                                id: readerButtons; y: 25; width: parent.width; spacing: 6
                                ReaderAction { objectName: "replyButton"; text: "Reply"; iconName: "reply"; prominent: mail.composingMode === "" || mail.composingMode === "reply"; outlined: !prominent; visible: backend.folder !== "sent"; enabled: mail.newMessageAvailable; onClicked: mail.compose("reply") }
                                ReaderAction { objectName: "replyAllButton"; visible: mail.replyAllAvailable && backend.folder !== "sent"; text: "Reply all"; iconName: "replyAll"; prominent: mail.composingMode === "reply_all"; outlined: !prominent; enabled: mail.newMessageAvailable; onClicked: mail.compose("reply_all") }
                                ReaderAction { objectName: "forwardButton"; text: "Forward"; iconName: "forward"; prominent: mail.composingMode === "forward"; outlined: !prominent; enabled: mail.newMessageAvailable; onClicked: mail.compose("forward") }
                                Item { width: 13; height: 32; Rectangle { x: 6; y: 6; width: 1; height: 20; color: Theme.line2 } }
                                ReaderAction { objectName: "delayConversationButton"; visible: !mail.anyStarred && backend.folder !== "sent"; text: "Delay"; iconName: "clock"; outlined: true; enabled: !backend.mutationInFlight && mail.canMutate; tip: "Hold this conversation for later · D"; onClicked: backend.setStarred(true) }
                                ReaderAction { objectName: "doneConversationButton"; visible: mail.anyStarred && backend.folder !== "sent"; text: "Done"; iconName: "check"; outlined: true; enabled: !backend.mutationInFlight && mail.canMutate; tip: "Clear unread and stars · E"; onClicked: backend.doneConversation() }
                                Item { visible: summarizeThread.visible; width: 13; height: 32; Rectangle { x: 6; y: 6; width: 1; height: 20; color: Theme.line2 } }
                                FmButton {
                                    id: summarizeThread; objectName: "summarizeConversation"
                                    readonly property bool open: mail.threadSummaryOpen || mail.threadSummaryBusy
                                    visible: mail.members.length > 1 && assistance.summariesEnabled
                                    enabled: mail.threadSummaryBusy || ((!assistance.busy || !!mail.threadSummaryResult) && backend.connected)
                                    iconName: "summary"; iconSize: 14; implicitWidth: 32; implicitHeight: 32
                                    leftPadding: 0; rightPadding: 0; topPadding: 0; bottomPadding: 0
                                    foreground: open || hovered ? Theme.ai : Theme.inkAlpha(.7); hoverBrightness: 1
                                    tip: mail.threadSummaryBusy ? "Summarizing the thread on this machine" : !mail.threadSummaryResult ? "Summarize the whole thread" : mail.threadSummaryOpen ? "Hide the thread summary" : "Show the thread summary"
                                    background: Rectangle {
                                        radius: 5; color: summarizeThread.open ? Theme.withAlpha(Theme.ai, .12) : summarizeThread.hovered ? Theme.inkAlpha(.05) : "transparent"
                                        border.color: summarizeThread.visualFocus ? Theme.focusBorder : summarizeThread.open ? Theme.withAlpha(Theme.ai, .45) : Theme.line2
                                    }
                                    onClicked: mail.toggleThreadSummary()
                                }
                            }
                        }
                        // The whole thread's summary, written on this machine on request.
                        SummaryCard {
                            id: threadSummary; objectName: "threadSummary"
                            visible: readerActions.visible && showing && (busy || failed || mail.threadSummaryOpen)
                            Layout.fillWidth: true; Layout.bottomMargin: visible ? 16 : 0
                            title: "Thread summary"; label: ((mail.thread.categories || [])[0] || {}).name || ""
                            stages: ["Reading " + mail.members.length + " messages…", "Summarizing the thread…"]
                            messageId: backend.selectedId
                            modelName: (assistance.settings.model || {}).name || ""
                            result: mail.threadSummaryResult; members: mail.members
                            busy: mail.threadSummaryBusy
                            error: mail.threadSummaryOpen ? assistance.errorFor("thread", backend.selectedId) : ""
                            canSummarize: assistance.summariesEnabled && !assistance.busy && backend.connected
                            closable: true
                            onRegenerate: assistance.start("thread", "")
                            onCancelRequested: assistance.cancel()
                            onCloseRequested: mail.showThreadSummary(false)
                            onOpenSource: messageId => assistance.openSource(messageId)
                        }
                        // Its line stays reserved under the buttons, so a failure
                        // arriving later never moves the thread.
                        Text {
                            objectName: "readerNote"
                            visible: mail.members.length > 0 && !mail.binMode && (readerActions.visible || text.length > 0)
                            Layout.fillWidth: true; Layout.preferredHeight: Math.ceil(Theme.fontSize(11.5) * 1.5); Layout.bottomMargin: visible ? 8 : 0
                            text: mail.readerNote
                            color: text === mail.failureNote ? Theme.error : Theme.subtle
                            font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(11.5)
                            elide: Text.ElideRight; textFormat: Text.PlainText
                            FmToolTip { shown: noteHover.hovered && parent.truncated; text: parent.text }
                            HoverHandler { id: noteHover }
                        }
                        // Spam and Trash replace Reply, Forward and Delay.
                        Item {
                            id: binActions; objectName: "binActionsRow"
                            visible: mail.binMode && mail.members.length > 0 && backend.selectedId === mail.thread.id
                            Layout.fillWidth: true; Layout.bottomMargin: visible ? 8 : 0
                            implicitHeight: visible ? binButtons.implicitHeight + 25 : 0
                            Rectangle { width: parent.width; height: 1; color: Theme.inkAlpha(.07) }
                            Flow {
                                id: binButtons; y: 25; width: parent.width; spacing: 6
                                ReaderAction {
                                    objectName: mail.spamMode ? "notSpamButton" : "restoreButton"
                                    text: mail.spamMode ? "Move to Inbox" : "Restore"; iconName: mail.spamMode ? "inbox" : "restore"; prominent: true
                                    tip: mail.spamMode ? "Not spam · move to Inbox and trust this sender" : "Move back to Inbox"
                                    enabled: mail.spamMode ? !backend.binBusy : !backend.mutationInFlight && mail.canMutate
                                    onClicked: mail.spamMode ? backend.notSpam() : backend.fileConversation("restore")
                                }
                                ReaderAction {
                                    id: purgeButton; objectName: "deleteForeverButton"
                                    text: "Delete permanently"; iconName: "bin"; enabled: !backend.binBusy
                                    foreground: Theme.error
                                    background: Rectangle { radius: 5; color: purgeButton.hovered ? Theme.withAlpha(Theme.error, .08) : "transparent"; border.width: 1; border.color: purgeButton.visualFocus ? Theme.focusBorder : Theme.withAlpha(Theme.error, .4) }
                                    onClicked: binConfirm.ask("delete")
                                }
                            }
                        }
                        ReplyComposer {
                            id: reply
                            objectName: "replyPanel"
                            Layout.fillWidth: true; Layout.topMargin: visible ? (mail.members.length ? 12 : 24) : 0
                            visible: mail.replyOpen
                            // Opening, it fades in and settles 10px up over 200 ms.
                            transform: Translate { id: replyShift }
                            onVisibleChanged: if (visible) replyOpening.restart()
                            ParallelAnimation {
                                id: replyOpening
                                NumberAnimation { target: reply; property: "opacity"; from: 0; to: 1; duration: Theme.ms(200); easing.type: Easing.OutCubic }
                                NumberAnimation { target: replyShift; property: "y"; from: 10; to: 0; duration: Theme.ms(200); easing.type: Easing.OutCubic }
                            }
                            minimumHeight: mode === "new" && !readOnly ? Math.max(420, (mail.ApplicationWindow.window ? mail.ApplicationWindow.window.height : readerPane.height) * 0.5) : 0
                            // The whole form fits in the reader, with its 12 px above and 24 below.
                            availableHeight: reader.height - 36
                            onHeightChanged: mail.keepReplyInView(true)
                            onDismissed: mail.setReplyOpen(false)
                            onRecentOpened: function(id) { mail.leaveComposerFor(function() { backend.openConversation(id) }) }
                            onAllMailWith: function(email) { if (email) backend.query = email }
                        }
                    }
                    Column {
                        id: emptyReader
                        objectName: "emptyReader"
                        readonly property bool clear: backend.folder === "inbox" && !mail.filtering && backend.accounts.length > 0 && ((backend.currentAccount.inbox_review_count ?? backend.currentAccount.unread_conversation_count) || 0) === 0 && ((backend.currentAccount.inbox_held_count ?? backend.currentAccount.held_conversation_count) || 0) === 0
                        readonly property var nextEvent: (backend.todayEvents || []).find(function(e) { return !e.all_day && e.start_hour > backend.nowHour }) || null
                        readonly property bool inboxClear: clear && !mail.firstLoading
                        // Clearing the inbox through work gets one small moment
                        // when the clear reader first shows: the check lands
                        // with a little overshoot, a ring spreads from it and
                        // the lines follow. Opening an inbox that was already
                        // clear (at launch, on switching account) shows it still.
                        readonly property bool waiting: backend.folder === "inbox" && !mail.filtering && backend.accounts.length > 0 && !mail.firstLoading && !clear
                        readonly property bool showingClear: visible && inboxClear
                        property bool armed: false
                        onWaitingChanged: if (waiting) armed = true
                        onShowingClearChanged: {
                            if (!showingClear || !armed) return
                            armed = false
                            if (!Theme.reducedMotion) celebration.restart()
                        }
                        Connections {
                            target: backend
                            function onAccountChanged() { emptyReader.armed = false; celebration.complete(); Qt.callLater(function() { emptyReader.armed = emptyReader.waiting }) }
                        }
                        SequentialAnimation {
                            id: celebration
                            PropertyAction { target: clearCheck; property: "scale"; value: .4 }
                            PropertyAction { target: clearCheck; property: "rotation"; value: -25 }
                            PropertyAction { target: clearCheck; property: "opacity"; value: 0 }
                            PropertyAction { targets: [clearTitle, clearLine, nextEventRow]; property: "opacity"; value: 0 }
                            PropertyAction { targets: [clearTitleShift, clearLineShift, nextEventShift]; property: "y"; value: 8 }
                            PropertyAction { target: clearRing; property: "opacity"; value: 0 }
                            ParallelAnimation {
                                NumberAnimation { target: clearCheck; property: "opacity"; to: 1; duration: Theme.quickMs; easing.type: Theme.enterEasing }
                                NumberAnimation { target: clearCheck; property: "scale"; to: 1; duration: Theme.emphasisMs; easing.type: Easing.OutBack; easing.overshoot: 2.2 }
                                NumberAnimation { target: clearCheck; property: "rotation"; to: 0; duration: Theme.emphasisMs; easing.type: Theme.enterEasing }
                                // The ring answers the check's landing, and is gone before it matters.
                                SequentialAnimation {
                                    PauseAnimation { duration: Theme.quickMs }
                                    PropertyAction { target: clearRing; property: "scale"; value: 1 }
                                    PropertyAction { target: clearRing; property: "opacity"; value: .55 }
                                    ParallelAnimation {
                                        NumberAnimation { target: clearRing; property: "scale"; to: 2.6; duration: 560; easing.type: Theme.enterEasing }
                                        NumberAnimation { target: clearRing; property: "opacity"; to: 0; duration: 560; easing.type: Theme.enterEasing }
                                    }
                                }
                                // Each line starts before the one above has settled.
                                SequentialAnimation {
                                    PauseAnimation { duration: 80 }
                                    ParallelAnimation {
                                        NumberAnimation { target: clearTitle; property: "opacity"; to: 1; duration: Theme.glideMs; easing.type: Theme.enterEasing }
                                        NumberAnimation { target: clearTitleShift; property: "y"; to: 0; duration: Theme.glideMs; easing.type: Theme.enterEasing }
                                    }
                                }
                                SequentialAnimation {
                                    PauseAnimation { duration: 150 }
                                    ParallelAnimation {
                                        NumberAnimation { target: clearLine; property: "opacity"; to: 1; duration: Theme.glideMs; easing.type: Theme.enterEasing }
                                        NumberAnimation { target: clearLineShift; property: "y"; to: 0; duration: Theme.glideMs; easing.type: Theme.enterEasing }
                                    }
                                }
                                SequentialAnimation {
                                    PauseAnimation { duration: 220 }
                                    ParallelAnimation {
                                        NumberAnimation { target: nextEventRow; property: "opacity"; to: 1; duration: Theme.glideMs; easing.type: Theme.enterEasing }
                                        NumberAnimation { target: nextEventShift; property: "y"; to: 0; duration: Theme.glideMs; easing.type: Theme.enterEasing }
                                    }
                                }
                            }
                        }
                        visible: mail.members.length===0 && !mail.replyOpen && !mail.readerLoading; x: readerPane.margin; y: 8 + Math.round((mail.ApplicationWindow.window ? mail.ApplicationWindow.window.height : readerPane.height) * (inboxClear ? 0.16 : 0.18)); width: Math.min(inboxClear ? 520 : 640, parent.width-readerPane.margin*2); spacing: 0
                        // Inbox clear: a small check, the title, one line and today's next event.
                        RowLayout {
                            visible: emptyReader.inboxClear
                            spacing: 10
                            FmIcon {
                                id: clearCheck; objectName: "inboxClearCheck"
                                name: "checkCircle"; size: 18; color: Theme.accent
                                Rectangle { id: clearRing; anchors.centerIn: parent; width: 16; height: 16; radius: 8; color: "transparent"; border.width: 1.5; border.color: Theme.accent; opacity: 0 }
                            }
                            Text { id: clearTitle; transform: Translate { id: clearTitleShift } objectName: "inboxClearTitle"; text: "Inbox clear"; color: Theme.text; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(19); font.weight: Font.Medium; font.letterSpacing: 0}
                        }
                        Text {
                            id: clearLine; transform: Translate { id: clearLineShift }
                            visible: emptyReader.inboxClear
                            leftPadding: 28; topPadding: 10; width: parent.width; wrapMode: Text.Wrap; textFormat: Text.PlainText
                            text: "Nothing to review or delayed in " + (backend.currentAccount.name || "this account") + "."
                            color: Theme.inkAlpha(.5); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12.5)
                            lineHeightMode: Text.FixedHeight; lineHeight: Math.round(Theme.fontSize(12.5) * 1.55)
                        }
                        ItemDelegate {
                            id: nextEventRow
                            objectName: "inboxClearNext"
                            transform: Translate { id: nextEventShift }
                            visible: emptyReader.inboxClear && !!emptyReader.nextEvent
                            x: 28; width: parent.width - 28; implicitHeight: 40; topPadding: 12; bottomPadding: 4; leftPadding: 0; rightPadding: 0
                            hoverEnabled: true
                            background: Rectangle { color: "transparent"; Rectangle { width: parent.width; height: Theme.ruleWidth; color: Theme.line } }
                            FmToolTip { shown: nextEventRow.hovered; text: "Open in Calendar" }
                            onClicked: if (emptyReader.nextEvent) mail.showEventInCalendar(emptyReader.nextEvent)
                            contentItem: RowLayout {
                                spacing: 10
                                Text { text: "NEXT"; color: Theme.inkAlpha(.4); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(10.5); font.letterSpacing: 0.66 }
                                Text { text: emptyReader.nextEvent ? emptyReader.nextEvent.time_display || "" : ""; color: Theme.accent; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12); font.features: {"tnum": 1} }
                                Text { Layout.fillWidth: true; Layout.maximumWidth: implicitWidth; text: emptyReader.nextEvent ? String(emptyReader.nextEvent.title || "") : ""; textFormat: Text.PlainText; elide: Text.ElideRight; color: nextEventRow.hovered ? Theme.accent : Theme.text; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12); Behavior on color { ColorAnimation { duration: Theme.ms(150) } } }
                                FmIcon { name: "next"; size: 10; color: Theme.inkAlpha(.4) }
                                Item { Layout.fillWidth: true }
                            }
                        }
                        Text {
                            objectName: "emptyReaderTitle"
                            visible: !emptyReader.inboxClear
                            text: mail.firstLoading ? "Loading…" : mail.draftsMode ? "Nothing open." : backend.folder === "sent" ? "Nothing here." : "Nothing open."
                            color: Theme.text; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(30); font.weight: Font.Medium; font.letterSpacing: 0; width: parent.width; wrapMode: Text.Wrap
                        }
                        Item { visible: !emptyReader.inboxClear; width: 1; height: 12 }
                        Text {
                            visible: !mail.firstLoading && !emptyReader.inboxClear
                            text: mail.draftsMode ? "Pick a draft on the left to keep writing, or press <font color=\"" + Theme.text + "\">" + ShortcutRegistry.label(ShortcutRegistry.sequence("compose", backend.preferences)).toLowerCase() + "</font> for a new message."
                                : backend.folder === "sent" ? "No sent mail from " + (backend.currentAccount.name || "this account") + (mail.filtering ? " matches. Clear the filter or pick another day." : " is open. Pick a message on the left.")
                                : "Pick a conversation on the left. Mail stays in " + (backend.currentAccount.name || "the selected account") + "; your calendar remains in view below."
                            textFormat: Text.StyledText
                            color: Theme.inkAlpha(.55); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(14); width: parent.width; wrapMode: Text.Wrap
                            lineHeightMode: Text.FixedHeight; lineHeight: Math.round(Theme.fontSize(14) * 1.6)
                        }
                    }
                }
            }
        }
    }
    // Unsent drafts of this account as one row of chips, bottom right, above
    // the day strip. Hidden while a composer is open.
    Row {
        id: draftChipRow
        objectName: "draftChips"
        visible: mail.chipsVisible
        anchors { right: parent.right; bottom: parent.bottom; rightMargin: 16; bottomMargin: 12 }
        spacing: 6; layoutDirection: Qt.RightToLeft
        width: Math.min(implicitWidth, Math.min(760, mail.width * 0.62))
        z: 20
        FmButton {
            objectName: "draftChipsMore"
            visible: mail.chipRows.length > 4
            text: "+" + (mail.chipRows.length - 4)
            implicitHeight: 28; leftPadding: 8; rightPadding: 8; topPadding: 0; bottomPadding: 0
            font.pointSize: Theme.fontPoints(11.5); foreground: hovered ? Theme.text : Theme.inkAlpha(.5); background: Item {}
            tip: "All drafts"
            onClicked: mail.leaveComposerFor(function() { backend.folder = "drafts" }, true)
        }
        Repeater {
            model: mail.chipRows.slice(0, 4).reverse()
            AbstractButton {
                id: chip
                required property var modelData
                objectName: "draftChip_" + modelData.draft_id
                hoverEnabled: true
                implicitHeight: 26; leftPadding: 9; rightPadding: 10; topPadding: 0; bottomPadding: 0
                implicitWidth: Math.min(240, leftPadding + 11 + 7 + chipText.implicitWidth + rightPadding)
                anchors.verticalCenter: parent ? parent.verticalCenter : undefined
                Accessible.name: "Draft: " + mail.chipSubject(modelData)
                FmToolTip { shown: chip.hovered; text: mail.chipTip(chip.modelData) }
                onClicked: mail.openDraftChip(modelData)
                background: Rectangle { radius: 5; color: chip.hovered ? Theme.bg4 : Theme.canvasAlpha(.85); border.width: chip.visualFocus ? 1 : 0; border.color: Theme.focusBorder; Behavior on color { ColorAnimation { duration: Theme.ms(150) } } }
                contentItem: RowLayout {
                    spacing: 7
                    FmIcon { name: chip.modelData.mode !== "new" && chip.modelData.conversation_id ? "reply" : "edit"; size: 11; color: Theme.accent }
                    Text { id: chipText; Layout.fillWidth: true; text: mail.chipSubject(chip.modelData); textFormat: Text.PlainText; elide: Text.ElideRight; color: chip.hovered ? Theme.text : Theme.inkAlpha(.7); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(11.5); Behavior on color { ColorAnimation { duration: Theme.ms(150) } } }
                }
            }
        }
    }
    // Spam and Trash, behind the ▾ after Drafts.
    Popup {
        id: binMenu; objectName: "binMenu"
        // Eases in from just under full size; closes at once.
        transformOrigin: Popup.TopLeft
        enter: Transition { ParallelAnimation { NumberAnimation { property: "opacity"; from: 0; to: 1; duration: Theme.ms(Theme.quickMs); easing.type: Theme.enterEasing } NumberAnimation { property: "scale"; from: 0.97; to: 1; duration: Theme.ms(Theme.quickMs); easing.type: Theme.enterEasing } } }
        parent: Overlay.overlay; padding: 4; focus: true
        width: Math.max(150, binChoices.implicitWidth + 8)
        // Any press outside closes it, and so does moving to another mailbox
        // or filter by any means. A press on its own button closes it too,
        // so that button's click must not open it again.
        closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
        property double closedAt: 0
        onAboutToHide: closedAt = Date.now()
        Connections { target: backend; function onQueryChanged() { binMenu.close() } }
        onAboutToShow: { const point = binMenuButton.mapToItem(parent, 0, binMenuButton.height + 6); x = point.x; y = point.y; backend.loadBinMenuCounts() }
        background: Rectangle { color: Theme.bg2; border.color: Theme.line2; radius: 8 }
        contentItem: Column {
            id: binChoices
            Repeater {
                model: [{folder: "spam", name: "Spam", icon: "shield"}, {folder: "trash", name: "Trash", icon: "bin"}]
                ItemDelegate {
                    id: binChoice
                    required property var modelData
                    readonly property bool current: backend.folder === modelData.folder
                    readonly property var count: backend.binMenuCounts.account === backend.account ? backend.binMenuCounts[modelData.folder] : undefined
                    objectName: modelData.folder + "ScopeButton"
                    width: Math.max(142, implicitWidth); implicitHeight: 30; leftPadding: 10; rightPadding: 10; topPadding: 0; bottomPadding: 0
                    hoverEnabled: true
                    background: Rectangle { radius: 5; color: binChoice.hovered ? Theme.bg4 : binChoice.current ? Theme.bg6 : "transparent" }
                    onHoveredChanged: if (hovered && !binChoice.current) backend.prefetchFolder(binChoice.modelData.folder)
                    onClicked: { binMenu.close(); mail.leaveComposerFor(function() { backend.folder = binChoice.modelData.folder }) }
                    contentItem: RowLayout {
                        spacing: 8
                        FmIcon { name: binChoice.modelData.icon; size: 12; color: Theme.inkAlpha(.45) }
                        Text { Layout.fillWidth: true; text: binChoice.modelData.name; color: binChoice.current || binChoice.hovered ? Theme.text : Theme.inkAlpha(.75); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12) }
                        Text { text: binChoice.count ? binChoice.count : ""; color: Theme.inkAlpha(.4); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(11); font.features: {"tnum": 1} }
                    }
                }
            }
        }
    }
    // Esc on a new message with something written.
    FmDialog {
        id: keepDialog; objectName: "keepMessageDialog"
        title: "Keep this message?"
        width: Math.min(400, parent ? parent.width - 48 : 400)
        contentItem: Label {
            text: "Save it to Drafts to finish later, or discard it."
            color: Theme.inkAlpha(0.6); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12); wrapMode: Text.Wrap
        }
        footer: RowLayout {
            spacing: 6
            Item { implicitWidth: 16 }
            FmButton { objectName: "keepMessageDiscard"; text: "Discard"; implicitHeight: 32; leftPadding: 12; rightPadding: 12; foreground: Theme.error; font.weight: Font.Medium; onClicked: { keepDialog.close(); backend.discardDraft(backend.draftDiscardContext()) } }
            Item { Layout.fillWidth: true }
            FmButton { objectName: "keepMessageEditing"; text: "Keep editing"; shortcutHint: "Esc"; outlined: true; implicitHeight: 32; leftPadding: 14; rightPadding: 14; font.weight: Font.Medium; onClicked: { keepDialog.close(); reply.focusEditor() } }
            FmButton { objectName: "keepMessageSave"; text: "Save to Drafts"; shortcutHint: "↵"; prominent: true; implicitHeight: 32; leftPadding: 14; rightPadding: 14; font.weight: Font.Medium; onClicked: keepDialog.saveToDrafts() }
            Item { implicitWidth: 16 }
        }
        function saveToDrafts() { close(); backend.saveDraft(); mail.setReplyOpen(false); backend.noteSavedToDrafts() }
        Shortcut { sequences: ["Return", "Enter"]; enabled: keepDialog.visible; onActivated: keepDialog.saveToDrafts() }
    }
    AttachmentViewer { id: mediaViewer; backdrop: mail }
    function showAttachment(attachment) {
        const media=[]
        for(const message of members) for(const item of message.attachments || []) {
            if(item.state === "available" && (/^(image|video)\//i.test(item.mime_type || "") || /\.(png|jpe?g|gif|webp|bmp|ico|tiff?|mp4|webm|mov|m4v|mkv|avi)$/i.test(item.name || ""))) media.push(item)
        }
        const index=media.findIndex(item=>item.id===attachment.id)
        if(index>=0)mediaViewer.show(backend.account,media,index)
    }
    Connections { target: backend; function onMailSelectionRestored(rows) { mail.selectionRows=rows; if(rows.length) { mail.selectionAnchor=rows[0].id; mail.selectionCursor=rows[rows.length-1].id } } }
    HoverHandler { enabled: senderCard.opened; onPointChanged: senderCard.trackPointer(point.scenePosition) }
    SenderCard { id: senderCard; formatAge: mail.relativeTime; onConversationOpened: function(id) { backend.openConversation(id) } }
    LinkCard { id: linkCard }
    // Plain-text links read without "https://www." in ink with an underline,
    // and blocked ones are struck through; the content service owns the block
    // list. Rich text ignores palette.link, so the colour is inline.
    function presentLinks(html) {
        const service = backend.htmlContent
        const labelled = String(html || "").replace(/(<a\b[^>]*>)https?:\/\/(?:www\.)?/gi, "$1")
        const styled = service && service.revision >= 0 ? service.styleLinks(labelled) : labelled
        return styled.replace(/<a (?![^>]*\bstyle=)([^>]*)>/gi, "<a $1 style=\"color:" + Theme.text + ";text-decoration:underline\">")
    }
    function openBodyLink(link) {
        const service = backend.htmlContent
        if (service && service.linkBlocked(link)) return
        backend.openLink(link)
    }
    Popup {
        id: railCategories; objectName: "allCategoriesPopup"; parent: Overlay.overlay; width: 280; padding: 12
        // Eases in from just under full size; closes at once.
        transformOrigin: Popup.TopLeft
        enter: Transition { ParallelAnimation { NumberAnimation { property: "opacity"; from: 0; to: 1; duration: Theme.ms(Theme.quickMs); easing.type: Theme.enterEasing } NumberAnimation { property: "scale"; from: 0.97; to: 1; duration: Theme.ms(Theme.quickMs); easing.type: Theme.enterEasing } } }
        height: Math.min(categoryChoices.implicitHeight + 24, parent.height - 100); focus: true
        onAboutToShow: { const point = allLabels.mapToItem(parent, 0, allLabels.height + 8); x = Math.max(12,Math.min(point.x,parent.width-width-12)); y = Math.max(64,Math.min(point.y,parent.height-height-12)) }
        background: Rectangle { color: Theme.surface; border.color: Theme.controlBorder; radius: 8 }
        contentItem: ScrollView {
                ScrollBar.vertical: FmScrollBar {}
            property bool blocksWorkspaceShortcuts: true
            clip: true; contentWidth: availableWidth; ScrollBar.horizontal.policy: ScrollBar.AlwaysOff
            ColumnLayout {
                id: categoryChoices; width: railCategories.availableWidth; spacing: 4
                Text { text: "All categories"; font.pointSize: Theme.fontPoints(12); color: Theme.text }
                Repeater { model: mail.categoriesByName
                    FmButton { required property var modelData; Layout.fillWidth: true; textAlignment: Text.AlignLeft; text: modelData.name + "  " + modelData.conversation_count; selected: mail.categoryOn(modelData.id); onClicked: mail.toggleCategory(modelData.id) }
                }
            }
        }
    }
    function menuKey(id) { const key = ShortcutRegistry.sequence(id, backend.preferences); return key === "Delete" ? "Del" : ShortcutRegistry.label(key) }
    function labelState(categoryId) {
        const rows = mail.menuRows, has = rows.filter(function(row) { return (row.categories || []).some(function(c) { return c.id === categoryId }) }).length
        return has === 0 ? "none" : has === rows.length ? "all" : "some"
    }
    FmMenu {
        id: morePopup; readonly property bool outgoing: mail.menuRows.some(function(row) { return row.is_outgoing }); objectName: "mailContextMenu"; parent: Overlay.overlay
        onAboutToShow: {
            if (!mail.rowMenu) {
                mail.menuRows = backend.snapshotMailRows([backend.selectedId])
                const p=readerActions.mapToItem(parent,readerActions.width,readerActions.height+6)
                x=Math.max(8,Math.min(p.x-width,parent.width-width-8)); y=Math.max(8,Math.min(p.y,parent.height-implicitHeight-8))
            }
        }
        onClosed: { const source = mail.messageMenuSource; mail.rowMenu = false; labelMenu.close(); formatMenu.close(); mail.messageMenuSource = null; mail.messageMenuAssistance = null; mail.menuMember = null; if (source) Qt.callLater(function() { if (source && source.releaseRaw) source.releaseRaw() }) }
        readonly property bool drafts: mail.menuRows.length > 0 && mail.menuRows.every(function(row) { return row.is_draft === true })
        readonly property bool allUnread: mail.menuRows.length > 0 && mail.menuRows.every(function(row) { return row.unread === true })
        readonly property bool single: mail.menuRows.length === 1
        readonly property bool mailRows: !drafts && !outgoing
        closePolicy: labelMenu.visible || formatMenu.visible ? Popup.CloseOnEscape : Popup.CloseOnEscape | Popup.CloseOnPressOutside
        contentItem: ColumnLayout {
            spacing: 0
            Text { visible: mail.menuRows.length > 1; text: mail.menuRows.length + " SELECTED"; color: Theme.subtle; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(10.5); font.letterSpacing: .44; Layout.leftMargin: 9; Layout.topMargin: 5; Layout.bottomMargin: 6 }
            FmMenuItem {
                objectName: "classifyConversation"; text: "Classify thread"; iconName: "summary"
                visible: !mail.rowMenu && assistance.classificationEnabled && mail.members.some(m => !m.sent)
                enabled: !assistance.busy && backend.connected
                onClicked: { morePopup.close(); assistance.start("classification", (mail.members.filter(m => !m.sent).slice(-1)[0] || {}).id || "") }
            }
            FmMenuItem { objectName: "contextOutgoing"; visible: morePopup.outgoing && morePopup.single; text: mail.menuRows[0] && mail.menuRows[0].outgoing_state === "queued" ? "Undo send" : "Send status"; onClicked: { const row = mail.menuRows[0]; morePopup.close(); backend.outgoingAction(row.id, row.outgoing_state === "queued") } }
            FmMenuItem { objectName: "contextReply"; visible: morePopup.mailRows && backend.folder !== "sent" && morePopup.single; text: "Reply"; shortcut: mail.menuKey("reply"); enabled: mail.newMessageAvailable && !backend.draftBusy; onClicked: mail.menuMember ? mail.composeFrom("reply", mail.menuMember) : mail.composeMenu("reply") }
            FmMenuItem {
                objectName: "contextReplyAll"; text: "Reply all"; shortcut: mail.menuKey("replyAll")
                visible: morePopup.mailRows && backend.folder !== "sent" && morePopup.single && (mail.menuMember ? mail.replyAllFor(mail.menuMember) : !mail.rowMenu && mail.replyAllAvailable)
                enabled: mail.newMessageAvailable && !backend.draftBusy
                onClicked: mail.menuMember ? mail.composeFrom("reply_all", mail.menuMember) : mail.composeMenu("reply_all")
            }
            FmMenuItem { objectName: "contextForward"; visible: morePopup.mailRows && morePopup.single; text: "Forward"; shortcut: mail.menuKey("forward"); enabled: mail.newMessageAvailable && !backend.draftBusy; onClicked: mail.menuMember ? mail.composeFrom("forward", mail.menuMember) : mail.composeMenu("forward") }
            FmMenuItem { objectName: "contextRead"; visible: morePopup.mailRows && backend.folder !== "sent"; separatorAbove: morePopup.single; text: morePopup.allUnread ? "Mark as read" : "Mark as unread"; shortcut: mail.menuKey("unread"); enabled: mail.selectionMutable; onClicked: mail.selectionAction(morePopup.allUnread ? "read" : "unread") }
            FmMenuItem { objectName: "contextDelay"; visible: morePopup.mailRows && backend.folder !== "sent" && !mail.menuRows.every(function(row) { return row.starred }); text: "Delay"; shortcut: mail.menuKey("delay"); enabled: mail.selectionMutable; onClicked: mail.selectionAction("delay") }
            FmMenuItem { objectName: "unholdConversationButton"; visible: morePopup.mailRows && mail.menuRows.some(function(row) { return row.starred }); text: "Remove delay"; shortcut: mail.menuRows.every(function(row) { return row.starred }) ? mail.menuKey("delay") : ""; enabled: mail.selectionMutable; onClicked: mail.selectionAction("undelay") }
            FmMenuItem { objectName: "contextDone"; visible: morePopup.mailRows && backend.folder !== "sent"; text: "Mark done"; shortcut: mail.menuKey("done"); enabled: mail.selectionMutable; onClicked: mail.selectionAction("done") }
            FmMenuItem { id: labelsMenuButton; objectName: "contextLabels"; visible: morePopup.mailRows && backend.folder !== "sent"; text: "Labels"; shortcut: labelMenu.flip ? "‹" : "›"; enabled: mail.selectionMutable && backend.categories.length > 0; onClicked: labelMenu.open(); onHoveredChanged: if (hovered && enabled && !labelMenu.opened) { formatMenu.close(); labelMenu.open() } }
            FmMenuItem {
                objectName: "contextResummarize"; text: "Resummarize"
                visible: mail.rowMenu && !!mail.messageMenuAssistance && mail.messageMenuAssistance.canResummarize
                enabled: !!mail.messageMenuAssistance && mail.messageMenuAssistance.canSummarize
                foreground: !enabled ? Theme.faint : Theme.ai
                onClicked: { const panel = mail.messageMenuAssistance; morePopup.close(); panel.summarize() }
            }
            FmMenuItem { id: formatMenuButton; objectName: "contextFormat"; visible: !!mail.messageMenuSource && mail.messageMenuSource.hasHtml && mail.messageMenuSource.canChoose; text: "Format"; tip: mail.messageMenuSource ? "For all mail from " + mail.messageMenuSource.sender : ""; shortcut: "‹"; onClicked: formatMenu.open(); onHoveredChanged: if (hovered && !formatMenu.opened) { labelMenu.close(); formatMenu.open() } }
            FmMenuItem { objectName: "contextMessageSource"; visible: !!mail.messageMenuSource; text: "View source"; onClicked: { const source = mail.messageMenuSource; morePopup.close(); source.show() } }
            FmMenuItem { objectName: "archiveConversation"; visible: morePopup.mailRows && !backend.syntheticProfile && backend.folder !== "trash"; separatorAbove: true; text: "Archive in Gmail"; enabled: mail.selectionMutable; onClicked: mail.selectionAction("archive") }
            FmMenuItem { objectName: "trashConversation"; visible: morePopup.mailRows && !backend.syntheticProfile && backend.folder !== "trash"; text: "Delete"; shortcut: mail.menuKey("delete"); danger: true; tip: "Move to Trash in Gmail"; enabled: mail.selectionMutable; onClicked: mail.selectionAction("trash") }
            FmMenuItem { objectName: "restoreConversation"; visible: !morePopup.drafts && !backend.syntheticProfile && backend.folder === "trash"; separatorAbove: true; text: "Restore from Trash"; enabled: mail.selectionMutable; onClicked: mail.selectionAction("restore") }
            FmMenuItem { objectName: "contextDiscard"; visible: morePopup.drafts; text: mail.menuRows.length > 1 ? "Discard drafts…" : "Discard draft…"; shortcut: mail.menuKey("delete"); danger: true; enabled: mail.selectionMutable; onClicked: { discardSelection.rows = mail.menuRows.slice(); morePopup.close(); discardSelection.open() } }
            FmMenuItem { visible: backend.syntheticProfile && !morePopup.drafts; separatorAbove: true; text: "Local captures"; onClicked: { morePopup.close(); capturePopup.open() } }
        }
    }
    FmMenu {
        id: formatMenu; objectName: "messageFormatMenu"; parent: morePopup.contentItem; keepInside: false
        minimumWidth: 154; x: -width - 9; y: formatMenuButton.y - 5
        contentItem: ColumnLayout {
            spacing: 0
            FmMenuItem { objectName: "messageFormatHtml"; text: "HTML"; markable: true; marked: !!mail.messageMenuSource && !mail.messageMenuSource.plain; onClicked: { if (mail.messageMenuSource) mail.messageMenuSource.setPlain(false); morePopup.close() } }
            FmMenuItem { objectName: "messageFormatPlain"; text: "Plain text"; markable: true; marked: !!mail.messageMenuSource && mail.messageMenuSource.plain; onClicked: { if (mail.messageMenuSource) mail.messageMenuSource.setPlain(true); morePopup.close() } }
        }
    }
    TextMenu { id: textMenu }
    FmMenu {
        id: labelMenu; objectName: "mailLabelSubmenu"; parent: morePopup.contentItem; minimumWidth: 172; keepInside: false
        readonly property bool flip: morePopup.x + morePopup.width + 180 > (mail.Overlay.overlay ? mail.Overlay.overlay.width : mail.width) - 8
        height: Math.min(implicitHeight, (mail.Overlay.overlay ? mail.Overlay.overlay.height : mail.height)-24)
        closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutsideParent
        onAboutToShow: { x = flip ? -width - 9 : morePopup.width - 1; y = Math.max(-morePopup.y + 8, Math.min(labelsMenuButton.y - 5, (mail.Overlay.overlay ? mail.Overlay.overlay.height : mail.height)-morePopup.y-height-8)) }
        contentItem: ScrollView {
            ScrollBar.vertical: FmScrollBar {}
            implicitHeight: labelOptions.implicitHeight; implicitWidth: labelOptions.implicitWidth; clip: true; contentWidth: availableWidth
            ColumnLayout { id: labelOptions; width: labelMenu.availableWidth; spacing: 0
                Repeater { model: backend.categories
                    FmMenuItem {
                        required property var modelData
                        readonly property string membership: labelMenu.visible ? mail.labelState(modelData.id) : "none"
                        objectName: "applyLabel_" + modelData.id
                        text: modelData.name; markable: true; marked: membership !== "none"; partial: membership === "some"
                        tip: membership === "all" ? "Remove from " + (mail.menuRows.length > 1 ? "all selected" : "this conversation") : membership === "some" ? "On some selected conversations · click to apply to all" : "Apply to " + (mail.menuRows.length > 1 ? "all selected" : "this conversation")
                        enabled: mail.selectionMutable
                        onClicked: mail.selectionAction(membership === "all" ? "label_remove" : "label_add", modelData.id)
                    }
                }
            }
        }
    }
    // Deleting for good always asks first; it cannot be undone.
    FmConfirmDialog {
        id: binConfirm; objectName: "binConfirm"
        property string action: ""
        property string subject: ""
        function ask(what) { action = what; subject = mail.thread.subject || "(No subject)"; open() }
        title: action === "empty" ? (mail.spamMode ? "Empty spam?" : "Empty trash?") : "Delete permanently?"
        message: action === "empty" ? (mail.binMessageCount === 0 && mail.binStoredCount > 0
                ? "Gmail’s Trash is already empty. " + mail.binStoredCount + (mail.binStoredCount === 1 ? " message" : " messages") + " left here will be removed from this computer."
                : Math.max(mail.binMessageCount, mail.binStoredCount) + (Math.max(mail.binMessageCount, mail.binStoredCount) === 1 ? " message" : " messages") + " will be deleted for good. This can’t be undone.")
            : "“" + subject + "” will be deleted for good. This can’t be undone."
        actionText: action === "empty" ? (mail.spamMode ? "Empty spam" : "Empty trash") : "Delete"
        actionObjectName: "binConfirmAction"; messageObjectName: "binConfirmMessage"
        destructive: true
        onAccepted: action === "empty" ? backend.emptyBin() : backend.deleteForever()
    }
    FmConfirmDialog {
        id: reconnectPrompt; objectName: "reconnectFullAccess"
        title: "Reconnect to delete for good"
        message: "Deleting mail for good needs full access to " + (backend.currentAccount.email || "this Gmail account") + ". Reconnect it to allow this; nothing else changes."
        actionText: "Reconnect"; actionObjectName: "reconnectFullAccessAction"; cancelObjectName: "reconnectFullAccessCancel"
        onAccepted: mail.reconnectForFullAccess()
    }
    Connections {
        target: backend
        function onBinActionFinished(action, ok, result) { if (!ok && result.code === "reconnect_required") reconnectPrompt.open() }
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
        // Eases in from just under full size; closes at once.
        enter: Transition { ParallelAnimation { NumberAnimation { property: "opacity"; from: 0; to: 1; duration: Theme.ms(Theme.quickMs); easing.type: Theme.enterEasing } NumberAnimation { property: "scale"; from: 0.97; to: 1; duration: Theme.ms(Theme.quickMs); easing.type: Theme.enterEasing } } }
        width: Math.min(560,parent.width-40); height: Math.min(420,parent.height-80); anchors.centerIn: parent
        padding: 20; focus: true; closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
        background: Rectangle { color: Theme.surface; border.color: Theme.controlBorder; radius: 8 }
        contentItem: ColumnLayout {
            spacing: 14
            RowLayout {
                Layout.fillWidth: true
                Text { text: "Local captures"; color: Theme.text; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(15); font.weight: Font.Medium; Layout.fillWidth: true }
                FmButton { iconName: "close"; tip: "Close local captures"; onClicked: capturePopup.close() }
            }
            Text { text: "Saved MIME copies on this machine. No messages were delivered."; color: Theme.muted; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(11.5); wrapMode: Text.Wrap; Layout.fillWidth: true }
            ListView {
                Layout.fillWidth: true; Layout.fillHeight: true; clip: true; model: backend.captures; spacing: 8
                ScrollBar.vertical: FmScrollBar {}
                delegate: RowLayout {
                    required property var modelData
                    width: ListView.view.width; height: 52; spacing: 10
                    ColumnLayout {
                        Layout.fillWidth: true; spacing: 4
                        Text { text: Theme.oneLine(modelData.subject) || "(No subject)"; maximumLineCount: 1; color: Theme.text; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12); elide: Text.ElideRight; Layout.fillWidth: true }
                        Text { text: modelData.state+" · "+Qt.formatDateTime(new Date(modelData.created_at*1000),"d MMM, HH:mm"); color: Theme.muted; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(11.5) }
                    }
                    FmButton { text: "Open"; onClicked: { if (modelData.draft_id) backend.openDraft(modelData.draft_id); else backend.selectConversation(modelData.conversation_id); capturePopup.close() } }
                    FmButton { text: "Inspect"; enabled: !backend.draftBusy; onClicked: backend.inspectCapture(modelData.operation_id) }
                }
                footer: FmButton { visible: backend.capturesMore; text: "Load more"; onClicked: backend.loadMoreCaptures() }
            }
            Text { text: backend.captureStatus; visible: text.length>0; color: Theme.muted; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(11.5); wrapMode: Text.Wrap; Layout.fillWidth: true }
        }
    }
    TapHandler { parent: readerActions; acceptedButtons: Qt.RightButton; onTapped: mail.openActions() }
    Shortcut { sequences: [StandardKey.SelectAll]; enabled: mail.singleKeyActionsAllowed && mail.listHasFocus(); onActivated: { mail.selectionRows = mail.frozenRows(mail.visibleRows().slice(0,200).map(function(row) { return row.id })); } }
    Shortcut { sequence: "Escape"; enabled: mail.visible && mail.hasSelection && !mail.popupOpen; onActivated: mail.clearSelection() }
    Shortcut { sequence: "Down"; enabled: ShortcutRegistry.unassigned("Down","mail",backend.preferences) && mail.singleKeyActionsAllowed && mail.listHasFocus(); onActivated: mail.moveSelection(1,false) }
    Shortcut { sequence: "Up"; enabled: ShortcutRegistry.unassigned("Up","mail",backend.preferences) && mail.singleKeyActionsAllowed && mail.listHasFocus(); onActivated: mail.moveSelection(-1,false) }
    Shortcut { sequence: "PgDown"; enabled: ShortcutRegistry.unassigned("PgDown","mail",backend.preferences) && mail.singleKeyActionsAllowed && mail.listHasFocus(); onActivated: mail.pageSelection(1,false) }
    Shortcut { sequence: "PgUp"; enabled: ShortcutRegistry.unassigned("PgUp","mail",backend.preferences) && mail.singleKeyActionsAllowed && mail.listHasFocus(); onActivated: mail.pageSelection(-1,false) }
    Shortcut { sequence: "Shift+PgDown"; enabled: ShortcutRegistry.unassigned("Shift+PgDown","mail",backend.preferences) && mail.singleKeyActionsAllowed && mail.listHasFocus(); onActivated: mail.pageSelection(1,true) }
    Shortcut { sequence: "Shift+PgUp"; enabled: ShortcutRegistry.unassigned("Shift+PgUp","mail",backend.preferences) && mail.singleKeyActionsAllowed && mail.listHasFocus(); onActivated: mail.pageSelection(-1,true) }
    Shortcut { sequence: "Home"; enabled: ShortcutRegistry.unassigned("Home","mail",backend.preferences) && mail.singleKeyActionsAllowed && mail.listHasFocus(); onActivated: mail.edgeSelection(false,false) }
    Shortcut { sequence: "End"; enabled: ShortcutRegistry.unassigned("End","mail",backend.preferences) && mail.singleKeyActionsAllowed && mail.listHasFocus(); onActivated: mail.edgeSelection(true,false) }
    Shortcut { sequence: "Shift+Home"; enabled: ShortcutRegistry.unassigned("Shift+Home","mail",backend.preferences) && mail.singleKeyActionsAllowed && mail.listHasFocus(); onActivated: mail.edgeSelection(false,true) }
    Shortcut { sequence: "Shift+End"; enabled: ShortcutRegistry.unassigned("Shift+End","mail",backend.preferences) && mail.singleKeyActionsAllowed && mail.listHasFocus(); onActivated: mail.edgeSelection(true,true) }
    Shortcut { sequences: ["Menu","Shift+F10"]; enabled: mail.singleKeyActionsAllowed && mail.listHasFocus(); onActivated: mail.keyboardRowMenu() }
    Shortcut { sequence: ShortcutRegistry.sequence("delete",backend.preferences); enabled: mail.singleKeyActionsAllowed && mail.listHasFocus() && mail.selectionMutable; onActivated: {
        if (mail.draftsMode) { discardSelection.rows = mail.hasSelection ? mail.selectionRows : backend.snapshotMailRows([backend.activeDraftId]); if (discardSelection.rows.length) discardSelection.open() }
        else if (!backend.syntheticProfile && backend.folder !== "trash") mail.selectionAction("trash")
    } }
    Shortcut { sequence: "Shift+Down"; enabled: ShortcutRegistry.unassigned("Shift+Down","mail",backend.preferences) && mail.singleKeyActionsAllowed && mail.listHasFocus(); onActivated: mail.moveSelection(1,true) }
    Shortcut { sequence: "Shift+Up"; enabled: ShortcutRegistry.unassigned("Shift+Up","mail",backend.preferences) && mail.singleKeyActionsAllowed && mail.listHasFocus(); onActivated: mail.moveSelection(-1,true) }
    Shortcut { sequence: ShortcutRegistry.sequence("unread",backend.preferences); enabled: mail.readerKeyActionsAllowed && !mail.draftsMode && mail.canMutate; onActivated: mail.selectionAction(mail.hasSelection ? (mail.selectionRows.every(function(row) { return row.unread }) ? "read" : "unread") : mail.anyUnread ? "read" : "unread") }
    Shortcut { sequence: ShortcutRegistry.sequence("compose",backend.preferences); enabled: mail.singleKeyActionsAllowed; onActivated: mail.newMessage() }
    Shortcut { sequence: ShortcutRegistry.sequence("reply",backend.preferences); enabled: mail.readerKeyActionsAllowed && !mail.hasSelection && mail.members.length>0 && !mail.thread.is_outgoing; onActivated: mail.compose("reply") }
    Shortcut { sequence: ShortcutRegistry.sequence("replyAll",backend.preferences); enabled: mail.readerKeyActionsAllowed && !mail.hasSelection && mail.members.length>0 && mail.replyAllAvailable; onActivated: mail.compose("reply_all") }
    Shortcut { sequence: ShortcutRegistry.sequence("forward",backend.preferences); enabled: mail.readerKeyActionsAllowed && !mail.hasSelection && mail.members.length>0 && !mail.thread.is_outgoing; onActivated: mail.compose("forward") }
    Shortcut { sequence: ShortcutRegistry.sequence("done",backend.preferences); enabled: mail.readerKeyActionsAllowed && !mail.draftsMode && (mail.hasSelection || mail.members.length>0) && mail.selectionMutable; onActivated: mail.selectionAction("done") }
    Shortcut { sequence: ShortcutRegistry.sequence("delay",backend.preferences); enabled: mail.readerKeyActionsAllowed && !mail.draftsMode && (mail.hasSelection || mail.members.length>0) && mail.selectionMutable; onActivated: mail.selectionAction(mail.hasSelection || !mail.anyStarred ? "delay" : "undelay") }
    Shortcut { sequence: ShortcutRegistry.sequence("scopes",backend.preferences); enabled: mail.singleKeyActionsAllowed; onActivated: { const next = backend.folder==="inbox" ? "sent" : backend.folder==="sent" ? "drafts" : "inbox"; mail.leaveComposerFor(function() { backend.folder = next }, next === "drafts") } }
    Shortcut { sequence: ShortcutRegistry.sequence("next",backend.preferences); enabled: mail.singleKeyActionsAllowed; onActivated: mail.moveSelection(1) }
    Shortcut { sequence: ShortcutRegistry.sequence("prev",backend.preferences); enabled: mail.singleKeyActionsAllowed; onActivated: mail.moveSelection(-1) }
    Shortcut { sequence: "Escape"; enabled: mail.visible && mail.replyOpen && !mail.hasSelection && !mail.popupOpen; onActivated: mail.escapeComposer() }
}
