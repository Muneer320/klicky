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
