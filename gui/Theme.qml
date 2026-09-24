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
    // Account colours, 8×2 in the picker; the engine gives each new account the
    // first unused one. Accounts without a saved colour use palette order.
    readonly property var accountPalette: ["#C9A36B", "#E0B85A", "#CF8F6A", "#E07F7F", "#D58E9A", "#C98BC9", "#AE97D4", "#8C8FE0",
        "#8AA9D6", "#6FA3C7", "#6FB5B2", "#7FC4A4", "#8FB39A", "#B9C77A", "#C9A58F", "#A3AAB3"]
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
