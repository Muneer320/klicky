# Configuration reference

[Field guide](https://klicky.muneer320.tech/guide.html#configuration) · [CLI](CLI.md) · [Custom sounds](SOUND_PACKS.md)

For everyday changes, use the Windows tray or CLI. On macOS the menu helper toggles sounds; pack/volume changes use the CLI. Edit files only while Klicky is stopped.

## Settings

`config.toml` has two fields. Defaults:

```toml
sound_pack = "eg-oreo"
volume = 0.8
```

| Field | Value | Meaning |
|---|---|---|
| `sound_pack` | Pack directory name | Selects the OGG/JSON pack; default `eg-oreo`. |
| `volume` | Finite number, `0.0`–`1.0` | Playback level; zero mutes. |

Pause is a session control, not a third configuration field. Startup is managed through the operating system's service/desktop controls.

## File locations

| System | User data root | Secondary sound root |
|---|---|---|
| Linux | `~/.config/klicky`, or `$XDG_CONFIG_HOME/klicky` | `/usr/share/klicky/sounds` |
| macOS | `~/Library/Application Support/klicky` | None by default; the installer puts packs in the user root |
| Windows | `%LOCALAPPDATA%\Klicky` | `sounds` beside the executable |

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

Normal upgrades and removal keep personal configuration and custom sounds. `--purge` / `-Purge` explicitly removes them. The Windows uninstaller refuses linked/junction data trees rather than following them into unrelated locations.

[Install, update and remove](INSTALLATION.md) · [Troubleshooting](TROUBLESHOOTING.md)
