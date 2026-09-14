import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

ColumnLayout {
    id: setup
    objectName: "googleLabelSetup"
    spacing: 10
    property string account: backend.account
    property string selectedLabelId: ""
    property double selectedCatalogRevision: -1
    property string selectedCategoryId: ""
    property var expectedProviderLabelId: null
    property var confirmation: ({})
    readonly property var state: backend.googleLabelState.account === account ? backend.googleLabelState : ({})
    readonly property var categories: backend.categories
    readonly property var userLabels: backend.googleLabelState.account === account ? backend.googleLabels.filter(function(label) {
        return label.present && label.selectable && (!label.category_id || label.category_id === setup.selectedCategoryId)
    }) : []
    readonly property var selectedLabel: userLabels.find(function(label) { return label.id === setup.selectedLabelId }) || ({})
    readonly property var selectedCategory: categories.find(function(category) { return category.id === setup.selectedCategoryId }) || ({})
    readonly property bool catalogChanged: selectedLabelId.length > 0 && selectedCatalogRevision !== state.catalog_revision
    readonly property bool mappingChanged: selectedCategoryId.length > 0 && (selectedCategory.provider_label_id || null) !== expectedProviderLabelId
    readonly property bool busy: !!state.loading || !!state.refreshing || !!state.binding
    readonly property bool refreshReady: backend.connected && backend.accounts.length >= 0 && backend.googleSync
        && backend.accountCapability(account, "mail_read") && backend.googleSyncState(account).active === true
    readonly property bool canBind: backend.connected && backend.accounts.length >= 0 && backend.accountCapability(account, "mail_write")
        && state.available === true
        && !busy && !state.pending && !catalogChanged && !mappingChanged && !!selectedLabel.id
        && (selectedCategoryId.length > 0 ? !!selectedCategory.id && expectedProviderLabelId !== selectedLabelId : localName.text.trim().length > 0)
    function loadCached() { if (visible && !backend.syntheticProfile && account.length) backend.loadGoogleLabels(account, false) }
    function resetSelection() { selectedLabelId = ""; selectedCatalogRevision = -1; selectedCategoryId = ""; expectedProviderLabelId = null; localName.text = "" }
    function bindingValues() { return {account: account, label: selectedLabelId, category: selectedCategoryId, name: localName.text.trim(), expected: expectedProviderLabelId, revision: selectedCatalogRevision} }
    function submit(values) { backend.bindGoogleLabel(values.account, values.label, values.category, values.name, values.expected, values.revision) }
    onAccountChanged: { resetSelection(); loadCached() }
    onVisibleChanged: loadCached()
    Component.onCompleted: loadCached()
    Connections {
        target: backend
        function onConnectedChanged() { if (backend.connected) setup.loadCached() }
        function onGoogleLabelBindingFinished(account, ok, result, error) {
            if (account === setup.account && ok) setup.resetSelection()
        }
    }
    RowLayout {
        Layout.fillWidth: true; spacing: 10
        Text { text: "Google labels · " + ((backend.accounts.find(function(a) { return a.id === setup.account }) || {}).name || setup.account); color: Theme.muted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(12); textFormat: Text.PlainText; Layout.fillWidth: true; elide: Text.ElideRight }
        FmButton { objectName: "googleLabelsRefresh"; text: setup.state.refreshing ? "Refreshing…" : "Refresh Google labels"; outlined: true; implicitHeight: 30; enabled: !setup.busy && setup.refreshReady; onClicked: backend.refreshGoogleLabels(setup.account) }
        FmButton { objectName: "googleLabelsCancelRefresh"; text: "Cancel"; visible: !!setup.state.refreshing; implicitHeight: 30; onClicked: backend.cancelGoogleOperation(setup.account, "") }
    }
    Text {
        text: !setup.state.available ? "No cached Google labels yet. Start sync in Accounts, then refresh labels."
            : setup.state.stale_connection || setup.state.connection_state !== "connected" ? "These cached labels are from an earlier connection. You can save links locally; refresh Google labels when sync is available."
            : "Link an existing Google user label to a local label. This setup does not create or rename labels on Google."
        color: Theme.subtle; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(12); wrapMode: Text.Wrap; Layout.fillWidth: true
    }
    ColumnLayout {
        Layout.fillWidth: true; spacing: 8; enabled: !setup.busy && !setup.state.pending
        RowLayout {
            Layout.fillWidth: true; spacing: 10
            Text { text: "Local label"; color: Theme.subtle; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(12); Layout.preferredWidth: 82 }
            FmComboBox {
                id: localLabel; objectName: "googleLabelsLocalCategory"; Layout.fillWidth: true; implicitHeight: 30
                model: ["Create a local label"].concat(setup.categories.map(function(category) { return category.name }))
                currentIndex: setup.selectedCategoryId.length ? setup.categories.findIndex(function(category) { return category.id === setup.selectedCategoryId }) + 1 : 0
                onActivated: {
                    let category = currentIndex > 0 ? setup.categories[currentIndex - 1] : null
                    setup.selectedCategoryId = category ? category.id : ""; setup.expectedProviderLabelId = category ? category.provider_label_id || null : null
                    setup.selectedLabelId = ""; setup.selectedCatalogRevision = -1
                    currentIndex = Qt.binding(function() { return setup.selectedCategoryId.length ? setup.categories.findIndex(function(category) { return category.id === setup.selectedCategoryId }) + 1 : 0 })
                }
            }
        }
        RowLayout {
            Layout.fillWidth: true; spacing: 10
            Text { text: "Google label"; color: Theme.subtle; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(12); Layout.preferredWidth: 82 }
            FmComboBox {
                id: googleLabel; objectName: "googleLabelsProviderLabel"; Layout.fillWidth: true; implicitHeight: 30
                model: setup.userLabels.map(function(label) { return label.name || label.id })
                currentIndex: setup.userLabels.findIndex(function(label) { return label.id === setup.selectedLabelId })
                displayText: currentIndex < 0 ? "Choose a Google user label" : setup.userLabels[currentIndex] ? setup.userLabels[currentIndex].name || setup.userLabels[currentIndex].id : ""
                onActivated: {
                    let label = setup.userLabels[currentIndex]; if (!label) return
                    setup.selectedLabelId = label.id; setup.selectedCatalogRevision = setup.state.catalog_revision
                    if (!setup.selectedCategoryId.length && !localName.text.length) localName.text = (label.name || "").slice(0, 256)
                    currentIndex = Qt.binding(function() { return setup.userLabels.findIndex(function(label) { return label.id === setup.selectedLabelId }) })
                }
            }
        }
        RowLayout {
            visible: !setup.selectedCategoryId.length; Layout.fillWidth: true; spacing: 10
            Text { text: "Name"; color: Theme.subtle; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(12); Layout.preferredWidth: 82 }
            TextField { id: localName; objectName: "googleLabelsCreateName"; Layout.fillWidth: true; implicitHeight: 32; maximumLength: 256; placeholderText: "Local label name"; color: Theme.text; placeholderTextColor: Theme.subtle; selectByMouse: true; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(12); background: Rectangle { color: Theme.canvas; border.color: Theme.controlBorder; radius: 5 } }
        }
        Text { visible: setup.selectedCategoryId.length > 0; text: setup.expectedProviderLabelId ? "Current Google label: " + setup.expectedProviderLabelId : "This local label has no Google link."; color: Theme.subtle; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(11); textFormat: Text.PlainText; wrapMode: Text.Wrap; Layout.fillWidth: true }
        Text { visible: setup.catalogChanged || setup.mappingChanged; text: "The catalog or link changed. Choose the local and Google labels again before saving."; color: Theme.accent; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(11); wrapMode: Text.Wrap; Layout.fillWidth: true }
    }
    RowLayout {
        Layout.fillWidth: true; spacing: 8
        FmButton {
            objectName: "googleLabelsBind"; text: setup.state.binding ? "Saving…" : setup.state.pending ? "Retry link" : setup.selectedCategoryId.length ? setup.expectedProviderLabelId ? "Remap local label" : "Link local label" : "Create local label"; outlined: true; implicitHeight: 30
            enabled: !setup.busy && (setup.state.pending ? backend.connected : setup.canBind)
            onClicked: {
                if (setup.state.pending) { backend.retryGoogleLabelBinding(setup.account); return }
                if (!setup.canBind) return
                const values = setup.bindingValues()
                if (values.category.length && values.expected) { setup.confirmation = values; remapConfirmation.open() }
                else setup.submit(values)
            }
        }
        Item { Layout.fillWidth: true }
        FmButton { objectName: "googleLabelsMore"; text: "Load more labels"; visible: !!setup.state.next_after; implicitHeight: 30; enabled: !setup.busy && backend.connected; onClicked: backend.loadGoogleLabels(setup.account, true) }
    }
    Text {
        visible: backend.googleLabelState.account === setup.account && backend.googleLabels.some(function(label) { return !label.present })
        text: "Unavailable linked Google labels: " + backend.googleLabels.filter(function(label) { return !label.present }).map(function(label) { return label.name || label.id }).join(", ")
        color: Theme.accent; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(11); textFormat: Text.PlainText; wrapMode: Text.Wrap; Layout.fillWidth: true
    }
    Text { visible: !!setup.state.status; text: setup.state.status || ""; color: Theme.accent; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(12); textFormat: Text.PlainText; wrapMode: Text.Wrap; Layout.fillWidth: true }
    Dialog {
        id: remapConfirmation; objectName: "googleLabelsRemapConfirmation"; parent: Overlay.overlay; anchors.centerIn: parent; modal: true
        implicitWidth: 344; width: Math.min(implicitWidth, parent.width - 32)
        implicitHeight: topPadding + bottomPadding + remapBody.implicitHeight + (header ? header.implicitHeight + spacing : 0) + (footer ? footer.implicitHeight + spacing : 0)
        title: "Remap local label?"; standardButtons: Dialog.Ok | Dialog.Cancel
        background: Rectangle { color: Theme.surface; border.color: Theme.controlBorder; radius: 8 }
        palette.windowText: Theme.text; palette.buttonText: Theme.text
        onOpened: standardButton(Dialog.Ok).text = "Remap"
        contentItem: Item {
            implicitHeight: remapBody.implicitHeight
            Text { id: remapBody; width: remapConfirmation.availableWidth; text: "Replace this local label's Google link. Existing recorded message changes keep their original destination. Future label changes use the selected Google label."; color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(13); wrapMode: Text.Wrap; textFormat: Text.PlainText }
        }
        onAccepted: setup.submit(setup.confirmation)
    }
}
