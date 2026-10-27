# Klicky

> Mechanical keyboard sounds for Linux and macOS, played from prepared audio the moment a key event arrives.

<p align="center">
  <a href="https://github.com/Muneer320/klicky/releases"><img src="https://img.shields.io/badge/Linux_release-v0.4.0-2ea44f" alt="Linux release"></a>
  <a href="https://www.rust-lang.org/"><img src="https://img.shields.io/badge/Rust-1.87%2B-dea584" alt="Rust"></a>
  <a href="https://github.com/Muneer320/klicky/blob/master/LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue" alt="MIT license"></a>
</p>

---

## Project Name

**Klicky**

Klicky is a lightweight desktop application that turns ordinary typing into customizable mechanical keyboard sounds.

## Problem Statement

Many users enjoy the auditory feedback of mechanical keyboards but use laptop, membrane, or other keyboards that do not provide it. Existing software approaches can feel disconnected from typing when audio decoding or playback setup happens on every keypress.

Klicky addresses this with a persistent, low-latency background daemon that captures keyboard events and plays prepared sound samples immediately.

## Target Users

Klicky is built for:

- Laptop and desktop users who want mechanical keyboard sound feedback without replacing their keyboard
- Developers, students, writers, and other heavy typists
- Users who want customizable typing sounds
- Linux users who prefer a native, local application

## Solution

Klicky listens for global keyboard events through native platform input mechanisms and maps those events to the currently selected sound pack.

Audio is prepared during startup and reused during operation instead of repeatedly decoding and initializing playback for every keypress. A persistent audio mixer handles playback while a local control interface allows the user to change settings at runtime.

On Linux, Klicky can run as a systemd user service. Omarchy users can optionally control the service from the desktop status bar.

## Key Features

- **System-wide keyboard sounds** across applications
- **10 bundled sound packs** with different mechanical keyboard recordings
- **Runtime sound-pack switching**
- **Live volume control**
- **Background daemon operation**
- **systemd user-service integration on Linux**
- **Optional Omarchy status-bar integration**
- **Local Unix-socket control interface**
- **Low-latency prepared audio playback**
- **Local-first operation with no normal network requests**

Example controls:

```bash
klicky list
klicky switch <sound-pack>
klicky volume 0.5
klicky status
klicky service status
klicky service start
klicky service stop
```

## Tech Stack

| Technology | Purpose |
|---|---|
| Rust | Core application, daemon and CLI |
| evdev | Linux keyboard event handling |
| rdev / native input APIs | macOS global keyboard input |
| Rodio | Audio playback and persistent mixer |
| Symphonia | Audio decoding |
| Serde | Serialization and configuration |
| TOML | User configuration |
| systemd | Linux background service |
| Quickshell / QML | Optional Omarchy status-bar integration |
| GitHub Actions | CI, testing and release builds |
| Arch Linux / Debian packages | Linux distribution |

## Architecture

Klicky uses an event-driven architecture centered around one user-session daemon.

```text
                    ┌──────────────────┐
                    │     Keyboard     │
                    └────────┬─────────┘
                             │
                 ┌───────────┴───────────┐
                 ▼                       ▼
        ┌─────────────────┐     ┌─────────────────┐
        │   Linux evdev   │     │ macOS native    │
        │  Wayland / X11  │     │ global input    │
        └────────┬────────┘     └────────┬────────┘
                 └───────────┬───────────┘
                             ▼
                  ┌─────────────────────┐
                  │   Event Mapping     │
                  └──────────┬──────────┘
                             ▼
                  ┌─────────────────────┐
                  │      Daemon         │
                  │  runtime control    │
                  └───────┬─────┬───────┘
                          │     ▲
                          │     │
                 ┌────────▼─┐ ┌─┴─────────┐
                 │ Prepared │ │ CLI / IPC │
                 │   PCM    │ └───────────┘
                 └────┬─────┘
                      ▼
             ┌──────────────────┐
             │ Persistent Rodio │
             │      Mixer       │
             └────────┬─────────┘
                      ▼
             ┌──────────────────┐
             │  Audio Output    │
             └──────────────────┘
```

The keyboard path and control path meet in the same user-session daemon. Audio samples are prepared ahead of time and dispatched through a persistent mixer, keeping the per-keypress path lightweight.

## Local Setup

### Requirements

- Linux or macOS
- Rust 1.87+
- An audio output device
- A supported keyboard input environment

### Clone

```bash
git clone https://github.com/Muneer320/klicky.git
cd klicky
```

### Build from source

```bash
cargo build --release
```

### Linux / Omarchy

The published Linux v0.4.0 release provides an Arch Linux package:

```bash
sudo pacman -U ./klicky-0.4.0-1-x86_64.pkg.tar.zst
systemctl --user enable --now klicky.service
```

Check the running application:

```bash
klicky status
```

### macOS

Klicky can be built and installed from the current source using the macOS installation workflow documented in the repository.

## Environment Variables

Klicky does not require API keys or external service credentials for normal operation.

Optional environment overrides:

| Variable | Purpose |
|---|---|
| `KLICKY_SYSTEM_SOUNDS_DIR` | Use a custom additional system sound-pack directory |
| `KLICKY_SKIP_UDEV=1` | Skip udev-rule installation when using the Linux source installer |

User configuration is stored locally under:

```text
~/.config/klicky/
```

on Linux by default.

## Deployment

Klicky is distributed as a native desktop application rather than a browser-only application.

### Website

https://muneer320.github.io/klicky/

### Source Repository

https://github.com/Muneer320/klicky

### Releases

https://github.com/Muneer320/klicky/releases

The v0.4.0 release includes packages for Arch Linux x86_64 and Debian/Ubuntu amd64.

For desktop software, the repository provides packaged builds and source installation instructions so an evaluator can run the application without reverse-engineering the project.

## Demo

**Demo Video:** [G-Drive Link](https://drive.google.com/file/d/1hEttpMaugoL3aleS5EE4ItJ8kwWFxOAD/view?usp=sharing)

## Security

Klicky observes global key-down events because that is required to produce system-wide typing sounds. The Linux installer uses a keyboard-only `uaccess` rule to grant the active desktop user access to the required device, and the daemon runs as the logged-in user rather than root.

Normal operation does not store typed text, log key names, or make network requests. Benchmark mode can print mapped key names and should not be used while entering sensitive information.

## License

MIT License.

See [LICENSE](LICENSE) for the project license and [Sound Lincese](sounds/LICENSE.md) for the bundled sound-asset notice.
