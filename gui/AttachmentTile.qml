import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Dialogs

Rectangle {
    id: tile
    required property var attachment
    required property string account
    property bool available: attachment.state === "available"
    property real maximumWidth: 280
    readonly property bool isImage: /^image\//i.test(attachment.mime_type || "") || /\.(png|jpe?g|gif|webp|bmp|ico|tiff?)$/i.test(attachment.name || "")
    readonly property bool popupOpen: viewer.opened || picker.visible
    property string pickerAccount: ""
    property string pickerId: ""
    property bool previewFinished: false
    property bool componentReady: false
    readonly property string identity: account + "\n" + attachment.id
    property string previewTicket: ""
    property string saveTicket: ""
    property string previewUrl: ""
    property string feedback: ""
    property bool saveFailed: false
    readonly property string statusText: feedback || (!available ? "Not downloaded yet" : formatSize(attachment.size))
    property int imageWidth: 0
    property int imageHeight: 0
    objectName: "attachment_" + attachment.id
    width: Math.max(100, Math.min(280, maximumWidth))
    implicitHeight: contents.implicitHeight
    height: implicitHeight
    radius: 5; color: Theme.bg2; border.color: Theme.line2

    function formatSize(bytes) {
        if (!(bytes >= 0)) return "Available offline"
        if (bytes < 1024) return bytes + " B"
        if (bytes < 1024 * 1024) return Math.ceil(bytes / 1024) + " KB"
        return (bytes / (1024 * 1024)).toFixed(1) + " MB"
    }
    function loadPreview() {
        if (visible && available && isImage && !previewTicket && !previewUrl)
            previewTicket = backend.requestAttachmentPreview(account, attachment.id)
    }
    function saveTo(destination) {
        saveFailed = false; feedback = "Saving…"
        saveTicket = backend.saveAttachment(account, attachment.id, destination)
    }
    function chooseDestination() {
        pickerAccount = account; pickerId = attachment.id
        picker.selectedFile = backend.attachmentSaveLocation(attachment.name)
        picker.open()
    }
    Component.onCompleted: { componentReady = true; loadPreview() }
    onIdentityChanged: {
        if (!componentReady) return
        viewer.close(); picker.close(); previewUrl = ""; previewTicket = ""; previewFinished = false
        imageWidth = 0; imageHeight = 0; feedback = ""; saveTicket = ""
        loadPreview()
    }
    onVisibleChanged: loadPreview()
    onAvailableChanged: loadPreview()
    Connections {
        target: backend
        function onAttachmentPreviewReady(ticket, result, ok) {
            if (ticket !== tile.previewTicket) return
            tile.previewFinished = true
            if (ok && result.account === tile.account && result.id === tile.attachment.id) {
                tile.previewUrl = result.preview_url || ""
                tile.imageWidth = result.width || 0; tile.imageHeight = result.height || 0
            }
        }
        function onAttachmentSaved(ticket, error) {
            if (ticket !== tile.saveTicket) return
            tile.saveTicket = ""; tile.saveFailed = error.length > 0
            tile.feedback = error || "Saved"
        }
    }
    ColumnLayout {
        id: contents; width: parent.width; spacing: 0
        Button {
            id: previewButton
            objectName: "attachmentPreview_" + tile.attachment.id
            visible: tile.isImage; Layout.fillWidth: true; Layout.preferredHeight: 150
            enabled: thumbnail.status === Image.Ready
            padding: 8
            Accessible.name: "View image: " + tile.attachment.name
            background: Rectangle { color: previewButton.hovered ? Theme.bg4 : "transparent"; radius: 5; border.color: previewButton.activeFocus ? Theme.focusBorder : "transparent" }
            contentItem: Item {
                Image {
                    id: thumbnail; objectName: "attachmentThumbnail_" + tile.attachment.id
                    anchors.fill: parent; source: tile.previewUrl; asynchronous: true; autoTransform: true; retainWhileLoading: true
                    sourceSize: Qt.size(560, 300); fillMode: Image.PreserveAspectFit
                }
                Label {
                    anchors.centerIn: parent; visible: thumbnail.status !== Image.Ready
                    text: !tile.available ? "Available after sync" : (!tile.previewFinished || thumbnail.status === Image.Loading) ? "Loading preview…" : "Preview unavailable"
                    color: Theme.muted; font.pixelSize: Theme.fontSize(12)
                }
            }
            onClicked: { viewer.actualSize = false; viewer.open() }
        }
        RowLayout {
            Layout.fillWidth: true; Layout.margins: 8; spacing: 8
            FmIcon { name: "attachment"; size: 14 }
            AbstractButton {
                id: filenameButton
                objectName: "attachmentOpen_" + tile.attachment.id
                Layout.fillWidth: true; implicitHeight: 42
                enabled: tile.available && (!tile.isImage || thumbnail.status === Image.Ready)
                Accessible.name: "Open " + tile.attachment.name
                background: Rectangle { color: "transparent"; radius: 3; border.color: filenameButton.activeFocus ? Theme.focusBorder : "transparent" }
                contentItem: Column {
                    spacing: 3
                    Label {
                        width: parent.width; text: tile.attachment.name; textFormat: Text.PlainText
                        elide: Text.ElideMiddle; color: Theme.text
                        font.pixelSize: Theme.fontSize(12); font.underline: filenameButton.hovered
                    }
                    Label {
                        objectName: "attachmentStatus_" + tile.attachment.id
                        width: parent.width; text: tile.statusText; textFormat: Text.PlainText
                        elide: Text.ElideRight; color: tile.saveFailed ? Theme.error : Theme.muted
                        font.pixelSize: Theme.fontSize(11)
                        Accessible.name: tile.statusText
                    }
                }
                ToolTip.visible: hovered || (activeFocus && tile.saveFailed)
                ToolTip.text: tile.saveFailed ? tile.feedback : tile.attachment.name
                onClicked: {
                    if (tile.isImage) { viewer.actualSize = false; viewer.open() }
                    else backend.openAttachment(tile.attachment.id)
                }
            }
            FmButton {
                objectName: "attachmentSave_" + tile.attachment.id
                iconName: tile.saveTicket ? "refresh" : tile.feedback === "Saved" ? "check" : "download"; implicitWidth: 28; implicitHeight: 28
                enabled: tile.available && !tile.saveTicket
                tip: "Save attachment"; Accessible.name: "Save " + tile.attachment.name
                onClicked: tile.chooseDestination()
            }
        }
    }

    FileDialog {
        id: picker; title: "Save attachment"; fileMode: FileDialog.SaveFile
        onAccepted: { tile.saveFailed = false; tile.feedback = "Saving…"; tile.saveTicket = backend.saveAttachment(tile.pickerAccount, tile.pickerId, selectedFile) }
    }
    FmDialog {
        id: viewer; objectName: "attachmentViewer_" + tile.attachment.id
        property bool actualSize: false
        title: tile.attachment.name
        width: Math.min(1100, parent ? parent.width - 48 : 1100)
        height: Math.min(800, parent ? parent.height - 48 : 800)
        standardButtons: Dialog.Close
        contentItem: ColumnLayout {
            spacing: 12
            RowLayout {
                Layout.fillWidth: true
                Label {
                    Layout.fillWidth: true
                    text: tile.feedback || tile.imageWidth + " × " + tile.imageHeight
                    textFormat: Text.PlainText; elide: Text.ElideRight
                    color: tile.saveFailed ? Theme.error : Theme.muted; font.pixelSize: Theme.fontSize(12)
                    HoverHandler { id: modalStatusHover }
                    ToolTip.visible: modalStatusHover.hovered && tile.saveFailed; ToolTip.text: tile.feedback
                }
                FmButton { objectName: "attachmentActualSize_" + tile.attachment.id; text: viewer.actualSize ? "Fit image" : "Actual size"; outlined: true; onClicked: { viewer.actualSize = !viewer.actualSize; imageScroll.contentX = 0; imageScroll.contentY = 0 } }
                FmButton { iconName: "download"; tip: "Save attachment"; Accessible.name: "Save attachment"; enabled: !tile.saveTicket; onClicked: tile.chooseDestination() }
            }
            Flickable {
                id: imageScroll; Layout.fillWidth: true; Layout.fillHeight: true; clip: true
                contentWidth: viewer.actualSize ? Math.max(width, tile.imageWidth) : width
                contentHeight: viewer.actualSize ? Math.max(height, tile.imageHeight) : height
                boundsBehavior: Flickable.StopAtBounds
                ScrollBar.vertical: ScrollBar {}
                ScrollBar.horizontal: ScrollBar {}
                Image {
                    width: viewer.actualSize ? tile.imageWidth : imageScroll.width
                    height: viewer.actualSize ? tile.imageHeight : imageScroll.height
                    x: Math.max(0, (imageScroll.contentWidth - width) / 2)
                    y: Math.max(0, (imageScroll.contentHeight - height) / 2)
                    source: viewer.visible ? tile.previewUrl : ""; asynchronous: true; autoTransform: true; retainWhileLoading: true
                    fillMode: Image.PreserveAspectFit
                    sourceSize: viewer.actualSize ? Qt.size(tile.imageWidth, tile.imageHeight) : Qt.size(Math.ceil(imageScroll.width * 2), Math.ceil(imageScroll.height * 2))
                }
            }
        }
    }
}

