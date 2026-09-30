import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

// The right-click menu for text, in the same style as the mail list's menu.
// `target` is a TextEdit, TextArea or TextField, or the HTML view, which
// copies through copySelection(). Editable text adds Undo, Redo, Cut and
// Paste; `extras` rows (such as the composer's formatting) follow Paste,
// `leading` rows (such as spelling suggestions) come first, and `pasteText`
// can replace the plain Paste (the composer pastes plain text).
FmMenu {
    id: menu
    objectName: "textMenu"
    property Item target: null
    default property alias extras: extraRows.data
    property alias leading: leadingRows.data
    property string pasteLabel: "Paste"
    property var pasteText: null
    readonly property bool editable: !!target && target.readOnly === false
    readonly property bool selected: !!target && (target.hasSelection !== undefined ? target.hasSelection : String(target.selectedText || "").length > 0)
    readonly property string key: Qt.platform.os === "osx" ? "⌘" : "Ctrl+"
    parent: Overlay.overlay
    minimumWidth: 180
    // Opens at `x`, `y` in `item`, which becomes the target.
    function show(item, x, y) {
        target = item
        const point = item.mapToItem(parent, x, y)
        menu.x = point.x; menu.y = point.y
        open()
    }
    function run(action) { close(); if (target) action(target) }
    contentItem: ColumnLayout {
        spacing: 0
        ColumnLayout { id: leadingRows; Layout.fillWidth: true; spacing: 0 }
        FmMenuItem { objectName: "textMenuUndo"; visible: menu.editable; separatorAbove: leadingRows.implicitHeight > 0; text: "Undo"; shortcut: menu.key + "Z"; enabled: menu.editable && menu.target.canUndo; onClicked: menu.run(function(t) { t.undo() }) }
        FmMenuItem { objectName: "textMenuRedo"; visible: menu.editable; text: "Redo"; shortcut: menu.key + (Qt.platform.os === "osx" ? "⇧Z" : "Y"); enabled: menu.editable && menu.target.canRedo; onClicked: menu.run(function(t) { t.redo() }) }
        FmMenuItem { objectName: "textMenuCut"; visible: menu.editable; separatorAbove: true; text: "Cut"; shortcut: menu.key + "X"; enabled: menu.selected; onClicked: menu.run(function(t) { t.cut() }) }
        FmMenuItem { objectName: "textMenuCopy"; text: "Copy"; shortcut: menu.key + "C"; enabled: menu.selected; onClicked: menu.run(function(t) { if (t.copySelection) t.copySelection(); else t.copy() }) }
        FmMenuItem { objectName: "textMenuPaste"; visible: menu.editable; text: menu.pasteLabel; shortcut: menu.key + "V"; enabled: menu.editable && menu.target.canPaste; onClicked: menu.run(function(t) { if (menu.pasteText) menu.pasteText(); else t.paste() }) }
        ColumnLayout { id: extraRows; Layout.fillWidth: true; spacing: 0; visible: children.length > 0 }
        FmMenuItem { objectName: "textMenuSelectAll"; separatorAbove: true; text: "Select all"; shortcut: menu.key + "A"; onClicked: menu.run(function(t) { t.selectAll() }) }
    }
}
