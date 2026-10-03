// Singleton design tokens: the four colour themes, text-size scaling, font and
// line-height helpers, account colours and small shared formatters. Main.qml
// binds themeId, textSize and devicePixelRatio from the user's preferences.
pragma Singleton
import QtQuick
QtObject {
    id: theme
    property string themeId: "graphite"
    property string textSize: "default"
    property real devicePixelRatio: 1
    readonly property real pixelScale: devicePixelRatio > 0 ? devicePixelRatio : 1
    readonly property real ruleWidth: Math.max(1, Math.round(pixelScale)) / pixelScale
    function snap(value) { return Math.round(value * pixelScale) / pixelScale }
    // Thin, axis-aligned rules only. Preserve the logical layout slot and snap
    // its painted child. These helpers deliberately exclude rotated/scaled
    // ancestors; reading every offset keeps scrolling/layout bindings live.
    function snapX(item, value) {
        let offset = 0
        for (let ancestor = item; ancestor; ancestor = ancestor.parent) offset += ancestor.x
        return snap(offset + value) - offset
    }
    function snapY(item, value) {
        let offset = 0
        for (let ancestor = item; ancestor; ancestor = ancestor.parent) offset += ancestor.y
        return snap(offset + value) - offset
    }
    readonly property var themes: [
        { id: "graphite", name: "Graphite", description: "Near-black with warm amber. The default.", scheme: "dark", bg0: "#0B0C0E", bg: "#0E0F11", bg1: "#121316", bg2: "#131417", bg3: "#17181C", bg4: "#191A1E", bg5: "#1B1C20", bg6: "#1F2126", line: "#232529", line2: "#2A2C31", line3: "#3A3D44", ink: "#E9EAEC", accent: "#C9A36B", warn: "#D9A441", red: "#E0645A", shadow: "#000000" },
        { id: "paper", name: "Paper", description: "Warm off-white, ink type, oxblood accent.", scheme: "light", bg0: "#EDE9E1", bg: "#F4F1EA", bg1: "#EFEBE3", bg2: "#EAE5DC", bg3: "#E5E0D6", bg4: "#E1DBD0", bg5: "#DDD6CA", bg6: "#D6CEC1", line: "#DCD5C8", line2: "#CFC6B7", line3: "#B9AE9C", ink: "#1C1B18", accent: "#8A2E2A", warn: "#B9791A", red: "#B3382F", shadow: "#3C321E" },
        { id: "terminal", name: "Terminal", description: "Phosphor green on deep black.", scheme: "dark", bg0: "#050705", bg: "#080B08", bg1: "#0A0E0A", bg2: "#0D120D", bg3: "#101610", bg4: "#131A13", bg5: "#161E16", bg6: "#1A241A", line: "#182218", line2: "#213021", line3: "#2E432E", ink: "#C9DCC4", accent: "#5FBF6E", warn: "#C9B15A", red: "#D46A5E", shadow: "#000000" },
        { id: "harbour", name: "Harbour", description: "Deep blue-black with a cool sea-glass accent.", scheme: "dark", bg0: "#070B12", bg: "#0A0F18", bg1: "#0D131D", bg2: "#101722", bg3: "#131B27", bg4: "#161F2C", bg5: "#182130", bg6: "#1C2637", line: "#1E2939", line2: "#273345", line3: "#364559", ink: "#E6ECF2", accent: "#7FC4B6", warn: "#E0B04A", red: "#E27070", shadow: "#000000" }
    ]
    readonly property var current: themes.find(item => item.id === themeId) || themes[0]
    readonly property bool light: current.scheme === "light"
    readonly property real fontScale: textSize === "small" ? 13 / 14 : textSize === "large" ? 15.5 / 14 : 1
    readonly property string fontFamily: "Geist"
    // Design sizes include half pixels (12.5px). QFont pixel sizes are whole
    // numbers, so fonts use fontPoints(); fontSize() stays exact for geometry.
    function fontSize(base) { return Math.max(1, base * fontScale) }
    property FontMetrics pointMetrics: FontMetrics { font.family: theme.fontFamily; font.pointSize: 75 }
    readonly property real pointsPerPixel: proseMetrics.height > 50 && pointMetrics.height > 0 ? 75 * proseMetrics.height / (100 * pointMetrics.height) : 0.75
    function fontPoints(base) { return fontSize(base) * pointsPerPixel }
    function fontPixels(font) { return font.pixelSize > 0 ? font.pixelSize : font.pointSize / pointsPerPixel }
    // Only presentation markup is generated here; message/model text remains
    // literal, including any HTML-looking content supplied by a sender.
    // Rich-text line-height percentages scale the font's natural line height,
    // whereas the design's CSS multiples scale the font size.
    property FontMetrics proseMetrics: FontMetrics { font.family: theme.fontFamily; font.pixelSize: 100 }
    readonly property real naturalLeading: proseMetrics.height > 50 ? proseMetrics.height / 100 : 1
    function leadingPercent(multiple) { return Math.round(multiple / naturalLeading * 100) }
    // Qt draws a FixedHeight line at its top; CSS centres it in the line box.
    // Use as topPadding/bottomPadding to put glyphs where half-leading would.
    function leadingTop(font, lineHeight) { return (lineHeight - fontPixels(font) * naturalLeading) / 2 }
    function leadingBottom(font, lineHeight) { return -Math.abs(leadingTop(font, lineHeight)) }
    function proseMarkup(value, leading) {
        const escaped = String(value || "").replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/\n/g, "<br>")
        return '<p style="white-space:pre-wrap;line-height:' + leadingPercent(leading) + '%;margin:0">' + escaped + '</p>'
    }
    function withAlpha(value, opacity) { return Qt.rgba(value.r, value.g, value.b, Math.max(0, Math.min(1, opacity))) }
    function inkAlpha(opacity) { return withAlpha(text, opacity) }
    // A hover lift, like CSS brightness(): each channel scaled, alpha kept.
    function brighten(value, factor) {
        const c = Qt.color(value)
        return Qt.rgba(Math.min(1, c.r * factor), Math.min(1, c.g * factor), Math.min(1, c.b * factor), c.a)
    }
    function accentAlpha(opacity) { return withAlpha(accent, opacity) }
    function warnAlpha(opacity) { return withAlpha(warn, opacity) }
    function errorAlpha(opacity) { return withAlpha(error, opacity) }
    function canvasAlpha(opacity) { return withAlpha(canvas, opacity) }
    function shadowAlpha(opacity) { return withAlpha(shadow, opacity) }

    readonly property color bg0: current.bg0
    readonly property color bg: current.bg
    readonly property color bg1: current.bg1
    readonly property color bg2: current.bg2
    readonly property color bg3: current.bg3
    readonly property color bg4: current.bg4
    readonly property color bg5: current.bg5
    readonly property color bg6: current.bg6
    readonly property color canvas: bg
    readonly property color surface: bg2
    readonly property color selected: bg4
    readonly property color hover: bg6
    readonly property color reader: bg0
    readonly property color line: current.line
    readonly property color line2: current.line2
    readonly property color line3: current.line3
    readonly property color controlBorder: line2
    readonly property color focusBorder: line3
    readonly property color text: current.ink
    readonly property color ink: text
    readonly property color muted: inkAlpha(0.55)
    readonly property color subtle: inkAlpha(0.4)
    readonly property color secondary: inkAlpha(0.72)
    readonly property color faint: inkAlpha(0.35)
    readonly property color accent: current.accent
    readonly property color ai: themeId === "paper" ? "#4F6C86" : themeId === "terminal" ? "#7FA98A" : themeId === "harbour" ? "#A7BBD0" : "#9BB0C4"
    readonly property color amber: accent
    // Search matches: the accent as a tint behind text, stronger
    // on an HTML message's white paper.
    readonly property color matchTint: Qt.rgba(accent.r, accent.g, accent.b, light ? .34 : .32)
    readonly property color matchPaper: Qt.rgba(accent.r, accent.g, accent.b, .45)
    // Account colours, 8×2 in the picker; the engine gives each new account the
    // first unused one. Accounts without a saved colour use palette order.
    readonly property var accountPalette: ["#C9A36B", "#E0B85A", "#CF8F6A", "#E07F7F", "#D58E9A", "#C98BC9", "#AE97D4", "#8C8FE0",
        "#8AA9D6", "#6FA3C7", "#6FB5B2", "#7FC4A4", "#8FB39A", "#B9C77A", "#C9A58F", "#A3AAB3"]
    // Your answer to an invitation. Maybe and not-yet-answered events show as
    // uncommitted (hatched); declined ones are dimmed and struck through.
    // Google Calendar keeps notes written in its editor as HTML; show them as
    // text with line breaks, bullets and link targets kept.
    function plainNotes(text) {
        let s = String(text || "").trim()
        if (!/<\s*\/?\s*(br|p|div|a|b|i|u|ul|ol|li|span|strong|em|font|h[1-6]|hr|blockquote|table|tr|td)\b[^>]*>/i.test(s)) return s
        s = s.replace(/<a\b[^>]*href\s*=\s*"([^"]*)"[^>]*>([\s\S]*?)<\/a>/gi, function(m, href, label) {
            const t = label.replace(/<[^>]*>/g, "").trim()
            return !t || t === href || href.indexOf("mailto:") === 0 ? (t || href) : t + " (" + href + ")"
        })
        s = s.replace(/<\s*br\s*\/?>/gi, "\n").replace(/<\s*li\b[^>]*>/gi, "\n• ").replace(/<\s*\/\s*(p|div|li|h[1-6]|tr|blockquote|ul|ol)\s*>/gi, "\n").replace(/<[^>]*>/g, "")
        s = s.replace(/&nbsp;/g, " ").replace(/&lt;/g, "<").replace(/&gt;/g, ">").replace(/&quot;/g, "\"").replace(/&#39;|&apos;/g, "'")
            .replace(/&#(\d+);/g, function(m, n) { return String.fromCharCode(Number(n)) }).replace(/&amp;/g, "&")
        return s.replace(/[ \t]+\n/g, "\n").replace(/\n{3,}/g, "\n\n").trim()
    }
    // Notes as rich text: escaped, line breaks kept, web links clickable.
    function linkedNotes(text, linkColor) {
        const escape = function(s) { return s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;") }
        const parts = String(text || "").split(/(https?:\/\/[^\s<>"'()\[\]]*[^\s<>"'()\[\].,;:!?])/)
        return parts.map(function(part, i) {
            return i % 2 ? "<a href=\"" + escape(part) + "\" style=\"color:" + linkColor + "\">" + escape(part) + "</a>" : escape(part)
        }).join("").replace(/\n/g, "<br>")
    }
    // Your answer to an invitation: "yes", "maybe", "" (not answered) or "no".
    // Undefined when the event is not an invitation: one you organise, or one
    // without guests. Guests but no answer of yours on record counts as yes.
    function rsvpOf(event) {
        if (!event || event.busy_only) return undefined
        const people = event.attendees || []
        if (!people.length) return undefined
        const me = people.find(function(p) { return p.self === true })
        if (me ? me.organizer === true : organizedByAccount(event)) return undefined
        const answer = event.self_response || (me ? me.responseStatus : "")
        return answer === "tentative" ? "maybe" : answer === "needsAction" ? "" : answer === "declined" ? "no" : "yes"
    }
    function organizedByAccount(event) {
        const rows = typeof backend === "undefined" || !backend.allAccounts ? [] : backend.allAccounts
        const row = rows.find(function(a) { return a.id === event.account }) || {}
        return !!row.email && String(event.organizer || "").toLowerCase() === String(row.email).toLowerCase()
    }
    function eventOpen(event) { const answer = rsvpOf(event); return answer === "maybe" || answer === "" }
    function eventDeclined(event) { return rsvpOf(event) === "no" }
    // Text for one line: line breaks and runs of spaces read as one space.
    // Mail subjects and summaries sometimes carry a stray line break.
    function oneLine(text) { return String(text || "").replace(/\s+/g, " ").trim() }
    // Sender initials: both first letters when the second word is capitalised,
    // otherwise the first letter. Punctuation and symbols count as spaces.
    function initialsOf(name) {
        const words = String(name || "?").replace(/[!-\/:-@\[-`{-~\u00a0-\u00bf\u2000-\u206f]/g, " ").trim().split(/\s+/).filter(function(w) { return w.length })
        const second = words.length > 1 ? words[1].charAt(0) : ""
        const capital = second.length && second !== second.toLowerCase() && second === second.toUpperCase()
        return (capital ? words[0].charAt(0) + second : (words[0] || "?").charAt(0)).toUpperCase()
    }
    // A stable hue for a name: 31-multiplier string hash modulo 360.
    function nameHue(name) {
        let x = 0
        for (const ch of String(name || "?")) x = (x * 31 + ch.charCodeAt(0)) >>> 0
        return x % 360
    }
    // CSS oklch(L C H) in sRGB, clamped to the gamut.
    function oklch(l, c, h) {
        const a = c * Math.cos(h * Math.PI / 180), b = c * Math.sin(h * Math.PI / 180)
        const lc = Math.pow(l + .3963377774 * a + .2158037573 * b, 3)
        const mc = Math.pow(l - .1055613458 * a - .0638541728 * b, 3)
        const sc = Math.pow(l - .0894841775 * a - 1.291485548 * b, 3)
        function gamma(v) { v = Math.max(0, Math.min(1, v)); return v <= .0031308 ? 12.92 * v : 1.055 * Math.pow(v, 1 / 2.4) - .055 }
        return Qt.rgba(gamma(4.0767416621 * lc - 3.3077115913 * mc + .2309699292 * sc),
                       gamma(-1.2684380046 * lc + 2.6097574011 * mc - .3413193965 * sc),
                       gamma(-.0041960863 * lc - .7034186147 * mc + 1.707614701 * sc), 1)
    }
    function responseLabel(event) { const answer = rsvpOf(event); return answer === "maybe" ? "Maybe" : answer === "" ? "Not answered" : answer === "no" ? "Declined" : "" }
    function accountColor(account) {
        const rows = typeof backend === "undefined" || !backend.allAccounts ? [] : backend.allAccounts
        const index = rows.findIndex(function(row) { return row.id === account })
        if (index >= 0 && rows[index].color) return rows[index].color
        return accountPalette[Math.max(0, index) % accountPalette.length]
    }
    function accountTint(account, opacity) { return withAlpha(Qt.color(accountColor(account)), opacity) }
    readonly property color warn: current.warn
    readonly property color error: current.red
    readonly property color red: error
    readonly property color shadow: current.shadow
    readonly property color overlay: shadowAlpha(0.45)
    readonly property color selection: accentAlpha(0.25)
    readonly property color selectedText: text
    readonly property color scrollThumb: inkAlpha(0.14)
    // Pinned Qt Basic multiplies ScrollBar/ScrollIndicator's palette color by
    // 0.75 while active. Compensate so its painted thumb is exactly 14% ink.
    readonly property color scrollPalette: inkAlpha(0.14 / 0.75)
    readonly property color timeline: bg0
    readonly property color timelinePast: inkAlpha(0.25)
    readonly property color timelineCurrent: accentAlpha(0.12)
    readonly property color timelineNext: bg4
    readonly property int headerHeight: 56
    readonly property int comfortableRowHeight: 52
    readonly property int compactRowHeight: 44
    function accountIcon(id) { return id === "windscribe" ? "wind" : id === "personal" ? "person" : "sliders" }
}
