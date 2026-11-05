# Windows development preview

The Windows implementation targets Windows 11 x64 interactive desktop sessions. It is under qualification. A successful build or an internal dispatch timing does not establish physical keypress-to-audible latency. Bluetooth output can add noticeable delay; use laptop speakers or a wired output for the reference listening check.

## Build and run

Install the Rust MSVC toolchain and Visual Studio C++ build tools, then build from the repository root:

```powershell
cargo build --locked --release
$env:KLICKY_SYSTEM_SOUNDS_DIR = (Resolve-Path .\sounds).Path
.\target\release\klicky.exe start --seconds 30
```

Select your intended output in Windows before starting. After the tray tooltip says Ready, type in an editor. The command above ends automatically after 30 seconds. Without `--seconds`, Klicky stays in the notification area until you choose Quit Klicky, press Ctrl+C in its console, or run `klicky stop`. It does not require elevation or an input driver.

The console-free entrypoint is `klicky-desktop.exe`. Both entrypoints run the same engine and enforce one owner per logon session. Use `--paused` to open the tray without key sounds. `--benchmark` prints aggregate callback-entry-to-mixer timings at exit; it does not measure audible latency or record key names. Neither entrypoint accesses the microphone or reconstructs typed text.

The tray menu offers sound packs, volume in 10% steps, Pause, Resume, Retry audio, Refresh sound packs and Quit. The CLI also accepts exact volume values:

```powershell
.\target\release\klicky.exe status
.\target\release\klicky.exe list
.\target\release\klicky.exe switch eg-oreo
.\target\release\klicky.exe volume 0.8
.\target\release\klicky.exe pause
.\target\release\klicky.exe resume
.\target\release\klicky.exe retry-audio
.\target\release\klicky.exe stop
```

Switch and volume changes apply live after preparation and persistence succeed. When no instance owns this session, they save for next start. An unresponsive or unverified instance does not permit an offline fallback. Pause is temporary and is not saved. Stop succeeds only after the verified server process exits.

## Login startup and background launch

Login startup is opt-in. These commands manage this bundle's per-user Startup shortcut and desktop process:

```powershell
.\target\release\klicky.exe service enable
.\target\release\klicky.exe service disable
.\target\release\klicky.exe service start
.\target\release\klicky.exe service stop
.\target\release\klicky.exe service status
```

Enable registers the sibling `klicky-desktop.exe` and starts it now. Disable removes its recognized shortcut and stops this bundle's verified process. Start and stop affect the current session without changing login startup. A different portable copy is preserved. Background launch uses the sibling executable directly and waits for a verified state; Waiting for audio is reported as such.

Windows Settings or Task Manager may independently disable login startup. An existing matching shortcut is preserved byte for byte, so Enable does not override that choice. The status command reports registration, not Windows approval. Use **Settings > Apps > Startup** to change that approval. Shortcut recognition checks the target, arguments, working directory, elevation/launch flags, window mode, hotkey, icon and ownership description. Unrecognized or other-bundle `Klicky.lnk` shortcuts and changes to those launch settings are preserved and require you to resolve the conflict.

Keep the bundle at a stable location while login startup is registered. A running desktop process holds `.klicky.bundle.lock` through process exit. Installers must take that marker exclusively before replacing or removing bundle files; do not delete a held marker or force-close a process to bypass it. Updates also need to account for other logged-on sessions.

## Files and desktop behavior

Configuration is `%LOCALAPPDATA%\Klicky\config.toml`. Custom packs go in `%LOCALAPPDATA%\Klicky\sounds`; they override bundled names using Windows ordinal name comparison. Discovery accepts at most 512 directories per root and 512 distinct names across roots. Bundled packs resolve beside the executable in `sounds`, or through an absolute `KLICKY_SYSTEM_SOUNDS_DIR`. The working directory is not a fallback. Do not edit the configuration while testing live changes; use the controls above.

Capture remains disabled until Windows verifies this process's session is active and unlocked. Lock, disconnect, suspend and logoff notifications disable playback and discard obsolete input. Unlock and resume recheck session state and reopen audio. No accumulated keystrokes are replayed. The output follows the default multimedia endpoint; Retry audio permits an explicit retry after an error.

Input is passive Raw Input. Klicky does not suppress keys or synthesize typing. Firmware-only Fn actions and vendor-specific consumer controls may not produce a supported keyboard event. Layout, media-key, remote-session and device-change behavior require physical qualification.

## Qualification limits

These development binaries are not a Windows release. Physical-input-to-audible measurements, matched Linux baseline comparisons, long-running resource tests, additional Windows hardware and installation/upgrade validation remain release gates. Listening results on one speaker route do not qualify all outputs. Unsigned test artifacts can show Windows reputation warnings; no signing or automatic publication is configured by these entrypoints.
