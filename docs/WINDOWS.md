# Windows development preview

The Windows implementation targets Windows 11 x64 interactive desktop sessions. It is under qualification. A successful build or an internal dispatch timing does not establish physical keypress-to-audible latency. Bluetooth output can add noticeable delay; use laptop speakers or a wired output for the reference listening check.

## Portable test bundle

The [unsigned portable workflow](https://github.com/Muneer320/klicky/blob/master/.github/workflows/windows-portable.yml) builds test artifacts without publishing a release. Extract the complete ZIP into a writable folder at a stable location. Keep both executables, the `sounds` folder and notices together. Rust and Visual Studio are needed to build the bundle, not to run it.

From the extracted folder, start with:

```powershell
.\klicky-desktop.exe --paused
```

Open its tray menu and choose Resume when ready to hear typing sounds. Choose Quit Klicky to exit, or run `.\klicky.exe stop`. Startup remains opt-in. The source-build command examples below also work in the extracted folder after replacing `.\target\release\` with `.\`.

Each ZIP contains `BUILD-INFO.json` with the exact source revision, target, toolchain, direct DLL imports, file hashes and dependency/source inventory. `THIRD-PARTY.txt` explains the included notices and unchanged decoder sources. The adjacent `.sha256` file identifies the ZIP. These unsigned artifacts remain unqualified development previews.

To build from a clean committed checkout with PowerShell 7, Rust 1.87 MSVC, Visual Studio C++ tools and a Windows SDK:

```powershell
pwsh -NoProfile -File .\scripts\build-windows-portable.ps1
```

Output goes to `target/windows-dist`. The builder uses static CRT linkage, checks executable metadata/manifests and rejects unexpected direct DLL imports. It does not overwrite an existing identified archive. A clean Windows 11 launch still requires qualification; import inspection cannot establish every runtime dependency or audible latency. No installer, automatic update or signing is provided by this ZIP.

## Installer test candidate

The same artifact workflow also compiles an unsigned installer. Installation, upgrade and failure recovery remain under qualification; use a disposable test environment first. This is not a published Windows release or a WinGet package.

The installer targets Windows 11 x64 and installs for the current user in `%LOCALAPPDATA%\Programs\Klicky`. It requests no elevation, adds no PATH entry and downloads no runtime components. Its Start menu shortcut opens Klicky paused. Login startup is unchecked by default and offered on a fresh install; existing startup registration and Windows approval are preserved during upgrades. No app starts automatically after a fresh silent install.

Build from a clean committed checkout with the portable build prerequisites and the official Inno Setup 7.1.1 x64 compiler:

```powershell
pwsh -NoProfile -File .\scripts\build-windows-installer.ps1 -CompilerPath 'C:\path\to\ISCC.exe'
```

The builder first creates the identified portable archive, verifies its complete payload inventory, then compiles `*-setup.exe` and an adjacent SHA-256 file in `target/windows-dist`. Supply `-PortableArchive` to reuse an archive from the exact same committed source. The workflow pins the compiler download by SHA-256 and uploads artifacts without publishing or installing Klicky.

For controlled silent testing, use `/VERYSILENT /SUPPRESSMSGBOXES /NORESTART` and an explicit `/LOG="C:\path\to\install.log"`. Login startup requires an explicit `/TASKS="startup"` on a fresh install. Custom `/DIR` locations are refused by this preview. Exit code 10 means the files were installed but startup registration or restoring the previous running state was not confirmed; inspect the log and tray controls.

Updates verify the installed control executable against the previous installer identity, request graceful shutdown of this bundle, wait for actual process exit and acquire the bundle marker exclusively. A hung owner or another signed-in session blocks replacement. Other portable copies are preserved. An update restarts only a previously running instance and retains its temporary pause intent. Quit requests already pending when preparation begins do not authorize a restart.

Before overwriting an existing install, the installer makes a verified recovery copy in `%LOCALAPPDATA%\KlickyInstaller\recovery`. It retains previous uninstall ownership across upgrades and preserves existing Start menu entries. A precommit installation failure attempts to restore previous files, the prior Start shortcut and the application's Installed Apps registration. If restoration fails or the installer is interrupted, retain that recovery folder and keep the partial bundle stopped. Its `payload` contains the previous bundle; `RECOVERY-FILES.txt` and `RECOVERY-SHA256.txt` identify the copied files. External metadata is saved separately when present. Power-loss recovery and the full clean-machine upgrade matrix remain unqualified. Recovery copies are outside the application and can remain after an interrupted operation.

Remove Klicky through **Settings > Apps > Installed apps**. Removal first stops and verifies this bundle, takes the exclusive marker and removes only its recognized login shortcut. Foreign startup entries are preserved. Configuration and custom packs in `%LOCALAPPDATA%\Klicky` remain. Do not manually replace files in a running installed bundle or move a portable copy into the managed install directory.

## Build and run

Install the Rust MSVC toolchain and Visual Studio C++ build tools, then build from the repository root:

```powershell
cargo build --locked --release
$env:KLICKY_SYSTEM_SOUNDS_DIR = (Resolve-Path .\sounds).Path
.\target\release\klicky.exe start --seconds 30
```

Windows builds require the Windows SDK resource compiler alongside the C++ tools. Both executables embed a Klicky icon, per-executable version information and an application manifest. The manifest requests normal user privileges, per-monitor DPI awareness and long-path support. Windows resources are mandatory: a missing resource compiler fails the build. Linux and macOS builds do not compile these resources.

The Windows file metadata labels these binaries as development previews; it does not represent a published Windows release. The Windows icon is exported from the shared [Klicky brand mark](https://github.com/Muneer320/klicky/blob/master/docs/BRANDING.md). Asset regeneration is a separate development task; building or running Klicky does not require Node.js.

Select your intended output in Windows before starting. After the tray tooltip says Ready, type in an editor. The command above ends automatically after 30 seconds. Without `--seconds`, Klicky stays in the notification area until you choose Quit Klicky, press Ctrl+C in its console, or run `klicky stop`. It does not require elevation or an input driver.

The console-free entrypoint is `klicky-desktop.exe`. Both entrypoints run the same engine and enforce one owner per logon session. Use `--paused` to open the tray without key sounds. `--benchmark` prints aggregate callback-entry-to-mixer timings at exit; it does not measure audible latency or record key names. Neither entrypoint accesses the microphone or reconstructs typed text.

The tray menu offers sound packs, volume in 10% steps, Pause, Resume, Retry audio, Refresh sound packs and Quit. Its **Start with Windows** submenu adds or removes login startup, refreshes registration status and opens Windows Startup settings. Registration does not override Windows approval. Startup work runs separately from playback and pack discovery; Quit waits for in-flight startup work before releasing the bundle. The CLI also accepts exact volume values:

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
.\target\release\klicky.exe service start --paused
.\target\release\klicky.exe service stop
.\target\release\klicky.exe service status
```

Enable registers the sibling `klicky-desktop.exe` and starts it now. Disable removes its recognized shortcut and stops this bundle's verified process. Start and stop affect the current session without changing login startup. A different portable copy is preserved. Background launch uses the sibling executable directly and waits for a verified state; Waiting for audio is reported as such.

`service start --paused` applies to a newly launched desktop process. It does not change an already-running instance's pause state.

Windows Settings or Task Manager may independently disable login startup. An existing matching shortcut is preserved byte for byte, so Enable does not override that choice. The status command reports registration, not Windows approval. Use **Settings > Apps > Startup** to change that approval. Shortcut recognition checks the target, arguments, working directory, elevation/launch flags, window mode, hotkey, icon and ownership description. Unrecognized or other-bundle `Klicky.lnk` shortcuts and changes to those launch settings are preserved and require you to resolve the conflict.

Keep the bundle at a stable location while login startup is registered. A running desktop process holds `.klicky.bundle.lock` through process exit. Installers must take that marker exclusively before replacing or removing bundle files; do not delete a held marker or force-close a process to bypass it. Updates also need to account for other logged-on sessions.

## Files and desktop behavior

Configuration is `%LOCALAPPDATA%\Klicky\config.toml`. Custom packs go in `%LOCALAPPDATA%\Klicky\sounds`; they override bundled names using Windows ordinal name comparison. Discovery accepts at most 512 directories per root and 512 distinct names across roots. Bundled packs resolve beside the executable in `sounds`, or through an absolute `KLICKY_SYSTEM_SOUNDS_DIR`. The working directory is not a fallback. Do not edit the configuration while testing live changes; use the controls above.

Capture remains disabled until Windows verifies this process's session is active and unlocked. Lock, disconnect, suspend and logoff notifications disable playback and discard obsolete input. Unlock and resume recheck session state and reopen audio. No accumulated keystrokes are replayed. The output follows the default multimedia endpoint; Retry audio permits an explicit retry after an error.

Input is passive Raw Input. Klicky does not suppress keys or synthesize typing. Firmware-only Fn actions and vendor-specific consumer controls may not produce a supported keyboard event. Layout, media-key, remote-session and device-change behavior require physical qualification.

## Qualification limits

These development binaries are not a Windows release. Physical-input-to-audible measurements, matched Linux baseline comparisons, long-running resource tests, additional Windows hardware and installation/upgrade validation remain release gates. Listening results on one speaker route do not qualify all outputs. Unsigned test artifacts can show Windows reputation warnings; no signing or automatic publication is configured by these entrypoints.
