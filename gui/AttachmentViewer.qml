// Full-window viewer for a message's image and video attachments, opened by
// MailView with show(account, items, index); step() moves between them.
// Previews come from backend.requestAttachmentPreview().
import QtQuick
import QtQuick.Window
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Effects
import QtMultimedia

Popup {
    id: viewer; objectName: "attachmentViewer"
    parent: Overlay.overlay
    x: 0; y: 0; width: parent ? parent.width : 0; height: parent ? parent.height : 0
    modal: true; focus: true; padding: 0
    closePolicy: Popup.CloseOnEscape
    property var backdrop: null
    property string account: ""
    property var items: []
    property int index: 0
    property string ticket: ""
    property var preview: ({})
    property string error: ""
    property bool preparingPoster:false
    // The player and its video output are created for the first video shown in
    // a viewing and released on close, so images and startup never load the
    // media backend.
    property bool mediaActive: false
    readonly property var player: playerLoader.item
    readonly property var audio: player ? player.audioOutput : null
    readonly property bool playing: !!player && player.playbackState === MediaPlayer.PlayingState
    readonly property var attachment: items[index] || ({})
    readonly property bool video: isVideo(attachment)
    function isVideo(item) { return /^video\//i.test(item.mime_type || "") || /\.(mp4|webm|mov|m4v|mkv|avi)$/i.test(item.name || "") }
    readonly property real ratio: video ? (videoLoader.item && videoLoader.item.sourceRect.width > 0 && videoLoader.item.sourceRect.height > 0 ? videoLoader.item.sourceRect.width/videoLoader.item.sourceRect.height : 16/9) : preview.width && preview.height ? preview.width/preview.height : 4/3
    function show(accountId,media,at) { account=accountId;items=media;index=at;open();load() }
    function load() {
        // Read the item itself: from onIndexChanged, the `attachment` and
        // `video` bindings may still describe the previous one.
        const item=items[index] || ({})
        preparingPoster=false; if (player) { player.stop(); player.source="" } preview={}; error=""
        if (isVideo(item)) mediaActive=true
        ticket=opened && item.id ? backend.requestAttachmentPreview(account,item.id,isVideo(item)) : ""
    }
    function step(delta) { if (!items.length) return;index=(index+delta+items.length)%items.length }
    function togglePlayback() { if (video && player) {preparingPoster=false;posterReady.stop();playing ? player.pause() : player.play()} }
    function clock(ms) { const s=Math.max(0,Math.floor(ms/1000));return Math.floor(s/60)+":"+String(s%60).padStart(2,"0") }
    function size(bytes) { return bytes >= 1048576 ? (bytes/1048576).toFixed(1)+" MB" : bytes >= 1024 ? Math.ceil(bytes/1024)+" KB" : bytes+" B" }
    onIndexChanged: if (opened) load()
    onClosed: { preparingPoster=false; posterReady.stop(); if (player) { player.stop(); player.source="" } mediaActive=false; ticket=""; preview={}; items=[] }
    Connections {
        target: backend
        function onAccountChanged() { viewer.close() }
        function onAttachmentPreviewReady(reply,result,ok) {
            if (reply !== viewer.ticket) return
            viewer.ticket=""
            const item=viewer.items[viewer.index] || ({})
            if (!ok || result.account !== viewer.account || result.id !== item.id || !result.preview_url) { viewer.error=result.preview_error || result.message || "Preview unavailable";return }
            // An error left by the previous item's player no longer applies.
            viewer.error=""
            viewer.preview=result
            if (viewer.isVideo(item)) {viewer.preparingPoster=true; viewer.mediaActive=true; viewer.player.source=result.preview_url}
        }
    }
    background: Rectangle { color: "#e80e0f11" }
    contentItem: Item {
        property bool blocksWorkspaceShortcuts: true
        focus: true
        Keys.onPressed: function(event) {
            if (event.key === Qt.Key_Left) viewer.step(-1)
            else if (event.key === Qt.Key_Right) viewer.step(1)
            else if (event.key === Qt.Key_Space || event.key === Qt.Key_K) viewer.togglePlayback()
            else if (event.key === Qt.Key_M && viewer.video && viewer.audio) viewer.audio.muted=!viewer.audio.muted
            else if (event.key === Qt.Key_Escape) viewer.close()
            else return
            event.accepted=true
        }
        MultiEffect { anchors.fill:parent; source:viewer.backdrop; blurEnabled:true; blur:1; blurMax:24; opacity:.18; visible:GraphicsInfo.api !== GraphicsInfo.Software && !!viewer.backdrop }
        MouseArea { anchors.fill:parent; onClicked:viewer.close() }
        FmButton { anchors.top:parent.top; anchors.right:parent.right; anchors.margins:22; iconName:"close"; implicitWidth:32; tip:"Close · Esc"; foreground:"#e9eaec"; onClicked:viewer.close() }
        ColumnLayout {
            id: frameColumn; anchors.centerIn:parent
            width:Math.max(0,Math.min(viewer.width-120,(viewer.height-140)*viewer.ratio,1400)); spacing:16
            Rectangle {
                id: frame; Layout.fillWidth:true; Layout.preferredHeight:Math.max(0,Math.min(viewer.height-140,width/viewer.ratio)); radius:6; color:"#090a0c"; clip:true
                Image {
                    objectName:"attachmentViewerImage"; anchors.fill:parent; visible:!viewer.video
                    source:viewer.video ? "" : viewer.preview.preview_url || ""
                    asynchronous:true; retainWhileLoading:true; fillMode:Image.PreserveAspectFit
                    // Cover the physical display pixels, with coarse steps so a
                    // resize does not request a new decode for each pixel moved.
                    sourceSize:Qt.size(Math.ceil(frame.width*Screen.devicePixelRatio/128)*128,Math.ceil(frame.height*Screen.devicePixelRatio/128)*128)
                }
                Loader { id: videoLoader; anchors.fill:parent; active:viewer.mediaActive; visible:viewer.video; sourceComponent: VideoOutput { fillMode:VideoOutput.PreserveAspectFit } }
                MouseArea { anchors.fill:parent; hoverEnabled:true; onClicked:viewer.togglePlayback() }
                HoverHandler { id: mediaHover }
                FmButton {
                    anchors.centerIn:parent; visible:viewer.video && !viewer.playing && !viewer.error.length
                    implicitWidth:64; implicitHeight:64; iconName:"play"; iconSize:22; foreground:"#e9eaec"
                    background:Rectangle { radius:32; color:"#b00e0f11";border.color:"#60e9eaec" }
                    onClicked:viewer.togglePlayback()
                }
                Text { anchors.centerIn:parent; width:parent.width-32; text:viewer.error || (viewer.ticket ? "Loading…" : ""); color:"#e9eaec"; wrapMode:Text.Wrap; horizontalAlignment:Text.AlignHCenter; font.pointSize:Theme.fontPoints(12) }
                Rectangle {
                    visible:viewer.video; anchors.left:parent.left;anchors.right:parent.right;anchors.bottom:parent.bottom;height:44
                    color:"#d00e0f11"; opacity:!viewer.playing || mediaHover.hovered || controlsHover.hovered ? 1 : 0
                    Behavior on opacity { NumberAnimation {duration: Theme.ms(180); easing.type: Theme.enterEasing} }
                    HoverHandler {id:controlsHover}
                    RowLayout {
                        anchors.fill:parent;anchors.leftMargin:8;anchors.rightMargin:8;spacing:10
                        FmButton {iconName:viewer.playing ? "pause" : "play";foreground:"#e9eaec";implicitWidth:28;tip:"Play / pause · Space";onClicked:viewer.togglePlayback()}
                        Slider {
                            id:seek;Layout.fillWidth:true;from:0;to:Math.max(1,viewer.player ? viewer.player.duration : 0);value:viewer.player ? viewer.player.position : 0
                            enabled:!!viewer.player && viewer.player.seekable;onMoved:viewer.player.position=value
                            background:Rectangle { x:seek.leftPadding;y:seek.topPadding+seek.availableHeight/2-2;width:seek.availableWidth;height:3;color:"#3a3d44";radius:2;Rectangle {width:seek.visualPosition*parent.width;height:parent.height;radius:2;color:Theme.accent} }
                            handle:Rectangle {x:seek.leftPadding+seek.visualPosition*(seek.availableWidth-width);y:seek.topPadding+seek.availableHeight/2-4;width:8;height:8;radius:4;color:Theme.accent}
                        }
                        Text {text:viewer.player ? viewer.clock(viewer.player.position)+" / "+viewer.clock(viewer.player.duration) : viewer.clock(0)+" / "+viewer.clock(0);color:"#b8b9bd";font.pointSize:Theme.fontPoints(11);font.features:{"tnum":1}}
                        FmButton {iconName:viewer.audio && viewer.audio.muted ? "mute" : "volume";foreground:"#e9eaec";implicitWidth:28;tip:"Mute · M";onClicked:if (viewer.audio) viewer.audio.muted=!viewer.audio.muted}
                    }
                }
            }
            RowLayout {
                Layout.fillWidth:true;spacing:16
                Text {Layout.fillWidth:true;Layout.minimumWidth:0;elide:Text.ElideRight;text:[viewer.attachment.name,viewer.preview.width ? viewer.preview.width+" × "+viewer.preview.height : "",viewer.size(viewer.attachment.size || 0)].filter(Boolean).join(" · ");color:"#a5a6a9";font.pointSize:Theme.fontPoints(12);textFormat:Text.PlainText}
                Text {text:(viewer.index+1)+" of "+viewer.items.length;color:"#737479";font.pointSize:Theme.fontPoints(11.5)}
                FmButton {text:"Download";foreground:"#c9a36b";onClicked:backend.downloadAttachment(viewer.account,viewer.attachment.id,viewer.attachment.name)}
            }
        }
        FmButton {anchors.left:parent.left;anchors.leftMargin:24;anchors.verticalCenter:parent.verticalCenter;visible:viewer.items.length>1;iconName:"previous";foreground:"#e9eaec";tip:"Previous · ←";onClicked:viewer.step(-1)}
        FmButton {anchors.right:parent.right;anchors.rightMargin:24;anchors.verticalCenter:parent.verticalCenter;visible:viewer.items.length>1;iconName:"next";foreground:"#e9eaec";tip:"Next · →";onClicked:viewer.step(1)}
    }
    Loader {
        id: playerLoader; active: viewer.mediaActive
        sourceComponent: MediaPlayer { objectName:"attachmentMediaPlayer";videoOutput:videoLoader.item;audioOutput:AudioOutput {objectName:"attachmentAudio"}
            onMediaStatusChanged: if (mediaStatus===MediaPlayer.LoadedMedia && viewer.preparingPoster) {audioOutput.muted=true;play();posterReady.start()}
            // Clearing the source between items is not a failure to show.
            onErrorOccurred:{if (!source.toString()) return;viewer.preparingPoster=false;viewer.error=errorString}
        }
    }
    Timer {id:posterReady;interval:100;onTriggered:{if(viewer.preparingPoster && viewer.player) {viewer.preparingPoster=false;viewer.player.pause();viewer.player.position=0;viewer.audio.muted=false}}}
}
