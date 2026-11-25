# Linux tray controls (optional source helper)

[Installation](INSTALLATION.md) · [CLI contract](CLI.md) · [Architecture](ARCHITECTURE.md)

`klicky-tray` adds a native menu to desktops with StatusNotifierItem and D-BusMenu support. It is a separate controller for an existing Klicky engine, not another audio engine. It is not bundled with the v0.7.0 release or enabled by the normal installer.

## Build and install

Use Linux, Rust 1.87 or newer, Python 3, `/usr/bin/env`, an existing Klicky engine installation, and a graphical user session with a compatible tray host and session D-Bus. Use the CLI from this checkout: it includes Linux service-child cleanup when the helper cancels a request. An older packaged CLI can provide the menu contract but cannot gain that cleanup from the helper alone; a hung, independently grouped `systemctl` may survive cancellation. The helper uses `ksni` 0.3.7 with Tokio/zbus and has its own lockfile. It does not add GTK, Qt or tray libraries to the default engine build. Your desktop supplies the native menu renderer.

From a checkout containing this helper, with the engine's [source build prerequisites](INSTALLATION.md) installed:

```sh
cargo build --release --locked --target-dir target
python3 scripts/install-linux-tray.py \
  --cli "$(pwd)/target/release/klicky"
```

This uses the updated CLI without replacing an existing engine installation. Keep the checkout and built CLI at that path, or install the updated CLI and pass its absolute installed path. The installer builds only the helper and uses Cargo's reported executable path, including configured target directories. For an already built helper, pass `--binary PATH` explicitly. Installation starts neither the helper nor playback. Open **Klicky controls** from the application menu to start the helper. A missing/incompatible CLI produces an unavailable state with controls disabled; it does not silently install or start an engine.

Only one copy of this helper can run per session. On Omarchy, choose either Klicky Bar or this tray helper as your visible controller; stop/disable the other controller using its own setup instructions. The installer does not change another bar's configuration.

## Controls and confirmed state

The menu shows Running, Paused, Stopped, Starting or unresponsive, or Unavailable. Pause/Resume, volume presets and sound-pack choices refresh from `klicky status --json`; matching actions are disabled. Volume and pack changes use the CLI's existing persistence. A failed or timed-out request displays an error and refreshes rather than claiming success. A timeout can occur after an engine accepted a command, so refresh before retrying.

Unusable sound-pack directory names are omitted from the choices without disabling playback controls. Up to 256 usable choices are shown; names are escaped/truncated for display while commands retain their exact original arguments.

- **Start playback / Stop playback** control the engine.
- **Start playback at login** changes the engine's startup preference only. Enabling it does not start playback now.
- **Quit tray helper (keep playback)** removes only the controller. It neither stops the engine nor changes startup.
- **Check for updates** runs only `klicky update --check` after an explicit click. Results appear in the menu. It never installs updates, changes files or runs automatic checks.

Audio-output choices appear only when the running engine advertises support. Default-only Linux engines retain the basic controls without an output picker. Device identity comes from the backend and opaque ID, not the display name. A missing remembered device is retained, and **Retry audio (keep Pause)** does not send Resume. This helper does not implement or qualify PipeWire, PulseAudio, ALSA or macOS audio routing.

CLI work runs outside menu callbacks. Each command has a deadline (status/audio queries: 5 seconds; controls: 10 seconds; explicit update check: 25 seconds) and bounded stdout/stderr (64 KiB each). Refresh is every 10 seconds and on opening the menu; unchanged snapshots do not republish menus. An explicit action cancels a pending background query. Quitting cancels active work and terminates that CLI process group. `--refresh-seconds 2..60` adjusts the interval for diagnosis.

## Login startup, upgrade and removal

Helper startup is separate from engine startup and disabled by default:

```sh
python3 scripts/install-linux-tray.py --autostart --cli "$(pwd)/target/release/klicky"
```

This explicitly installs a helper-only login entry with `--wait-for-tray`, allowing the desktop tray host to start later. Run the installer with `--no-autostart` to remove its unchanged helper login entry. Ordinary upgrades retain the previous helper-startup choice. Quit the helper before upgrading or removing it, then relaunch it after upgrading.

The installer owns only these per-user files:

| File | Purpose |
| --- | --- |
| `~/.local/bin/klicky-tray` | Helper executable |
| `$XDG_DATA_HOME/klicky-tray/klicky.png` | Existing Klicky icon |
| `$XDG_DATA_HOME/klicky-tray/installation.json` | Owned-file hashes |
| `$XDG_DATA_HOME/applications/io.github.muneer320.klicky-tray.desktop` | Application launcher |
| `$XDG_CONFIG_HOME/autostart/io.github.muneer320.klicky-tray.desktop` | Explicitly requested helper startup |

The default XDG data/config homes are `~/.local/share` and `~/.config`. The installer refuses symlink paths and refuses to overwrite unowned or modified files, including a login entry disabled by your desktop. Back up and resolve a modified helper file before reinstalling. A failed installation rolls back its file changes. Engine settings, custom packs and engine startup are outside its ownership.

```sh
python3 scripts/install-linux-tray.py --remove
```

Removal deletes only unchanged files recorded by this installer. Modified files are preserved with an explanation and an ownership record. The regular engine updater/uninstaller does not currently manage this optional source helper; remove it separately when removing the engine.

## Desktop compatibility and qualification

KDE Plasma and LXQt provide compatible tray hosts. GNOME Shell needs a separately installed, compatible AppIndicator/StatusNotifier extension; GNOME setup is tracked in [#204](https://github.com/Muneer320/klicky/issues/204). This change does not ship an extension. Bare i3 or another bar needs an SNI host or a separate CLI-based adapter.

Use `klicky-tray --help` for direct-run options. An absent tray host fails with guidance by default; `--wait-for-tray` stays available until a host appears. If a host disappears after registration, the helper waits for it to return while playback remains unchanged. A missing icon or unreadable menu can be desktop-theme specific: the installer supplies an absolute PNG path, while direct runs default to the `input-keyboard` theme icon.

Native desktop, theme, keyboard-accessibility and hardware typing checks must be recorded separately from protocol tests. X11 virtual-display tests do not establish Wayland, GNOME, physical audio routing or input-to-audible latency support. Leave those qualifications visibly unfinished until performed.

Current qualification finding: submenus near the right screen edge clipped in the Plasma 5.27.5 X11/Xvfb test session. Keyboard activation still worked, but complete visual compatibility remains under investigation. This helper remains a development candidate pending that check and physical typing qualification.
