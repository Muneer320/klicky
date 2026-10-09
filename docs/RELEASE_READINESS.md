# Release readiness and maintenance audit

Audit baseline: `980ab15f84c1ebcd73340f5579bf070e9363ca5e` on `master`, verified
against `origin/master` on 2026-10-09. This is a preparation report, not release
approval. Version remains 0.3.0. No tag, release, deployment, or version bump is
part of this work.

## Recommendation

Ready for release preparation; not ready to publish v0.4.0. Resolve the high
priority control/error cases, run the new policy through real service managers,
then prepare versions and candidate packages. Website review can proceed
independently. Do not dispatch APT publication just to preview the website.

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

## Fixed in this working tree

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

## Required before release: high priority

### R1. Control replies and persistence are not transactional

Evidence: `src/main.rs:165-184,280-306`, `src/ipc.rs:75-81`.
Switch validates only a directory, writes a command, and prints success. Decode
failure is logged in the daemon, while configuration-save errors for both switch
and volume are discarded. All send failures are treated as an offline request;
the client can overwrite config even when the daemon is merely unresponsive.
The non-lifecycle send uses blocking connect/write without a deadline.

Recommendation: bounded, result-bearing replies for switch/volume; distinguish
offline from delivery failure; surface persistence errors and define whether
an in-memory change should survive a failed save. Test invalid OGG/JSON, read-only
config, blocked socket, live unresponsive daemon, and simultaneous clients.
This needs a focused protocol change, not a speculative patch in the website task.

### R2. launchctl inspection errors can skip unloading

Evidence: `src/service.rs:83-87,101-117,148-170`.
`loaded()` returns false both for a missing job and for failed process execution
or any unsuccessful `launchctl print`. Stop/disable can skip bootout on an
operational failure and claim success; a queued retry could remain registered.
The helper unit test proves closure ordering, not the real probe or launchd state.

Recommendation: injectable command execution with a typed absent/error result;
validate real launchctl failure outputs before choosing an absence classifier.
Test missing launchctl, permission/session errors, pending retries, and job absence.
The general service subprocess helper also has no timeout; the three-second
lifecycle timeout does not bound the complete manager command.

### R3. Zero volume still gives function keys nonzero gain

Evidence: `src/main.rs:194-205` applies `(cfg.volume + 0.2).min(1.0)` to F-key
samples. At configured volume zero, this selects 0.2. This is a source-confirmed
gain inconsistency, not a new physical audio reproduction.
Recommendation: define zero as silence and test the gain policy independently;
confirm function and media keys on both platforms. Keep this as a separate
audio-behavior patch so the website task does not silently change sound policy.

### R4. Candidate service and package validation

Unit tests and compilation do not prove launchd throttling, stop/retry races,
login behavior, or real systemd shutdown. Debian CI extracts rather than installs
the package. `packaging/debian/postinst` and `postrm` reload udev rules but do not
manage running user services; no prerm exists. The source uninstaller ignores
stop failure before removing the binary/unit, and `--purge` can remove runtime
state if stopping failed. See `scripts/uninstall-linux.sh:8-23`.

Recommendation: test upgrades/removal with an active daemon, missing user bus,
and failed stops. Require verified shutdown before destructive purge. Validate
source-to-package migration and user-unit precedence on a real session.
Do not run these destructive scenarios against the user's ordinary installation.

## Required before publication: distribution gates

- Version-bearing files still say 0.3.0: Cargo.toml, Klicky's Cargo.lock entry,
  Arch PKGBUILD/.SRCINFO, Debian changelog, and man-page header. Preserve historical
  entries when adding the future version; do not replace dependency versions.
- `.github/workflows/packages.yml:29,106` asserts binary version 0.3.0.
  `scripts/test_apt_repository_integration.py:102,108` asserts/prints 0.3.0-1.
  Derive these from package metadata in the release preparation task. The old
  version in APT unit fixtures is legitimate test data.
- Update README, website, packaging/APT guides and dispatch default deliberately.
  The current release is v0.3.0; v0.4.0 has not been published.
- `release.yml` publishes automatically on a v* tag after its Debian/Arch jobs,
  without waiting for separate CI workflows. Verify all candidate CI first.
  The Arch release job follows PKGBUILD's tag; package CI substitutes a commit.
  Add/check consistency between the tag, Cargo version, and Arch metadata.
- The release workflow uses GITHUB_TOKEN; it does not automatically trigger the
  APT release-event workflow. Dispatch from master only after explicit approval.
  Prior API inspection found Pages allows master and gh-pages, not tag refs.
- APT publication checks a matching fingerprint and `${tag#v}-1` package version,
  verifies SHA256SUMS, signs metadata, and verifies InRelease. It replaces the
  archive with the selected package and has no downgrade guard or old-package
  retention. Secret-name presence and past success do not prove current key
  expiry/passphrase validity. No automated post-deployment installation exists.
- GitHub package checksums are not independently signed. APT metadata is signed.
  Treat release tags as immutable; release reruns currently upload with clobber.
- macOS remains a source installation; no signed/notarized binary pipeline.

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
- No dependency vulnerability scan was performed. Do not infer absence of
  vulnerabilities from compilation. Review RustSec results and platform-specific
  dependency advisories before making security assurances.

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

Current working-tree validation:

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

 No current-tree native Mac build or real service-manager run is
claimed. Browser checks exercise Chromium and automation, not a screen reader or
Safari. The redesigned page remains unpublished until separately approved.
