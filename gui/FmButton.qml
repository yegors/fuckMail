import QtQuick
import QtQuick.Controls
import QtQuick.Templates as T

// Lists, grids and dialogs repeat this button hundreds of times, so a plain
// label builds only its text and background; the icon, shortcut hint and
// hover help are created when used. The layout is a row of
// [icon][label][hint], centred as a group unless aligned left.
T.ToolButton {
    id: control
    property bool selected: false
    property bool prominent: false
    property bool outlined: false
    property string tip: ""
    property string iconName: ""
    property int iconSize: 14
    property int iconSpacing: 7
    // Trailing icons follow the label, e.g. a picker's chevron.
    property bool iconTrailing: false
    property color iconColor: foreground
    property string shortcutHint: ""
    property color foreground: !enabled ? (prominent ? Theme.muted : Theme.subtle) : prominent ? Theme.canvas : selected || outlined ? Theme.text : Theme.muted
    property int textAlignment: Text.AlignHCenter
    property real radius: 5
    // Hover lifts the whole button: main (prominent) actions a little, the
    // rest more. 1 turns it off.
    property real hoverBrightness: prominent ? 1.12 : 1.3
    // No member function here: bindings of a delegate being destroyed must not call into it.
    readonly property real lift: hovered && enabled ? hoverBrightness : 1
    implicitHeight: Math.max(32, contentItem.implicitHeight + 10)
    implicitWidth: contentItem.implicitWidth + leftPadding + rightPadding
    // The Basic style's ToolButton defaults.
    padding: 6
    spacing: 6
    leftPadding: 11
    rightPadding: 11
    font.family: Theme.fontFamily
    font.pointSize: Theme.fontPoints(13)
    font.weight: selected || prominent ? Font.Medium : Font.Normal
    hoverEnabled: true
    contentItem: Item {
        id: content
        implicitWidth: label.implicitWidth + (row.item ? row.item.extraWidth : 0)
        implicitHeight: Math.max(label.implicitHeight, control.iconSize)
        // Centre on whole pixels, the way anchors.centerIn does.
        function centred(outer, inner) {
            const half = function(size) { return Math.trunc(size) % 2 ? (size + 1) / 2 : size / 2 }
            return Math.round(half(outer) - half(inner))
        }
        Text {
            id: label
            // A Row skips and hides a zero-width item; unconstrained, text would draw in full.
            visible: width > 0
            x: row.item ? row.item.labelX : control.textAlignment === Text.AlignLeft ? 0 : Math.max(0, (content.width - width) / 2)
            y: row.item ? row.item.y : content.centred(content.height, implicitHeight)
            width: row.item ? row.item.labelWidth : Math.min(implicitWidth, content.width)
            text: control.text
            font: control.font
            color: control.lift !== 1 ? Theme.brighten(control.foreground, control.lift) : control.foreground
            verticalAlignment: Text.AlignVCenter
            textFormat: Text.PlainText
            elide: Text.ElideRight
        }
        // The rest of the row, only for buttons with an icon or a shortcut hint.
        Loader {
            id: row
            active: control.iconName.length > 0 || control.shortcutHint.length > 0
            sourceComponent: Item {
                readonly property bool hasIcon: control.iconName.length > 0
                readonly property real gap: control.text.length && hasIcon ? control.iconSpacing : 0
                readonly property real iconPart: hasIcon ? control.iconSize + gap : 0
                readonly property real hintWidth: hint.item ? hint.item.textWidth + 8 : 0
                readonly property real extraWidth: iconPart + hintWidth
                readonly property real labelWidth: Math.max(0, width - hintWidth - iconPart)
                readonly property real labelOffset: hasIcon ? control.iconSize + gap : 0
                readonly property real labelX: x + placed(labelOffset, labelWidth)
                // Row offsets run left to right; a trailing icon mirrors them.
                function placed(offset, size) { return control.iconTrailing ? width - offset - size : offset }
                width: Math.min(label.implicitWidth + extraWidth, content.width)
                height: Math.max(labelWidth > 0 ? label.implicitHeight : 0, hasIcon ? control.iconSize : 0,
                    hint.active ? Math.max(label.implicitHeight, control.iconSize) : 0)
                x: control.textAlignment === Text.AlignLeft ? 0 : Math.max(0, (content.width - width) / 2)
                y: content.centred(content.height, height)
                FmIcon {
                    visible: parent.hasIcon
                    x: parent.placed(0, width); anchors.verticalCenter: parent.verticalCenter
                    name: control.iconName; size: control.iconSize; color: control.lift !== 1 ? Theme.brighten(control.iconColor, control.lift) : control.iconColor
                }
                Loader {
                    id: hint
                    active: control.shortcutHint.length > 0
                    x: parent.placed(parent.labelWidth > 0 ? parent.labelOffset + parent.labelWidth + parent.gap : parent.labelOffset, width)
                    width: parent.hintWidth; height: Math.max(label.implicitHeight, control.iconSize)
                    sourceComponent: Item {
                        readonly property real textWidth: hintText.implicitWidth
                        Text { id: hintText; anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter; text: control.shortcutHint; color: control.foreground; opacity: 0.55; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(11) }
                    }
                }
            }
        }
    }
    background: Rectangle {
        radius: control.radius
        color: control.prominent ? (control.enabled ? Theme.brighten(Theme.text, control.lift) : Theme.controlBorder) : control.down || control.selected ? Theme.brighten(Theme.selected, control.lift) : control.hovered ? Theme.surface : "transparent"
        border.width: control.activeFocus || control.outlined ? 1 : 0
        border.color: control.activeFocus ? Theme.focusBorder : Theme.controlBorder
    }
    Loader { active: control.hovered && control.tip.length > 0; sourceComponent: FmToolTip { target: control; shown: true; text: control.tip } }
    Accessible.name: text.length ? text : tip
}
