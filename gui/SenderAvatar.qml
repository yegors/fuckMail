import QtQuick
import QtQuick.Controls
import QtQuick.Shapes
import QtQuick.Window

// A sender's round picture. When the engine finds one, an automated sender's
// brand icon sits on white and a person's photo fills the circle; otherwise
// initials show on a stable hue from the name. Callers can restyle the
// initials disc (the thread keeps its neutral one). `own` messages keep the
// account styling and nothing is looked up for them.
Item {
    id: avatar
    property string name: ""
    property string email: ""
    // people, automated or unknown, as the list classifies the sender.
    property string kind: "unknown"
    property bool own: false
    property int diameter: 28
    // The brand icon's side inside the white disc.
    property real brandSize: diameter - 10
    readonly property real hue: Theme.nameHue(name || email)
    property color discColor: Theme.oklch(.36, .06, hue)
    property color initialsColor: Theme.oklch(.9, .05, hue)
    property string initials: Theme.initialsOf(name || email)
    property real fontPoints: Theme.fontPoints(10.5)
    property int fontWeight: Font.Medium
    property real letterSpacing: 0
    property bool tipEnabled: false
    readonly property var service: backend.htmlContent || null
    readonly property var picture: !own && service && service.pictureRevision >= 0 && email.length ? service.senderPicture(email, kind) : ({})
    readonly property bool brand: picture.source === "brand"
    readonly property bool imageShown: picture.state === "ready" && image.status === Image.Ready
    readonly property string sourceLabel: picture.label || "No photo · initials"
    implicitWidth: diameter; implicitHeight: diameter
    width: implicitWidth; height: implicitHeight
    // A found picture shows at 75% so it is not too bright on the dark themes.
    readonly property real pictureOpacity: .75
    Rectangle {
        anchors.fill: parent; radius: width / 2
        color: !avatar.imageShown ? avatar.discColor : avatar.brand ? "#ffffff" : "transparent"
        opacity: avatar.imageShown ? avatar.pictureOpacity : 1
    }
    Text {
        visible: !avatar.imageShown
        anchors.centerIn: parent
        text: avatar.initials; color: avatar.initialsColor; textFormat: Text.PlainText
        font.family: Theme.fontFamily; font.pointSize: avatar.fontPoints; font.weight: avatar.fontWeight; font.letterSpacing: avatar.letterSpacing
    }
    Image {
        id: image
        objectName: "senderPicture"
        width: avatar.brand ? avatar.brandSize : avatar.diameter; height: width
        x: (avatar.diameter - width) / 2; y: x
        source: avatar.picture.state === "ready" ? avatar.picture.url : ""
        sourceSize: Qt.size(64, 64)
        fillMode: Image.PreserveAspectCrop; asynchronous: true; smooth: true; mipmap: true
        // Brand icons sit inside the disc; photos are cut to the circle below,
        // except in the software renderer, which cannot fill a shape with one.
        visible: avatar.imageShown && (avatar.brand || GraphicsInfo.api === GraphicsInfo.Software)
        opacity: avatar.pictureOpacity
    }
    // A shape fill draws its texture at the picture's pixel size, so a 64px
    // photo showed only its top-left corner in a 28px circle. Scale it to
    // cover the circle and centre it, as the crop fill mode would.
    readonly property real textureWidth: image.implicitWidth / Screen.devicePixelRatio
    readonly property real textureHeight: image.implicitHeight / Screen.devicePixelRatio
    readonly property real cover: diameter / Math.max(1, Math.min(textureWidth, textureHeight))
    Shape {
        visible: avatar.imageShown && !avatar.brand && GraphicsInfo.api !== GraphicsInfo.Software
        opacity: avatar.pictureOpacity
        anchors.fill: parent
        preferredRendererType: Shape.CurveRenderer
        ShapePath {
            strokeColor: "transparent"; fillItem: image
            fillTransform: Qt.matrix4x4(avatar.cover, 0, 0, (avatar.diameter - avatar.textureWidth * avatar.cover) / 2,
                                        0, avatar.cover, 0, (avatar.diameter - avatar.textureHeight * avatar.cover) / 2,
                                        0, 0, 1, 0, 0, 0, 0, 1)
            PathAngleArc { centerX: avatar.diameter / 2; centerY: avatar.diameter / 2; radiusX: avatar.diameter / 2; radiusY: radiusX; startAngle: 0; sweepAngle: 360 }
        }
    }
    // Where the picture came from, or that there is none.
    HoverHandler { id: hover; enabled: avatar.tipEnabled && !avatar.own }
    FmToolTip { shown: hover.hovered; text: avatar.sourceLabel }
}
