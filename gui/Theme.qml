pragma Singleton
import QtQuick
QtObject {
    id: theme
    property string themeId: "graphite"
    property string textSize: "default"
    readonly property var themes: [
        { id: "graphite", name: "Graphite", description: "Near-black with warm amber. The default.", scheme: "dark", bg0: "#0B0C0E", bg: "#0E0F11", bg1: "#121316", bg2: "#131417", bg3: "#17181C", bg4: "#191A1E", bg5: "#1B1C20", bg6: "#1F2126", line: "#232529", line2: "#2A2C31", line3: "#3A3D44", ink: "#E9EAEC", accent: "#C9A36B", warn: "#D9A441", red: "#E0645A", shadow: "#000000" },
        { id: "paper", name: "Paper", description: "Warm off-white, ink type, oxblood accent.", scheme: "light", bg0: "#EDE9E1", bg: "#F4F1EA", bg1: "#EFEBE3", bg2: "#EAE5DC", bg3: "#E5E0D6", bg4: "#E1DBD0", bg5: "#DDD6CA", bg6: "#D6CEC1", line: "#DCD5C8", line2: "#CFC6B7", line3: "#B9AE9C", ink: "#1C1B18", accent: "#8A2E2A", warn: "#B9791A", red: "#B3382F", shadow: "#3C321E" },
        { id: "harbour", name: "Harbour", description: "Deep blue-black with a cool sea-glass accent.", scheme: "dark", bg0: "#070B12", bg: "#0A0F18", bg1: "#0D131D", bg2: "#101722", bg3: "#131B27", bg4: "#161F2C", bg5: "#182130", bg6: "#1C2637", line: "#1E2939", line2: "#273345", line3: "#364559", ink: "#E6ECF2", accent: "#7FC4B6", warn: "#E0B04A", red: "#E27070", shadow: "#000000" }
    ]
    readonly property var current: themes.find(item => item.id === themeId) || themes[0]
    readonly property bool light: current.scheme === "light"
    readonly property real fontScale: textSize === "small" ? 13 / 14 : textSize === "large" ? 15.5 / 14 : 1
    readonly property string fontFamily: "Geist"
    function fontSize(base) { return Math.max(1, Math.round(base * fontScale)) }
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
    readonly property color amber: accent
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
