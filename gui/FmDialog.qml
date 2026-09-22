import QtQuick
import QtQuick.Controls
import QtQuick.Effects

Dialog {
    id: dialog
    property bool destructive: false
    property string titleObjectName: ""
    parent: Overlay.overlay
    anchors.centerIn: parent
    modal: true
    focus: true
    width: Math.min(360, parent ? parent.width - 48 : 360)
    padding: 22
    topPadding: title.length ? 0 : 22
    bottomPadding: footer && footer.visible ? 0 : 18
    spacing: 8
    implicitHeight: topPadding + bottomPadding + implicitContentHeight
        + (header && header.visible ? header.implicitHeight + spacing : 0)
        + (footer && footer.visible ? footer.implicitHeight + spacing : 0)
    closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
    font.family: Theme.fontFamily
    font.pixelSize: Theme.fontSize(13)
    palette.windowText: Theme.text
    palette.text: Theme.text
    palette.buttonText: Theme.text
    header: Label {
        objectName: dialog.titleObjectName
        width: dialog.width
        text: dialog.title
        visible: text.length > 0
        topPadding: 22; leftPadding: 22; rightPadding: 22; bottomPadding: 0
        wrapMode: Text.Wrap
        textFormat: Text.PlainText
        color: Theme.text
        font.family: Theme.fontFamily
        font.pixelSize: Theme.fontSize(16)
        font.weight: Font.Medium
        font.letterSpacing: -0.16
        lineHeight: 1.3
    }
    background: Rectangle {
        radius: 8; color: Theme.bg2; border.color: Theme.line2
        layer.enabled: GraphicsInfo.api !== GraphicsInfo.Software
        layer.effect: MultiEffect {
            shadowEnabled: true; shadowColor: Theme.shadowAlpha(0.6)
            shadowBlur: 1; shadowVerticalOffset: 24
        }
    }
    Overlay.modal: Rectangle { color: Theme.shadowAlpha(0.45) }
    footer: DialogButtonBox {
        implicitHeight: visible ? 32 + topPadding + bottomPadding : 0
        visible: count > 0
        alignment: Qt.AlignRight
        spacing: 6
        leftPadding: 22; rightPadding: 22; topPadding: 14; bottomPadding: 18
        background: null
        delegate: FmButton {
            readonly property bool affirmative: DialogButtonBox.buttonRole === DialogButtonBox.AcceptRole || DialogButtonBox.buttonRole === DialogButtonBox.DestructiveRole || DialogButtonBox.buttonRole === DialogButtonBox.YesRole
            implicitHeight: 32
            leftPadding: 14; rightPadding: 14
            font.pixelSize: Theme.fontSize(12.5); font.weight: Font.Medium
            prominent: affirmative
            shortcutHint: affirmative ? "↵" : ""
            outlined: !affirmative
            foreground: affirmative && dialog.destructive ? "#ffffff" : affirmative ? Theme.canvas : Theme.text
            background: Rectangle {
                radius: 5
                color: parent.affirmative ? (dialog.destructive ? Theme.error : Theme.text) : parent.hovered ? Theme.bg4 : "transparent"
                opacity: parent.down ? 0.8 : parent.hovered ? 0.88 : 1
                border.width: !parent.affirmative || parent.activeFocus ? 1 : 0
                border.color: parent.activeFocus ? Theme.focusBorder : Theme.line2
            }
        }
    }
}
