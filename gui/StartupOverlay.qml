import QtQuick
import QtQuick.Controls

Rectangle {
    id: loader
    objectName: "startupOverlay"
    property bool startupReady: false
    property bool startupFailed: false
    property string versionText: Qt.application.version ? "Version " + Qt.application.version : ""
    property bool dismissed: false
    color: Theme.canvas
    visible: !dismissed
    z: 100
    function release() {
        if (dismissed) return
        if (startupFailed) { fade.stop(); opacity = 0; dismissed = true }
        else if (startupReady && !fade.running) fade.start()
    }
    onStartupReadyChanged: release()
    onStartupFailedChanged: release()
    Component.onCompleted: release()
    NumberAnimation { id: fade; target: loader; property: "opacity"; to: 0; duration: 350; easing.type: Easing.OutCubic; onFinished: loader.dismissed = true }
    MouseArea { anchors.fill: parent }
    Column {
        anchors.centerIn: parent
        spacing: 14
        Row {
            anchors.horizontalCenter: parent.horizontalCenter
            Repeater {
                model: "fuckMail".split("")
                Text {
                    id: letter
                    required property string modelData
                    required property int index
                    text: modelData
                    color: Theme.text
                    font { family: Theme.fontFamily; pixelSize: 64; weight: Font.Medium; letterSpacing: -1.92 }
                    opacity: 0
                    transform: [Translate { id: offset; x: (2.17 - index * .62) * 64 }, Scale { id: zoom; origin.x: 0; origin.y: letter.height / 2; xScale: 4; yScale: 4 }]
                    SequentialAnimation {
                        running: !loader.dismissed
                        PauseAnimation { duration: index * 110 }
                        ParallelAnimation {
                            NumberAnimation { target: letter; property: "opacity"; to: 1; duration: 100 }
                            NumberAnimation { target: offset; property: "x"; to: 0; duration: 550; easing.type: Easing.OutCubic }
                            NumberAnimation { target: zoom; properties: "xScale,yScale"; to: 1; duration: 550; easing.type: Easing.OutCubic }
                        }
                    }
                }
            }
        }
        Text {
            anchors.horizontalCenter: parent.horizontalCenter
            text: loader.versionText
            visible: text.length > 0
            color: Theme.subtle
            font { family: Theme.fontFamily; pixelSize: 12; letterSpacing: .46 }
        }
    }
    Accessible.role: Accessible.ProgressBar
    Accessible.name: "Opening fuckMail"
}
