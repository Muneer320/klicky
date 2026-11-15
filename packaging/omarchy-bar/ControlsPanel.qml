import QtQuick
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

  Ui.KeyboardPanel {
    id: popup
    anchorItem: root.anchorItem
    owner: root.hostWidget
    bar: root.bar
    open: root.opened
    focusTarget: panelContent
    contentWidth: popup.fittedContentWidth(Commons.Style.space(340))
    contentHeight: popup.fittedContentHeight(panelContent.implicitHeight)

    FocusScope {
      id: panelContent
      width: parent.width
      implicitHeight: layout.implicitHeight
      Keys.onEscapePressed: root.close()
      ColumnLayout {
        id: layout
        width: parent.width
        spacing: Commons.Style.space(10)
        Label { text: "Klicky Bar"; font.bold: true; color: root.barForeground }
        Label {
          text: root.engine.busy ? "Updating…" : !root.engine.known ? "Engine unavailable" : !root.engine.running ? "Stopped" : root.engine.paused ? "Paused" : "Ready"
          color: root.barForeground
        }
        Label {
          Layout.fillWidth: true
          visible: text.length > 0
          text: root.engine.error || root.engine.actionError
          wrapMode: Text.WordWrap
          color: root.barForeground
        }
        RowLayout {
          Ui.Button { text: "Pause"; focusable: true; bordered: true; opacity: enabled ? 1 : 0.4; enabled: root.engine.known && root.engine.running && !root.engine.paused && !root.engine.busy; onClicked: root.engine.execute(["pause"]) }
          Ui.Button { text: "Resume"; focusable: true; bordered: true; opacity: enabled ? 1 : 0.4; enabled: root.engine.known && root.engine.running && root.engine.paused && !root.engine.busy; onClicked: root.engine.execute(["resume"]) }
        }
        Label { text: "Sound pack"; color: root.barForeground }
        ComboBox {
          Layout.fillWidth: true
          model: root.engine.packs
          currentIndex: root.engine.packs.indexOf(root.engine.soundPack)
          enabled: root.engine.known && !root.engine.busy
          onActivated: root.engine.execute(["switch", "--", currentText])
          Accessible.name: "Sound pack"
          palette.button: Commons.Color.popups.background
          palette.buttonText: root.barForeground
          palette.base: Commons.Color.popups.background
          palette.text: root.barForeground
          palette.highlight: Commons.Color.accent
        }
        Label { text: root.engine.known ? "Volume · " + Math.round(volume.value * 100) + "%" : "Volume"; color: root.barForeground }
        Slider {
          id: volume
          objectName: "klickyVolume"
          Layout.fillWidth: true
          from: 0; to: 1; stepSize: 0.01
          enabled: root.engine.known && !root.engine.busy
          Accessible.name: "Klicky volume"
          palette.highlight: Commons.Color.accent
          Binding { target: volume; property: "value"; value: root.engine.volume; when: !volume.pressed && !root.volumeEditing && root.engine.known && !root.engine.busy }
          onMoved: {
            root.volumeEditing = true
            root.volumeDraft = value
            volumeCommit.restart()
          }
          onPressedChanged: if (!pressed && volumeCommit.running) volumeCommit.restart()
        }
        Timer {
          id: volumeCommit
          interval: 250
          onTriggered: {
            if (volume.pressed) restart()
            else {
              root.engine.execute(["volume", root.volumeDraft.toFixed(2)])
              root.volumeEditing = false
            }
          }
        }
        CheckBox {
          text: "Start at login"
          palette.windowText: root.barForeground
          checked: root.engine.autostart
          enabled: root.engine.known && !root.engine.busy
          onClicked: root.engine.execute(["service", "autostart", checked ? "enable" : "disable"])
        }
        Label { text: "Audio: system default output"; color: root.barForeground }
        RowLayout {
          Ui.Button {
            focusable: true
            bordered: true
            opacity: enabled ? 1 : 0.4
            text: root.engine.running ? "Stop engine" : "Start engine"
            enabled: root.engine.known && !root.engine.busy
            onClicked: root.engine.execute(["service", root.engine.running ? "stop" : "start"])
          }
          Ui.Button { text: "Refresh"; focusable: true; bordered: true; enabled: !root.engine.busy; onClicked: root.engine.refresh() }
        }
      }
    }
  }
}
