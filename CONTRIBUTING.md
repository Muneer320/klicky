# Contributing

Thank you for helping improve Klicky. Small, focused changes are easier to test across input stacks and operating systems than broad rewrites.

New to the product? Start with the [field guide](https://klicky.muneer320.tech/guide.html). For presentation work, use the [character story](docs/CHARACTER.md) and [brand asset guide](docs/BRANDING.md); keep instructions technically clear and the mascot recognizable.

For usage questions, try [Discussions Q&A](https://github.com/Muneer320/klicky/discussions/categories/q-a). Use [Ideas](https://github.com/Muneer320/klicky/discussions/categories/ideas) to discuss a proposal before turning it into an implementation issue. The [wiki](https://github.com/Muneer320/klicky/wiki) links to the maintained platform guides.

Please follow our [Code of Conduct](CODE_OF_CONDUCT.md) in issues, pull requests, Discussions and other project spaces. It includes a private contact for reporting concerns.

## Before starting

Open an issue before changing any of these areas:

- Public CLI commands or configuration keys
- Sound-pack format
- Input permissions
- Audio engine or buffer policy
- Platform support claims
- Production dependencies

Straightforward bug fixes, documentation corrections, and narrowly scoped tests can go directly to a pull request.

All repository changes must be developed on a feature branch and submitted through a pull request to `master`. Do not commit directly to `master`; wait for the relevant CI and validation checks to pass before merging.

## Development setup

Use the README's build prerequisites for [Linux](docs/INSTALLATION.md#linux-from-source), [macOS](docs/INSTALLATION.md#macos) or the [Windows guide](docs/WINDOWS.md). Install Rust 1.87 or newer; distribution-provided Rust packages may be older. Windows uses the MSVC toolchain and Windows SDK resource compiler. Building and running the automated tests does not require installing a login service or granting keyboard access.

Clone the repository, then run the [checks below](#tests):

```bash
git clone https://github.com/Muneer320/klicky.git
cd klicky
```

## macOS package builds

Package builds on macOS may set `KLICKY_MACOS_INSTALL_BINARY` **at compile time**
to a fixed, absolute launcher path in their managed prefix. That launcher must be
a regular executable; its parent directories may include a package manager's
stable symlink. Source installs keep the existing `~/.local/bin/klicky` checks.
The launcher should set `KLICKY_SYSTEM_SOUNDS_DIR` for bundled sounds. The menu
helper accepts `KLICKY_MENU_CLI` and `KLICKY_MENU_ICON` from its launcher to locate
the matching CLI and template image. Package lifecycle tests must cover those
paths after upgrades and must preserve the user's configuration on removal.

## Repository map

| Path         | Purpose                                                                     |
| ------------ | --------------------------------------------------------------------------- |
| `src/`       | See the complete [architecture module map](docs/ARCHITECTURE.md#module-map) |
| `tests/`     | CLI lifecycle and status regression tests                                   |
| `packaging/` | Distribution metadata, service units, desktop helpers, and Pages template   |
| `scripts/`   | Installation, removal, APT publication, and validation tools                |
| `sounds/`    | Bundled sound packs                                                         |
| `docs/`      | Architecture and platform documentation                                     |

## Engineering guidelines

- Understand the current path before replacing it.
- Prefer the smallest correct design.
- Keep platform details in their adapters. Preserve the existing Unix engine and the Windows coordinator boundaries.
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

For Rust changes, run:

```bash
cargo fmt --check
cargo test --locked
cargo clippy --locked --all-targets --all-features -- -D warnings
PYTHONUTF8=1 python3 -B scripts/check-docs.py
```

Clippy includes compilation checks. Run `cargo build --release` when changing
build behavior or preparing a package; package CI also builds release binaries.
For prose-only edits, the documentation check is sufficient locally.

PR validation runs once per commit, with superseded runs cancelled. Markdown-only
PRs keep the fast documentation, formatting and script checks, without rebuilding
native apps and packages. Website changes retain package/APT validation. Unknown
files and workflow changes use full checks; `master` and release candidates always
run the full native and package suite. Required check names remain available even
when their expensive work is unnecessary. A failed scope check fails the required
checks rather than silently accepting the change.

For APT changes, run `python3 -B -m unittest scripts/test_apt_repository.py` and, on Linux with GnuPG and reprepro, `python3 -B scripts/test_apt_repository_integration.py PACKAGE.deb`. See [APT validation coverage](docs/MAINTAINING_APT.md#validation-coverage) for what these checks establish. For website changes, follow the [website checks](docs/WEBSITE.md#preview-and-validate). In PowerShell, set `$env:PYTHONUTF8 = '1'` before running Python.

## macOS menu checks

Run `python3 -B scripts/test-macos-menu.py` on macOS to test the helper's subprocess client and native menu state. Run `cargo test` for playback, JSON status, and CLI regression coverage. These checks do not require installing a login service or granting keyboard access.

For manual validation, record the macOS version, CPU architecture, audio hardware, and installation method:

- Use keyboard navigation and VoiceOver in light and dark appearance. Check labels, submenus, disabled Pause/Resume actions, and pack/volume checkmarks.
- Pause while typing, then resume. Confirm sound stops without changing the daemon PID or replaying old input. Check that mute and pause remain separate.
- Set a non-preset CLI volume such as `0.73`, switch packs from the menu and CLI, and refresh after adding or removing a custom pack. Check the current selections.
- Try an invalid pack, unavailable CLI, and stalled command. Confirm errors remain visible, the last working pack remains selected, and the menu and Quit stay responsive.
- Toggle login startup while paused. Confirm the PID and pause state stay unchanged. Test service start/stop separately and confirm quitting the menu helper leaves the daemon running.
- Check source/Homebrew CLI and icon overrides, upgrades, and removal. Record Intel and Apple Silicon results separately.

Compilation and automated fixtures do not establish physical keyboard, audio, or VoiceOver behavior. Record those results separately in the PR, along with any checks that remain pending.

## CodeQL maintenance

The [advanced scanning workflow](.github/workflows/codeql.yml) covers Actions,
JavaScript/TypeScript, Python, Rust and the macOS Swift helper. Swift uses the
same direct `swiftc` build as the menu installer, without installing or running
the app. The workflow verifies that `KlickyMenu.swift` is in the extracted source
archive and retains SARIF results as short-lived CI artifacts.

Until the repository variable `CODEQL_ADVANCED_ENABLED` is `true`, analysis runs
without uploading results; the existing default setup continues to publish its
scans. Do not enable uploads while default setup is active.

For maintainers switching to advanced setup:

1. Review the workflow and verify successful extraction/analysis for all five
   languages on the intended commit. Keep default setup active during validation.
2. After merging, switch CodeQL from default to advanced setup, set
   `CODEQL_ADVANCED_ENABLED=true`, and run **CodeQL advanced** on `master`.
3. Verify all five uploaded language results and a PR comparison, including a
   Dependabot update. Check that existing alert dispositions are preserved. Only
   then retire obsolete default configurations or change required-check names.
4. To roll back, set the variable to `false` before restoring default setup with
   its previous language selection. Keep normal CI and scanning requirements in
   place throughout the transition.

GitHub [rejects advanced CodeQL uploads while default setup is enabled](https://docs.github.com/en/code-security/reference/code-scanning/sarif-files/troubleshoot-sarif-uploads/default-setup-enabled).

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

CI must pass on Windows, Linux and macOS. Compilation on a platform does not count as runtime verification on that platform. Windows artifact workflows create unsigned test packages without release publication. Installer lifecycle changes require installation/upgrade/removal and failure-recovery evidence; input/audio changes require physical measurements or explicit unresolved qualification limits. See the [Windows guide](docs/WINDOWS.md).

## Sound contributions

Only submit recordings you created or have permission to redistribute. Include the source, creator, and license or permission statement in the pull request.

## Style

Public documentation uses direct technical language, minimal emoji, and no em dashes. Prefer short paragraphs and concrete claims backed by code or measurements.
