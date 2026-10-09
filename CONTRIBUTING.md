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

Use the README's build prerequisites for [Linux](README.md#build-from-source-on-linux) or [macOS](README.md#build-and-install-on-macos). Install Rust 1.87 or newer; distribution-provided Rust packages may be older. Building and running the automated tests does not require installing a login service or granting keyboard access.

Clone the repository, then run the [checks below](#tests):

```bash
git clone https://github.com/Muneer320/klicky.git
cd klicky
```

## Repository map

| Path | Purpose |
|---|---|
| `src/` | See the complete [architecture module map](docs/ARCHITECTURE.md#module-map) |
| `tests/` | CLI lifecycle and status regression tests |
| `packaging/` | Distribution metadata, service units, desktop helpers, and Pages template |
| `scripts/` | Installation, removal, APT publication, and validation tools |
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
PYTHONUTF8=1 python3 -B scripts/check-docs.py
cargo build --release
```

For APT changes, run `python3 -B -m unittest scripts/test_apt_repository.py` and, on Linux with GnuPG and reprepro, `python3 -B scripts/test_apt_repository_integration.py PACKAGE.deb`. See [APT validation coverage](docs/APT_REPOSITORY.md#validation-coverage) for what these checks establish. For website changes, follow the [website checks](docs/WEBSITE.md#local-checks). In PowerShell, set `$env:PYTHONUTF8 = '1'` before running Python.

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
