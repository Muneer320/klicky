import QtQuick
import Quickshell.Io
import qs.Ui as Ui

Ui.BarWidget {
  id: root
  moduleName: "io.github.muneer320.klicky-bar"
  readonly property bool opened: popup.opened
  readonly property bool popoutSwitchClosing: popup.popoutSwitchClosing
  readonly property bool placementChosen: ["left", "right", "clock-left", "clock-right"].indexOf(setting("placement", "")) !== -1
  readonly property bool placementBusy: placementProcess.running
  property string placementError: ""
  property bool setupPrompted: false
  implicitWidth: button.implicitWidth
  implicitHeight: button.implicitHeight

  function open() { popup.open() }
  function close() { popup.close() }
  function toggle() { popup.toggle() }
  function closeForPopoutSwitch() { popup.closeForPopoutSwitch() }

  function choosePlacement(choice) {
    if (placementBusy || engineController.busy || popup.volumeEditing) return
    if (["left", "right", "clock-left", "clock-right"].indexOf(choice) === -1) return
    const sections = { "left": "left", "right": "right", "clock-left": "centerBegin", "clock-right": "centerEnd" }
    const section = sections[choice]
    if (!section) return
    const layout = bar && bar.shell ? bar.shell.barConfig.layout || {} : {}
    const others = (layout[section] || []).filter(function(entry) { return entry.id !== root.moduleName })
    const index = choice === "right" || choice === "clock-left" ? others.length : 0
    placementError = ""
    // Save the setup choice and placement in one native transaction: moving
    // may destroy this widget, so success must not need a follow-up write.
    placementProcess.command = ["timeout", "--kill-after=1s", "10s", "omarchy", "bar", "set", moduleName, "placement", choice, "--section", section, "--index", String(index)]
    placementProcess.running = true
  }

  function offerSetup() {
    if (bar && !placementChosen && !setupPrompted) setupTimer.restart()
  }
  onBarChanged: offerSetup()
  onSettingsChanged: offerSetup()
  Component.onCompleted: offerSetup()
  Timer {
    id: setupTimer
    interval: 400
    onTriggered: {
      if (root.bar && !root.placementChosen && !root.setupPrompted) {
        root.setupPrompted = true
        root.open()
      }
    }
  }
  Process {
    id: placementProcess
    stdout: StdioCollector {}
    stderr: StdioCollector {}
    onExited: function(code) {
      if (code !== 0) root.placementError = "Could not move Klicky. Please try again."
      else popup.placementExpanded = false
    }
  }

  Controller {
    id: engineController
    executable: root.setting("executable", "klicky")
    panelOpen: popup.opened
  }
  Ui.WidgetButton {
    id: button
    anchors.fill: parent
    bar: root.bar
    text: "󰌌"
    active: engineController.known && engineController.running && !engineController.paused
    dimmed: !active
    tooltipText: "Klicky Bar · " + (!engineController.known ? "Check engine" : !engineController.running ? "Stopped" : engineController.paused ? "Paused" : "Ready")
    onPressed: function(mouseButton) {
      if (mouseButton === Qt.LeftButton || mouseButton === Qt.RightButton) root.toggle()
    }
  }
  ControlsPanel {
    id: popup
    bar: root.bar
    anchorItem: button
    hostWidget: root
    engine: engineController
  }
}
