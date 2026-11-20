# Packaging and distribution

Klicky publishes native packages for Arch Linux and Debian-compatible distributions.

Current package automation targets x86_64 Arch Linux and amd64 Debian-compatible systems. Other architectures are not published until they have clean-environment package builds and hardware validation.

## Windows packages

Stable [GitHub releases](https://github.com/Muneer320/klicky/releases/latest) include the unsigned Windows 11 x64 installer and portable ZIP. Klicky is also available from the [Microsoft Store](https://apps.microsoft.com/detail/9NLVFX7T2X6Q); its MSIX packaging and update lifecycle are described in the [Store guide](WINDOWS_STORE.md). The [Windows guide](WINDOWS.md) covers build identity, notices, updates and recovery. The [package workflow](../.github/workflows/windows-portable.yml) builds candidates; the [release workflow](../.github/workflows/release.yml) gathers all platforms and checksums without publishing.

## Installed layout

```text
/usr/bin/klicky
/usr/lib/systemd/user/klicky.service
/usr/lib/udev/rules.d/70-klicky.rules
/usr/share/klicky/sounds/
/usr/share/doc/klicky/
/usr/share/licenses/klicky/
```

Configuration and user sound packs remain outside package management:

```text
~/.config/klicky/config.toml
~/.config/klicky/sounds/
```

User packs override system packs with the same name.

## Migrate from the source installer

A source installation places its binary and service unit under the user's home directory. The user unit overrides a package unit until it is removed.

From the existing checkout, run:

```bash
./scripts/uninstall-linux.sh
```

Do not use `--purge`. The command preserves `~/.config/klicky`, including the selected pack, volume, and user sound packs. Install the distribution package afterward and enable its user service:

```bash
systemctl --user enable --now klicky.service
```

## Arch package

The Arch package definition lives in `packaging/arch/`.

```bash
cd packaging/arch
makepkg --cleanbuild
sudo pacman -U klicky-*.pkg.tar.zst
systemctl --user enable --now klicky.service
```

Before publishing an AUR update:

1. Set `pkgver` and `pkgrel` in `PKGBUILD`.
2. Ensure the matching immutable Git tag exists.
3. Regenerate metadata with `makepkg --printsrcinfo > .SRCINFO`.
4. Run `makepkg --cleanbuild` and `namcap` on the manifest and package.
5. Install the package on Arch and verify the daemon with a physical keyboard.
6. Commit `PKGBUILD` and `.SRCINFO` to the AUR repository.

The AUR accepts only its `master` branch. Never publish `.SRCINFO` that differs from `makepkg --printsrcinfo`.

## Debian package

Debian metadata is stored in `Cargo.toml` under `[package.metadata.deb]`. Maintainer scripts are under `packaging/debian/`.

### Build and validate

Build on an amd64 Linux system with the [Linux build prerequisites](INSTALLATION.md#linux-from-source):

```bash
cargo install cargo-deb --version 3.8.0 --locked
cargo deb
```

Validate a package with:

```bash
lintian --suppress-tags initial-upload-closes-no-bugs \
  target/debian/klicky_*_amd64.deb
```

The suppressed tag requires a real Debian Intent-To-Package bug number and applies only when submitting to Debian itself. Do not fabricate one for GitHub release packages.

### Install a local package

Install the built package directly, without adding an APT repository:

```bash
sudo apt install ./target/debian/klicky_*_amd64.deb
systemctl --user enable --now klicky.service
```

For a downloaded release asset, pass its local path instead. APT resolves dependencies from your configured repositories; the distribution must provide the package's required libraries.

## Release automation

A `v*` tag or manual dispatch runs `.github/workflows/release.yml` with read-only repository permissions.

The workflow:

1. Verifies a pushed Git tag matches the Cargo version and archives the exact source.
2. Runs Linux/macOS and Windows checks, including native macOS lifecycle validation.
3. Builds and inspects Debian/Arch packages and the Windows installer/portable ZIP.
4. Verifies build identity, asset inventory and sidecars, then generates the release manifest and `SHA256SUMS`.
5. Uploads a combined candidate artifact for review. Release publication is a separate action; this workflow never publishes automatically.

The regular package workflow also builds both formats on pushes and pull requests.

## Release checklist

The [release workflow](../.github/workflows/release.yml) builds a candidate from a branch or version tag. It archives the exact source, builds native packages and records revision/tree, file sizes, hashes and signing status in `RELEASE-MANIFEST.json`. `SHA256SUMS` covers the payloads and manifest. A candidate run does not publish a GitHub release or deploy the site.

```bash
gh workflow run release.yml --ref REVIEWED_BRANCH
gh run list --workflow release.yml
gh run download RUN_ID --name klicky-VERSION-release-candidate --dir target/release-candidate
```

Replace the branch, run ID and version with the candidate being reviewed. The payload set includes Debian and Arch packages, the Windows installer and ZIP, source archive and checksum sidecars.

Before publication:

1. Keep Cargo/lockfile, Debian/Arch/resource versions, changelog and public download links aligned. Verify the source archive preserves executable script modes.
2. Pass cross-platform CI and package validation. Check bundled sounds, licenses, source obligations, checksums and the recorded source revision/tree.
3. Exercise first install, upgrade, removal and recovery with disposable settings/custom packs. Cover playing, paused and stopped sessions, login startup, data persistence and purge. Windows validation includes missing-CLI recovery, foreign-file refusal and active-bundle interlocks.
4. Check user-facing input/audio behavior on the available desktop hardware. Record listening feedback separately from dispatch or physical latency measurements; state any remaining hardware gaps.
5. Scan the exact Windows release files with current antivirus protection. The release workflow uses a disposable runner to remove inherited scan exclusions, update Defender and scan both original archives and expanded payloads. Missing protection, stale signatures or detections fail assembly. Its `windows-security-evidence` artifact records engine/signature versions and file hashes. This does not sign the standalone files or qualify Windows ARM support.

To publish, tag the reviewed commit with the version in `Cargo.toml`, build that tag, and verify the final downloaded assets. Publish the GitHub release with its notes, payloads, manifest and checksums. Keep published assets immutable; ship a new version rather than replacing bytes under an existing tag.

Then follow [APT publication](MAINTAINING_APT.md#publication), retaining all authenticated historical packages. The Pages environment deploys from `master`, not version tags; a workflow-token-created release also needs a manual APT dispatch. Confirm the archive and site deployed before changing installation guidance. A source-version bump ahead of its matching published release leaves the existing site intact.

After publication, verify public downloads, checksums, install/remove endpoints, signed APT metadata, sound previews and the custom domain. Keep package-manager manifests aligned with the release; advertise a channel only after it is available.

Microsoft Store updates have a separate build, certification and publication lifecycle. Keep the [package identity/version mapping](WINDOWS_STORE.md#build) stable, validate the exact MSIX, submit accurate capability/listing information and verify the public production package. Store signing does not sign the standalone EXE/ZIP files.

## APT repository

Klicky has a public, signed APT repository at [muneer320.github.io/klicky](https://muneer320.github.io/klicky/), using suite `stable`, component `main`, and architecture `amd64`.

Follow the [APT repository guide](APT_REPOSITORY.md) to install through the signed source. Stop the user service before replacing an installed package; the [update guide](INSTALLATION.md#updates-and-removal) includes the commands. Packages contain published release code rather than the current source tree. The archive retains older published packages when adding a release. Use `apt-cache policy klicky` to inspect the available versions and current candidate. The redesigned homepage and installation/removal helpers are live. Use `systemctl --user` for service controls across both retained versions.

[APT publication](MAINTAINING_APT.md#publication) consumes a published release `.deb` and `SHA256SUMS`, retains packages from the live signed index, signs regenerated repository metadata, and deploys the website and archive together to GitHub Pages. Publish the validated candidate as a stable GitHub release first. APT can then run through its release event or manual dispatch. Matching-version pushes to the listed `master` paths also deploy; a version bump ahead of publication leaves the current site intact. If a release is published using `GITHUB_TOKEN`, dispatch APT manually because that token's release event does not start another workflow.

Publication requires the configured signing secrets and GitHub Pages deployment permissions. The workflow stops if it cannot authenticate and recover every package version from the currently published APT index. See the guide for prerequisites, publication steps, test coverage, and key rotation.

## Distribution channels

Use the [installation guide](INSTALLATION.md) for setup, updates and removal. A channel may carry an older version while its package update is being reviewed; check the installed version with `klicky --version`.

| Channel | Availability and maintenance |
| --- | --- |
| **GitHub releases** | Arch/Debian packages, Windows installer and portable ZIP, source archives and SHA-256 checksums. The release workflow prepares candidates for review before publication. |
| **APT** | Signed project archive for amd64 Debian-compatible systems. See [repository maintenance](MAINTAINING_APT.md) for publication, retained versions and signing-key operations. |
| **Microsoft Store** | Published Windows package, with updates delivered through the Store. Follow the [MSIX guide](WINDOWS_STORE.md) when preparing submissions. |
| **Homebrew** | [Project tap](https://github.com/Muneer320/homebrew-tap) for Apple Silicon, including sound packs and the menu helper. Keep formula source hashes and lifecycle checks aligned with releases. Intel Macs use the source installer. |
| **Scoop** | [Project bucket](https://github.com/Muneer320/scoop-bucket) for Windows. Maintain manifest hashes, version-directory migration and owned startup cleanup. |
| **Omarchy** | Klicky Bar is available in the [official plugin marketplace](https://plugins.omarchy.org/plugin.html?id=io.github.muneer320.klicky-bar). The engine [package submission](https://github.com/omacom/omarchy-pkgs/pull/2829) is tracked separately; see the [Omarchy guide](OMARCHY.md). |

The [distribution tracking issue](https://github.com/Muneer320/klicky/issues/149) records outstanding rollout work. AUR publication remains deferred in [PR #9](https://github.com/Muneer320/klicky/pull/9); the Arch recipe and downloadable package are available independently.

Homebrew/core, Chocolatey, RPM repositories, Nixpkgs, Snap, Flathub and crates.io are not published Klicky channels. Do not advertise install commands for them before package review and runtime validation. In particular, a sandboxed build alone does not establish access to global keyboard input or desktop controls.
