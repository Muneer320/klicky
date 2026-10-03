# Acknowledgements

Klicky began as a Linux-focused evolution of [Sauhard74/klicky](https://github.com/Sauhard74/klicky), created by Sauhard Gupta.

The original project established the core ideas that remain central here:

- A small Rust daemon
- Global keyboard capture
- OGG sound packs decoded once at startup
- Per-key PCM caching
- Unix socket commands
- A KeyEcho-compatible sound-pack format
- macOS media-key handling

This repository preserves the original Git history and MIT license. The original copyright notice remains in `LICENSE`.

## Linux and cross-platform work

The current repository adds and maintains:

- Direct Linux `evdev` input
- Wayland and X11 independence
- Linux media-key mapping
- Platform-scoped dependencies
- A persistent Rodio 0.22 mixer
- Configurable low-latency buffer fallbacks
- Capped per-key loudness normalization
- Linux udev and systemd integration
- Omarchy Quickshell integration
- Linux and macOS CI
- Hardware-backed Linux benchmarks and documentation

## Related projects

- [KeyEcho](https://github.com/ZacharyL2/KeyEcho), sound-pack format compatibility and keyboard-sound prior art
- [rustyvibes](https://github.com/KunalBagaria/rustyvibes), Rust keyboard-sound tooling
- [Klack](https://tryklack.com/), native macOS keyboard sounds
- [keyb](https://keyb.vercel.app/), browser-based keyboard sound exploration

## Sound recordings

The bundled sound packs were inherited from the original repository. The repository-level MIT license is preserved, but individual recording provenance is not separately documented upstream. Contributors adding or replacing sounds must provide a clear source and redistribution permission.
