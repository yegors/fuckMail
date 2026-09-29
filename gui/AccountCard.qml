import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Shapes

// Settings → Accounts card. One status line in the header, then one row per
// stage: Mail and Calendar downloads, the account's labels, and the local
// model. Each row is label | meter | controls, with every count in a fixed
// slot so the bars in a card start at the same x.
Rectangle {
    id: card
    required property var rowData
    readonly property var modelData: rowData
    property var progress: null
    property var policy: null
    property var analysis: null
    // SettingsView: reconnect(), editAccount(), openLabels(), pickColor(),
    // selectPage(), startSync(), restore().
    property var host: null
    objectName: "accountCard_" + modelData.id
    readonly property string accountId: modelData.id
    readonly property var lifecycle: modelData.lifecycle || {}
    readonly property bool archived: lifecycle.state === "removed"
    readonly property var cleanup: lifecycle.cleanup || {}
    readonly property var removal: (backend.accountLifecycle.operations || {})[accountId] || {}
    readonly property var syncState: { const changed = backend.googleSync; return backend.googleSyncState(accountId) }
    readonly property var pipeline: progress ? progress.view(accountId) : ({mail: {}, cal: {}, ai: {}})
    readonly property var aiPolicy: policy ? policy.of(accountId) : ({})
    readonly property bool syncing: syncState.changing === true || syncState.refresh_requested === true
        || ((syncState.progress || {}).mail === "running") || ((syncState.progress || {}).calendar === "running")
    readonly property color hue: Qt.color(Theme.accountColor(accountId))
    property var labelNames: []
    property string labelTicket: ""
    function loadLabels() { if (backend.connected && !archived) labelTicket = backend.labelRequest("label_catalog", {}, accountId) }
    Component.onCompleted: loadLabels()
    Connections {
        target: backend
        function onLabelReply(ticket, method, result, ok) {
            if (ticket !== card.labelTicket) return
            card.labelTicket = ""
            if (ok) card.labelNames = (result.categories || []).map(c => c.name)
        }
        function onConnectedChanged() { if (backend.connected) card.loadLabels() }
    }
    Connections {
        target: card.host
        ignoreUnknownSignals: true
        function onLabelsEdited(account) { if (account === card.accountId) card.loadLabels() }
    }
    Layout.fillWidth: true
    implicitHeight: content.implicitHeight
    color: Theme.canvas; border.color: Theme.line2; radius: 8; clip: true

    component IconButton: AbstractButton {
        id: iconButton
        property string iconName: ""
        property string tip: ""
        property bool spinning: false
        property color iconColor: Theme.inkAlpha(.7)
        property int boxSize: 32
        property bool framed: true
        implicitWidth: boxSize; implicitHeight: boxSize
        hoverEnabled: true
        onSpinningChanged: if (!spinning) spinIcon.rotation = 0
        background: Rectangle { radius: 5; color: iconButton.hovered && iconButton.enabled ? (iconButton.framed ? Theme.bg2 : Theme.withAlpha(Theme.ai, .12)) : "transparent"; border.color: iconButton.framed ? (iconButton.visualFocus ? Theme.focusBorder : Theme.line2) : "transparent" }
        contentItem: Item {
            FmIcon {
                id: spinIcon
                anchors.centerIn: parent; name: iconButton.iconName; size: iconButton.boxSize > 26 ? 14 : 12
                color: iconButton.enabled ? iconButton.iconColor : Theme.faint
                RotationAnimator on rotation { running: iconButton.spinning; from: 0; to: 360; duration: 900; loops: Animation.Infinite }
            }
        }
        Accessible.name: tip
        FmToolTip { shown: iconButton.hovered && iconButton.tip.length > 0; text: iconButton.tip }
    }
    // A download or model meter: count in a 136 px slot, a thin bar, and a short status.
    component Meter: RowLayout {
        id: meter
        property var stage: ({})
        property bool twoTone: false
        property color track: Theme.inkAlpha(.07)
        property string countName: ""
        spacing: 12
        Text {
            objectName: meter.countName
            text: meter.stage.count || ""
            Layout.preferredWidth: 136; Layout.maximumWidth: 136; elide: Text.ElideRight
            color: Theme.inkAlpha(.85); font { family: Theme.fontFamily; pointSize: Theme.fontPoints(12.5); features: {"tnum": 1} }
            HoverHandler { id: countHover }
            FmToolTip { shown: countHover.hovered && !!meter.stage.countTitle; text: meter.stage.countTitle || "" }
        }
        Rectangle {
            id: track
            Layout.fillWidth: true; Layout.minimumWidth: 24; implicitHeight: 3; radius: 2; color: meter.track; clip: true
            // Progress, not layout, animates: a resize or a neighbouring row's
            // change never moves a bar, and a finished stage never moves at all.
            property real done: Math.max(0, Math.min(1, meter.stage.frac || 0))
            property real queued: Math.max(0, Math.min(1, meter.stage.qfrac || 0))
            Behavior on done { enabled: meter.stage.live === true; NumberAnimation { duration: 500 } }
            Behavior on queued { enabled: meter.stage.live === true; NumberAnimation { duration: 500 } }
            Rectangle { width: parent.width * track.done; height: 3; color: meter.stage.fill || Theme.ai }
            Rectangle {
                visible: meter.twoTone
                x: parent.width * track.done
                width: parent.width * track.queued; height: 3; color: Theme.withAlpha(Theme.ai, .28)
            }
            // Google reports no calendar total: an active first pass shimmers.
            Rectangle {
                id: shimmer
                visible: !meter.twoTone && meter.stage.live === true && (meter.stage.frac || 0) === 0
                width: track.width * .3; height: 3; color: meter.stage.fill || Theme.amber
                SequentialAnimation on x { running: shimmer.visible; loops: Animation.Infinite; NumberAnimation { from: -track.width * .3; to: track.width; duration: 1400; easing.type: Easing.InOutSine } }
            }
        }
        Text {
            objectName: meter.countName.length ? meter.countName.replace("Count", "Status") : ""
            text: meter.stage.short || ""
            // Its full width; the bar takes what is left and gives way first.
            Layout.preferredWidth: implicitWidth; Layout.minimumWidth: 0; elide: Text.ElideRight
            color: meter.stage.live ? Theme.inkAlpha(.78) : Theme.inkAlpha(.5)
            font { family: Theme.fontFamily; pointSize: Theme.fontPoints(12); features: {"tnum": 1} }
        }
    }
    component StageLabel: RowLayout {
        id: stageLabel
        property string iconName: ""
        property color iconColor: Theme.faint
        property string text: ""
        property color textColor: Theme.inkAlpha(.6)
        property bool live: false
        Layout.preferredWidth: 100; Layout.minimumWidth: 100; Layout.maximumWidth: 100; spacing: 8
        FmIcon {
            name: stageLabel.iconName; size: 12; color: stageLabel.iconColor
            opacity: 1
            SequentialAnimation on opacity { running: stageLabel.live; loops: Animation.Infinite; NumberAnimation { to: .35; duration: 700 } NumberAnimation { to: 1; duration: 700 } }
        }
        Text { text: stageLabel.text; color: stageLabel.textColor; font { family: Theme.fontFamily; pointSize: Theme.fontPoints(13) } Layout.fillWidth: true; elide: Text.ElideRight }
    }
    component StageRow: Rectangle {
        default property alias cells: rowLayout.data
        property color rule: Theme.line
        Layout.fillWidth: true; implicitHeight: Math.max(46, rowLayout.implicitHeight + 12)
        color: "transparent"
        Rectangle { width: parent.width; height: 1; color: parent.rule }
        RowLayout { id: rowLayout; anchors.fill: parent; anchors.leftMargin: 18; anchors.rightMargin: 18; spacing: 14 }
    }

    ColumnLayout {
        id: content
        width: parent.width; spacing: 0
        RowLayout {
            Layout.fillWidth: true; Layout.margins: 18; Layout.topMargin: 16; Layout.bottomMargin: 16; spacing: 14
            AbstractButton {
                id: avatar
                objectName: "accountAvatar_" + card.accountId
                implicitWidth: 36; implicitHeight: 36; hoverEnabled: true
                enabled: !card.archived && backend.connected
                onClicked: { const point = mapToItem(Overlay.overlay, 0, height + 8); card.host.pickColor(card.accountId, point.x, point.y) }
                background: Item {
                    Rectangle { anchors.centerIn: parent; visible: avatar.hovered; width: 42; height: 42; radius: 21; color: card.hue }
                    Rectangle { anchors.centerIn: parent; visible: avatar.hovered; width: 40; height: 40; radius: 20; color: Theme.canvas }
                    Rectangle { anchors.fill: parent; radius: 18; color: Theme.withAlpha(card.hue, .2) }
                }
                contentItem: Text {
                    text: card.modelData.name.trim().split(/\s+/).map(n => n.slice(0, 1)).slice(0, 2).join("").toUpperCase()
                    horizontalAlignment: Text.AlignHCenter; verticalAlignment: Text.AlignVCenter
                    color: card.hue; font { family: Theme.fontFamily; pointSize: Theme.fontPoints(12); weight: Font.Medium }
                }
                Accessible.name: "Account colour"
                FmToolTip { shown: avatar.hovered; text: "Account colour" }
            }
            ColumnLayout {
                Layout.fillWidth: true; spacing: 3
                Text {
                    objectName: "accountName_" + card.accountId
                    text: card.modelData.name; textFormat: Text.PlainText; Layout.fillWidth: true; elide: Text.ElideRight
                    color: Theme.text; font { family: Theme.fontFamily; pointSize: Theme.fontPoints(15); weight: Font.Medium }
                }
                RowLayout {
                    Layout.fillWidth: true; spacing: 6
                    FmIcon { name: "google"; size: 12; color: Theme.inkAlpha(.5); visible: !backend.syntheticProfile }
                    Text { text: card.modelData.email; color: Theme.inkAlpha(.5); font.pointSize: Theme.fontPoints(12.5); elide: Text.ElideRight; Layout.fillWidth: true; Layout.maximumWidth: implicitWidth; textFormat: Text.PlainText }
                    // Local Retention, changed through Edit…; mail past it is still in Google.
                    Rectangle {
                        id: retentionBadge
                        objectName: "accountRetention_" + card.accountId
                        readonly property string period: ({"1_month": "1 month", "3_months": "3 months", "90_days": "90 days", "1_year": "1 year", "2_years": "2 years", "5_years": "5 years"})[card.modelData.mail_retention || "all"] || ""
                        implicitWidth: retentionText.implicitWidth + 12; implicitHeight: 16; radius: 3
                        color: "transparent"; border.width: Theme.ruleWidth; border.color: Theme.controlBorder
                        Text { id: retentionText; anchors.centerIn: parent; text: retentionBadge.period ? "Keeps " + retentionBadge.period : "Keeps everything"; color: Theme.inkAlpha(.6); font.pointSize: Theme.fontPoints(11); textFormat: Text.PlainText }
                        HoverHandler { id: retentionHover }
                        FmToolTip { shown: retentionHover.hovered; text: retentionBadge.period ? "Mail older than " + retentionBadge.period + " is removed from this machine · still in Google" : "All mail is kept on this machine" }
                    }
                    Item { Layout.fillWidth: true }
                }
            }
            // One status line: Downloading / Processing / Up to date / Paused / Disconnected.
            RowLayout {
                id: statusLine
                objectName: "accountStatus_" + card.accountId
                readonly property string phase: card.archived ? "archived" : backend.syntheticProfile ? "local" : card.pipeline.phase
                readonly property color tone: phase === "err" ? Theme.error : phase === "paused" || phase === "archived" ? Theme.inkAlpha(.5)
                    : phase === "dl" ? Theme.amber : phase === "ai" ? Theme.ai : Theme.inkAlpha(.7)
                property alias text: statusText.text
                spacing: 7; Layout.rightMargin: 6
                Rectangle {
                    width: 6; height: 6; radius: 3
                    color: statusLine.phase === "idle" || statusLine.phase === "local" ? Theme.ai : statusLine.phase === "paused" || statusLine.phase === "archived" ? Theme.inkAlpha(.3) : statusLine.tone
                    SequentialAnimation on opacity { running: statusLine.phase === "dl" || statusLine.phase === "ai"; loops: Animation.Infinite; NumberAnimation { to: .35; duration: 700 } NumberAnimation { to: 1; duration: 700 } }
                }
                Text {
                    id: statusText
                    text: statusLine.phase === "archived" ? "Archived" : statusLine.phase === "local" ? "Local copy" : card.pipeline.status || ""
                    color: statusLine.tone; font { family: Theme.fontFamily; pointSize: Theme.fontPoints(12.5); features: {"tnum": 1} }
                    TapHandler { enabled: statusLine.phase === "err"; onTapped: card.host.reconnect(card.modelData) }
                    HoverHandler { enabled: statusLine.phase === "err"; cursorShape: Qt.PointingHandCursor }
                }
            }
            FmButton {
                objectName: "reconnectGoogle_" + card.accountId
                visible: !card.archived && !backend.syntheticProfile && card.pipeline.err
                text: "Reconnect"; iconName: "refresh"; prominent: true; implicitHeight: 32
                enabled: backend.connected && !backend.googleAuthBusy
                onClicked: card.host.reconnect(card.modelData)
            }
            Row {
                spacing: 4
                IconButton {
                    objectName: "refreshGoogle_" + card.accountId
                    visible: !card.archived && !backend.syntheticProfile && !card.pipeline.err
                    iconName: "refresh"; spinning: card.syncing && card.syncState.active === true
                    tip: card.syncState.active ? "Sync now" : "Resume syncing"
                    enabled: backend.connected && !card.syncState.changing
                    onClicked: card.syncState.active ? backend.refreshGoogleSync(card.accountId) : card.host.startSync(card.accountId)
                }
                IconButton {
                    objectName: "googleSync_" + card.accountId
                    visible: !card.archived && !backend.syntheticProfile && !card.pipeline.err
                    iconName: card.syncState.owned ? "pause" : "play"
                    tip: card.syncState.owned ? "Pause this account · downloading and the local model" : "Resume this account"
                    enabled: backend.connected && !card.syncState.changing
                    onClicked: {
                        if (card.syncState.owned) { backend.pauseGoogleSync(card.accountId); card.progress.markPaused(card.accountId, true) }
                        else { card.host.startSync(card.accountId); card.progress.markPaused(card.accountId, false) }
                    }
                }
                IconButton {
                    id: moreButton
                    objectName: "accountOptions_" + card.accountId
                    visible: !card.archived
                    iconName: "more"; tip: "More"
                    onClicked: { const point = mapToItem(Overlay.overlay, width, height); accountActions.x = point.x - accountActions.width; accountActions.y = point.y + 6; accountActions.open() }
                }
            }
        }
        // Download: Mail. Hidden when mail sync is off.
        StageRow {
            visible: !card.archived && !backend.syntheticProfile && card.pipeline.mailShown
            StageLabel { iconName: "fetch"; text: "Mail"; live: card.pipeline.mail.live === true; iconColor: card.pipeline.err ? Theme.error : card.pipeline.mail.live ? Theme.amber : Theme.inkAlpha(.3) }
            Meter { Layout.fillWidth: true; stage: card.pipeline.mail; countName: "accountMailCount_" + card.accountId }
            Item { implicitWidth: 0 }
        }
        // Download: Calendar. No row means calendar sync is off.
        StageRow {
            visible: !card.archived && !backend.syntheticProfile && card.pipeline.calShown
            StageLabel { iconName: "calendar"; text: "Calendar"; live: card.pipeline.cal.live === true; iconColor: card.pipeline.err ? Theme.error : card.pipeline.cal.live ? Theme.amber : Theme.inkAlpha(.3) }
            Meter { Layout.fillWidth: true; stage: card.pipeline.cal; countName: "accountCalendarCount_" + card.accountId }
            Item { implicitWidth: 0 }
        }
        StageRow {
            visible: !card.archived
            StageLabel { iconName: "tag"; text: "Labels"; iconColor: Theme.inkAlpha(.4) }
            Text {
                objectName: "accountLabels_" + card.accountId
                text: card.labelNames.length ? card.labelNames.slice(0, 4).join(", ") + (card.labelNames.length > 4 ? " +" + (card.labelNames.length - 4) : "") : "No labels yet"
                Layout.fillWidth: true; elide: Text.ElideRight; color: Theme.inkAlpha(.6); font { family: Theme.fontFamily; pointSize: Theme.fontPoints(12.5) }
            }
            AbstractButton {
                id: editLabels
                objectName: "editLabels_" + card.accountId
                hoverEnabled: true; enabled: backend.connected
                onClicked: card.host.openLabels(card.accountId)
                contentItem: Row {
                    spacing: 4
                    Text { text: "Edit"; color: editLabels.hovered ? Theme.text : Theme.inkAlpha(.75); font { family: Theme.fontFamily; pointSize: Theme.fontPoints(12.5) } anchors.verticalCenter: parent.verticalCenter }
                    FmIcon { name: "next"; size: 10; color: editLabels.hovered ? Theme.text : Theme.inkAlpha(.75); anchors.verticalCenter: parent.verticalCenter }
                }
                background: null
                Accessible.name: "Edit labels"
            }
        }
        // Signature for new messages and replies from this account.
        StageRow {
            id: signatureRow
            visible: !card.archived
            readonly property string signature: card.modelData.signature_text || ""
            StageLabel { iconName: "edit"; text: "Signature"; iconColor: Theme.inkAlpha(.4) }
            Text {
                objectName: "accountSignature_" + card.accountId
                text: signatureRow.signature.length ? signatureRow.signature.split("\n").map(line => line.trim()).filter(line => line.length).join(" · ") : "None"
                Layout.fillWidth: true; elide: Text.ElideRight; textFormat: Text.PlainText
                color: signatureRow.signature.length ? Theme.inkAlpha(.6) : Theme.inkAlpha(.35); font { family: Theme.fontFamily; pointSize: Theme.fontPoints(12.5) }
            }
            AbstractButton {
                id: editSignature
                objectName: "editSignature_" + card.accountId
                hoverEnabled: true; enabled: backend.connected
                onClicked: card.host.editSignature(card.accountId)
                contentItem: Row {
                    spacing: 4
                    Text { text: (card.modelData.signature_text || "").length ? "Edit" : "Add"; color: editSignature.hovered ? Theme.text : Theme.inkAlpha(.75); font { family: Theme.fontFamily; pointSize: Theme.fontPoints(12.5) } anchors.verticalCenter: parent.verticalCenter }
                    FmIcon { name: "next"; size: 10; color: editSignature.hovered ? Theme.text : Theme.inkAlpha(.75); anchors.verticalCenter: parent.verticalCenter }
                }
                background: null
                Accessible.name: "Edit signature"
            }
        }
        // Local model: silver-blue tint, done and queued as solid and pale segments.
        StageRow {
            id: modelRow
            visible: !card.archived
            rule: Theme.withAlpha(Theme.ai, .18); color: Theme.withAlpha(Theme.ai, .045)
            readonly property var ai: card.pipeline.ai
            StageLabel { iconName: "summary"; text: "Local model"; textColor: Theme.ai; iconColor: Theme.ai; live: modelRow.ai.live === true }
            Meter { Layout.fillWidth: true; stage: modelRow.ai; twoTone: true; track: Theme.withAlpha(Theme.ai, .12); countName: "accountAnalysisCount_" + card.accountId }
            Row {
                spacing: 6
                AbstractButton {
                    id: setUp
                    visible: !modelRow.ai.modelReady
                    hoverEnabled: true
                    anchors.verticalCenter: parent.verticalCenter
                    onClicked: card.host.selectPage("Local LLM")
                    contentItem: Text { text: "Set up"; verticalAlignment: Text.AlignVCenter; color: setUp.hovered ? Theme.text : Theme.ai; font { family: Theme.fontFamily; pointSize: Theme.fontPoints(12) } }
                    background: null
                }
                AbstractButton {
                    id: catchUp
                    objectName: "accountBacklog_" + card.accountId
                    visible: modelRow.ai.modelReady && modelRow.ai.unprocessed > 0 && !card.pipeline.err
                    anchors.verticalCenter: parent.verticalCenter
                    hoverEnabled: true; enabled: backend.connected
                    implicitHeight: 22; leftPadding: 9; rightPadding: 7
                    onClicked: { card.analysis.refresh(card.accountId); catchUpMenu.open() }
                    background: Rectangle {
                        radius: height / 2; color: catchUp.hovered ? Theme.withAlpha(Theme.ai, .1) : "transparent"
                        Shape {
                            anchors.fill: parent; preferredRendererType: Shape.CurveRenderer
                            ShapePath {
                                fillColor: "transparent"; strokeColor: Theme.withAlpha(Theme.ai, .45); strokeWidth: 1
                                strokeStyle: ShapePath.DashLine; dashPattern: [3, 2]
                                PathRectangle { x: .5; y: .5; width: catchUp.width - 1; height: catchUp.height - 1; radius: (catchUp.height - 1) / 2 }
                            }
                        }
                    }
                    contentItem: Row {
                        spacing: 5
                        Text { text: "Catch up"; color: Theme.ai; font { family: Theme.fontFamily; pointSize: Theme.fontPoints(12) } anchors.verticalCenter: parent.verticalCenter }
                        FmIcon { name: "chevronDown"; size: 9; color: Theme.withAlpha(Theme.ai, .7); anchors.verticalCenter: parent.verticalCenter }
                    }
                    HoverHandler { id: catchUpHover }
                    FmToolTip { shown: catchUpHover.hovered && !catchUpMenu.opened; text: card.progress ? card.progress.fmtN(modelRow.ai.unprocessed) + " messages arrived before summarizing or labeling was on. Queue some or all of them for the local model." : "" }
                    Menu {
                        id: catchUpMenu
                        objectName: "catchUpMenu_" + card.accountId
                        y: catchUp.height + 6; x: catchUp.width - width; width: 280; padding: 5
                        readonly property var stats: card.analysis ? card.analysis.stats[card.accountId] || {} : {}
                        readonly property real perMessage: Number((card.progress.analysis[card.accountId] || {}).seconds_per_message || 0)
                        Label { width: parent.width; leftPadding: 10; rightPadding: 10; topPadding: 6; bottomPadding: 6; text: "Summarize & label older mail from…"; color: Theme.inkAlpha(.45); font.pointSize: Theme.fontPoints(12) }
                        Repeater {
                            model: [{key: "week", label: "Last week"}, {key: "month", label: "Last month"}, {key: "year", label: "Last year"}, {key: "all", label: "All"}]
                            MenuItem {
                                id: rangeOption
                                required property var modelData
                                objectName: "backlogRange_" + card.accountId + "_" + modelData.key
                                readonly property var count: (catchUpMenu.stats.ranges || {})[modelData.key]
                                // A wider range may be chosen while a narrower one runs.
                                enabled: count > 0
                                onTriggered: {
                                    if (card.aiPolicy.paused) card.policy.change(card.accountId, {paused: false})
                                    card.analysis.start(card.accountId, modelData.key)
                                }
                                contentItem: RowLayout {
                                    spacing: 12
                                    Text { text: rangeOption.modelData.key === "all" ? "All " + (card.progress ? card.progress.fmtN(modelRow.ai.unprocessed) : "") + " unprocessed" : rangeOption.modelData.label; color: rangeOption.enabled ? Theme.text : Theme.subtle; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(13); Layout.fillWidth: true }
                                    Text {
                                        text: rangeOption.count === undefined ? "…" : !rangeOption.count ? "—"
                                            : card.progress.fmtN(rangeOption.count) + (catchUpMenu.perMessage > 0 ? " · " + card.progress.shortEta(rangeOption.count * catchUpMenu.perMessage) : "")
                                        color: Theme.subtle; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12); font.features: {"tnum": 1}
                                    }
                                }
                                background: Rectangle { color: rangeOption.highlighted ? Theme.hover : "transparent"; radius: 4 }
                            }
                        }
                        MenuSeparator { contentItem: Rectangle { implicitHeight: 1; color: Theme.line } }
                        Label { width: parent.width; padding: 10; text: "Runs in the background · already processed mail is skipped"; wrapMode: Text.Wrap; color: Theme.inkAlpha(.45); font.pointSize: Theme.fontPoints(12) }
                        background: Rectangle { color: Theme.bg2; border.color: Theme.line2; radius: 6 }
                    }
                }
                Repeater {
                    model: [{key: "summaries", label: "Summarize", title: "Auto-summarize new mail"}, {key: "classification", label: "Label", title: "Auto-label new mail and flag automated senders"}]
                    AbstractButton {
                        id: chip
                        required property var modelData
                        objectName: "accountAi_" + card.accountId + "_" + modelData.key
                        readonly property bool on: card.aiPolicy[modelData.key] !== false
                        anchors.verticalCenter: parent.verticalCenter
                        hoverEnabled: true; implicitHeight: 22; leftPadding: 9; rightPadding: 9
                        opacity: modelRow.ai.modelReady ? 1 : .45
                        enabled: backend.connected && !(card.policy && card.policy.saving)
                        onClicked: modelRow.ai.modelReady ? card.policy.change(card.accountId, {[modelData.key]: !on}) : card.host.selectPage("Local LLM")
                        background: Rectangle { radius: height / 2; color: chip.on ? Theme.withAlpha(Theme.ai, .12) : "transparent"; border.color: chip.hovered ? Theme.ai : chip.on ? Theme.withAlpha(Theme.ai, .5) : Theme.line2 }
                        contentItem: Text { text: chip.modelData.label; verticalAlignment: Text.AlignVCenter; horizontalAlignment: Text.AlignHCenter; color: chip.on ? Theme.ai : Theme.inkAlpha(.4); font { family: Theme.fontFamily; pointSize: Theme.fontPoints(12) } }
                        Accessible.role: Accessible.CheckBox; Accessible.checked: on; Accessible.name: modelData.title
                        FmToolTip { shown: chip.hovered; text: chip.modelData.title + (chip.on ? " · on" : " · off") }
                    }
                }
                IconButton {
                    objectName: "accountAiPause_" + card.accountId
                    visible: modelRow.ai.canToggle === true
                    anchors.verticalCenter: parent.verticalCenter
                    framed: false; boxSize: 24; iconColor: Theme.ai
                    iconName: card.aiPolicy.paused ? "play" : "pause"
                    tip: card.aiPolicy.paused ? "Resume the local model" : "Pause the local model · downloading carries on"
                    enabled: backend.connected && !(card.policy && card.policy.saving)
                    onClicked: {
                        // Pausing holds the queue, Catch up included; resuming continues it.
                        card.policy.change(card.accountId, {paused: !card.aiPolicy.paused})
                    }
                }
            }
        }
        Text {
            visible: text.length > 0; Layout.fillWidth: true; Layout.margins: 18; Layout.topMargin: 10; Layout.bottomMargin: 10
            text: (card.analysis ? card.analysis.errors[card.accountId] || "" : "") || (card.policy && card.policy.status.length && card.policy.statusAccount === card.accountId ? card.policy.status : "")
            color: Theme.error; font.pointSize: Theme.fontPoints(12); wrapMode: Text.Wrap; textFormat: Text.PlainText
        }
        // A removed account keeps a read-only archive until it is deleted or restored.
        ColumnLayout {
            visible: !backend.syntheticProfile && (card.archived || (card.removal.status || "").length > 0); Layout.fillWidth: true; Layout.margins: 18; spacing: 8
            Text { visible: card.archived; text: "Your saved mail, drafts and history remain available in a read-only archive."; Layout.fillWidth: true; wrapMode: Text.Wrap; color: Theme.inkAlpha(.45); font.pointSize: Theme.fontPoints(12.5) }
            Text { visible: card.cleanup.state !== undefined; text: "Local credential cleanup: " + (card.cleanup.state || "") + (card.cleanup.code ? " · " + card.cleanup.code : ""); color: Theme.subtle; font.pointSize: Theme.fontPoints(12) }
            Text { visible: (card.removal.status || "").length > 0; text: card.removal.status || ""; Layout.fillWidth: true; wrapMode: Text.Wrap; color: Theme.subtle; font.pointSize: Theme.fontPoints(12) }
            Flow {
                Layout.fillWidth: true; spacing: 8
                FmButton { objectName: "openAccountArchive_" + card.accountId; text: "Open archive"; visible: card.archived; outlined: true; onClicked: card.host.openArchive(card.accountId) }
                FmButton { objectName: "deleteAccountArchive_" + card.accountId; text: "Delete archive…"; visible: card.archived; enabled: backend.connected && !card.removal.busy && card.cleanup.state === "complete" && !backend.unsavedAccountBuffer; onClicked: backend.previewArchiveDeletion(card.accountId) }
                FmButton { objectName: "retryAccountCleanup_" + card.accountId; text: "Retry cleanup"; visible: card.archived && (card.cleanup.state === "pending" || card.cleanup.state === "failed"); enabled: backend.connected && !card.removal.busy; onClicked: backend.retryAccountCleanup(card.accountId) }
                FmButton { objectName: "checkAccountRemoval_" + card.accountId; text: "Check saved result"; visible: card.removal.uncertain || false; enabled: backend.connected && !card.removal.busy; onClicked: backend.checkAccountRemoval(card.accountId, card.removal.request_id) }
                FmButton { objectName: "retryAccountRemoval_" + card.accountId; text: "Retry same removal"; visible: card.removal.uncertain || false; enabled: backend.connected && !card.removal.busy; onClicked: backend.retryAccountCleanup(card.accountId) }
                FmButton { objectName: "restoreAccount_" + card.accountId; text: "Restore…"; visible: card.archived; enabled: backend.connected && !backend.googleAuthBusy && !card.removal.busy && card.cleanup.state === "complete"; onClicked: card.host.restore(card.modelData) }
            }
        }
    }
    Popup {
        id: accountActions; parent: Overlay.overlay; width: 200; padding: 5; focus: true
        closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
        background: Rectangle { color: Theme.bg2; border.color: Theme.line2; radius: 6 }
        contentItem: ColumnLayout {
            spacing: 2
            FmButton { objectName: "editAccount_" + card.accountId; text: "Edit…"; Layout.fillWidth: true; textAlignment: Text.AlignLeft; enabled: backend.connected; onClicked: { accountActions.close(); card.host.editAccount(card.modelData) } }
            FmButton { objectName: "removeAccount_" + card.accountId; text: "Remove"; Layout.fillWidth: true; textAlignment: Text.AlignLeft; foreground: Theme.error; enabled: backend.connected && !backend.googleAuthBusy && !card.removal.busy; onClicked: { accountActions.close(); backend.previewAccountRemoval(card.accountId) } }
        }
    }
}
