func expectFailure(_ text: String, _ operation: () throws -> Void) {
    do { try operation(); fatalError("Expected failure: \(text)") }
    catch { precondition(error.localizedDescription.contains(text), error.localizedDescription) }
}

let client = CLIClient(binary: "/bin/sh", timeout: 0.4, outputLimit: 32768)
let success = try client.invoke(["-c", "printf '%s' \"$1\"", "fixture", "a pack with spaces; $(echo literal)"])
precondition(String(decoding: success, as: UTF8.self) == "a pack with spaces; $(echo literal)")
expectFailure("fixture diagnostic") { _ = try client.invoke(["-c", "printf 'fixture diagnostic' >&2; exit 2"]) }
expectFailure("too much output") { _ = try client.invoke(["-c", "yes diagnostic >&2"]) }
let started = ProcessInfo.processInfo.systemUptime
expectFailure("timed out") { _ = try client.invoke(["-c", "exec sleep 5"]) }
precondition(ProcessInfo.processInfo.systemUptime - started < 1.5)
do { _ = try CLIClient(binary: "/nonexistent/klicky").invoke([]); fatalError("Accepted a missing CLI") } catch {}
// An inherited pipe must not make us wait for EOF after the CLI exits.
let inheritedPipe = ProcessInfo.processInfo.systemUptime
let inheritedOutput = try client.invoke(["-c", "sleep 1 & printf done"])
precondition(String(decoding: inheritedOutput, as: UTF8.self) == "done")
precondition(ProcessInfo.processInfo.systemUptime - inheritedPipe < 0.8)
let valid = Data(#"{"schema_version":1,"running":true,"process_exists":true,"paused":true,"sound_pack":"pack","volume":0.73,"packs":["pack"],"autostart":false}"#.utf8)
let decoded = try PlaybackState.decode(valid)
precondition(decoded.paused && decoded.volume == 0.73)
expectFailure("invalid Klicky status") {
    _ = try PlaybackState.decode(Data(String(decoding: valid, as: UTF8.self).replacingOccurrences(of: "0.73", with: "1.5").utf8))
}
expectFailure("invalid Klicky status") {
    _ = try PlaybackState.decode(Data(String(decoding: valid, as: UTF8.self).replacingOccurrences(of: "\"schema_version\":1", with: "\"schema_version\":2").utf8))
}
do { _ = try PlaybackState.decode(Data("not json".utf8)); fatalError("Accepted malformed JSON") } catch {}
print("Menu client tests passed")


// Exercise real AppKit menu items and dispatch through the same action selector
// used by native clicks. This does not substitute for physical audio/VoiceOver QA.
extension KlickyMenu {
    func testNativeMenu() throws {
        item = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)
        defer { NSStatusBar.system.removeStatusItem(item) }
        menu.autoenablesItems = false
        item.menu = menu

        func entry(_ title: String, in parent: NSMenu? = nil) -> NSMenuItem {
            guard let result = (parent ?? menu).item(withTitle: title) else {
                fatalError("Missing menu item: \(title)")
            }
            return result
        }
        func load(running: Bool = true, paused: Bool = false, volume: Double = 0.73) {
            state = PlaybackState(schema_version: 1, running: running, process_exists: running,
                                  paused: paused, sound_pack: "--help", volume: volume,
                                  packs: ["--help", "-custom"], autostart: true)
            render()
        }
        for appearance in [NSAppearance.Name.aqua, .darkAqua] {
            NSApplication.shared.appearance = NSAppearance(named: appearance)
            load()
            precondition(entry("Pause").isEnabled && !entry("Resume").isEnabled)
            precondition(entry("Start Klicky at Login").state == .on)
            precondition(!entry("Start Klicky").isEnabled && entry("Stop Klicky").isEnabled)
            let volume = entry("Volume").submenu!
            precondition(entry("73%", in: volume).state == .on)
            let packs = entry("Sound Pack").submenu!
            precondition(entry("--help", in: packs).state == .on)
            for pack in ["--help", "-custom"] {
                precondition(entry(pack, in: packs).representedObject as? [String] == ["switch", "--", pack])
            }
            load(paused: true)
            precondition(!entry("Pause").isEnabled && entry("Resume").isEnabled)
            load(volume: 0)
            precondition(entry("0% (Mute)", in: entry("Volume").submenu!).state == .on)
            precondition(entry("Pause").isEnabled && !entry("Resume").isEnabled)
            load(running: false)
            precondition(!entry("Pause").isEnabled && !entry("Resume").isEnabled)
            precondition(entry("Start Klicky").isEnabled && !entry("Stop Klicky").isEnabled)
            lastError = "fixture action failed"
            render()
            precondition(entry("Error: fixture action failed").toolTip == "fixture action failed")
            actionInFlight = true
            render()
            precondition(!entry("Refresh Status").isEnabled && entry("Quit Menu Bar App").isEnabled)
            actionInFlight = false
            lastError = nil
        }
        load()
        let packMenu = entry("Sound Pack").submenu!
        packMenu.performActionForItem(at: packMenu.index(of: entry("--help", in: packMenu)))
        let deadline = ProcessInfo.processInfo.systemUptime + 5
        while busy && ProcessInfo.processInfo.systemUptime < deadline {
            RunLoop.current.run(until: Date(timeIntervalSinceNow: 0.01))
        }
        precondition(!busy && lastError == nil, "Native menu action did not finish successfully")
    }
}

let fixtureDirectory = FileManager.default.temporaryDirectory.appendingPathComponent("klicky-native-menu-" + UUID().uuidString)
try FileManager.default.createDirectory(at: fixtureDirectory, withIntermediateDirectories: true)
defer { try? FileManager.default.removeItem(at: fixtureDirectory) }
let fixture = fixtureDirectory.appendingPathComponent("klicky")
let argumentsFile = fixtureDirectory.appendingPathComponent("arguments")
let statusFile = fixtureDirectory.appendingPathComponent("status.json")
try valid.write(to: statusFile)
let script = """
#!/bin/sh
if [ "$1" = status ]; then
    /bin/cat "\(statusFile.path)"
else
    printf '%s\\n' "$@" > "\(argumentsFile.path)"
fi
"""
try script.write(to: fixture, atomically: true, encoding: .utf8)
try FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: fixture.path)
let oldCLI = ProcessInfo.processInfo.environment["KLICKY_MENU_CLI"]
setenv("KLICKY_MENU_CLI", fixture.path, 1)
defer {
    if let oldCLI = oldCLI { setenv("KLICKY_MENU_CLI", oldCLI, 1) }
    else { unsetenv("KLICKY_MENU_CLI") }
}
let application = NSApplication.shared
application.setActivationPolicy(.accessory)
let nativeMenu = KlickyMenu()
try nativeMenu.testNativeMenu()
let recordedArguments = try String(contentsOf: argumentsFile, encoding: .utf8)
precondition(recordedArguments == "switch\n--\n--help\n")
print("Native AppKit menu checks passed in Aqua and Dark Aqua; option-like pack action dispatched correctly")
