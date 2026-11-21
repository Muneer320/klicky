# Configuration reference

[Field guide](https://klicky.muneer320.tech/guide.html#configuration) · [CLI](CLI.md) · [Custom sounds](SOUND_PACKS.md)

For everyday changes, use the Windows tray, macOS menu helper or CLI. The macOS menu offers pause/resume, volume (including mute), sound-pack selection and refresh, service start/stop and login startup. Edit files only while Klicky is stopped.

## Settings

Existing configurations need no changes. The everyday defaults are:

```toml
sound_pack = "eg-oreo"
volume = 0.8
```

| Field | Value | Meaning |
|---|---|---|
| `sound_pack` | Pack directory name | Selects the OGG/JSON pack; default `eg-oreo`. |
| `volume` | Finite number, `0.0`–`1.0` | Playback level; zero mutes. |

Pause is a session control and is not persisted in configuration. Startup is managed through the operating system's service/desktop controls.

## Audio-output preference

An optional `[audio_output]` table stores the routing policy. Omitting it means system default and keeps existing configuration files compatible:

```toml
[audio_output]
mode = "system_default"
```

In source builds after v0.7.0, Windows supports `mode = "device"` with `backend = "wasapi"`; Linux supports `backend = "pulse_audio"` with either PulseAudio or PipeWire's PulseAudio server. The opaque `id` comes from `klicky audio outputs`. Use the CLI to select it rather than guessing or editing a running engine's configuration. Other schema backends (`core_audio`, `pipe_wire`, `alsa`) are reserved and rejected by these adapters. macOS still requires system default.

A missing remembered output is retained and retried without playing through a different device. A new unavailable selection is rejected without changing the saved preference. `klicky audio follow-default` restores system routing while retaining pack and volume. Driver reinstallations, re-pairing or renamed Linux sinks can change IDs, requiring a new selection. Linux's ALSA-only fallback supports system default, not remembered device selection. See the [CLI routing guide](CLI.md#audio-output-controls) for the fallback boundaries.

Backend IDs are case-sensitive values, not device names or indexes. Pack/volume saves retain the output preference; an invalid preference fails validation instead of being silently erased. [Audio commands and structured status](CLI.md#audio-output-controls) describe capabilities and recovery.

## File locations

| System | User data root | Secondary sound root |
|---|---|---|
| Linux | `~/.config/klicky`, or `$XDG_CONFIG_HOME/klicky` | `/usr/share/klicky/sounds` |
| macOS | `~/Library/Application Support/klicky` | None by default; the installer puts packs in the user root |
| Windows standalone / Scoop | `%LOCALAPPDATA%\Klicky` | `sounds` beside the executable |
| Windows Microsoft Store | [Package-private app data](WINDOWS.md#files-and-custom-sounds) for new settings | `sounds` inside the package |

Settings live at `ROOT/config.toml`; custom packs go in `ROOT/sounds/PACK_NAME`. Windows bundled packs resolve beside the executable, independently of your working directory.

User packs take precedence over secondary packs with the same name. On Windows, names use Windows ordinal comparison. Keep an entire custom pack together; it needs both `sound.ogg` and `config.json`.

## Environment options

| Variable | Scope | Meaning |
|---|---|---|
| `KLICKY_SYSTEM_SOUNDS_DIR` | Pack discovery | Override the secondary sound root. Windows requires an absolute path. Does not replace the primary user root. |
| `XDG_CONFIG_HOME` | Linux configuration lookup | Choose an alternative user config base. |
| `XDG_RUNTIME_DIR` | Unix runtime lookup | Location for private runtime ownership/IPC files when available. |
| `KLICKY_SKIP_UDEV=1` | Linux source installer | Skip the supplied device rule when input access is managed elsewhere. |

These are environment variables, not extra `config.toml` fields. Avoid manually changing runtime PID, socket, pipe or ownership files while an instance is running.

## Saving changes and keeping your data

When stopped, pack/volume commands save for the next start. During playback, a successful command confirms loading, saving and applying the change. If a response fails or times out, check status/logs before retrying.

The EXE/ZIP, Linux and macOS installation methods keep personal configuration and custom sounds during normal upgrades and removal. `--purge` / `-Purge` explicitly removes them. The Windows uninstaller refuses linked/junction data trees rather than following them into unrelated locations.

Microsoft Store packages use Windows-managed app data for new settings. Repair preserves that data; Reset and removal clear it. Files retained from an earlier EXE installation can remain in `%LOCALAPPDATA%\Klicky`. See the [Store data lifecycle](WINDOWS_STORE.md#runtime-integration) before clearing app data or removing a Store package.

[Install, update and remove](INSTALLATION.md) · [Troubleshooting](TROUBLESHOOTING.md)
