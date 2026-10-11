import QtQuick
import QtQuick.Controls
import QtQuick.Effects

// Shared context menu surface: 196px minimum, 5px padding, bg2 card with
// the popover radius and shadow.
Popup {
    id: menu
    // Eases in from just under full size; closes at once.
    transformOrigin: Popup.TopLeft
    enter: Transition { ParallelAnimation { NumberAnimation { property: "opacity"; from: 0; to: 1; duration: Theme.ms(Theme.quickMs); easing.type: Theme.enterEasing } NumberAnimation { property: "scale"; from: 0.97; to: 1; duration: Theme.ms(Theme.quickMs); easing.type: Theme.enterEasing } } }
    property real minimumWidth: 196
    // Submenus are positioned relative to their parent menu instead.
    property bool keepInside: true
    padding: 5
    width: Math.max(minimumWidth, implicitContentWidth + leftPadding + rightPadding)
    focus: true
    closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
    // Keep the whole menu inside its overlay once its size is known. Only
    // while shown: before that a popup reads x and y as 0, and a clamp would
    // move every menu to the left edge.
    function clamp() {
        if (!parent || !keepInside) return
        x = Math.max(8, Math.min(x, parent.width - width - 8))
        y = Math.max(8, Math.min(y, parent.height - height - 8))
    }
    onVisibleChanged: if (visible) clamp()
    onHeightChanged: if (visible) clamp()
    onWidthChanged: if (visible) clamp()
    background: Rectangle {
        color: Theme.bg2; border.color: Theme.line2; radius: 8
        layer.enabled: GraphicsInfo.api !== GraphicsInfo.Software
        layer.effect: MultiEffect { shadowEnabled: true; shadowColor: Theme.shadowAlpha(.55); shadowBlur: .8; shadowVerticalOffset: 18 }
    }
}
