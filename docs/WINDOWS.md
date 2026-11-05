# Klicky on Windows

Use Klicky on a **Windows 11 x64 desktop**. Its tray menu changes sounds and volume, pauses playback and controls login startup. Windows downloads currently use unsigned preview builds.

## Install and start

Download a Windows installer from [Releases](https://github.com/Muneer320/klicky/releases). If no public preview is attached yet, use the latest [Windows build artifact](https://github.com/Muneer320/klicky/actions/workflows/windows-portable.yml), extract the artifact ZIP and run `*-setup.exe`.

The wizard installs the app and all ten packs for your user. It needs no elevation, Rust or Visual Studio. Open Klicky from Start, then choose **Resume** from the tray. Login startup is optional. [Other installation methods](https://github.com/Muneer320/klicky/blob/master/docs/INSTALLATION.md#windows)

<details>
<summary><strong>Portable use</strong></summary>

Extract the complete portable ZIP into a stable folder and open `klicky-desktop.exe`. Keep both executables, `sounds` and notices together. Use `--paused` to start without sounds. Remove login startup before moving or deleting a portable bundle.

</details>

## Everyday controls

The tray offers packs, volume in 10% steps, Pause/Resume, Retry audio, Refresh sound packs and Quit. **Start with Windows** manages this bundle's login shortcut and opens Windows Startup settings. Windows may independently disable or defer startup; [startup help](https://github.com/Muneer320/klicky/blob/master/docs/TROUBLESHOOTING.md).

The CLI lives at `%LOCALAPPDATA%\Programs\Klicky\klicky.exe` for an installer setup. Run these from a portable folder, or use that installed path:

```powershell
.\klicky.exe status
.\klicky.exe list
.\klicky.exe switch eg-oreo
.\klicky.exe volume 0.5
.\klicky.exe pause
.\klicky.exe resume
.\klicky.exe retry-audio
.\klicky.exe stop
```

Pack and volume changes apply after loading and saving succeed. If the app is not running, they save for next start. Pause is temporary. Stop waits for confirmed process exit.

<details>
<summary><strong>Background launch and startup from the CLI</strong></summary>

```powershell
.\klicky.exe service enable
.\klicky.exe service disable
.\klicky.exe service start
.\klicky.exe service start --paused
.\klicky.exe service stop
.\klicky.exe service status
```

Enable registers the sibling desktop executable and starts it; disable removes its recognized shortcut and stops this bundle. Start/stop changes this session without altering startup. `start --paused` affects a newly launched instance, not an already-running one. Existing Windows approval and edited/foreign startup entries are preserved. Startup work runs separately from playback; Quit waits for in-flight work.

</details>

## Files and custom sounds

Settings live in `%LOCALAPPDATA%\Klicky\config.toml`. Put custom packs in `%LOCALAPPDATA%\Klicky\sounds`; they override bundled packs with the same name. Bundled sounds resolve beside the executable, not from the current working directory. `KLICKY_SYSTEM_SOUNDS_DIR` can supply an absolute alternate root. Discovery accepts up to 512 directories per root and 512 distinct pack names.

Klicky follows the default multimedia output. Choose Retry audio if an output needs reopening. Playback pauses outside the verified active/unlocked desktop and discards obsolete input on session transitions. Input is passive Raw Input: it does not suppress keys or synthesize text. [Troubleshooting](https://github.com/Muneer320/klicky/blob/master/docs/TROUBLESHOOTING.md)

## Diagnostics

```powershell
& "$env:LOCALAPPDATA\Programs\Klicky\klicky.exe" diagnostics
```

This reads aggregate counters without pausing or restarting the app, recording keys or accessing a microphone. Compare reports with the same process ID and creation time:

| Counter | Meaning |
|---|---|
| Input `sent` | Presses sent to the coordinator |
| Input `repeats` | Held-key repeats suppressed intentionally |
| Input `stale` / `overflow` | Input rejected for age or queue capacity |
| `engine_stale` / `obsolete_epoch` | Presses rejected after input delivery or invalidated by state changes |
| `voice_drops` | Playback rejected at the voice limit |
| `dispatched` | Sounds handed to the mixer |

Snapshots are approximate concurrent totals, not audible-latency measurements. `input: null` means the input backend is absent. Older previews need an update to support this command. Keep reports local unless you choose to share them.

## Installation and recovery

The installer uses `%LOCALAPPDATA%\Programs\Klicky`, preserves configuration and custom sounds on removal, and updates only after verifying shutdown and exclusive bundle ownership. It restarts a previously running app with its pause choice preserved. Another signed-in session or an unconfirmed shutdown blocks replacement.

If an update fails, the installer attempts to restore previous files, Start entry and Installed Apps registration. It keeps unresolved recovery material in `%LOCALAPPDATA%\KlickyInstaller\recovery`. Preserve that folder until recovery finishes. `payload` holds the old bundle; the adjacent file/hash lists and external metadata describe its snapshot. Ask for help before running a partial bundle or deleting the recovery copy.

Remove the installed app through **Settings → Apps → Installed apps**. Edited or other-bundle startup entries are preserved. Do not manually replace a running bundle or move a portable copy into the managed install directory.

<details>
<summary><strong>Build from source</strong></summary>

Install Rust 1.87+ MSVC, Visual Studio C++ tools and the Windows SDK resource compiler. From the checkout:

```powershell
cargo build --locked --release
$env:KLICKY_SYSTEM_SOUNDS_DIR = (Resolve-Path .\sounds).Path
.\target\release\klicky-desktop.exe --paused
```

The console entry point `klicky.exe start` uses the same engine. `--seconds 30` bounds a session; `--benchmark` prints aggregate internal dispatch diagnostics at exit, without key names. The shared [brand assets](https://github.com/Muneer320/klicky/blob/master/docs/BRANDING.md) are committed; Node is not needed to build or run the app.

</details>

<details>
<summary><strong>Build portable and installer packages</strong></summary>

Use PowerShell 7 with the source-build tools above, from a clean committed checkout:

```powershell
pwsh -NoProfile -File .\scripts\build-windows-portable.ps1
pwsh -NoProfile -File .\scripts\build-windows-installer.ps1 -CompilerPath 'C:\path\to\ISCC.exe'
```

The installer compiler is official Inno Setup 7.1.1 x64. Output is under `target/windows-dist`. `-PortableArchive` reuses an archive from the same source. Builders verify manifests, DLL imports, payload inventory and licenses; `BUILD-INFO.json` records source/toolchain/hash provenance. Archives contain notices and the required decoder source material.

For unattended installation, use `/VERYSILENT /SUPPRESSMSGBOXES /NORESTART` and an explicit `/LOG="C:\path\to\install.log"`. Fresh silent installs do not launch the app. `/TASKS="startup"` opts in to login startup; custom `/DIR` locations are refused. Exit 10 means files installed but startup registration or restarting was not confirmed; inspect the log and tray.

[Release engineering checklist](https://github.com/Muneer320/klicky/blob/master/docs/RELEASE_READINESS.md) · [Measurement tooling](https://github.com/Muneer320/klicky/blob/master/tools/windows-qualification/USAGE.md)

</details>
