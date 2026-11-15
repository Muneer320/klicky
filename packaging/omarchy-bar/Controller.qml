import QtQuick
import Quickshell.Io

Item {
  id: root

  property string executable: "klicky"
  property bool panelOpen: false
  property bool known: false
  property bool running: false
  property bool paused: false
  property bool autostart: false
  property real volume: 0
  property string soundPack: ""
  property var packs: []
  property string error: ""
  property string actionError: ""
  readonly property bool busy: action.running
  property int epoch: 0
  property int statusEpoch: 0

  function refresh() {
    if (status.running || action.running) return
    statusEpoch = epoch
    status.command = bounded(["status", "--json"], "20s")
    status.running = true
  }

  function bounded(args, limit) {
    return ["timeout", "--kill-after=1s", limit, executable].concat(args)
  }

  function acceptStatus(text, code) {
    if (statusEpoch !== epoch || action.running) return
    known = false
    if (code !== 0) {
      error = "Cannot read Klicky status. Check that a compatible Klicky engine is installed."
      return
    }
    try {
      const value = JSON.parse(text)
      if (value.schema_version !== 1 || typeof value.running !== "boolean"
          || typeof value.process_exists !== "boolean" || typeof value.paused !== "boolean"
          || typeof value.autostart !== "boolean" || typeof value.volume !== "number"
          || !isFinite(value.volume) || value.volume < 0 || value.volume > 1
          || typeof value.sound_pack !== "string" || !Array.isArray(value.packs)
          || value.packs.some(function(pack) { return typeof pack !== "string" || !pack.length || pack.length > 256 })) {
        throw new Error("Unsupported status")
      }
      if (value.process_exists && !value.running) {
        error = "Klicky is starting or unresponsive. Refresh before changing playback."
        return
      }
      running = value.running
      paused = value.paused
      autostart = value.autostart
      volume = value.volume
      soundPack = value.sound_pack
      packs = value.packs
      known = true
      error = ""
    } catch (_) {
      error = "Klicky Bar needs status --json schema 1. Update the Klicky engine."
    }
  }

  function execute(args) {
    if (busy || !known) return false
    epoch += 1
    actionError = ""
    known = false
    action.command = bounded(args, "30s")
    action.running = true
    return true
  }

  Component.onCompleted: refresh()
  Timer {
    interval: root.panelOpen ? 2000 : 10000
    running: true
    repeat: true
    onTriggered: root.refresh()
  }
  Timer { id: afterAction; interval: 50; onTriggered: root.refresh() }
  Process {
    id: status
    stdout: StdioCollector { id: statusOutput }
    stderr: StdioCollector {}
    onExited: function(code) {
      root.acceptStatus(statusOutput.text, code)
      if (root.statusEpoch !== root.epoch) afterAction.restart()
    }
  }
  Process {
    id: action
    stdout: StdioCollector {}
    stderr: StdioCollector {}
    onExited: function(code) {
      if (code !== 0) root.actionError = "The command failed or timed out. Refresh to check Klicky before retrying."
      root.known = false
      afterAction.restart()
    }
  }
}
