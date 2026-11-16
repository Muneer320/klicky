import QtQuick
import Quickshell.Io
import qs.Ui as Ui

Ui.BarWidget {
  id: root
  moduleName: "io.github.muneer320.klicky-bar"
  readonly property bool opened: popup.opened
  readonly property bool popoutSwitchClosing: popup.popoutSwitchClosing
  readonly property bool placementChosen: ["left", "right", "clock-left", "clock-right"].indexOf(setting("placement", "")) !== -1
  readonly property bool placementBusy: placementTimer.running
  property string placementError: ""
  property string requestedPlacement: ""
  property string requestedSection: ""
  property int placementDeadline: 0
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
    requestedPlacement = choice
    requestedSection = section
    placementDeadline = 40
    // Moving can destroy this widget. Let a bounded detached helper finish
    // the move and save the choice independently of the QML object's lifetime.
    placementProcess.command = ["timeout", "--kill-after=1s", "15s", "bash", decodeURIComponent(Qt.resolvedUrl("move-placement.sh").toString().replace(/^file:\/\//, "")), choice, section, String(index)]
    placementTimer.restart()
    placementProcess.startDetached()
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
  }
  Timer {
    id: placementTimer
    interval: 400
    repeat: true
    onTriggered: {
      const layout = root.bar && root.bar.shell ? root.bar.shell.barConfig.layout || {} : {}
      const entries = layout[root.requestedSection] || []
      if (root.setting("placement", "") === root.requestedPlacement && entries.some(function(entry) { return entry.id === root.moduleName })) {
        stop()
        popup.placementExpanded = false
      } else if (--root.placementDeadline <= 0) {
        stop()
        root.placementError = "Could not save bar placement. Please try again."
      }
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
