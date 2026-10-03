# Sound packs

Klicky uses the same single-file slicing model as KeyEcho sound packs. One OGG file stores every recorded key sound, and one JSON file maps platform-neutral key names to time ranges.

## Directory layout

```text
my-pack/
├── config.json
└── sound.ogg
```

Install a pack by copying its directory into the Klicky sound directory:

```bash
cp -R my-pack ~/.config/klicky/sounds/
klicky switch my-pack
```

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

Start Klicky in the foreground and switch to the pack:

```bash
klicky start
klicky switch my-pack
```

If the daemon is already managed by systemd:

```bash
klicky switch my-pack
journalctl --user -u klicky.service -f
```

A malformed JSON file or undecodable OGG stream produces a load error and leaves the current pack active.

## Redistribution

Only redistribute recordings you created or have permission to share. A JSON timing map does not change the copyright status of the underlying audio.
