// Settings > Shortcuts page: the ShortcutRegistry table. Click a row and press
// a key to rebind it; a key already used in the same scope moves here and the
// other row is left without a shortcut. Saved to preferences.shortcuts.
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Shapes

ColumnLayout {
    id: panel
    property string recording: ""
    property string hint: ""
    property var activeRow:null
    function cancelOutside(point) {
        if(!activeRow)return
        const local=activeRow.mapFromItem(null,point.x,point.y)
        if(local.x<0 || local.y<0 || local.x>activeRow.width || local.y>activeRow.height)recording=""
    }
    spacing: 0
    onVisibleChanged: if (!visible) recording = ""
    // Single letters read as typed ("j"); combinations keep their names.
    function keyText(key) { const text = ShortcutRegistry.label(key); return /^[A-Z]$/.test(text) ? text.toLowerCase() : text }
    function save(row,scope,key) {
        const map = Object.assign({},backend.preferences.shortcuts || {}), moved = []
        if (key) for (const section of ShortcutRegistry.sections) {
            if (section.scope !== scope && section.scope !== "all" && scope !== "all") continue
            for (const other of section.rows) if (!other.fixed && other.id !== row.id && ShortcutRegistry.sequence(other.id,backend.preferences) === key) {
                map[other.id] = null; moved.push(other.label)
            }
        }
        map[row.id] = key || null
        backend.setPreferences({shortcuts:map}); recording = ""
        hint = moved.length ? ShortcutRegistry.label(key) + " moved from ‘" + moved.join("’, ‘") + "’, which now has no shortcut." : ""
    }
    Repeater {
        model: ShortcutRegistry.sections
        ColumnLayout {
            id: group
            required property var modelData
            Layout.fillWidth:true; Layout.topMargin:28; spacing:0
            Text { text:group.modelData.name; color:Theme.inkAlpha(.45); font.family:Theme.fontFamily; font.pointSize:Theme.fontPoints(11); Layout.bottomMargin:6 }
            Rectangle { Layout.fillWidth:true; implicitHeight:1; color:Theme.line }
            Repeater {
                model:group.modelData.rows
                ItemDelegate {
                    id: row
                    required property var modelData
                    readonly property bool recording:panel.recording === modelData.id
                    property bool blocksWorkspaceShortcuts:recording
                    readonly property string key:ShortcutRegistry.sequence(modelData.id,backend.preferences)
                    Layout.fillWidth:true; implicitHeight:41; leftPadding:0; rightPadding:0; topPadding:0; bottomPadding:1
                    objectName:"shortcut_"+modelData.id
                    onClicked: if (!modelData.fixed) { panel.recording=modelData.id;panel.activeRow=row;panel.hint="";forceActiveFocus() }
                    onActiveFocusChanged: if (!activeFocus && recording) Qt.callLater(function(){if(!row.activeFocus && panel.recording===row.modelData.id)panel.recording=""})
                    Keys.onPressed: function(event) {
                        if (!recording) return
                        event.accepted=true
                        if (event.key === Qt.Key_Escape) { panel.recording=""; return }
                        const key=ShortcutRegistry.combo(event)
                        if (!key) return
                        if (["Ctrl+K","Meta+K","Ctrl+Return","Meta+Return"].indexOf(key)>=0) { panel.hint=ShortcutRegistry.label(key)+" is fixed. Press another shortcut.";return }
                        panel.save(modelData,group.modelData.scope,key)
                    }
                    background: Rectangle { x:-8; width:parent.width+16; color:row.recording || (row.hovered && !row.modelData.fixed) ? Theme.surface : "transparent"; Rectangle { x:8; anchors.bottom:parent.bottom; width:parent.width-16;height:1;color:Theme.line } }
                    contentItem: RowLayout {
                        spacing:12
                        Text { text:row.modelData.label; color:row.modelData.fixed ? Theme.inkAlpha(.45) : Theme.text; font.family:Theme.fontFamily; font.pointSize:Theme.fontPoints(12.5); Layout.fillWidth:true; wrapMode:Text.Wrap }
                        Text { visible:!row.modelData.fixed && row.key !== row.modelData.key; text:"edited"; color:Theme.accent; font.family:Theme.fontFamily; font.pointSize:Theme.fontPoints(10.5) }
                        FmButton { visible:row.recording; text:"Default"; focusPolicy:Qt.NoFocus;implicitHeight:26; onClicked:panel.save(row.modelData,group.modelData.scope,row.modelData.key) }
                        FmButton { visible:row.recording; text:"None";focusPolicy:Qt.NoFocus;implicitHeight:26; onClicked:panel.save(row.modelData,group.modelData.scope,"") }
                        Rectangle {
                            implicitWidth:Math.max(24,keyLabel.implicitWidth+14); implicitHeight:24; radius:4
                            color:"transparent"; border.width:row.recording ? 0 : 1; border.color:Theme.line2
                            Text { id:keyLabel; anchors.centerIn:parent; text:row.recording ? "Press keys" : row.key ? panel.keyText(row.key) : "None"; color:row.recording ? Theme.inkAlpha(.6) : row.modelData.fixed ? Theme.inkAlpha(.4) : row.key ? Theme.inkAlpha(.85) : Theme.inkAlpha(.35); font.family:Theme.fontFamily; font.pointSize:Theme.fontPoints(11.5); font.features:{"tnum":1} }
                            Shape { anchors.fill:parent;visible:row.recording
                                SequentialAnimation on opacity { running:row.recording;loops:Animation.Infinite; NumberAnimation {to:.3;duration:500} NumberAnimation {to:1;duration:500} }
                                ShapePath {strokeColor:Theme.text;strokeWidth:1;strokeStyle:ShapePath.DashLine;dashPattern:[2,2];fillColor:"transparent"; PathSvg {path:"M .5 .5 H "+(keyLabel.parent.width-.5)+" V 23.5 H .5 Z"} }
                            }
                        }
                    }
                }
            }
        }
    }
    RowLayout {
        Layout.fillWidth: true; Layout.topMargin: 18; spacing: 12
        Text {
            objectName: "shortcutHint"; Layout.fillWidth: true; wrapMode: Text.Wrap
            text: panel.hint || (panel.recording ? "Press a key or combination, like Ctrl+R · Esc to cancel" : "Click a row to change it. Dimmed ones are fixed.")
            color: Theme.inkAlpha(.45); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(11.5)
        }
        FmButton {
            objectName: "resetShortcuts"; text: "Reset all"; implicitHeight: 20; leftPadding: 0; rightPadding: 0; topPadding: 0; bottomPadding: 0
            font.pointSize: Theme.fontPoints(11.5); foreground: hovered ? Theme.text : Theme.accent; background: Item {}
            onClicked: { panel.recording = ""; panel.hint = ""; backend.setPreferences({shortcuts: {}}) }
        }
    }
}
