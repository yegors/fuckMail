import QtQuick
import QtQuick.Layouts

// Who is invited and how each answered, from the provider's attendee
// statuses. The organiser comes first, then the guests, then you on an
// invitation; on your own event you come first as "Organizer · you". Your own
// status follows your Going? answer. In the editor, `guests` lists the current
// addresses: ones the saved event does not have yet read "Invite on save".
ColumnLayout {
    id: list
    property var event: ({})
    // Editor only: the guest addresses as edited, or null for the event's own.
    property var guests: null
    property int avatarSize: 20
    property real nameSize: 13
    property real statusSize: 12
    property int rowHeight: 30
    // Names the rows for tests: prefix + "Attendee_N" / "AttendeeStatus_N".
    property string prefix: "attendee"
    spacing: 0
    readonly property var statusText: ({yes: "Accepted", maybe: "Maybe", no: "Declined", "": "No reply", added: "Invite on save"})
    function statusColor(status) {
        return status === "yes" ? Theme.inkAlpha(.75) : status === "maybe" ? Theme.inkAlpha(.55) : status === "no" ? Theme.error
            : status === "added" ? Theme.accent : Theme.inkAlpha(.4)
    }
    function answer(person) {
        const value = person.responseStatus
        return value === "accepted" ? "yes" : value === "tentative" ? "maybe" : value === "declined" ? "no" : ""
    }
    readonly property var rows: {
        const e = event || {}
        if (e.busy_only) return []
        const account = (backend.allAccounts || []).find(function(a) { return a.id === e.account }) || {}
        const own = String(account.email || "").toLowerCase()
        const lower = function(value) { return String(value || "").toLowerCase() }
        const people = (e.attendees || []).filter(function(p) { return !p.resource && (p.email || p.displayName) })
        const isMe = function(p) { return p.self === true || (own.length > 0 && lower(p.email) === own) }
        const invitation = Theme.rsvpOf(e) !== undefined
        const organizerEmail = lower(e.organizer || (people.find(function(p) { return p.organizer === true }) || {}).email)
        let listed = people
        if (list.guests) {
            const saved = (e.attendees || []).map(function(p) { return lower(p.email) })
            listed = list.guests.map(function(address) {
                const found = people.find(function(p) { return lower(p.email) === lower(address) })
                return found || {email: address, added: saved.indexOf(lower(address)) < 0}
            })
        }
        const row = function(p, role) {
            return {name: p.displayName || p.email, email: p.email || "", role: role, own: false,
                    status: p.added ? "added" : role === "Organizer" && !p.responseStatus ? "yes" : list.answer(p)}
        }
        const me = {name: account.name || account.email || "You", email: account.email || "", own: true,
                    role: invitation ? "You" : "Organizer · you", status: invitation ? Theme.rsvpOf(e) : "yes"}
        const out = []
        if (invitation) {
            const organizer = people.find(function(p) { return lower(p.email) === organizerEmail && !isMe(p) })
                || (organizerEmail.length ? {email: organizerEmail} : null)
            if (organizer) out.push(row(organizer, "Organizer"))
            listed.forEach(function(p) { if (!isMe(p) && lower(p.email) !== organizerEmail) out.push(row(p, "")) })
            out.push(me)
        } else {
            out.push(me)
            listed.forEach(function(p) { if (!isMe(p)) out.push(row(p, "")) })
        }
        return out
    }
    Repeater {
        model: list.rows
        RowLayout {
            required property var modelData
            required property int index
            objectName: list.prefix + "_" + index
            Layout.fillWidth: true; Layout.minimumHeight: list.rowHeight; spacing: 9
            SenderAvatar {
                diameter: list.avatarSize; brandSize: Math.round(list.avatarSize * .66)
                own: modelData.own; name: modelData.name; email: modelData.email
                discColor: modelData.own ? Theme.accountTint(list.event.account, .22) : Theme.oklch(.36, .06, hue)
                initialsColor: modelData.own ? Theme.accountColor(list.event.account) : Theme.oklch(.9, .05, hue)
                initials: modelData.own ? String(modelData.name).charAt(0).toUpperCase() : Theme.initialsOf(modelData.name)
                fontPoints: Theme.fontPoints(list.avatarSize >= 24 ? 9.5 : 8.5)
            }
            Text {
                text: modelData.name; textFormat: Text.PlainText; elide: Text.ElideRight
                Layout.minimumWidth: 0; Layout.preferredWidth: implicitWidth; Layout.maximumWidth: implicitWidth
                color: Theme.text; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(list.nameSize)
                font.strikeout: modelData.status === "no"
            }
            Text {
                id: role
                visible: modelData.role.length > 0
                text: modelData.role; color: Theme.inkAlpha(.4); textFormat: Text.PlainText
                font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(11.5)
            }
            Item { Layout.fillWidth: true }
            Text {
                objectName: list.prefix + "Status_" + index
                text: list.statusText[modelData.status] || ""; color: list.statusColor(modelData.status); textFormat: Text.PlainText
                font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(list.statusSize)
            }
        }
    }
}
