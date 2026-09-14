import QtQuick

// Small line icons rendered as geometry, independent of installed symbol fonts.
Canvas {
    id: icon
    property string name: ""
    property color color: Theme.muted
    property int size: 16
    implicitWidth: size
    implicitHeight: size
    width: size
    height: size
    onNameChanged: requestPaint()
    onColorChanged: requestPaint()
    onWidthChanged: requestPaint()
    onHeightChanged: requestPaint()
    onPaint: {
        const c = getContext("2d")
        c.reset()
        c.scale(width / 16, height / 16)
        c.strokeStyle = color
        c.lineWidth = 1.5
        c.lineCap = "round"
        c.lineJoin = "round"
        function line(points) { c.moveTo(points[0], points[1]); for (let i = 2; i < points.length; i += 2) c.lineTo(points[i], points[i + 1]) }
        function circle(x,y,r) { c.moveTo(x+r,y); c.arc(x,y,r,0,Math.PI*2) }
        c.beginPath()
        switch (name) {
        case "mail": c.roundedRect(2,3.5,12,9,1.3,1.3); line([2.5,4.5,8,8.7,13.5,4.5]); break
        case "calendar": c.roundedRect(2.5,3.5,11,10,1,1); line([2.5,7,13.5,7]); line([5.5,2,5.5,5]); line([10.5,2,10.5,5]); break
        case "search": circle(7,7,4.5); line([10.5,10.5,14,14]); break
        case "close": line([4,4,12,12]); line([12,4,4,12]); break
        case "previous": line([10,3.5,5.5,8,10,12.5]); break
        case "next": line([6,3.5,10.5,8,6,12.5]); break
        case "chevronDown":
        case "down": line([4,6,8,10,12,6]); break
        case "arrowUp": line([8,13,8,3]); line([3.5,7.5,8,3,12.5,7.5]); break
        case "undo": line([6,2.5,2.5,6,6,9.5]); c.moveTo(3,6); c.bezierCurveTo(14,3,15,13,8,13); break
        case "redo": line([10,2.5,13.5,6,10,9.5]); c.moveTo(13,6); c.bezierCurveTo(2,3,1,13,8,13); break
        case "trash": line([2.5,4,13.5,4]); line([6,2,10,2]); line([4,4,4.7,14,11.3,14,12,4]); line([6.5,6.5,6.8,11.5]); line([9.5,6.5,9.2,11.5]); break
        case "check": line([3.5,8,6.5,11,12.5,4.5]); break
        case "plus": line([8,3,8,13]); line([3,8,13,8]); break
        case "minus": line([3.5,8,12.5,8]); break
        case "combined": c.roundedRect(2.5,2.5,6.5,6.5,1,1); c.roundedRect(7,7,6.5,6.5,1,1); break
        case "filter": line([2.5,4,13.5,4]); line([5,8,11,8]); line([6.5,12,9.5,12]); break
        case "more": circle(3,8,0.7); circle(8,8,0.7); circle(13,8,0.7); break
        case "accounts": c.roundedRect(2,5,12,9,1.5,1.5); line([4.5,5,4.5,2.5,11.5,2.5,11.5,5]); break
        case "person": circle(8,4.5,2.3); c.moveTo(2.8,14); c.bezierCurveTo(2.8,7.8,13.2,7.8,13.2,14); break
        case "bot": c.roundedRect(3,6,10,7,1.5,1.5); line([8,6,8,3.5]); line([6.5,9.5,6.51,9.5]); line([9.5,9.5,9.51,9.5]); line([1.5,9,1.5,11]); line([14.5,9,14.5,11]); break
        case "unknown": circle(8,8,6); c.moveTo(6.1,5.5); c.bezierCurveTo(6.1,3,10.8,3,10.2,5.8); c.bezierCurveTo(10,6.8,8,7,8,8.8); line([8,11,8,11.2]); break
        case "wind":
            line([2,5,8.5,5]); c.bezierCurveTo(11,5,11,1.5,8.5,1.5); c.bezierCurveTo(7.5,1.5,6.75,2.3,6.75,3.25)
            line([2,8.5,11.5,8.5]); c.bezierCurveTo(14,8.5,14,12,11.5,12); c.bezierCurveTo(10.5,12,9.75,11.25,9.75,10.25)
            line([2,12,7.5,12]); break
        case "sliders": line([2.5,5.5,4,5.5]); line([7.5,5.5,13.5,5.5]); line([2.5,10.5,8.5,10.5]); line([12,10.5,13.5,10.5]); circle(5.75,5.5,1.75); circle(10.25,10.5,1.75); break
        case "general": line([3,4.5,13,4.5]); line([3,8,13,8]); line([3,11.5,9,11.5]); circle(11.5,11.5,1.5); break
        case "chip": c.roundedRect(3.5,3.5,9,9,1.5,1.5); line([6,1.5,6,3.5]); line([10,1.5,10,3.5]); line([6,12.5,6,14.5]); line([10,12.5,10,14.5]); line([1.5,6,3.5,6]); line([1.5,10,3.5,10]); line([12.5,6,14.5,6]); line([12.5,10,14.5,10]); break
        case "keyboard": c.roundedRect(1.5,4,13,8,1.5,1.5); line([4,7,5,7]); line([7.5,7,8.5,7]); line([11,7,12,7]); line([5,9.5,11,9.5]); break
        case "clock": circle(8,8,6); line([8,4,8,8,10.7,9.6]); break
        case "info": circle(8,8,6); line([8,7.5,8,11]); line([8,4.8,8,5]); break
        case "review": circle(8,8,5.5); circle(8,8,0.75); break
        case "checkCircle": circle(8,8,6); line([5.5,8.2,7.3,10,10.7,6.2]); break
        case "reply": line([6.5,3.5,2.5,7.5,6.5,11.5]); c.moveTo(3,7.5); c.bezierCurveTo(10,6,13.5,9.5,13.5,13); break
        case "replyAll": line([5,3.5,1,7.5,5,11.5]); line([8,3.5,4,7.5,8,11.5]); c.moveTo(5,7.5); c.bezierCurveTo(11,6,14,9.5,14,13); break
        case "forward": line([9.5,3.5,13.5,7.5,9.5,11.5]); c.moveTo(13,7.5); c.bezierCurveTo(6,6,2.5,9.5,2.5,13); break
        case "edit": line([10,3,13,6,6,13,2.5,13.5,3,10,10,3]); line([9,4,12,7]); break
        case "refresh": c.moveTo(13,7); c.arc(8,8,5,-0.2,Math.PI*0.86); line([3,13,3,10,6,10]); c.moveTo(3,9); c.arc(8,8,5,Math.PI-0.2,Math.PI*1.86); line([13,3,13,6,10,6]); break
        case "settings":
            circle(8,8,2.1)
            for (let i = 0; i < 8; ++i) { const a = Math.PI * i / 4; const b = a + 0.14; const d = a + 0.64; line([8+5*Math.cos(a),8+5*Math.sin(a),8+6.4*Math.cos(b),8+6.4*Math.sin(b),8+6.4*Math.cos(d),8+6.4*Math.sin(d),8+5*Math.cos(a+Math.PI/4),8+5*Math.sin(a+Math.PI/4)]) }
            break
        case "star":
            for (let j = 0; j <= 10; ++j) { const a = -Math.PI/2 + j*Math.PI/5; const r = j%2 ? 2.8 : 6; if (j === 0) c.moveTo(8+r*Math.cos(a),8+r*Math.sin(a)); else c.lineTo(8+r*Math.cos(a),8+r*Math.sin(a)) }
            break
        case "attachment": c.moveTo(5,10); c.lineTo(10,5); c.bezierCurveTo(13,2,16,5,13,8); c.lineTo(7,14); c.bezierCurveTo(3,18,-1,12,3,8); c.lineTo(9,2); break
        default: break
        }
        c.stroke()
    }
}
