# Acknowledgements

I started Klicky after seeing [Sauhard Gupta's original macOS project](https://github.com/Sauhard74/klicky) running on a Mac and wanting it on my Omarchy machine.

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

I expanded it to Linux and Windows, with help from contributors. The main additions include:

- Direct Linux `evdev` input
- Wayland and X11 independence
- Linux media-key mapping
- Platform-scoped dependencies
- A persistent Rodio 0.22 mixer
- Configurable low-latency buffer fallbacks
- Capped per-key loudness normalization
- Linux udev and systemd integration
- Native Windows input, audio and tray controls
- macOS menu controls and Omarchy Klicky Bar
- Cross-platform installers, packages and CI
- Hardware-backed Linux benchmarks and documentation

## Related projects

- [KeyEcho](https://github.com/ZacharyL2/KeyEcho), sound-pack format compatibility and keyboard-sound prior art
- [rustyvibes](https://github.com/KunalBagaria/rustyvibes), Rust keyboard-sound tooling
- [Klack](https://tryklack.com/), native macOS keyboard sounds
- [keyb](https://keyb.vercel.app/), browser-based keyboard sound exploration

## Sound recordings

The bundled recordings and timing maps are distributed under the repository MIT License. See [`sounds/LICENSE.md`](sounds/LICENSE.md).

Contributors adding or replacing sounds must provide a clear source and redistribution permission.

## Visual identity and typography

The Klicky mascot identity and selected cozy keyboard artwork establish the project's visual direction. The [asset library](assets/branding) contains the final illustrations and application marks.

The website and outlined wordmarks use Nunito by The Nunito Project Authors. The typeface and web subset retain the [SIL Open Font License](assets/website/fonts/OFL.txt), separately from Klicky's MIT-licensed application code.

The Microsoft Store badge is Microsoft's unmodified artwork, used to link to Klicky's Store listing. The local copy comes from [Microsoft's official badge image](https://get.microsoft.com/images/en-us%20dark.svg), following the [HTML-only badge guidance](https://github.com/microsoft/app-store-badge#html-only). The Microsoft mark is separate from Klicky's own artwork.

The [sound demo](https://klicky.muneer320.tech/#demo) uses Linux, Apple and Windows icons from Font Awesome Free 6.7.2 by Fonticons, Inc. under CC BY 4.0. See the [demo credits and icon license](assets/website/demo/CREDITS.md) for attribution and reuse.
