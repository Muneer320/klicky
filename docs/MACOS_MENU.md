# macOS menu validation

The playback portion of issue #109 extends the existing AppKit helper. Audio-output selection, Retry Audio and unavailable-device status remain dependent on #107. These controls must not be described as complete output-routing support.

## Automated coverage

Run `python3 -B scripts/test-macos-menu.py` on macOS. It compiles the helper's actual request client and checks argument boundaries, nonzero exits, missing executables, oversized stderr, deadlines, inherited pipes, malformed status, unsupported schema versions and out-of-range volume. No installation or keyboard permission is required.

`cargo test` covers cancelled active/queued samples, pause/resume event filtering, confirmed live JSON state versus saved configuration, offline pack enumeration and missing-daemon pause behavior. On macOS it also checks that login startup changes do not invoke launchctl or start a daemon. Existing lifecycle tests remain enabled.

The helper uses one serial request worker. Each subprocess has a 15-second deadline and a 1 MiB combined output limit; daemon IPC has a three-second deadline. Only one mutation is admitted at a time. A click during a status poll waits for that poll. Native menu updates run on the main thread, and unchanged polls do not rebuild the menu. Action errors persist until an explicit refresh or another action.

## Physical checks before closing #109

Record macOS version, CPU architecture, audio hardware and source or Homebrew installation method. Compilation and fixtures do not establish physical input/audio behavior.

- Open with keyboard navigation and VoiceOver in light and dark appearance. Inspect labels, submenu navigation, disabled Pause/Resume and current pack/volume checkmarks.
- Pause while typing. Confirm sound stops, the PID stays the same and Resume does not replay old input. Confirm mute remains active rather than paused.
- Set a non-preset CLI volume such as `0.73`; confirm the menu includes and marks 73%. Change packs from both the menu and CLI, and add/remove a custom pack before Refresh Sound Packs.
- Reject a malformed pack. Confirm the error stays visible and the last working pack remains selected. Exercise an unavailable CLI and a stalled command; confirm the menu and Quit remain responsive.
- Toggle login startup while paused; confirm the PID and pause state remain unchanged. Start/stop the service separately. Quit Menu Bar App and confirm the daemon keeps running.
- Verify source and Homebrew CLI/icon overrides, upgrade and removal. Run installation lifecycle CI on Apple Silicon and Intel. Validate device selection, removal/reconnect and Retry Audio after #107 integration.

Physical checks and Intel validation must be recorded separately from automated results; they have not been performed by the fixture suite.
