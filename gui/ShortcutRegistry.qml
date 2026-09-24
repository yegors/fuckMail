pragma Singleton
import QtQuick

QtObject {
    readonly property var sections: [
        {name:"Mail",scope:"mail",rows:[
            {id:"next",label:"Next conversation",key:"J"},{id:"prev",label:"Previous conversation",key:"K"},
            {id:"reply",label:"Reply",key:"R"},{id:"replyAll",label:"Reply all",key:"A"},{id:"forward",label:"Forward",key:"F"},
            {id:"unread",label:"Mark read / unread",key:"U"},{id:"delay",label:"Delay · undelay",key:"D"},{id:"done",label:"Done",key:"E"},
            {id:"compose",label:"New message",key:"N"},{id:"scopes",label:"Inbox · Sent · Drafts",key:"S"},{id:"search",label:"Search",key:"/"},
            {id:"delete",label:"Delete",key:"Delete"},{id:"find",label:"Search",key:"Ctrl+K",fixed:true},{id:"accounts",label:"Next · previous mailbox",key:"Tab · Shift+Tab",fixed:true}]},
        {name:"Calendar",scope:"calendar",rows:[
            {id:"day",label:"Day view",key:"D"},{id:"week",label:"Week view",key:"W"},{id:"month",label:"Month view",key:"M"},
            {id:"agenda",label:"Agenda view",key:"A"},{id:"back",label:"Previous",key:"Left"},{id:"fwd",label:"Next",key:"Right"},
            {id:"today",label:"Today",key:"T"},{id:"newEv",label:"New event",key:"N"}]},
        {name:"Everywhere",scope:"all",rows:[
            {id:"view",label:"Mail · Calendar",key:"V"},{id:"settings",label:"Settings",key:","},{id:"undo",label:"Undo",key:"Z"},
            {id:"send",label:"Send the message · save the event",key:"Ctrl+Return",fixed:true},{id:"close",label:"Close or cancel",key:"Esc",fixed:true}]}]
    // Every Shortcut re-reads its key whenever preferences change; look defaults up directly.
    readonly property var defaults: {
        const keys = {}
        for (const section of sections) for (const row of section.rows) if (keys[row.id] === undefined) keys[row.id] = row.key
        return keys
    }
    function sequence(id, preferences) {
        const overrides = (preferences || {}).shortcuts || {}
        if (overrides[id] !== undefined) return overrides[id] || ""
        return defaults[id] !== undefined ? defaults[id] : ""
    }
    function unassigned(key,scope,preferences) {
        for(const section of sections) if(section.scope===scope || section.scope==="all")
            for(const row of section.rows) if(!row.fixed && sequence(row.id,preferences)===key)return false
        return true
    }
    function label(key) {
        if (!key) return "—"
        return key.replace(/Left/g,"←").replace(/Right/g,"→").replace(/Up/g,"↑").replace(/Down/g,"↓").replace(/Return/g,"Enter")
            .replace(/Ctrl/g,Qt.platform.os === "osx" ? "Cmd" : "Ctrl").replace(/Meta/g,Qt.platform.os === "osx" ? "Ctrl" : "Win")
    }
    function combo(event) {
        const names = {}; names[Qt.Key_Left]="Left"; names[Qt.Key_Right]="Right"; names[Qt.Key_Up]="Up"; names[Qt.Key_Down]="Down"
        names[Qt.Key_Space]="Space"; names[Qt.Key_Delete]="Delete"; names[Qt.Key_Backspace]="Backspace"
        names[Qt.Key_Return]="Return"; names[Qt.Key_Enter]="Return"; names[Qt.Key_Home]="Home"; names[Qt.Key_End]="End"
        names[Qt.Key_PageUp]="PgUp"; names[Qt.Key_PageDown]="PgDown"
        const base = names[event.key] || (event.key >= Qt.Key_F1 && event.key <= Qt.Key_F35 ? "F" + (event.key-Qt.Key_F1+1) : event.key >= 33 && event.key <= 126 ? String.fromCharCode(event.key).toUpperCase() : "")
        if (!base) return ""
        return [event.modifiers & Qt.ControlModifier ? "Ctrl" : "",event.modifiers & Qt.AltModifier ? "Alt" : "",event.modifiers & Qt.ShiftModifier ? "Shift" : "",event.modifiers & Qt.MetaModifier ? "Meta" : "",base].filter(Boolean).join("+")
    }
}
