# klicky

Make your Mac keyboard sound like a mechanical keyboard. System-wide, low-latency, lightweight.

**0.33ms average latency** — every keypress under 1ms. 3.5MB binary. Zero runtime dependencies.

https://github.com/user-attachments/assets/klicky-demo.mp4

## How it works

klicky runs as a background daemon that:
1. Listens for keypresses via macOS CGEventTap (system-wide, works in any app)
2. Plays pre-decoded PCM audio for each key — sounds are decoded once at startup, not on every keypress
3. Accepts runtime commands via Unix socket (switch sounds, adjust volume, stop)

## Install

### From source (requires Rust)

```bash
git clone https://github.com/Sauhard74/klicky.git
cd klicky
cargo build --release
cp target/release/klicky /usr/local/bin/  # or ~/.local/bin/
```

### Sound packs

klicky needs sound packs to work. Download them from the [releases page](https://github.com/Sauhard74/klicky/releases) or extract from the repo:

```bash
# Create sounds directory
mkdir -p "$(klicky status 2>/dev/null | grep -q 'Library' && echo ~/Library/Application\ Support/klicky/sounds || echo ~/.config/klicky/sounds)"

# On macOS, the sounds directory is:
# ~/Library/Application Support/klicky/sounds/
```

Each sound pack is a directory containing `sound.ogg` + `config.json`:

```
~Library/Application Support/klicky/sounds/
├── cherrymx-black-pbt/
│   ├── sound.ogg
│   └── config.json
├── cherrymx-blue-abs/
│   ├── sound.ogg
│   └── config.json
└── eg-oreo/
    ├── sound.ogg
    └── config.json
```

## Usage

```bash
klicky start                      # start the daemon (foreground)
klicky start &                    # start in background
klicky stop                       # stop the daemon
klicky list                       # list available sound packs
klicky switch cherrymx-blue-pbt   # switch to a different sound
klicky volume 0.5                 # set volume (0.0 - 1.0)
klicky status                     # show current state
klicky start --benchmark          # start with latency measurements
```

### Auto-start on login

Add this to your `~/.zshrc` or `~/.bashrc`:

```bash
# klicky - mechanical keyboard sounds
(pgrep -f "klicky start" > /dev/null 2>&1 || ~/.local/bin/klicky start &) > /dev/null 2>&1
```

### First run — Accessibility permission

klicky needs Accessibility permission to listen for keypresses:

1. Open **System Settings → Privacy & Security → Accessibility**
2. Click the **+** button
3. Add your terminal app (Terminal, iTerm2, Warp, etc.) or the `klicky` binary
4. Toggle it **on**

## Sound packs

### Included packs

| Pack | Type | Description |
|------|------|-------------|
| `cherrymx-black-pbt` | Linear | Deep, smooth thock with PBT keycaps |
| `cherrymx-black-abs` | Linear | Slightly brighter with ABS keycaps |
| `cherrymx-blue-pbt` | Clicky | Classic loud click with PBT |
| `cherrymx-blue-abs` | Clicky | Bright click with ABS |
| `cherrymx-brown-pbt` | Tactile | Subtle bump, no click, PBT |
| `cherrymx-brown-abs` | Tactile | Subtle bump with ABS |
| `cherrymx-red-pbt` | Linear | Light and smooth, PBT |
| `cherrymx-red-abs` | Linear | Light and poppy, ABS |
| `eg-oreo` | Linear | Everglide Oreo — creamy thock |
| `eg-crystal-purple` | Tactile | Everglide Crystal Purple — crisp |
| `nk-cream` | Linear | NovelKeys Cream — deep buttery thock |

### Custom sound packs

Create your own sound pack:

1. Record or obtain a single audio file with all key sounds
2. Create a `config.json` mapping each key to `[start_ms, duration_ms]` in the audio file:

```json
{
  "defines": {
    "KeyA": [0, 150],
    "KeyB": [150, 120],
    "Space": [270, 180]
  },
  "name": "My Custom Pack",
  "key_define_type": "single"
}
```

3. Save as `sound.ogg` + `config.json` in a new directory under the sounds folder

Supported key names: `KeyA`-`KeyZ`, `Num0`-`Num9`, `F1`-`F12`, `Space`, `Return`, `Backspace`, `Tab`, `Escape`, `ShiftLeft`, `ShiftRight`, `ControlLeft`, `ControlRight`, `Alt`, `AltGr`, `MetaLeft`, `MetaRight`, `CapsLock`, `Comma`, `Dot`, `Slash`, `SemiColon`, `Quote`, `LeftBracket`, `RightBracket`, `BackSlash`, `BackQuote`, `Minus`, `Equal`, `UpArrow`, `DownArrow`, `LeftArrow`, `RightArrow`, `Delete`, `Home`, `End`, `PageUp`, `PageDown`, `Insert`, `Function`, and numpad keys (`Kp0`-`Kp9`, `KpPlus`, `KpMinus`, `KpMultiply`, `KpDivide`, `KpReturn`, `KpDelete`).

## Latency

klicky is built for speed. Benchmark results on Apple M-series:

```
=== Latency (219 keypresses) ===
  avg:  0.33ms
  min:  0.02ms
  max:  0.69ms
```

This measures time from CGEventTap callback to audio dispatch. Add ~5-12ms for CoreAudio buffer output, giving **~6-13ms total perceived latency** — imperceptible to humans.

### Why so fast?

| Optimization | Impact |
|---|---|
| Pre-decode all OGG → raw PCM at startup | 0ms runtime decode (vs ~10-20ms for on-the-fly) |
| Persistent audio OutputStream | No per-keypress device setup |
| Arc\<Vec\<i16\>\> sample sharing | Zero-copy playback from cache |
| 500μs event loop | Sub-millisecond response |

## Tech stack

- **Rust** — safe, fast, no GC pauses
- **rdev** — cross-platform global key listener (CGEventTap on macOS)
- **rodio** — audio playback
- **symphonia** — OGG Vorbis decoding
- **clap** — CLI framework
- **Unix domain sockets** — IPC between CLI and daemon

## Inspired by

- [KeyEcho](https://github.com/ZacharyL2/KeyEcho) — Tauri-based mechanical keyboard sounds (sound pack format compatible)
- [keyb](https://keyb.vercel.app) — Beautiful web-based mechanical keyboard
- [Klack](https://tryklack.com/) — Native macOS mechanical keyboard app
- [rustyvibes](https://github.com/KunalBagaria/rustyvibes) — Rust CLI for keyboard sounds

## License

MIT
