# Changelog

All notable changes to Klicky are documented here.

The project follows [Semantic Versioning](https://semver.org/).

## Unreleased

### Added

- Signed amd64 APT repository publication through GitHub Pages
- Deb822 source configuration with a repository-specific `Signed-By` key
- End-to-end APT index, signature, and package-discovery validation

### Fixed

- Daemon stop waits up to three seconds for confirmed process exit and reports shutdown failures instead of treating IPC delivery as success
- A persistent ownership lock serializes startup and PID/socket cleanup; only the owning daemon removes runtime state, including after initialization failures
- Service start and enable verify daemon responsiveness after the service manager returns; status reports event-loop responsiveness separately from PID-based running state
- Regression coverage for rapid stop/start, stale or missing runtime state, stalled shutdown, invalid acknowledgments, and service readiness

## 0.3.0 - 2026-10-03

### Added

- User-overridable system sound packs under `/usr/share/klicky/sounds`
- Explicit MIT ownership notice for bundled sound assets
- Arch Linux `PKGBUILD` and generated `.SRCINFO`
- Debian package metadata, maintainer scripts, changelog, and manual page
- Clean-environment Arch and Debian package CI
- Tag-driven package release assets and SHA-256 checksums

### Changed

- Completed Cargo metadata for native distribution packages
- Distribution services run the package-managed `/usr/bin/klicky` binary

## 0.2.0 - 2026-10-03

### Added

- Native Linux input through `evdev`
- Wayland and X11 support through one display-server-independent backend
- Linux modifier, navigation, numpad, and media-key mapping
- Keyboard-only udev access rule
- Default-target systemd user service
- Optional Omarchy center-bar toggle
- Linux and macOS CI
- Focused tests for key mapping, key-down filtering, ignored buttons, and audio normalization
- Architecture, benchmark, sound-pack, security, and contribution documentation

### Changed

- Upgraded playback from Rodio 0.19 to Rodio 0.22
- Replaced per-key detached sinks with one persistent mixer
- Requested 256-frame output with 512, 1024, and device-selected fallbacks
- Normalized quiet per-key samples at pack load time with a 6x gain ceiling
- Scoped macOS-only dependencies to macOS builds
- Made CLI copy platform-neutral

### Fixed

- Runtime PID and socket files now live in a private runtime directory with owner-only permissions
- Input-reader failure now terminates the daemon so systemd can restart and rediscover devices
- Linux builds no longer attempt to link Apple frameworks
- Quiet sound packs now remain audible without globally overdriving loud packs
- Linux audio buffering no longer defaults to the previously observed 25 ms control delay
- Fallible IPC line reads no longer use an iterator pattern that could repeat errors forever

## 0.1.0

Initial upstream implementation by Sauhard Gupta:

- macOS global keyboard listener
- Mechanical keyboard sound playback
- Predecoded per-key sample cache
- Runtime switching and volume control
- Unix socket IPC
- macOS function and media-key support
- Included sound packs
