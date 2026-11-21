# CLI reference

[Field guide](https://klicky.muneer320.tech/guide.html#cli) · [Configuration](CONFIGURATION.md) · [Troubleshooting](TROUBLESHOOTING.md)

Use `klicky --help` or `klicky help COMMAND` to inspect the build you installed. Pick the command you need; you do not need to run every example.

## Find the executable

- Linux package: `/usr/bin/klicky`.
- Linux/macOS source installer: `~/.local/bin/klicky`; add `~/.local/bin` to your shell PATH if desired.
- Windows installer: `%LOCALAPPDATA%\Programs\Klicky\klicky.exe`. It is not registered globally on PATH.
- Windows portable: `klicky.exe` inside the extracted bundle, alongside `klicky-desktop.exe` and `sounds`.
- Microsoft Store or Scoop: `klicky` in a new terminal. For a missing Store alias, check **Settings → Apps → Advanced app settings → App execution aliases**.

Standalone installer PowerShell example:

```powershell
& "$env:LOCALAPPDATA\Programs\Klicky\klicky.exe" status
```

The examples below use `klicky`, which works directly with Microsoft Store or Scoop. For a standalone Windows installation, substitute the installed invocation above or `.\klicky.exe` from the portable folder.

## Updates

The update commands are available from v0.7.0. Checks contact GitHub only when requested; no keyboard events or preferences are sent. Store and package-manager availability can lag behind GitHub. Source, portable and unrecognized installations receive manual guidance. See [update behavior and supported routes](INSTALLATION.md#centralized-update-command).

```powershell
& "$env:LOCALAPPDATA\Programs\Klicky\klicky.exe" update --check
```

## Common commands

| Command | What it does |
|---|---|
| `update --check` | Explicitly check the latest stable release and display the installation channel. |
| `update` | Explain and confirm the installation-specific update handoff. `--yes` accepts the handoff, not installer terms or completion. |
| `list` | List available user and system/bundled sound packs. |
| `switch NAME` | Request a live pack change, or save it for the next start if stopped. |
| `volume LEVEL` | Set a finite value from `0.0` to `1.0`; zero mutes playback. |
| `status` | Show current configuration and responsive live state. |
| `start` | Run the engine in the foreground. Stop the managed instance first. |
| `stop` | Stop the current instance and wait for confirmed process exit. |
| `pause` | Silence playback for this session without stopping the process. |
| `resume` | Resume sounds without replaying paused events. |
| `service enable` | Register login startup and start now. |
| `service disable` | Disable owned login startup and stop now. |
| `service start` | Start this session without changing login startup. |
| `service stop` | Stop this session while keeping login startup. |
| `service status` | Inspect startup and running state. |
| `--help` / `--version` | Show available commands or the build version. |

```bash
klicky list
klicky switch eg-oreo
klicky volume 0.5
klicky status
```

A successful live pack/volume command confirms the change was prepared, saved and applied. An error or timeout means the change is not confirmed; inspect status and logs before retrying. Use controls rather than editing configuration while the app runs.

On Linux and macOS, the `volume` confirmation and the text `status` volume field print the chosen 0.0 to 1.0 value. A quiet setting such as `0.04` stays `0.04` instead of rounding to `0.0`, and the endpoints stay `0.0` and `1.0`. `status --json` continues to emit a numeric volume value.

Service commands manage systemd user units on Linux, LaunchAgents on macOS and a per-user desktop app on Windows. Store installations use a Windows startup task; standalone installations use an owned Startup shortcut. They do not install an elevated Windows service. Windows can independently disable or defer login startup.

## Audio-output controls

`klicky audio status` prints the running engine's confirmed audio state as JSON. It reports the requested policy, active endpoint when identifiable, output availability, manual pause and supported operations. It never substitutes saved settings for a live response. Existing `status`, `status --json` and Windows diagnostics retain their formats.

| Command | Behavior |
|---|---|
| `audio status` | Query audio state without opening a new stream or changing settings. |
| `audio follow-default` | Save system-default output policy, preserving pack, volume, pause and startup. Works offline for the next start too. |
| `audio retry` | Request recovery on Windows; check status afterward for actual availability. Does not resume a manually paused session. |
| `audio outputs` | Enumerate outputs when the platform adapter advertises `can_enumerate`. |
| `audio select --backend BACKEND --id ID` | Request a backend-scoped opaque device ID when `can_select` is true. |

The shared interface currently supports system-default playback. Explicit selection and enumeration are not enabled yet: both return an error without saving or changing the current route. Unix retry also returns an unsupported-operation error; stop/start the service to reopen its output. Helpers must inspect capabilities instead of assuming every platform offers every action.

Audio JSON schema version 1 contains `requested` (`mode: system_default` or `mode: device` with `backend` and `id`), `active`, `availability`, `manual_paused`, `can_select`, `can_enumerate` and `can_retry`. `requested` is null while preferences have not yet loaded. Availability is `initializing`, `available`, `unavailable`, `recovering` or `inactive`; it is independent of manual pause. `active` is null when no endpoint can be confirmed, including Unix backends that cannot identify their desktop-server route. An open Unix stream is not a measured end-to-end audio-health check. Windows reports its confirmed WASAPI endpoint ID; its optional display name is currently null.

IDs are opaque, backend-scoped and case-sensitive. Never select by enumeration index or display name: names can repeat. Unsupported, invalid or failed requests must not trigger a fallback to another device. Requests use the existing bounded control transport; there is no background enumeration or polling added by these commands. An older engine cannot serve the new audio commands and needs an upgrade.

## Linux/macOS helper controls

`status --json` emits schema version 1 with `running`, `process_exists`, `paused`, `sound_pack`, `volume`, `packs` and `autostart`. A responsive daemon supplies the live playback fields; when stopped, pack and volume come from saved preferences and `paused` is false. A live process that does not answer is reported as `running: false, process_exists: true`. Invalid or unsupported live responses fail rather than being replaced with saved preferences. IPC requests have a three-second deadline.

`service autostart enable` and `service autostart disable` change login startup without starting, stopping or resuming the current daemon. Existing `service enable` and `service disable` keep their start/stop behavior.

Pause is independent of volume and login startup, and resets on a daemon restart. It cancels active and queued sources. Resume discards input captured before the resume transition. Muting with `volume 0` keeps the daemon active.

## Windows-only controls

| Command | What it does |
|---|---|
| `retry-audio` | Reopen the default multimedia output. |
| `diagnostics` | Read aggregate input/playback counters. No typed text or microphone recording. |
| `service start --paused` | Launch a new background desktop instance paused. Does not pause an existing one. |

Windows foreground `start` also accepts:

| Flag | Meaning |
|---|---|
| `--paused` | Start without sounds. |
| `--seconds N` | Bound the session to 1–3600 seconds. |
| `--benchmark` | Print aggregate internal dispatch diagnostics at exit, without key names. |

See [Windows diagnostics and recovery](WINDOWS.md) for counter meanings and bundle ownership. Hidden installer-preparation hooks are implementation details, not everyday commands.

## Linux/macOS benchmark mode

```bash
klicky start --benchmark
```

This prints mapped key names and internal timing. Use only with controlled input; do not type passwords or private text. It does not measure physical keypress-to-audible latency. Windows-only `retry-audio`, `diagnostics`, `--paused` and `--seconds` are not Unix CLI features.

[Measurement guide](BENCHMARKS.md) · [Architecture](ARCHITECTURE.md)
