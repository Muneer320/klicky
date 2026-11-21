import QtQuick
import Quickshell.Io
import "Audio.js" as Audio

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
  property bool audioKnown: false
  property var audioState: null
  property var audioList: null
  property string audioError: ""
  property int audioEpoch: 0
  property string audioKind: "status"
  property bool audioOverflow: false
  property bool forceOutputs: true
  readonly property bool audioUnavailable: audioKnown && audioState.availability !== "available"

  onExecutableChanged: {
    epoch += 1
    known = false
    clearAudio()
    Qt.callLater(root.refresh)
  }
  onPanelOpenChanged: if (panelOpen) { forceOutputs = true; refresh() }

  function refresh(force) {
    if (force === true) { forceOutputs = true; actionError = "" }
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
      clearAudio()
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
        clearAudio()
        error = "Klicky is starting or unresponsive. Refresh before changing playback."
        return
      }
      running = value.running
      paused = value.paused
      autostart = value.autostart
      volume = value.volume
      soundPack = value.sound_pack
      if (JSON.stringify(packs) !== JSON.stringify(value.packs)) packs = value.packs
      known = true
      error = ""
      if (running) refreshAudio()
      else clearAudio()
    } catch (_) {
      clearAudio()
      error = "Klicky Bar needs status --json schema 1. Update the Klicky engine."
    }
  }

  function execute(args) {
    if (busy || !known) return false
    epoch += 1
    if (audioQuery.running) audioQuery.running = false
    clearAudio()
    actionError = ""
    known = false
    action.command = bounded(args, "30s")
    action.running = true
    return true
  }

  function clearAudio() {
    audioKnown = false
    audioState = null
    audioList = null
    audioError = ""
    forceOutputs = true
  }

  function refreshAudio() {
    if (!known || !running || action.running || audioQuery.running) return
    audioEpoch = epoch
    audioKind = "status"
    audioOverflow = false
    audioQuery.command = bounded(["audio", "status"], "5s")
    audioQuery.running = true
  }

  function acceptAudio(text, code) {
    if (audioEpoch !== epoch || action.running || !known || !running) return
    if (code !== 0 || audioOverflow) {
      if (audioKind === "status") { audioKnown = false; audioState = null }
      audioList = null
      audioError = "Audio controls could not refresh. Update the engine or try Refresh."
      return
    }
    try {
      if (audioKind === "status") {
        const next = Audio.status(text)
        const changed = !audioState || JSON.stringify(next) !== JSON.stringify(audioState)
        if (changed) audioState = next
        audioKnown = true
        audioError = ""
        if (next.can_enumerate && (forceOutputs || changed || !audioList)) {
          forceOutputs = false
          audioKind = "outputs"
          audioOverflow = false
          audioQuery.command = bounded(["audio", "outputs"], "5s")
          audioQuery.running = true
        } else if (!next.can_enumerate) audioList = null
      } else {
        const nextList = Audio.outputs(text)
        if (JSON.stringify(nextList) !== JSON.stringify(audioList)) audioList = nextList
        audioError = ""
      }
    } catch (_) {
      if (audioKind === "status") { audioKnown = false; audioState = null }
      audioList = null
      audioError = "Audio controls need a compatible engine and valid audio status."
    }
  }

  function chooseAudio(value) {
    if (!known || !running || !audioKnown) return false
    const args = Audio.command(value, audioState, audioList)
    return args !== null && execute(args)
  }

  function retryAudio() {
    return known && running && audioKnown && audioState.can_retry && execute(["audio", "retry"])
  }

  function boundAudioOutput(text) {
    if (audioQuery.running && !audioOverflow && Audio.bytes(text) > 32768) {
      audioOverflow = true
      // Terminate timeout, which forwards termination to its child. Its
      // kill-after deadline bounds a CLI that ignores termination.
      audioQuery.running = false
    }
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
    id: audioQuery
    stdout: StdioCollector { id: audioOutput; waitForEnd: false; onTextChanged: root.boundAudioOutput(text) }
    stderr: StdioCollector { waitForEnd: false; onTextChanged: root.boundAudioOutput(text) }
    onExited: function(code) {
      root.acceptAudio(audioOutput.text, code)
      if (root.audioEpoch !== root.epoch) Qt.callLater(root.refreshAudio)
    }
  }
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
