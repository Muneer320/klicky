import AppKit
import Foundation

final class KlickyMenu: NSObject, NSApplicationDelegate {
    private var item: NSStatusItem!
    private let toggle = NSMenuItem(title: "Turn Klicky On", action: #selector(toggleKlicky), keyEquivalent: "")
    private let status = NSMenuItem(title: "Checking status…", action: nil, keyEquivalent: "")
    private let binary = FileManager.default.homeDirectoryForCurrentUser
        .appendingPathComponent(".local/bin/klicky").path
    private var running = false
    private var enabled = false

    func applicationDidFinishLaunching(_ notification: Notification) {
        item = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)
        item.button?.title = "⌨"
        item.button?.toolTip = "Klicky"

        let menu = NSMenu()
        toggle.target = self
        menu.addItem(status)
        menu.addItem(toggle)
        menu.addItem(.separator())
        let quit = NSMenuItem(title: "Quit Menu Bar App", action: #selector(quitApp), keyEquivalent: "q")
        quit.target = self
        menu.addItem(quit)
        item.menu = menu

        refresh()
        Timer.scheduledTimer(withTimeInterval: 2, repeats: true) { [weak self] _ in
            self?.refresh()
        }
    }

    private func invoke(_ arguments: [String]) -> (Bool, String) {
        let task = Process()
        task.executableURL = URL(fileURLWithPath: binary)
        task.arguments = arguments
        let output = Pipe()
        task.standardOutput = output
        task.standardError = output
        do {
            try task.run()
            let data = output.fileHandleForReading.readDataToEndOfFile()
            task.waitUntilExit()
            return (task.terminationStatus == 0, String(decoding: data, as: UTF8.self))
        } catch {
            return (false, error.localizedDescription)
        }
    }

    private func refresh() {
        let (ok, result) = invoke(["service", "status"])
        running = ok && result.contains("running: yes")
        enabled = ok && result.contains("autostart: enabled")
        status.title = ok ? (running ? "Klicky is On" : "Klicky is Off") : "Klicky is unavailable"
        toggle.title = running ? "Turn Klicky Off" : (enabled ? "Turn Klicky On" : "Enable Klicky")
        toggle.isEnabled = ok
    }

    @objc private func toggleKlicky() {
        toggle.isEnabled = false
        let action = running ? "stop" : (enabled ? "start" : "enable")
        DispatchQueue.global(qos: .userInitiated).async { [weak self] in
            _ = self?.invoke(["service", action])
            DispatchQueue.main.async { self?.refresh() }
        }
    }

    @objc private func quitApp() {
        NSApplication.shared.terminate(nil)
    }
}

let app = NSApplication.shared
app.setActivationPolicy(.accessory)
let delegate = KlickyMenu()
app.delegate = delegate
app.run()
