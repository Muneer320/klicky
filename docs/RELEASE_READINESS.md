# Release readiness and maintenance audit

Audit baseline: `455ddacfdd39c015884207131c5154fa7006cd2c` (PR #7 merge) on `master`,
verified against `origin/master` on 2026-10-11.

**Status: v0.4.0 release candidate prepared.** Version bumped to 0.4.0 in all
relevant files. Reliability hardening (PR #7) merged and validated. macOS
functionality and launchd behavior manually verified on hardware by the
repository owner. All CI checks passed on the feature branch. No tag, GitHub
release, or APT publication has been triggered yet.

## Recommendation

Ready to tag and publish v0.4.0 after owner review of the release-preparation PR.
The high-priority reliability items (R1–R4) documented below were addressed in PR
#7; the notes are preserved for historical context and as known-limitations
baselines for future work. APT publication requires a separate manual
`workflow_dispatch` after the release is published (see § APT publication sequence
below).

## State preserved and obsolete notes retired

The pre-existing unstaged changelog additions describe:

1. The public service command for systemd and LaunchAgents.
2. The macOS source installer and login startup.
3. The optional Swift menu helper and separate LaunchAgent.
4. macOS CI compilation of the helper.
5. Installation/service documentation in README and the manual.

All five entries are preserved. They belong in eventual release preparation,
not an invented version heading today. The old untracked CODEX_REVIEW_NOTES.md
reviewed f803f7c, referenced a superseded proposed README, and described fixed
lifecycle, status, path, and APT documentation problems. Its unresolved findings
are consolidated below; it is removed rather than maintained as a second,
contradictory audit. Attribution, benchmark history, and user guides remain.

Historical machine bookkeeping from those notes: an earlier session installed
Rust and Klicky plus sounds and both LaunchAgents on the Mac named holly, and
created `/tmp/klicky-validation.f4wzFf`. This audit did not access or remove those
files. Check that temporary checkout on the Mac before deciding whether to remove
it; do not treat the old notes as proof of its current state.

## Completed website and audit work (`de8b7b9`)

- **Custom pack range overflow:** `src/soundpack.rs:43-48` added two untrusted
  sample offsets before clamping. The regression
  `extreme_timing_ranges_do_not_panic_or_wrap` loads a real bundled OGG with
  extreme u64 timing values and failed on the original code with integer
  overflow. Saturating addition now preserves documented truncation/skipping.
  The test covers a normal range, an overlong duration, and an out-of-file start.
- **Benchmark terminology:** `docs/BENCHMARKS.md` called the percentile selection
  nearest-rank; `src/main.rs::print_latency_summary` uses zero-based index
  selection. Corrected the description without changing recorded numbers.
- **Pack validation instructions:** the foreground start blocks its terminal;
  `docs/SOUND_PACKS.md` now uses a second terminal and explains delivery versus
  completion and the macOS pack location.
- **Website:** replaces the generic archive page with a product-specific visual
  explanation and clear install paths. Removed the static "operational" badge,
  which was not a live health check. Preserved generated archive metadata and
  all download endpoints. See [website maintenance](WEBSITE.md).

## Resolved reliability findings (R1–R4, resolved in PR #7) [Historical context]

The high-priority reliability items below (R1–R4) were identified in the pre-release
audit and resolved in PR #7 (merged at `455ddac`). They are retained here as historical
context and design baselines for future work, and are no longer release blockers:

- **R1 (Control replies & persistence):** Closed in PR #7. Control requests (switch/volume)
  now use bounded socket timeouts, return structured result-bearing replies
  (`IpcResponse::Ok` / `IpcResponse::Error`), distinguish offline daemon from delivery
  failures, and save configuration atomically via temporary-file replacement with explicit
  error reporting.
- **R2 (launchctl inspection errors):** Closed in PR #7. `service.rs` uses typed
  `ServiceInspectResult` distinguishing absent/unloaded jobs (code 113) from execution/permission
  failures, preventing false-positive unload skips. Subprocess execution is bounded by timeouts.
- **R3 (Zero volume gain):** Closed in PR #7. Function key gain scales proportionally with
  configured volume; at volume 0.0, gain is strictly 0.0 (silence).
- **R4 (Candidate service & package validation):** Closed in PR #7. Added a dedicated Debian
  package lifecycle test harness (`scripts/test_install_lifecycle.py`, 5/5 passing) validating
  clean install, upgrade, removal, active daemon stop, and `prerm` script handling. macOS
  launchd service behavior was verified on hardware.

### R1. Control replies and persistence are not transactional [Resolved in PR #7]

Evidence: `src/main.rs:165-184,280-306`, `src/ipc.rs:75-81`.
Switch validated only a directory, wrote a command, and printed success. Decode
failure was logged in the daemon, while configuration-save errors for both switch
and volume were discarded. All send failures were treated as an offline request;
the client could overwrite config even when the daemon was merely unresponsive.
The non-lifecycle send used blocking connect/write without a deadline.

Resolution: Implemented bounded, result-bearing replies for switch/volume; distinguished
offline from delivery failure; surfaced persistence errors and implemented atomic file
replacement.

### R2. launchctl inspection errors can skip unloading [Resolved in PR #7]

Evidence: `src/service.rs:83-87,101-117,148-170`.
`loaded()` previously returned false both for a missing job and for failed process execution
or any unsuccessful `launchctl print`. Stop/disable could skip bootout on an
operational failure and claim success; a queued retry could remain registered.

Resolution: Implemented typed inspection results (`ServiceInspectResult`) classifying
absent vs failed executions, with timeout-bounded subprocess execution.

### R3. Zero volume still gives function keys nonzero gain [Resolved in PR #7]

Evidence: `src/main.rs:194-205` previously applied `(cfg.volume + 0.2).min(1.0)` to F-key
samples. At configured volume zero, this selected 0.2.

Resolution: Gain now scales proportionally with configured volume, guaranteeing true
silence when volume is set to 0.0.

### R4. Candidate service and package validation [Resolved in PR #7]

Unit tests and compilation do not prove launchd throttling, stop/retry races,
login behavior, or real systemd shutdown. Debian CI previously extracted rather than installed
the package, and no `prerm` script existed to stop active daemons before removal.

Resolution: Implemented Debian `prerm` package script, added regression tests in
`scripts/test_install_lifecycle.py` covering install, upgrade, removal, and active-daemon
stop scenarios, and validated macOS launchd on hardware.

## Distribution gates and release constraints

- **Version metadata (Completed in PR #8):**
  All version-bearing files have been updated from 0.3.0 to 0.4.0 (`Cargo.toml`,
  `Cargo.lock`, Arch `PKGBUILD` and `.SRCINFO`, Debian `changelog` `0.4.0-1`,
  `docs/klicky.1`, CI version assertions in `packages.yml`, and `apt-repository.yml`
  default input). Historical changelog entries and fixtures are preserved.
- **Candidate CI verification (Completed in PR #8):**
  All candidate CI jobs (`ubuntu-latest`, `macos-latest`, `debian-package`,
  `arch-package`, `apt-repository`) have passed on the release branch before tagging.
- **Release workflow trigger:**
  `release.yml` publishes automatically upon pushing a `v*` tag after its package build
  jobs. Tag only after PR #8 is merged into `master` and candidate CI is confirmed passing.
- **APT publication decoupling:**
  The release workflow uses `GITHUB_TOKEN` and does **not** trigger the APT release workflow.
  Dispatching the APT repository publication requires a manual `workflow_dispatch` on
  `apt-repository.yml` from `master` with tag input `v0.4.0` after the GitHub release is published.
- **APT repository integrity and history:**
  APT publication checks the GPG key fingerprint (`DBB67AE478D2FFCEC6637B559899E554D3580D8F`),
  verifies `${tag#v}-1` package version, verifies the candidate against `SHA256SUMS`,
  downloads and preserves previously published packages from `klicky.muneer320.tech`
  (retaining full release history), re-signs metadata, and verifies `InRelease`.
  Note: There is no downgrade guard; do not publish lower version numbers after higher ones.
- **Checksum signing constraint:**
  GitHub package checksums in `SHA256SUMS` are published unsigned. Debian APT repository
  metadata (`InRelease`) is GPG-signed. Release tags should be treated as immutable.
- **macOS distribution constraint:**
  macOS remains a source installation (`scripts/install-mac.sh`); there is no signed or
  notarized native binary distribution pipeline.

## Known limitations and medium-priority follow-ups

| Finding and evidence | Impact and recommended validation |
|---|---|
| Config::load creates defaults; save directly truncates/writes (`src/config.rs:25-46`) | Read-like status/list can write. Concurrent or interrupted saves can corrupt config. Separate read/default handling and implement atomic replace with failure/concurrency tests. Config values loaded from TOML are not validated by the CLI's volume checks. |
| Swift helper polls synchronously and parses human output (`packaging/macos/KlickyMenu.swift:34-65`) | Slow processes can freeze refresh; toggle failures are discarded. Use asynchronous bounded requests, explicit errors, and structured status. Its fixed binary path now agrees with service installation. |
| Linux discovery runs only at startup; any reader failure ends the listener (`src/listener/linux.rs:13-37`) | New keyboards need restart. Disconnect causes daemon failure/restart under systemd, potentially affecting remaining keyboards. Test no-device startup, unplug/replug, multiple keyboards, and restart-rate limiting. |
| Persistent sink has no application-level reopen path (`src/player.rs:99-114`) | Device change/suspend behavior depends on backend. Test first, then design recovery if needed; responsive IPC is not audio health. |
| Supplementary media tap silently returns on creation/source failure (`src/media_keys.rs:122-130`) | Normal input may work while media sounds do not. Surface diagnostic state; verify permission revocation and tap recovery physically. Static mutable FFI state assumes a single listener; do not add restart calls without ownership review. |
| Sound decoder treats every packet IoError as EOF (`src/soundpack.rs:111-115`) | Non-EOF failures may yield partial packs. Distinguish UnexpectedEof and other errors; add truncated/corrupt-input tests before changing decode policy. Full decode and per-key copies have no resource limit for untrusted packs. |
| Listener panic is not joined/reported (`src/main.rs:135-154`) | Only a returned Err reaches the main loop; a panic can leave a responsive but inputless daemon. Add supervised thread termination tests before promising health detection. |
| IPC accept errors are flattened and channels are unbounded (`src/ipc.rs:49-67`, `src/main.rs:129-158`) | Persistent accept failure may spin; same-user clients can queue commands or delay control. Bound work/queues if defending against local resource exhaustion. No network exposure was found. |
| Runtime-path parent trust (`src/config.rs::ensure_runtime_dir`) | chmod follows directory symlinks; private runtime placement assumes a trustworthy parent. O_NOFOLLOW protects the lock file, not all path components. Threat-model shared/writable XDG parents before adding hardening. |
| Status PID identity is protocol-level, not OS peer authentication (`src/lifecycle.rs:119-151`) | Stale/reused PID alone no longer establishes responsiveness. Same-user endpoint spoofing and PID reuse around a probe are not cryptographically prevented. Existing private directory/socket permissions are the boundary. |
| Startup conservatively refuses a live PID without the lock (`src/lifecycle.rs:77-94`) | Protects an older/unmanaged process but can block after crash plus PID reuse. Test and document safe diagnosis rather than deleting state automatically. |
| macOS path validator checks any execute bit (`src/service.rs:218-232`) | Does not prove effective-user ACL access, executable format, or continued path identity after validation. Readiness catches many execution failures; filesystem validation is not a security sandbox. |
| Omarchy uses systemctl is-active (`packaging/omarchy/klicky.qml:31-36`) | Shows service-manager activity, unlike Swift's IPC-responsive state. Do not imply both indicators test input/audio health. |

## Test blind spots and documentation debt

- Lifecycle tests cover ownership, bounded stop, retained target identity,
  startup errors, and bad acknowledgments. The subprocess shutdown test uses
  controlled synchronization and is meaningful. Its ignored fixture is invoked
  by the parent test, not abandoned coverage.
- `tests/lifecycle.rs` and `tests/status.rs` are Linux-only. Several fake-server
  tests have unbounded accept/join operations; a regression before connection
  could hang the test process. Add harness deadlines rather than sleeps.
- Service unit tests validate plist fields/path policy and callback order, not
  launchctl execution. A matching status reply proves an event loop only.
- The Mac media-key historical article mixes past observations with broad claims
  about duplicate regular/media events. Keep its history, but validate mappings,
  held-key repeats and duplicate sounds on current macOS before rewriting it.
- Architecture module map was missing lifecycle/service; corrected in this task.
- No real menu screenshot exists. A future screenshot is useful but optional;
  the new website does not fabricate one.
- The cleanup follow-up below includes a RustSec scan. Repeat it for the exact
  release candidate; a clean advisory scan does not establish absence of defects.

## Platform and release acceptance plan

The user's physical Linux/Omarchy and macOS testing confirmed typing, playback,
pack switching, volume controls, normal startup/control and menu behavior. Those
results stand. They do not establish the latest failure-recovery scenarios.

1. Resolve R1/R2/R3 and define safe uninstall failure handling with regression tests.
2. Mac: verify installed path rejection, permission grant/revocation, fresh install
   and upgrade, old-plist migration, logout/login, crash restart, repeat failures,
   pending-retry stop/disable, and hung shutdown. Check after waiting beyond the
   throttle interval that intentional stop remains stopped.
3. Linux: real systemd crash/restart/hung-stop tests; package upgrade/removal;
   user-local unit migration; udev ACLs; keyboard attach/detach; no-device startup.
4. Both: suspend/resume, output-device replacement, zero-volume F/media keys,
   rapid typing and pack controls. Record OS, hardware, package and commit.
5. Prepare v0.4.0 metadata and documentation only in a separately approved task.
6. Validate exact candidate CI/packages, then obtain release/tag approval.
7. Verify released checksums; separately approve APT deployment from master;
   verify live signatures, candidate version and a real install/upgrade.

## Validation evidence

Baseline remote CI at 980ab15 passed Linux (37 tests), macOS (20 tests), strict
Clippy, both release builds, Swift compilation, Debian/Arch builds, and APT tests:
[CI](https://github.com/Muneer320/klicky/actions/runs/37858725984),
[packages](https://github.com/Muneer320/klicky/actions/runs/37858726009).
This supersedes the old "no macOS build" caveat, but not manager/hardware gaps.

Website/audit validation before the cleanup follow-up:

- Linux Rust 1.90: focused regression failed before the fix and passed after it;
  cargo fmt --check, cargo check --all-targets, cargo test (38 passed; one
  subprocess fixture ignored directly but invoked by its parent), and strict
  all-targets/all-features Clippy passed.
- Linux Rust 1.87: cargo check --locked --all-targets passed. This closes the Linux
  compile-level MSRV gap for this lockfile; CI still has no pinned MSRV job and
  macOS MSRV remains unchecked.
- Eight APT unit/template tests passed; real signed-repository integration passed
  using the checksum-verified published 0.3.0-1 Debian package and an ephemeral
  test key. It verified InRelease, apt update and package discovery, not an
  installed service or production signing credentials.
- Chromium: light/dark responsive layouts, internal anchors, native keyboard
  disclosures, focus states, reduced motion, base path and console checks.
  At 320, 360, 768 and 1440 CSS pixels in both themes, axe-core 4.10.3 WCAG
  A/AA checks passed. No browser console/runtime errors were reported.
- All 15 distinct external links returned HTTP 200; repository fragment targets
  matched current Markdown headings. Prettier HTML formatting, the UTF-8
  documentation checker, eight local Python tests, and git diff --check passed.
- Production stays a self-contained HTML page, rendered by the existing Python
  builder; no TypeScript or frontend bundle build exists.

No local native Mac build or real service-manager run is
claimed. Browser checks exercise Chromium and automation, not a screen reader or
Safari. The redesigned page remains unpublished until separately approved.

## Cleanup follow-up, 2026-10-09

The website/audit commit `de8b7b9` is on `origin/master`. Its remote
[CI](https://github.com/Muneer320/klicky/actions/runs/37863178407) and
[package builds](https://github.com/Muneer320/klicky/actions/runs/37863178424)
passed. Master pushes run checks, not the release or Pages publication workflows.
The five unrelated changelog additions remain unstaged and unchanged.

### Cleanup decisions

| Item | Evidence and decision |
|---|---|
| Legacy banner | Removed `assets/klicky-banner.svg`: no tracked content, installer, package manifest or workflow references it. The current README has no banner and the website uses inline SVG. Git history retains the old illustration. |
| Static Deb822 copy | Removed `packaging/apt/klicky.sources`: no consumer reads it. `build-apt-repository.py::render_sources` already generates the published file from the configured URL. The public download path remains unchanged. |
| Index-writing wrapper | Inlined `write_index`, a single-use one-statement wrapper with no other callers. Rendering remains in the tested `render_index`; signing and output replacement are unchanged. |
| Contributor duplication | Replaced repeated native prerequisite commands and the partial module map with README/architecture links; retained contributor policy, test commands and added APT/website validation navigation. |
| APT test version | A local `0.0.0+cleanup1` fixture passed signature/index generation but failed the old hardcoded `0.3.0-1` assertion. The revised test checks the input package's exact candidate, under a stable locale, and passes with both versions. No project/package release version changed. |
| Dependency advisory | Cargo audit 0.22.2 found informational unsoundness [RUSTSEC-2026-0190](https://rustsec.org/advisories/RUSTSEC-2026-0190.html) in anyhow 1.0.102. No `downcast_mut` calls exist in Klicky's source/tests. Updated only that lockfile entry to patched 1.0.103; no dependency was added or removed. |

### Rechecked and deliberately retained

The audit traced every Rust module, both integration suites, script consumers,
package manifests, all workflows, desktop helpers, documentation and bundled
pack metadata. No further Rust dead code was established by compiler/Clippy
checks. Absence of text references alone was not treated as proof of dead code.

- R1 through R4 remain open. This cleanup does not change runtime command,
  volume, configuration, uninstall, or service-manager behavior.
- Persistent lifecycle lock ownership, cleanup ordering and PID-matched replies
  retain regression coverage. A live reused PID can still conservatively block
  startup; responsive status remains an IPC check, not an input/audio test.
- Blocking non-lifecycle sends and manager subprocesses, unbounded channels,
  silent media-tap failures, decoder I/O-as-EOF, unsupervised listener panics,
  Swift main-thread polling, startup-only input discovery and output-device
  recovery remain the verified follow-ups listed above.
- The direct `cocoa` dependency has no Rust import, but macOS FFI/framework
  linkage and the older transitive cocoa used by rdev need a native build before
  removal. Symphonia's default codecs may support custom packs beyond OGG;
  narrowing features needs an explicit compatibility decision.
- Source and packaged systemd units intentionally have different executable
  paths. The APT builder's `--existing-root` option is tested compatibility,
  despite not being used by publication. Neither is obsolete.
- Historical media-key notes, licenses, pack recordings, `.SRCINFO`, changelog
  history and the ignored subprocess fixture remain useful and are retained.
- No accidental build artifacts were found tracked. Ignored local build/browser artifacts
  were left alone. Future work should bound fake-server test accept/join waits
  and test native macOS MSRV, installation upgrades/removal and real managers.

The next implementation work should address R1's bounded command results and
atomic configuration persistence, followed by launchctl error classification
and safe uninstall. Handle the zero-volume gain policy in a focused audio fix
with regression tests; then run the physical/platform acceptance plan above.

### Cleanup validation

- Linux container, Rust 1.90: formatting, all-target check, 38 tests (same count
  as before cleanup), strict all-target/all-feature Clippy, and release build
  passed. The ignored subprocess fixture is still exercised by its parent test.
- Rust 1.87: locked all-target check passed with the updated lockfile.
- `cargo audit --deny warnings` passed with cargo-audit 0.22.2 and RustSec database
  commit `550efd3d587a29b2e2c2b21b17a440da4fede999` (1,295 advisories).
- Eight APT unit tests and signed integration with both the checksum-verified
  release package and alternate local fixture passed. The Windows sandbox first
  denied a temporary-directory operation; rerunning outside it passed without
  changing the tests. The alternate fixture deliberately failed the old version
  assertion before the test was corrected.
- Website checks passed at 320, 360, 768 and 1440 pixels in light/dark modes,
  including axe, keyboard/focus, reduced motion, anchors, console and base path.
- UTF-8 documentation checks, 53 local Markdown links/anchors, all ten pack
  timing maps, shell syntax, ShellCheck, checksum-verified actionlint 1.7.12
  (the same SC2016 exclusion as CI), and `git diff --check` passed.

The cleanup changes no Rust source, workflows, service definitions or release
versions. Its dependency update still needs the normal native macOS CI run;
real hardware/service-manager checks remain outstanding. No screen-reader,
Safari, package upgrade/removal or live deployment test is claimed.

## APT publication sequence (post-release)

After the v0.4.0 GitHub release is published (tag `v0.4.0`, not a draft):

1. **Manual `workflow_dispatch`** on [`Publish APT repository`](https://github.com/Muneer320/klicky/actions/workflows/apt-repository.yml):
   - Go to Actions → "Publish APT repository" → Run workflow.
   - Enter `v0.4.0` in the tag field (default is already `v0.4.0`).
   - Click "Run workflow" on branch `master`.
2. The workflow will:
   - Download `klicky_0.4.0-1_amd64.deb` and `SHA256SUMS` from the release.
   - Verify the SHA256 checksum and package version (`0.4.0-1`).
   - Download all previously published versions from the live APT repository (`klicky.muneer320.tech`) to preserve history.
   - Re-sign and rebuild a complete APT repository containing all versions (old and new).
   - Deploy the updated site to GitHub Pages.
3. **Post-deployment verification:**
   - Confirm `https://klicky.muneer320.tech/dists/stable/main/binary-amd64/Packages` lists `Version: 0.4.0-1`.
   - Run `apt-get update && apt-cache policy klicky` with the repository configured; confirm `0.4.0-1` is the candidate.
   - Verify `InRelease` signature: `gpg --verify InRelease`.
   - Optionally install the package on a clean Debian/Ubuntu VM to confirm `klicky --version` = `klicky 0.4.0`.

**Key constraints:**
- The release workflow uses `GITHUB_TOKEN` and does **not** automatically trigger the APT publication workflow.
- APT publication requires secrets `APT_GPG_PRIVATE_KEY`, `APT_GPG_PASSPHRASE`, and `APT_GPG_FINGERPRINT` to be present and valid.
- The fingerprint must match `DBB67AE478D2FFCEC6637B559899E554D3580D8F`.
- History retention is implemented: all previously published `.deb` packages are re-included in the repository.
- There is no downgrade guard; do not publish lower version numbers after higher ones have been published.
- Do not dispatch APT publication until the GitHub release is confirmed published and the SHA256SUMS file is present.

## v0.4.0 release candidate validation

- PR #7 CI (`ubuntu-latest`, `macos-latest`, `arch-package`, `debian-package`) passed on
  commit `321b862` (push run after the final clippy fixes).
  [CI](https://github.com/Muneer320/klicky/actions/runs/38145052346)
  [Packages](https://github.com/Muneer320/klicky/actions/runs/38145052345)
- Local Linux validation on `foaly` (Rust 1.90): `cargo fmt --check` passed,
  `cargo clippy --all-targets --all-features -- -D warnings` passed (0 warnings),
  `cargo test --bin klicky` passed (38 passed, 1 ignored/subprocess fixture).
- macOS functionality and launchd behavior manually verified on hardware by the
  repository owner (confirmed before PR #7 was merged).
- Arch package `makepkg --printsrcinfo` diff passes; `.SRCINFO` field order matches
  generated output.
- Debian package lintian passes; version assertion checks `klicky 0.4.0`.
- PR #8 release candidate CI passed on all jobs:
  - [CI run 38146004981](https://github.com/Muneer320/klicky/actions/runs/38146004981) (`ubuntu-latest`, `macos-latest`)
  - [Packages run 38146004946](https://github.com/Muneer320/klicky/actions/runs/38146004946) (`debian-package`, `arch-package`, `apt-repository`)
- Candidate package artifacts and SHA256 checksums verified from CI run 38146004946:
  - `klicky_0.4.0-1_amd64.deb`: `f94833bbd4f31be6eacd8df0cf9984e8b1d7f461b794355c4637423c51997626`
  - `klicky-0.4.0-1-x86_64.pkg.tar.zst`: `69be542bce8a2a886431c54f4e9583ebac82679c45010c2d256a267798467f31`

