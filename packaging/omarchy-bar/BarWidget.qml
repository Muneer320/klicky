import QtQuick
import qs.Ui as Ui

Ui.BarWidget {
  id: root
  moduleName: "io.github.muneer320.klicky-bar"
  readonly property bool opened: popup.opened
  readonly property bool popoutSwitchClosing: popup.popoutSwitchClosing
  implicitWidth: button.implicitWidth
  implicitHeight: button.implicitHeight

  function open() { popup.open() }
  function close() { popup.close() }
  function toggle() { popup.toggle() }
  function closeForPopoutSwitch() { popup.closeForPopoutSwitch() }

  Controller { id: engineController; panelOpen: popup.opened }
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
