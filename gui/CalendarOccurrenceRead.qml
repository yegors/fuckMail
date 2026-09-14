import QtQuick

QtObject {
    id: reader
    property var selection: ({})
    property bool active: false
    property bool timelineSource: false
    property bool busy: false
    property int generation: 0
    property string correlation: ""
    property var captured: ({})
    readonly property var sourceEvents: timelineSource ? backend.todayEvents : backend.events
    readonly property var authority: {
        backend.accounts; backend.calendarSources; backend.googleSync
        return backend.calendarOccurrenceContext(selection.account || "", selection.calendar_id || "")
    }
    signal resolved(var event)
    signal failed(string message)
    function source() { return sourceEvents.find(function(e) { return e.id === reader.selection.id }) || ({}) }
    function sourceStamp() {
        // Expanded rows carry the master ETag even when a retained exception
        // changes. Fence the complete bounded, permitted projection so those
        // changes cannot publish an older read or silently re-enable the form.
        return JSON.stringify(source())
    }
    function context() { return {authority: JSON.stringify(authority), source: sourceStamp()} }
    function current(stamp) {
        return stamp && Object.keys(authority).length > 0 && stamp.authority === JSON.stringify(authority)
            && stamp.source === sourceStamp() && source().can_resolve_occurrence === true
    }
    function invalidate() { ++generation; correlation = ""; busy = false; captured = ({}) }
    function reject(message) { invalidate(); failed(message) }
    function check() {
        if (busy && (!active || selection.id !== captured.id || !current(captured.context)))
            reject("The selected occurrence or calendar changed. Check this occurrence again.")
    }
    function read() {
        if (!active || busy) return
        if (!selection.occurrence_selector || source().can_resolve_occurrence !== true || !Object.keys(authority).length) {
            failed("This occurrence is not available for editing. Start sync for its writable calendar and reopen its current details."); return
        }
        const selector = Object.assign({}, selection.occurrence_selector, {account: selection.account, calendar_id: selection.calendar_id})
        correlation = "gui-occurrence-" + Date.now() + "-" + (++generation) + "-" + Math.floor(Math.random() * 1000000000)
        captured = {id: selection.id, selector: selector, event: Object.assign({}, selection), context: context()}
        busy = true
        backend.readCalendarOccurrence(selector, correlation)
    }
    onActiveChanged: if (!active) invalidate()
    onSelectionChanged: check()
    onAuthorityChanged: check()
    onSourceEventsChanged: check()
    property Connections replies: Connections {
        target: backend
        function onCalendarOccurrenceReadFinished(token, ok, result, error) {
            if (!reader.busy || token !== reader.correlation) return
            reader.check()
            if (!reader.busy) return
            const captured = reader.captured
            reader.invalidate()
            if (!ok) { reader.failed(error); return }
            const event = Object.assign({}, captured.event, result.event, {id: captured.id,
                event_id: result.event_id, etag: result.etag, occurrence: result.occurrence,
                occurrence_selector: Object.assign({}, captured.selector, {event_id: result.event_id}),
                occurrence_ui_context: captured.context})
            reader.resolved(event)
        }
    }
}
