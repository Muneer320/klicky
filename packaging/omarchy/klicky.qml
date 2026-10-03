import QtQuick
import Quickshell.Io
import qs.Ui

BarIconButton {
  id: root

  property string moduleName: "klicky"
  property var settings: ({})
  property bool running: false
  readonly property bool revealed: running || (bar && bar.centerSectionRevealHeld && !bar.centerHoverRevealSuppressed)

  text: "󰌌"
  active: running
  useActiveColor: false
  visible: revealed
  dimmed: !running
  tooltipText: running ? "Turn Klicky Off" : "Turn Klicky On"

  function refresh() {
    if (!statusProc.running) statusProc.running = true
  }

  Component.onCompleted: refresh()

  Timer {
    interval: 5000
    running: true
    repeat: true
    onTriggered: root.refresh()
  }

  Process {
    id: statusProc
    command: ["systemctl", "--user", "is-active", "--quiet", "klicky.service"]
    onExited: function(exitCode) {
      root.running = exitCode === 0
    }
  }

  Process {
    id: toggleProc
    onExited: refreshTimer.restart()
  }

  Timer {
    id: refreshTimer
    interval: 250
    onTriggered: root.refresh()
  }

  onPressed: function(button) {
    if (button !== Qt.LeftButton || toggleProc.running) return
    toggleProc.command = ["systemctl", "--user", root.running ? "stop" : "start", "klicky.service"]
    toggleProc.running = true
  }
}
