# CLI reference

[Field guide](https://klicky.muneer320.tech/guide.html#cli) · [Configuration](CONFIGURATION.md) · [Troubleshooting](TROUBLESHOOTING.md)

Use `klicky --help` or `klicky help COMMAND` to inspect the build you installed. Pick the command you need; you do not need to run every example.

## Find the executable

- Linux package: `/usr/bin/klicky`.
- Linux/macOS source installer: `~/.local/bin/klicky`; add `~/.local/bin` to your shell PATH if desired.
- Windows installer: `%LOCALAPPDATA%\Programs\Klicky\klicky.exe`. It is not registered globally on PATH.
- Windows portable: `klicky.exe` inside the extracted bundle, alongside `klicky-desktop.exe` and `sounds`.

PowerShell example:

```powershell
& "$env:LOCALAPPDATA\Programs\Klicky\klicky.exe" status
```

The examples below use `klicky`. In Windows, substitute the installed invocation above or `.\klicky.exe` from the bundle folder.

## Common commands

| Command | What it does |
|---|---|
| `list` | List available user and system/bundled sound packs. |
| `switch NAME` | Request a live pack change, or save it for the next start if stopped. |
| `volume LEVEL` | Set a finite value from `0.0` to `1.0`; zero mutes playback. |
| `status` | Show current configuration and responsive live state. |
| `start` | Run the engine in the foreground. Stop the managed instance first. |
| `stop` | Stop the current instance and wait for confirmed process exit. |
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

Service commands manage systemd user units on Linux, LaunchAgents on macOS and a per-user desktop app/Startup shortcut on Windows. They do not install an elevated Windows service. Windows can independently disable or defer login startup.

## Windows-only controls

| Command | What it does |
|---|---|
| `pause` | Pause sounds for the session. |
| `resume` | Resume when the desktop and audio are available. |
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

This prints mapped key names and internal timing. Use only with controlled input; do not type passwords or private text. It does not measure physical keypress-to-audible latency. Windows-only `pause`, `resume`, `retry-audio`, `diagnostics`, `--paused` and `--seconds` are not Unix CLI features.

[Measurement guide](BENCHMARKS.md) · [Architecture](ARCHITECTURE.md)
