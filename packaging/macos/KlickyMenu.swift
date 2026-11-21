import AppKit
import Foundation
import Darwin

struct PlaybackState: Decodable, Equatable {
    let schema_version: Int
    let running: Bool
    let process_exists: Bool
    let paused: Bool
    let sound_pack: String
    let volume: Double
    let packs: [String]
    let autostart: Bool

    static func decode(_ data: Data) throws -> PlaybackState {
        let state = try JSONDecoder().decode(PlaybackState.self, from: data)
        guard state.schema_version == 1, state.volume.isFinite, (0...1).contains(state.volume) else {
            throw MenuError.message("Unsupported or invalid Klicky status. Update the CLI and helper together.")
        }
        return state
    }
}

struct AudioDevice: Decodable, Equatable {
    let backend: String
    let id: String
    let name: String?

    var valid: Bool {
        backend == "core_audio" && AudioPreference.validID(id) &&
            name.map { !$0.isEmpty && $0.utf8.count <= 2048 } != false
    }
    var label: String { name ?? id }
}

struct AudioPreference: Decodable, Equatable {
    let mode: String
    let backend: String?
    let id: String?

    static func validID(_ id: String) -> Bool {
        !id.isEmpty && id.utf8.count <= 1024 &&
            !id.unicodeScalars.contains { CharacterSet.controlCharacters.contains($0) }
    }
    var valid: Bool {
        if mode == "system_default" { return backend == nil && id == nil }
        return mode == "device" && backend == "core_audio" && id.map(Self.validID) == true
    }
}

struct AudioState: Decodable, Equatable {
    let schema_version: Int
    let requested: AudioPreference?
    let active: AudioDevice?
    let availability: String
    let manual_paused: Bool
    let can_select: Bool
    let can_enumerate: Bool
    let can_retry: Bool

    static func decode(_ data: Data) throws -> AudioState {
        let state = try JSONDecoder().decode(Self.self, from: data)
        guard data.count <= 32768, state.schema_version == 1,
              ["initializing", "available", "unavailable", "recovering", "inactive"].contains(state.availability),
              state.requested.map({ $0.valid }) != false,
              state.active.map({ $0.valid }) != false else {
            throw MenuError.message("Unsupported or invalid audio status. Update the CLI and helper together.")
        }
        if let active = state.active {
            guard let requested = state.requested, state.availability == "available",
                  requested.mode == "system_default" || requested.id == active.id else {
                throw MenuError.message("Audio output does not match the requested route.")
            }
        }
        return state
    }

    var summary: String {
        switch availability {
        case "available": return active.map { "Output: " + $0.label } ?? "Audio output ready"
        case "initializing": return "Opening audio output…"
        case "recovering": return "Reconnecting audio output…"
        case "unavailable": return "Selected audio output is unavailable"
        default: return "Audio output is inactive"
        }
    }
}

struct AudioOutputs: Decodable, Equatable {
    let schema_version: Int
    let devices: [AudioDevice]
    let default_id: String?
    let suggested_speakers_id: String?

    static func decode(_ data: Data) throws -> AudioOutputs {
        guard data.count <= 32768 else { throw MenuError.message("Audio output list is too large.") }
        let list = try JSONDecoder().decode(Self.self, from: data)
        let ids = Set(list.devices.map { $0.id })
        guard list.schema_version == 1, list.devices.count <= 64,
              list.devices.allSatisfy({ $0.valid }), ids.count == list.devices.count,
              [list.default_id, list.suggested_speakers_id].compactMap({ $0 }).allSatisfy({ ids.contains($0) }) else {
            throw MenuError.message("Unsupported or invalid audio output list.")
        }
        return list
    }
}

struct AudioChoice {
    let title: String
    let arguments: [String]
    let checked: Bool
    let enabled: Bool
    let detail: String
}

// Keep selection identity and checkmarks independent of AppKit/display names.
func audioChoices(_ state: AudioState, _ outputs: AudioOutputs?) -> [AudioChoice] {
    var choices = [AudioChoice(title: "Follow System Default", arguments: ["audio", "follow-default"],
        checked: state.requested?.mode == "system_default", enabled: state.can_select && state.requested != nil,
        detail: "Follow macOS output changes for Klicky.")]
    let devices = outputs?.devices ?? []
    let counts = Dictionary(grouping: devices, by: { $0.label }).mapValues { $0.count }
    for (index, device) in devices.enumerated() {
        var title = device.label
        if counts[title, default: 0] > 1 { title += " (\(index + 1))" }
        if device.id == outputs?.default_id { title += " (System Default)" }
        choices.append(AudioChoice(title: title,
            arguments: ["audio", "select", "--backend", "core-audio", "--id", device.id],
            checked: state.requested?.mode == "device" && state.requested?.id == device.id,
            enabled: state.can_select && state.requested != nil, detail: device.id))
    }
    if state.requested?.mode == "device", let id = state.requested?.id,
       !devices.contains(where: { $0.id == id }) {
        let label = state.active.map { $0.id == id ? $0.label : id } ?? id
        choices.append(AudioChoice(title: label + (outputs == nil ? " (Saved selection)" : " (Unavailable)"),
            arguments: [], checked: true, enabled: false, detail: id))
    }
    return choices
}

enum MenuError: LocalizedError {
    case message(String)
    var errorDescription: String? {
        switch self { case .message(let text): return text }
    }
}

// Runs only on the serial worker. Nonblocking reads bound both output and elapsed
// time, even when a child fills stderr or leaves a pipe open in a descendant.
struct CLIClient {
    let binary: String
    var timeout: TimeInterval = 15
    var outputLimit = 1_048_576

    func invoke(_ arguments: [String]) throws -> Data {
        let task = Process()
        task.executableURL = URL(fileURLWithPath: binary)
        task.arguments = arguments
        task.standardInput = FileHandle.nullDevice
        let stdout = Pipe(), stderr = Pipe()
        task.standardOutput = stdout
        task.standardError = stderr
        let pipes = [stdout, stderr]
        for pipe in pipes {
            let fd = pipe.fileHandleForReading.fileDescriptor
            guard fcntl(fd, F_SETFL, fcntl(fd, F_GETFL) | O_NONBLOCK) != -1 else {
                throw MenuError.message("Could not prepare Klicky command output.")
            }
        }
        defer {
            for pipe in pipes {
                try? pipe.fileHandleForReading.close()
                try? pipe.fileHandleForWriting.close()
            }
        }
        try task.run()
        var output = [Data(), Data()]
        let deadline = ProcessInfo.processInfo.systemUptime + timeout
        do {
            while true {
                let running = task.isRunning
                for (index, pipe) in pipes.enumerated() {
                    var bytes = [UInt8](repeating: 0, count: 8192)
                    while true {
                        let count = Darwin.read(pipe.fileHandleForReading.fileDescriptor, &bytes, bytes.count)
                        if count <= 0 { break }
                        output[index].append(contentsOf: bytes.prefix(count))
                        guard output[0].count + output[1].count <= outputLimit else {
                            throw MenuError.message("Klicky command returned too much output.")
                        }
                        guard ProcessInfo.processInfo.systemUptime < deadline else {
                            throw MenuError.message("Klicky command timed out. Try refreshing.")
                        }
                    }
                }
                if !running { break }
                guard ProcessInfo.processInfo.systemUptime < deadline else {
                    throw MenuError.message("Klicky command timed out. Try refreshing.")
                }
                Thread.sleep(forTimeInterval: 0.01)
            }
            task.waitUntilExit()
        } catch {
            if task.isRunning { kill(task.processIdentifier, SIGKILL) }
            task.waitUntilExit()
            throw error
        }
        guard task.terminationStatus == 0 else {
            let details = String(decoding: output[1].isEmpty ? output[0] : output[1], as: UTF8.self)
                .trimmingCharacters(in: .whitespacesAndNewlines)
            throw MenuError.message(details.isEmpty ? "Klicky command failed (\(task.terminationStatus))." : details)
        }
        return output[0]
    }
}

final class KlickyMenu: NSObject, NSApplicationDelegate, NSMenuDelegate {
    private var item: NSStatusItem!
    private let menu = NSMenu()
    private let worker = DispatchQueue(label: "dev.klicky.menu.requests", qos: .userInitiated)
    private let client = CLIClient(binary: ProcessInfo.processInfo.environment["KLICKY_MENU_CLI"] ??
        FileManager.default.homeDirectoryForCurrentUser.appendingPathComponent(".local/bin/klicky").path)
    private var state: PlaybackState?
    private var audio: AudioState?
    private var outputs: AudioOutputs?
    private var audioError: String?
    private var pendingOutputsRefresh = false
    private var busy = false
    private var actionInFlight = false
    private var pendingAction: [String]?
    private var lastError: String?
    private var timer: Timer?

    func applicationDidFinishLaunching(_ notification: Notification) {
        item = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)
        let iconPath = ProcessInfo.processInfo.environment["KLICKY_MENU_ICON"] ?? FileManager.default.homeDirectoryForCurrentUser
            .appendingPathComponent("Library/Application Support/klicky/branding/klicky-menu-template.png").path
        if let data = try? Data(contentsOf: URL(fileURLWithPath: iconPath)),
           let standard = NSBitmapImageRep(data: data) {
            let size = NSSize(width: 18, height: 18)
            let image = NSImage(size: size)
            standard.size = size
            image.addRepresentation(standard)
            let retinaPath = (iconPath as NSString).deletingPathExtension + "@2x.png"
            if let data = try? Data(contentsOf: URL(fileURLWithPath: retinaPath)),
               let retina = NSBitmapImageRep(data: data) {
                retina.size = size
                image.addRepresentation(retina)
            }
            image.isTemplate = true
            item.button?.image = image
            item.button?.imagePosition = .imageOnly
        } else {
            item.button?.title = "⌨"
        }
        item.button?.toolTip = "Klicky"

        item.button?.setAccessibilityLabel("Klicky playback controls")
        menu.autoenablesItems = false
        menu.delegate = self
        item.menu = menu
        render()
        refresh()
        timer = Timer(timeInterval: 2, repeats: true) { [weak self] _ in self?.refresh() }
        if let timer = timer { RunLoop.main.add(timer, forMode: .common) }
    }

    func menuWillOpen(_ menu: NSMenu) { request(nil, refreshOutputs: true) }

    private func add(_ title: String, action: Selector? = nil, arguments: [String]? = nil,
                     enabled: Bool = true, checked: Bool = false, to destination: NSMenu? = nil) {
        let entry = NSMenuItem(title: title, action: action, keyEquivalent: "")
        entry.target = self
        entry.representedObject = arguments
        entry.isEnabled = enabled && (action == #selector(quitApp) || (!actionInFlight && pendingAction == nil))
        entry.state = checked ? .on : .off
        (destination ?? menu).addItem(entry)
    }

    private func render() {
        menu.removeAllItems()
        let running = state?.running == true
        let paused = state?.paused == true
        let status = state.map { $0.running ? ($0.paused ? "Klicky is Paused" : "Klicky is Running") :
            ($0.process_exists ? "Klicky is Unresponsive" : "Klicky is Stopped") } ?? "Klicky is Unavailable"
        add(actionInFlight || pendingAction != nil ? "Updating Klicky…" : status, enabled: false)
        item.button?.toolTip = status
        if let error = lastError {
            add("Error: " + String(error.prefix(140)), enabled: false)
            menu.items.last?.toolTip = error
        }
        add("Refresh Status", action: #selector(refreshAction))
        menu.addItem(.separator())
        add("Pause", action: #selector(performAction(_:)), arguments: ["pause"], enabled: running && !paused)
        add("Resume", action: #selector(performAction(_:)), arguments: ["resume"], enabled: running && paused)

        let volume = NSMenu(title: "Volume")
        volume.autoenablesItems = false
        add("Volume", enabled: state != nil)
        menu.items.last?.submenu = volume
        // Include non-preset CLI values so the current volume always has a mark.
        var levels = Array(stride(from: 0, through: 100, by: 10)).map { Double($0) / 100 }
        if let current = state?.volume, !levels.contains(where: { abs($0 - current) < 0.00001 }) {
            levels.append(current)
            levels.sort()
        }
        for level in levels {
            let title = level == 0 ? "0% (Mute)" : String(format: "%g%%", level * 100)
            add(title, action: #selector(performAction(_:)), arguments: ["volume", String(level)],
                checked: state.map { abs($0.volume - level) < 0.00001 } ?? false, to: volume)
        }
        let packs = NSMenu(title: "Sound Pack")
        packs.autoenablesItems = false
        add("Sound Pack", enabled: state != nil)
        menu.items.last?.submenu = packs
        for pack in state?.packs ?? [] {
            add(pack, action: #selector(performAction(_:)), arguments: ["switch", "--", pack],
                checked: pack == state?.sound_pack, to: packs)
        }
        if let state = state, !state.packs.contains(state.sound_pack) {
            add(state.sound_pack + " (Unavailable)", enabled: false, checked: true, to: packs)
        }
        if state?.packs.isEmpty != false { add("No sound packs found", enabled: false, to: packs) }
        packs.addItem(.separator())
        add("Refresh Sound Packs", action: #selector(refreshAction), to: packs)
        let audioMenu = NSMenu(title: "Audio Output")
        audioMenu.autoenablesItems = false
        add("Audio Output", enabled: running)
        menu.items.last?.submenu = audioMenu
        if let audio = audio, running {
            add(audio.summary, enabled: false, to: audioMenu)
            audioMenu.items.last?.toolTip = audio.summary
            audioMenu.addItem(.separator())
            for choice in audioChoices(audio, outputs) {
                add(choice.title, action: #selector(performAction(_:)), arguments: choice.arguments,
                    enabled: choice.enabled, checked: choice.checked, to: audioMenu)
                audioMenu.items.last?.toolTip = choice.detail
            }
            if let outputs = outputs, outputs.devices.isEmpty {
                add("No audio outputs found", enabled: false, to: audioMenu)
            }
            audioMenu.addItem(.separator())
            add("Retry Audio", action: #selector(performAction(_:)), arguments: ["audio", "retry"],
                enabled: audio.can_retry, to: audioMenu)
        } else {
            add(running ? "Audio controls unavailable" : "Start Klicky to choose an output", enabled: false, to: audioMenu)
        }
        if let error = audioError, running {
            add("Error: " + String(error.prefix(100)), enabled: false, to: audioMenu)
            audioMenu.items.last?.toolTip = error
        }
        add("Refresh Audio Outputs", action: #selector(refreshAction), enabled: running, to: audioMenu)
        menu.addItem(.separator())
        add("Start Klicky", action: #selector(performAction(_:)), arguments: ["service", "start"],
            enabled: state != nil && !running && state?.process_exists == false)
        add("Stop Klicky", action: #selector(performAction(_:)), arguments: ["service", "stop"],
            enabled: state != nil && (running || state?.process_exists == true))
        add("Start Klicky at Login", action: #selector(toggleStartup), enabled: state != nil,
            checked: state?.autostart == true)
        menu.addItem(.separator())
        add("Quit Menu Bar App", action: #selector(quitApp))
        menu.items.last?.keyEquivalent = "q"
    }

    private func request(_ arguments: [String]?, refreshOutputs: Bool = false) {
        if busy {
            pendingOutputsRefresh = pendingOutputsRefresh || refreshOutputs
            // A click arriving during a status poll runs immediately after it.
            if let arguments = arguments, !actionInFlight, pendingAction == nil {
                pendingAction = arguments
                render()
            }
            return
        }
        busy = true
        actionInFlight = arguments != nil
        if actionInFlight { render() }
        let client = self.client
        let oldAudio = audio
        let oldOutputs = outputs
        worker.async { [weak self] in
            var actionError: String?
            if let arguments = arguments {
                do { _ = try client.invoke(arguments) }
                catch { actionError = error.localizedDescription }
            }
            var nextState: PlaybackState?
            var refreshError: String?
            do { nextState = try PlaybackState.decode(client.invoke(["status", "--json"])) }
            catch { refreshError = error.localizedDescription }
            var nextAudio: AudioState?
            var nextOutputs: AudioOutputs?
            var nextAudioError: String?
            if nextState?.running == true {
                var audioClient = client
                audioClient.timeout = 4
                audioClient.outputLimit = 32768
                do {
                    let confirmed = try AudioState.decode(audioClient.invoke(["audio", "status"]))
                    nextAudio = confirmed
                    if confirmed.can_enumerate {
                        if refreshOutputs || arguments != nil || oldOutputs == nil || oldAudio != confirmed {
                            nextOutputs = try AudioOutputs.decode(audioClient.invoke(["audio", "outputs"]))
                        } else { nextOutputs = oldOutputs }
                    }
                } catch { nextAudioError = error.localizedDescription }
            }
            DispatchQueue.main.async { [weak self, nextState, nextAudio, nextOutputs, nextAudioError, actionError, refreshError] in
                guard let self = self else { return }
                let oldState = self.state
                let oldError = self.lastError
                let audioChanged = self.audio != nextAudio || self.outputs != nextOutputs || self.audioError != nextAudioError
                self.busy = false
                self.actionInFlight = false
                self.state = nextState
                self.audio = nextAudio
                self.outputs = nextOutputs
                self.audioError = nextAudioError
                // Keep an action failure visible across polling until the next
                // explicit action or refresh, rather than hiding it two seconds later.
                if arguments != nil || actionError != nil || refreshError != nil {
                    self.lastError = actionError ?? refreshError
                }
                if let pending = self.pendingAction {
                    self.pendingAction = nil
                    self.pendingOutputsRefresh = false
                    self.request(pending, refreshOutputs: true)
                } else if self.pendingOutputsRefresh {
                    self.pendingOutputsRefresh = false
                    self.render()
                    self.request(nil, refreshOutputs: true)
                } else if oldState != nextState || oldError != self.lastError || audioChanged || arguments != nil {
                    self.render()
                }
            }
        }
    }

    private func refresh() { request(nil) }
    @objc private func refreshAction() { lastError = nil; render(); request(nil, refreshOutputs: true) }
    @objc private func performAction(_ sender: NSMenuItem) {
        guard let arguments = sender.representedObject as? [String] else { return }
        request(arguments)
    }
    @objc private func toggleStartup() {
        guard let state = state else { return }
        request(["service", "autostart", state.autostart ? "disable" : "enable"])
    }
    @objc private func quitApp() { NSApplication.shared.terminate(nil) }
}

#if !MENU_TEST
let app = NSApplication.shared
app.setActivationPolicy(.accessory)
let delegate = KlickyMenu()
app.delegate = delegate
app.run()
#endif
