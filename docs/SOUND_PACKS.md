# Sound packs

Klicky uses the same single-file slicing model as KeyEcho sound packs. One OGG file stores every recorded key sound, and one JSON file maps platform-neutral key names to time ranges.

Hear three included packs in the [45-second demo](https://klicky.muneer320.tech/#demo), or [listen to each pack](https://klicky.muneer320.tech/#sounds) before choosing one.

## Directory layout

```text
my-pack/
├── config.json
└── sound.ogg
```

Install a pack by copying its directory into your user sound directory, then select it. The steps below use a pack folder named `my-pack` in the current directory.

### Linux

Klicky uses `$XDG_CONFIG_HOME/klicky/sounds`, normally `~/.config/klicky/sounds`:

```bash
mkdir -p "${XDG_CONFIG_HOME:-$HOME/.config}/klicky/sounds"
cp -R my-pack "${XDG_CONFIG_HOME:-$HOME/.config}/klicky/sounds/"
klicky switch my-pack
```

### macOS

```bash
mkdir -p "$HOME/Library/Application Support/klicky/sounds"
cp -R my-pack "$HOME/Library/Application Support/klicky/sounds/"
klicky switch my-pack
```

### Windows (PowerShell)

Standalone installer, portable and Scoop copies use `%LOCALAPPDATA%\Klicky\sounds`. The standalone installer puts the CLI at `%LOCALAPPDATA%\Programs\Klicky\klicky.exe`, which works even when `klicky` is not on `PATH`. Scoop exposes `klicky` in a new terminal, and a portable copy runs `.\klicky.exe` from its own folder.

```powershell
$sounds = Join-Path $env:LOCALAPPDATA 'Klicky\sounds'
New-Item -ItemType Directory -Force -Path $sounds | Out-Null
Copy-Item -Recurse -LiteralPath '.\my-pack' -Destination $sounds
& (Join-Path $env:LOCALAPPDATA 'Programs\Klicky\klicky.exe') switch my-pack
```

A new Microsoft Store installation keeps custom packs in its [package-private sounds folder](WINDOWS.md#files-and-custom-sounds) instead, and exposes `klicky` in a new terminal:

```powershell
$sounds = Join-Path $env:LOCALAPPDATA 'Packages\MuneerAlam.Klicky_zj8ab2whxa2g0\LocalCache\Local\Klicky\sounds'
New-Item -ItemType Directory -Force -Path $sounds | Out-Null
Copy-Item -Recurse -LiteralPath '.\my-pack' -Destination $sounds
klicky switch my-pack
```

Store Reset and uninstall delete this folder, so back up your custom packs first.

### Remove a custom pack

Switch to another pack first, then delete only your pack's folder. `klicky list` shows the available names. Never delete bundled or package-managed packs.

```bash
klicky switch eg-oreo
rm -r -- "${XDG_CONFIG_HOME:-$HOME/.config}/klicky/sounds/my-pack"   # Linux
rm -r -- "$HOME/Library/Application Support/klicky/sounds/my-pack"  # macOS
```

```powershell
& (Join-Path $env:LOCALAPPDATA 'Programs\Klicky\klicky.exe') switch eg-oreo
Remove-Item -Recurse -LiteralPath (Join-Path $env:LOCALAPPDATA 'Klicky\sounds\my-pack')
```

For a Store installation, use `klicky switch eg-oreo` and the package-private path above. To update a pack, remove the old folder the same way before copying the new one; copying onto an existing folder nests the new copy inside it.

Linux distribution packages can install shared packs under `/usr/share/klicky/sounds`. User packs take precedence over system packs with the same directory name. This allows local customization without modifying package-managed files.

On macOS, user packs go in `~/Library/Application Support/klicky/sounds`. Windows standalone/Scoop copies use `%LOCALAPPDATA%\Klicky\sounds`; new Microsoft Store data uses the [package-private sounds folder](WINDOWS.md#files-and-custom-sounds). Back up Store custom packs before Reset or uninstall. Bundled packs stay in `sounds` beside the executable. Windows user packs override bundled names using Windows ordinal comparison. Windows validates pack names, bounds discovery/preparation and rejects malformed or incomplete OGG containers; see the [Windows guide](WINDOWS.md). Use the tray or CLI for live changes instead of editing active configuration.

## Configuration format

```json
{
  "defines": {
    "KeyA": [0, 120],
    "KeyB": [120, 115],
    "Space": [235, 180],
    "Return": [415, 150]
  },
  "name": "My Pack",
  "key_define_type": "single"
}
```

Klicky currently reads `defines`. Additional fields remain compatible metadata.

Each value contains:

```text
[start time in milliseconds, duration in milliseconds]
```

Ranges outside the decoded audio length are safely truncated or skipped.

## Supported key names

### Letters and digits

```text
KeyA through KeyZ
Num0 through Num9
```

### Function row

```text
F1 through F12
Escape
Function
```

### Editing and navigation

```text
Backspace Return Tab Space Delete Insert
Home End PageUp PageDown
UpArrow DownArrow LeftArrow RightArrow
```

### Modifiers

```text
ShiftLeft ShiftRight
ControlLeft ControlRight
Alt AltGr
MetaLeft MetaRight
CapsLock
```

### Punctuation

```text
Comma Dot Slash SemiColon Quote
LeftBracket RightBracket BackSlash BackQuote
Minus Equal
```

### Numpad

```text
Kp0 through Kp9
KpPlus KpMinus KpMultiply KpDivide
KpReturn KpDelete
NumLock
```

### Other mapped keys

```text
PrintScreen ScrollLock Pause
```

Linux media keys map to physical function-row names where a common laptop layout exists. For example, volume up maps to `F3` in the current Linux backend.

## Loudness normalization

Recordings from different packs vary widely in level. Klicky processes each key slice once when a pack loads:

1. Find the absolute sample peak.
2. Leave already-loud slices unchanged.
3. Raise quiet slices toward 85 percent full scale.
4. Cap gain at 6x.
5. Apply the user's `volume` setting during playback.

The cap protects nearly silent recordings from turning background noise into a loud transient. It also preserves a clean hot path because normalization never runs during keypress playback.

## Authoring guidance

- Use one or two channels consistently across the complete OGG file.
- Keep all slices at the same sample rate.
- Leave a small boundary between recordings so adjacent ranges do not overlap.
- Keep durations short. Mechanical switch samples usually need 80 to 250 ms.
- Avoid clipping during recording.
- Include all common keys or document fallback gaps.
- Test fast typing, modifiers, Space, Enter, and Backspace separately.

## Validate a pack

Stop any managed instance first, then start Klicky in the foreground:

```bash
klicky start
```

In a second terminal, request the switch:

```bash
klicky switch my-pack
```

If the daemon is already managed by systemd:

```bash
klicky switch my-pack
journalctl --user -u klicky.service -f
```

A malformed JSON file or undecodable OGG stream produces a load error and leaves the current pack active. A successful CLI response confirms the pack was loaded, saved and activated. If a response fails or times out, inspect status and logs before retrying. On macOS, custom packs live under `~/Library/Application Support/klicky/sounds/`.

## Redistribution

Only redistribute recordings you created or have permission to share. A JSON timing map does not change the copyright status of the underlying audio.
