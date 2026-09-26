import QtQuick

// Tinted boxes behind the parts of a one-line Text that match the search.
// Put it inside the Text; it clips to what the Text shows.
Item {
    id: marks
    property var phrases: []
    readonly property Item label: parent
    readonly property var ranges: phrases.length && label && label.text ? backend.matchRanges(label.text, phrases) : []
    anchors.fill: parent; z: -1; clip: true
    visible: ranges.length > 0
    FontMetrics { id: metrics; font: marks.label ? marks.label.font : Qt.application.font }
    Repeater {
        model: marks.ranges
        Rectangle {
            required property var modelData
            readonly property real startX: metrics.advanceWidth(marks.label.text.substring(0, modelData[0]))
            x: startX - 1; width: metrics.advanceWidth(marks.label.text.substring(0, modelData[1])) - startX + 2
            height: Math.round(metrics.height); y: Math.round((marks.height - height) / 2)
            radius: 2; color: Theme.matchTint
        }
    }
}
