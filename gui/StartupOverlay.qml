// Launch splash over the whole window. It shows for at least 1.75 s and fades
// once the engine is ready; if the engine could not start it stays up with the
// error and Try again / Show profile folder / Quit. Main.qml feeds it backend state.
import QtQuick
import QtQuick.Controls
import QtQuick.Window

Rectangle {
    id: loader
    objectName: "startupOverlay"
    property bool startupReady: false
    property bool startupFailed: false
    // The engine never started (e.g. it refused an older profile). The app
    // behind would be empty and unable to save, so keep covering it and say why.
    property bool engineUnavailable: false
    property string failureMessage: ""
    property string profilePath: ""
    signal retryRequested()
    signal revealProfileRequested()
    signal quitRequested()
    property string versionText: Qt.application.version ? "Version " + Qt.application.version : ""
    property bool dismissed: false
    property bool minimumElapsed: false
    property bool presented: false
    color: Theme.canvas
    visible: !dismissed
    z: 100
    function release() {
        if (dismissed) return
        if (engineUnavailable) { fade.stop(); opacity = 1 }
        else if (startupFailed) { fade.stop(); opacity = 0; dismissed = true }
        else if (startupReady && minimumElapsed && !fade.running) fade.start()
    }
    onStartupReadyChanged: release()
    onStartupFailedChanged: release()
    onEngineUnavailableChanged: release()
    Component.onCompleted: release()
    Connections {
        target: loader.Window.window
        function onFrameSwapped() {
            if (!loader.presented && loader.visible) { loader.presented = true; minimumDisplay.start() }
        }
    }
    Timer { id: minimumDisplay; interval: 1750; onTriggered: { loader.minimumElapsed = true; loader.release() } }
    NumberAnimation { id: fade; target: loader; property: "opacity"; to: 0; duration: Theme.ms(350); easing.type: Easing.OutCubic; onFinished: loader.dismissed = true }
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
                    font { family: Theme.fontFamily; pixelSize: 60; weight: Font.Medium; letterSpacing: 0 }
                    opacity: 0
                    transform: [Translate { id: offset; x: (2.17 - index * .62) * 60 }, Scale { id: zoom; origin.x: 0; origin.y: letter.height / 2; xScale: 4; yScale: 4 }]
                    SequentialAnimation {
                        running: loader.presented && !loader.dismissed
                        PauseAnimation { duration: Theme.ms(index * 110) }
                        ParallelAnimation {
                            NumberAnimation { target: letter; property: "opacity"; to: 1; duration: Theme.ms(100) }
                            NumberAnimation { target: offset; property: "x"; to: 0; duration: Theme.ms(550); easing.type: Easing.OutCubic }
                            NumberAnimation { target: zoom; properties: "xScale,yScale"; to: 1; duration: Theme.ms(550); easing.type: Easing.OutCubic }
                        }
                    }
                }
            }
        }
        Text {
            anchors.horizontalCenter: parent.horizontalCenter
            text: loader.versionText
            visible: text.length > 0
            color: Theme.inkAlpha(.4)
            font { family: Theme.fontFamily; pixelSize: 11; letterSpacing: .44 }
        }
        Column {
            objectName: "startupFailure"
            anchors.horizontalCenter: parent.horizontalCenter
            visible: loader.engineUnavailable
            topPadding: 26
            spacing: 12
            width: Math.min(520, loader.width - 48)
            Text {
                width: parent.width
                text: loader.failureMessage
                horizontalAlignment: Text.AlignHCenter
                wrapMode: Text.Wrap
                color: Theme.error
                font { family: Theme.fontFamily; pointSize: Theme.fontPoints(12.5) }
            }
            Text {
                width: parent.width
                visible: loader.profilePath.length > 0
                text: loader.profilePath
                horizontalAlignment: Text.AlignHCenter
                wrapMode: Text.WrapAnywhere
                color: Theme.subtle
                font { family: Theme.fontFamily; pointSize: Theme.fontPoints(11) }
            }
            Row {
                anchors.horizontalCenter: parent.horizontalCenter
                topPadding: 6
                spacing: 8
                FmButton { objectName: "startupRetry"; text: "Try again"; prominent: true; onClicked: loader.retryRequested() }
                FmButton { text: "Show profile folder"; outlined: true; visible: loader.profilePath.length > 0; onClicked: loader.revealProfileRequested() }
                FmButton { text: "Quit"; outlined: true; onClicked: loader.quitRequested() }
            }
        }
    }
    Accessible.role: engineUnavailable ? Accessible.AlertMessage : Accessible.ProgressBar
    Accessible.name: engineUnavailable ? failureMessage : "Opening fuckMail"
}
