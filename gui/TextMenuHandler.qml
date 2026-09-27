import QtQuick

// A right click on the text item this sits in opens the app's text menu,
// made on first use. The item sets `ContextMenu.menu: null` so Qt's own menu
// stays away.
TapHandler {
    id: handler
    acceptedButtons: Qt.RightButton
    property var menu: null
    readonly property Component menuComponent: Component { TextMenu {} }
    onTapped: function(point) {
        if (!menu) menu = menuComponent.createObject(handler.target)
        menu.show(handler.target, point.position.x, point.position.y)
    }
}
