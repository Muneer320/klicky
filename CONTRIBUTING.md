# Contributing

Thank you for helping improve Klicky. Small, focused changes are easier to test across input stacks and operating systems than broad rewrites.

## Before starting

Open an issue before changing any of these areas:

- Public CLI commands or configuration keys
- Sound-pack format
- Input permissions
- Audio engine or buffer policy
- Platform support claims
- Production dependencies

Straightforward bug fixes, documentation corrections, and narrowly scoped tests can go directly to a pull request.

## Development setup

Install Rust 1.87 or newer and the native audio development package for your operating system.

Arch Linux:

```bash
sudo pacman -S --needed rust alsa-lib pkgconf
```

Ubuntu or Debian:

```bash
sudo apt install cargo rustc pkg-config libasound2-dev
```

Fedora:

```bash
sudo dnf install cargo rust alsa-lib-devel pkgconf-pkg-config
```

Clone and validate:

```bash
git clone https://github.com/Muneer320/klicky.git
cd klicky
cargo fmt --check
cargo check --all-targets
cargo test
cargo clippy --all-targets --all-features -- -D warnings
```

## Repository map

| Path | Purpose |
|---|---|
| `src/listener/` | Platform input backends and key mapping |
| `src/player.rs` | Persistent low-latency audio output |
| `src/soundpack.rs` | Decode, slice, normalize, and cache sounds |
| `src/ipc.rs` | Unix socket command transport |
| `packaging/` | Reusable systemd, udev, and Omarchy assets |
| `scripts/` | Installation and removal helpers |
| `sounds/` | Bundled sound packs |
| `docs/` | Architecture and platform documentation |

## Engineering guidelines

- Understand the current path before replacing it.
- Prefer the smallest correct design.
- Keep platform details behind the listener boundary.
- Keep decoding, filesystem access, and device creation out of the keypress path.
- Add comments only for non-obvious constraints or platform quirks.
- Do not add abstractions for hypothetical platforms.
- Do not add a dependency when a small standard-library solution is clearer.
- Preserve the original project attribution and license.

## Tests

Write tests for behavior that can regress:

- Key mappings
- Key-down and repeat filtering
- Sound-pack parsing
- Normalization boundaries
- Configuration behavior
- IPC serialization
- Bugs with a deterministic reproduction

Do not add tests for trivial delegation or compiler-enforced properties. Hardware behavior needs a real device test and a written environment description.

Run before every pull request:

```bash
cargo fmt --check
cargo check --all-targets
cargo test
cargo clippy --all-targets --all-features -- -D warnings
cargo build --release
```

## Commits

- Use a feature branch.
- Keep each commit to one logical change.
- Use short, single-line commit subjects.
- Conventional prefixes such as `feat:`, `fix:`, `docs:`, and `perf:` are preferred.
- Do not mix unrelated formatting with behavior changes.
- Do not include generated build output or local configuration.

## Pull requests

A good pull request includes:

- The problem and why it matters
- The chosen approach
- Alternatives considered when the change is architectural
- Exact verification commands and results
- Hardware and desktop details for input or audio changes
- Documentation updates for user-visible behavior

CI must pass on Linux and macOS. Compilation on a platform does not count as runtime verification on that platform.

## Sound contributions

Only submit recordings you created or have permission to redistribute. Include the source, creator, and license or permission statement in the pull request.

## Style

Public documentation uses direct technical language, minimal emoji, and no em dashes. Prefer short paragraphs and concrete claims backed by code or measurements.
