import QtQuick

// The message header's summarize action: a 26px icon in the AI colour, shown
// only while the message has no summary and none is running.
MessageHeaderButton {
    id: button
    property string modelName: ""
    iconName: "summary"; iconSize: 13
    foreground: Theme.ai
    tip: "Summarize with " + (modelName || "the local model") + " — runs on this machine"
    Accessible.name: "Summarize"
    background: Rectangle { radius: 5; color: button.hovered ? Theme.withAlpha(Theme.ai, .12) : "transparent"; border.width: button.visualFocus ? 1 : 0; border.color: Theme.focusBorder }
}
