// One attachment card under an expanded message in MailView: an image
// thumbnail, a video poster frame or a file-type badge, plus Download. Images
// and videos emit openMedia() for AttachmentViewer; other files go to backend.openAttachment().
import QtQuick
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
    onIdentityChanged: {previewUrl="";previewTicket="";previewFinished=false;feedback="";imageWidth=0;imageHeight=0;videoDuration=0;loadPreview()}
    onPreviewActiveChanged: loadPreview()
    onAvailableChanged: loadPreview()
    Connections {
        target:backend
        function onAttachmentPreviewReady(ticket,result,ok) {
            if(ticket!==tile.previewTicket)return
            tile.previewTicket="";tile.previewFinished=true
            if(ok && result.account===tile.account && result.id===tile.attachment.id) {tile.previewUrl=result.preview_url||"";tile.imageWidth=result.width||0;tile.imageHeight=result.height||0}
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
                Image {id:thumbnail;objectName:"attachmentThumbnail_"+tile.attachment.id;anchors.fill:parent;visible:tile.isImage;source:tile.isImage ? tile.previewUrl : "";asynchronous:true;autoTransform:true;sourceSize:Qt.size(760,475);fillMode:Image.PreserveAspectCrop}
                Loader {
                    id:videoPoster;anchors.fill:parent;active:tile.previewActive && tile.isVideo && !!tile.previewUrl
                    sourceComponent:Component {
                        VideoOutput {
                            id:poster;fillMode:VideoOutput.PreserveAspectCrop
                            // The decoder runs only until the poster frame arrives; the output keeps that frame.
                            property bool framed:false
                            Connections {target:poster.videoSink;function onVideoFrameChanged() {poster.framed=true}}
                            Loader {
                                id:decoder;active:!(poster.framed && posterStop.done)
                                sourceComponent:MediaPlayer {
                                    source:tile.previewUrl;videoOutput:poster
                                    onDurationChanged:tile.videoDuration=duration
                                    onMediaStatusChanged:if(mediaStatus===MediaPlayer.LoadedMedia) {play();posterStop.start()}
                                }
                            }
                            Timer {id:posterStop;interval:100;property bool done:false;onTriggered:{if(decoder.item)decoder.item.pause();done=true}}
                        }
                    }
                }
                Column {
                    anchors.centerIn:parent;visible:!tile.isImage && !tile.isVideo;spacing:0
                    FmIcon {name:"file";size:36;color:tile.fileTint;anchors.horizontalCenter:parent.horizontalCenter}
                    Text {text:tile.extension;anchors.horizontalCenter:parent.horizontalCenter;color:tile.fileTint;font.pointSize:Theme.fontPoints(9.5);font.weight:Font.DemiBold;font.letterSpacing:.6}
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
                    Text {width:parent.width;text:tile.attachment.name;elide:Text.ElideRight;color:Theme.text;font.pointSize:Theme.fontPoints(12.5);textFormat:Text.PlainText}
                    Text {objectName:"attachmentStatus_"+tile.attachment.id;width:parent.width;text:tile.feedback || (!tile.available ? "Available after sync" : tile.formatSize(tile.attachment.size)+" · "+(tile.imageWidth ? tile.imageWidth+" × "+tile.imageHeight : tile.isVideo && tile.videoDuration>0 ? tile.clock(tile.videoDuration) : tile.extension));color:tile.saveFailed ? Theme.error : Theme.inkAlpha(.45);font.pointSize:Theme.fontPoints(11.5);elide:Text.ElideRight}
                }
                onClicked:tile.open()
                FmToolTip {shown:filenameButton.hovered;text:tile.feedback || tile.attachment.name}
            }
            MessageHeaderButton {objectName:"attachmentSave_"+tile.attachment.id;iconName:"download";iconSize:13;enabled:tile.available && !tile.saveTicket;tip:"Download";onClicked:tile.saveTicket=backend.downloadAttachment(tile.account,tile.attachment.id,tile.attachment.name)}
        }
    }
}
