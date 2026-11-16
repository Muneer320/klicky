import QtQuick
import QtQuick.Window
import QtQuick.Controls
import QtQuick.Layouts
import qs.Ui as Ui
import qs.Commons as Commons

Ui.Panel {
  id: root
  moduleName: "io.github.muneer320.klicky-bar"
  manageIpc: false
  property var anchorItem: null
  property var hostWidget: null
  required property var engine
  property real volumeDraft: 0
  property bool volumeEditing: false
  property bool placementExpanded: false
  readonly property bool needsPlacement: !!hostWidget && !hostWidget.placementChosen
  readonly property bool moving: !!hostWidget && hostWidget.placementBusy
  readonly property bool canMove: !!hostWidget && !moving && !engine.busy && !volumeEditing
  readonly property color foreground: Commons.Color.popups.text
  readonly property color muted: Qt.darker(foreground, 1.45)
  readonly property string stateLabel: strings.t(engine.busy ? "status.updating" : !engine.known ? "status.unavailable" : !engine.running ? "status.stopped" : engine.paused ? "status.paused" : "status.ready")
  readonly property string currentPlacement: {
    const layout = bar && bar.shell ? bar.shell.barConfig.layout || {} : {}
    const sections = { "left": "left", "right": "right", "centerBegin": "clock-left", "centerEnd": "clock-right" }
    for (const section in sections) {
      if ((layout[section] || []).some(function(entry) { return entry.id === root.moduleName })) return sections[section]
    }
    return ""
  }

  Commons.Translations { id: strings; directory: Qt.resolvedUrl("i18n") }

  function reveal(control) {
    if (!control || !control.activeFocus || packSelect.popupOpen) return
    const point = control.mapToItem(layout, 0, 0)
    const viewport = scroll.contentItem as Flickable
    if (!viewport) return
    if (point.y < viewport.contentY) viewport.contentY = point.y
    else if (point.y + control.height > viewport.contentY + scroll.availableHeight)
      viewport.contentY = point.y + control.height - scroll.availableHeight
  }
  function packLabel(name) {
    if (name === "eg-oreo") return "Oreo"
    if (name === "eg-crystal-purple") return "Crystal Purple"
    const match = /^cherrymx-(black|blue|brown|red)-(abs|pbt)$/.exec(name)
    return match ? "Cherry MX " + match[1][0].toUpperCase() + match[1].slice(1) + " · " + match[2].toUpperCase() : name
  }
  Connections {
    target: root.engine
    function onKnownChanged() {
      if (root.engine.known && root.volumeEditing && !volume.pressed) volumeCommit.restart()
    }
  }

  Ui.KeyboardPanel {
    id: popup
    anchorItem: root.anchorItem
    owner: root.hostWidget
    bar: root.bar
    open: root.opened
    focusTarget: panelContent
    contentWidth: popup.fittedContentWidth(Commons.Style.space(380))
    contentHeight: popup.fittedContentHeight(layout.implicitHeight)
    FocusScope {
      id: panelContent
      anchors.fill: parent
      Keys.onEscapePressed: root.close()
      Connections {
        id: focusWatch
        target: panelContent.Window.window
        function onActiveFocusItemChanged() { root.reveal(focusWatch.target.activeFocusItem) }
      }
      ScrollView {
        id: scroll
        objectName: "klickyControlsScroll"
        anchors.fill: parent
        clip: true
        contentWidth: availableWidth
        contentHeight: layout.implicitHeight
        ScrollBar.horizontal.policy: ScrollBar.AlwaysOff
        ColumnLayout {
          id: layout
          width: scroll.availableWidth
          spacing: Commons.Style.space(14)
          Ui.PanelHero {
            Layout.fillWidth: true
            title: "Klicky"
            meta: root.stateLabel
            foreground: root.foreground
            iconComponent: Component {
              Rectangle {
                implicitWidth: Commons.Style.space(44)
                implicitHeight: implicitWidth
                radius: Commons.Style.cornerRadius
                color: Qt.rgba(Commons.Color.accent.r, Commons.Color.accent.g, Commons.Color.accent.b, 0.12)
                PanelText { anchors.centerIn: parent; text: "󰌌"; font.pixelSize: Commons.Style.font.display; color: Commons.Color.accent }
              }
            }
            trailingControl: Component {
              Ui.Button {
                objectName: "klickyRefresh"
                text: "󰑐"
                tooltipText: strings.t("refresh")
                focusable: true; bordered: true
                enabled: !root.engine.busy
                Accessible.name: strings.t("refresh")
                onActiveFocusChanged: root.reveal(this)
                onClicked: root.engine.refresh()
              }
            }
          }
          PanelText { Layout.fillWidth: true; text: strings.t("subtitle"); color: root.muted; font.pixelSize: Commons.Style.font.caption }
          Rectangle {
            Layout.fillWidth: true
            implicitHeight: problem.implicitHeight + Commons.Style.space(20)
            radius: Commons.Style.cornerRadius
            visible: problem.text.length > 0
            color: Qt.rgba(Commons.Color.urgent.r, Commons.Color.urgent.g, Commons.Color.urgent.b, 0.12)
            PanelText {
              id: problem
              anchors.fill: parent; anchors.margins: Commons.Style.space(10)
              text: root.hostWidget && root.hostWidget.placementError || root.engine.error || root.engine.actionError
            }
          }
          ColumnLayout {
            Layout.fillWidth: true
            visible: root.needsPlacement || root.placementExpanded
            spacing: Commons.Style.space(8)
            PanelText { Layout.fillWidth: true; text: strings.t(root.needsPlacement ? "setup.title" : "placement.title"); font.bold: true; font.pixelSize: Commons.Style.font.subtitle }
            PanelText { Layout.fillWidth: true; text: strings.t("setup.detail"); color: root.muted; font.pixelSize: Commons.Style.font.caption }
            GridLayout {
              Layout.fillWidth: true
              columns: 2
              columnSpacing: Commons.Style.space(8); rowSpacing: Commons.Style.space(8)
              Repeater {
                model: ["left", "right", "clock-left", "clock-right"]
                delegate: Ui.Button {
                  required property string modelData
                  objectName: "klickyPlace-" + modelData
                  Layout.fillWidth: true
                  text: strings.t("placement." + modelData)
                  iconText: modelData === "left" || modelData === "clock-left" ? "󰁍" : "󰁔"
                  focusable: true; bordered: true
                  selected: root.currentPlacement === modelData
                  enabled: root.canMove
                  opacity: enabled ? 1 : 0.4
                  onActiveFocusChanged: root.reveal(this)
                  onClicked: root.hostWidget.choosePlacement(modelData)
                }
              }
            }
            Ui.PanelSeparator { Layout.fillWidth: true; Layout.topMargin: Commons.Style.space(6) }
          }
          RowLayout {
            Layout.fillWidth: true
            spacing: Commons.Style.space(8)
            Ui.Button {
              Layout.fillWidth: true
              text: strings.t("pause"); iconText: "󰏤"
              focusable: true; bordered: true
              selected: root.engine.known && root.engine.running && root.engine.paused
              enabled: root.engine.known && root.engine.running && !root.engine.paused && !root.engine.busy
              opacity: enabled ? 1 : 0.4
              onActiveFocusChanged: root.reveal(this)
              onClicked: root.engine.execute(["pause"])
            }
            Ui.Button {
              Layout.fillWidth: true
              text: strings.t("resume"); iconText: "󰐊"
              focusable: true; bordered: true
              selected: root.engine.known && root.engine.running && !root.engine.paused
              enabled: root.engine.known && root.engine.running && root.engine.paused && !root.engine.busy
              opacity: enabled ? 1 : 0.4
              onActiveFocusChanged: root.reveal(this)
              onClicked: root.engine.execute(["resume"])
            }
          }
          Ui.PanelSectionHeader { text: strings.t("sound.title") }
          Ui.Dropdown {
            id: packSelect
            Layout.fillWidth: true
            label: strings.t("sound.pack"); showLabel: false
            options: root.engine.packs.map(function(name) { return {value: name, label: root.packLabel(name)} })
            enabled: root.engine.known && !root.engine.busy
            opacity: enabled ? 1 : 0.4
            Accessible.name: strings.t("sound.pack")
            Binding { target: packSelect; property: "value"; value: root.engine.soundPack; when: root.engine.known && !root.engine.busy && !packSelect.popupOpen }
            onChanged: function(value) { root.engine.execute(["switch", "--", value]) }
          }
          RowLayout {
            Layout.fillWidth: true
            PanelText { text: strings.t("volume.title"); color: root.muted }
            Item { Layout.fillWidth: true }
            PanelText { text: root.engine.known || root.volumeEditing ? Commons.I18n.formatNumber(Math.round(volume.value * 100)) + "%" : "—"; font.bold: true; font.pixelSize: Commons.Style.font.subtitle }
          }
          Slider {
            id: volume
            objectName: "klickyVolume"
            Layout.fillWidth: true
            from: 0; to: 1; stepSize: 0.01
            implicitHeight: Commons.Style.space(26)
            enabled: root.engine.known && !root.engine.busy
            opacity: enabled ? 1 : 0.4
            Accessible.name: strings.t("volume.title")
            onActiveFocusChanged: root.reveal(this)
            Binding { target: volume; property: "value"; value: root.engine.volume; when: !volume.pressed && !root.volumeEditing && root.engine.known && !root.engine.busy }
            onMoved: { root.volumeEditing = true; root.volumeDraft = value; volumeCommit.restart() }
            onPressedChanged: if (!pressed && volumeCommit.running) volumeCommit.restart()
            background: Rectangle {
              x: volume.leftPadding
              y: volume.topPadding + (volume.availableHeight - height) / 2
              width: volume.availableWidth; height: Commons.Style.space(4)
              radius: height / 2
              color: Qt.rgba(root.foreground.r, root.foreground.g, root.foreground.b, 0.16)
              Rectangle {
                width: parent.width * volume.position; height: parent.height
                anchors.left: volume.mirrored ? undefined : parent.left
                anchors.right: volume.mirrored ? parent.right : undefined
                radius: height / 2
                color: Commons.Color.accent
              }
            }
            handle: Rectangle {
              x: volume.leftPadding + volume.visualPosition * (volume.availableWidth - width)
              y: volume.topPadding + (volume.availableHeight - height) / 2
              width: Commons.Style.space(16); height: width; radius: width / 2
              color: root.foreground
              border.width: volume.activeFocus ? Commons.Style.space(2) : 0
              border.color: Commons.Color.accent
            }
          }
          Timer {
            id: volumeCommit
            interval: 250
            onTriggered: {
              if (volume.pressed) restart()
              else if (root.engine.execute(["volume", root.volumeDraft.toFixed(2)])) root.volumeEditing = false
            }
          }
          Ui.PanelSeparator { Layout.fillWidth: true }
          Ui.Toggle {
            Layout.fillWidth: true
            label: strings.t("startup.title"); description: strings.t("startup.detail")
            checked: root.engine.autostart
            enabled: root.engine.known && !root.engine.busy
            opacity: enabled ? 1 : 0.4
            onActiveFocusChanged: root.reveal(this)
            onClicked: root.engine.execute(["service", "autostart", checked ? "disable" : "enable"])
          }
          RowLayout {
            Layout.fillWidth: true
            Ui.Button {
              text: strings.t(root.engine.running ? "stop" : "start")
              focusable: true; bordered: true
              enabled: root.engine.known && !root.engine.busy
              opacity: enabled ? 1 : 0.4
              onActiveFocusChanged: root.reveal(this)
              onClicked: root.engine.execute(["service", root.engine.running ? "stop" : "start"])
            }
            Item { Layout.fillWidth: true }
            Ui.Button {
              objectName: "klickyPlacement"
              text: strings.t("placement.title")
              focusable: true; enabled: root.canMove
              onActiveFocusChanged: root.reveal(this)
              onClicked: root.placementExpanded = !root.placementExpanded
            }
          }
          PanelText { Layout.fillWidth: true; text: strings.t("audio.detail"); color: root.muted; font.pixelSize: Commons.Style.font.caption }
        }
      }
    }
  }
}
