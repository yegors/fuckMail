// One attachment card under an expanded message in MailView: an image
// thumbnail, a video poster frame or a file-type badge, plus Download. Images
// and videos emit openMedia() for AttachmentViewer; other files go to backend.openAttachment().
import QtQuick
import QtQuick.Window
import QtQuick.Controls
import QtQuick.Layouts
import QtMultimedia

Rectangle {
    id: tile
    required property var attachment
    required property string account
    property bool previewActive:visible
    property bool available: attachment.state === "available"
    property real maximumWidth: 280
    // The grid wraps before a preview exceeds 400 logical pixels. A fixed
    // per-screen size avoids decoding again on every divider/window resize.
    readonly property size thumbnailSize: Qt.size(Math.ceil(400 * Screen.devicePixelRatio), Math.ceil(250 * Screen.devicePixelRatio))
    readonly property bool isImage: /^image\//i.test(attachment.mime_type || "") || /\.(png|jpe?g|gif|webp|bmp|ico|tiff?)$/i.test(attachment.name || "")
    readonly property bool isVideo: /^video\//i.test(attachment.mime_type || "") || /\.(mp4|webm|mov|m4v|mkv|avi)$/i.test(attachment.name || "")
    readonly property bool popupOpen: false
    readonly property string extension: (attachment.name || "").split(".").pop().toUpperCase()
    readonly property color fileTint: extension === "PDF" ? Theme.error : ["DOC","DOCX","XLS","XLSX","CSV","PPT","PPTX"].indexOf(extension)>=0 ? Theme.ai : ["ZIP","RAR","7Z","TAR","GZ"].indexOf(extension)>=0 ? Theme.accent : Theme.muted
    property string previewTicket: ""
    property string previewUrl: ""
    property bool previewFinished: false
    property string saveTicket: ""
    property string feedback: ""
    property bool saveFailed: false
    property int imageWidth: 0
    property int imageHeight: 0
    property int videoDuration: 0
    // The video's first frame, kept by the backend once shown, so the decoder
    // can go and the card shows it again at once when scrolled back into view.
    property string posterUrl: ""
    // Set when this card's own decoder kept the poster: the decoder stays until
    // the kept picture is ready to show, so the card never blinks empty.
    property bool posterKept: false
    readonly property string identity: account+"/"+attachment.id
    signal openMedia(var attachment)
    objectName: "attachment_"+attachment.id
    width: maximumWidth; height: implicitHeight; implicitHeight: (width-12)*10/16+64
    radius:8; color:Theme.canvas; border.color:tileHover.hovered ? Theme.line3 : Theme.line2
    HoverHandler {id:tileHover}
    function formatSize(bytes) { return bytes>=1048576 ? (bytes/1048576).toFixed(1)+" MB" : bytes>=1024 ? Math.ceil(bytes/1024)+" KB" : bytes+" B" }
    function clock(ms) {const s=Math.floor(ms/1000);return Math.floor(s/60)+":"+String(s%60).padStart(2,"0")}
    function loadPreview() {if(previewActive && available && (isImage || isVideo) && !previewTicket && !previewFinished)previewTicket=backend.requestAttachmentPreview(account,attachment.id)}
    function open() {if (!available) return;if(isImage || isVideo)openMedia(attachment);else backend.openAttachment(attachment.id)}
    Component.onCompleted: loadPreview()
    onIdentityChanged: {previewUrl="";previewTicket="";previewFinished=false;feedback="";imageWidth=0;imageHeight=0;videoDuration=0;posterUrl="";posterKept=false;loadPreview()}
    onPreviewActiveChanged: loadPreview()
    onAvailableChanged: loadPreview()
    Connections {
        target:backend
        function onAttachmentPreviewReady(ticket,result,ok) {
            if(ticket!==tile.previewTicket)return
            tile.previewTicket="";tile.previewFinished=true
            if(ok && result.account===tile.account && result.id===tile.attachment.id) {
                // A poster kept earlier this run shows without decoding again.
                const kept=tile.isVideo && result.preview_url ? backend.videoPoster(tile.account,tile.attachment.id) : ({})
                if(kept.url) {tile.posterUrl=kept.url;tile.videoDuration=kept.duration||0}
                tile.previewUrl=result.preview_url||"";tile.imageWidth=result.width||0;tile.imageHeight=result.height||0
            }
        }
        function onAttachmentSaved(ticket,error) {if(ticket!==tile.saveTicket)return;tile.saveTicket="";tile.saveFailed=!!error;tile.feedback=error}
    }
    ColumnLayout {
        anchors.fill:parent;spacing:0
        Button {
            id:previewButton;objectName:"attachmentPreview_"+tile.attachment.id
            Layout.fillWidth:true;Layout.leftMargin:6;Layout.rightMargin:6;Layout.topMargin:6;Layout.preferredHeight:(tile.width-12)*10/16
            enabled:tile.available;padding:0;clip:true
            Accessible.name:"Open "+tile.attachment.name
            background:Rectangle {radius:5;color:tile.isImage || tile.isVideo ? Theme.bg4 : Theme.bg2;border.color:Theme.line}
            contentItem:Item {
                Image {id:thumbnail;objectName:"attachmentThumbnail_"+tile.attachment.id;anchors.fill:parent;visible:tile.isImage;source:tile.isImage ? tile.previewUrl : "";asynchronous:true;autoTransform:true;sourceSize:tile.thumbnailSize;fillMode:Image.PreserveAspectCrop}
                Image {id:posterImage;objectName:"attachmentPosterFrame_"+tile.attachment.id;anchors.fill:parent;visible:tile.isVideo && !!tile.posterUrl;source:tile.posterUrl;asynchronous:true;sourceSize:tile.thumbnailSize;fillMode:Image.PreserveAspectCrop}
                Loader {
                    id:videoPoster;anchors.fill:parent;active:tile.previewActive && tile.isVideo && !!tile.previewUrl && (!tile.posterUrl || (tile.posterKept && posterImage.status!==Image.Ready))
                    sourceComponent:Component {
                        VideoOutput {
                            id:poster;objectName:"attachmentPoster_"+tile.attachment.id;fillMode:VideoOutput.PreserveAspectCrop
                            // Decode until the first frame shows, however long that takes while
                            // other videos decode, then keep it and let the decoder go.
                            property bool framed:false
                            Connections {target:poster.videoSink;function onVideoFrameChanged() {if(!poster.framed && poster.videoSink.videoSize.width>0){poster.framed=true;posterStop.start()}}}
                            Loader {
                                id:decoder;objectName:"attachmentPosterDecoder";active:!posterStop.done
                                sourceComponent:MediaPlayer {
                                    source:tile.previewUrl;videoOutput:poster
                                    onDurationChanged:function() {tile.videoDuration=duration}
                                    onMediaStatusChanged:function() {if(mediaStatus===MediaPlayer.LoadedMedia) play();else if(mediaStatus===MediaPlayer.EndOfMedia || mediaStatus===MediaPlayer.InvalidMedia) posterStop.done=true}
                                    onErrorOccurred:posterStop.done=true
                                }
                            }
                            // If the frame cannot be kept, the paused player goes on showing it.
                            Timer {id:posterStop;interval:100;property bool done:false;onTriggered:{if(decoder.item)decoder.item.pause();const kept=backend.keepVideoPoster(poster.videoSink,tile.account,tile.attachment.id,tile.videoDuration);if(kept.url){tile.posterKept=true;tile.posterUrl=kept.url}}}
                        }
                    }
                }
                Column {
                    anchors.centerIn:parent;visible:!tile.isImage && !tile.isVideo;spacing:0
                    FmIcon {name:"file";size:36;color:tile.fileTint;anchors.horizontalCenter:parent.horizontalCenter}
                    Text {text:tile.extension;anchors.horizontalCenter:parent.horizontalCenter;color:tile.fileTint;font.pointSize:Theme.fontPoints(10.5);font.weight:Font.Medium;font.letterSpacing:.6}
                }
                FmIcon {anchors.centerIn:parent;visible:tile.isImage && !tile.previewUrl;name:"image";size:22;color:Theme.inkAlpha(.22)}
                Rectangle {anchors.centerIn:parent;visible:tile.isVideo;width:38;height:38;radius:19;color:Theme.canvasAlpha(.75);border.color:Theme.inkAlpha(.25);FmIcon {anchors.centerIn:parent;name:"play";size:12;color:Theme.text}}
                Rectangle {visible:tile.isVideo && tile.videoDuration>0;anchors.right:parent.right;anchors.bottom:parent.bottom;anchors.margins:6;width:durationText.implicitWidth+10;height:18;radius:3;color:Theme.canvasAlpha(.75);Text {id:durationText;anchors.centerIn:parent;text:tile.clock(tile.videoDuration);color:Theme.text;font.pointSize:Theme.fontPoints(10.5);font.features:{"tnum":1}}}
            }
            onClicked:tile.open()
        }
        RowLayout {
            Layout.fillWidth:true;Layout.leftMargin:12;Layout.rightMargin:8;Layout.topMargin:9;Layout.bottomMargin:9;spacing:8
            AbstractButton {
                id:filenameButton;objectName:"attachmentOpen_"+tile.attachment.id;Layout.fillWidth:true;implicitHeight:34;enabled:tile.available
                contentItem:Column {
                    spacing:2
                    Text {width:parent.width;text:tile.attachment.name;elide:Text.ElideRight;color:Theme.text;font.pointSize:Theme.fontPoints(12);textFormat:Text.PlainText}
                    Text {objectName:"attachmentStatus_"+tile.attachment.id;width:parent.width;text:tile.feedback || (!tile.available ? "Available after sync" : tile.formatSize(tile.attachment.size)+" · "+(tile.imageWidth ? tile.imageWidth+" × "+tile.imageHeight : tile.isVideo && tile.videoDuration>0 ? tile.clock(tile.videoDuration) : tile.extension));color:tile.saveFailed ? Theme.error : Theme.inkAlpha(.45);font.pointSize:Theme.fontPoints(11);elide:Text.ElideRight}
                }
                onClicked:tile.open()
                FmToolTip {shown:filenameButton.hovered;text:tile.feedback || tile.attachment.name}
            }
            MessageHeaderButton {objectName:"attachmentSave_"+tile.attachment.id;iconName:"download";iconSize:13;enabled:tile.available && !tile.saveTicket;tip:"Download";onClicked:tile.saveTicket=backend.downloadAttachment(tile.account,tile.attachment.id,tile.attachment.name)}
        }
    }
}
