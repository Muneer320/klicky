# Linux packaging

Klicky publishes native packages for Arch Linux and Debian-compatible distributions.

Current package automation targets x86_64 Arch Linux and amd64 Debian-compatible systems. Other architectures are not published until they have clean-environment package builds and hardware validation.

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

Build on an amd64 Linux system with the [Linux build prerequisites](../README.md#build-from-source-on-linux):

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

A `v*` tag triggers `.github/workflows/release.yml`.

The workflow:

1. Verifies the Git tag matches the Cargo version.
2. Builds and lints the Debian package.
3. Builds and inspects the Arch package.
4. Generates `SHA256SUMS`.
5. Creates the GitHub release when needed.
6. Uploads both packages and checksums.

The regular package workflow also builds both formats on pushes and pull requests.

## APT repository

Klicky has a public, signed APT repository at [muneer320.github.io/klicky](https://muneer320.github.io/klicky/), using suite `stable`, component `main`, and architecture `amd64`.

Follow the [APT repository guide](APT_REPOSITORY.md) to verify the dedicated signing-key fingerprint and install the Deb822 source with `Signed-By`. Once configured, use `sudo apt update` and `sudo apt install klicky`. This installs a published release package, not the current source tree. The currently published package is `0.3.0-1`; use `systemctl --user` for its service controls. The redesigned homepage is not live until a successful Pages deployment is verified.

[APT publication](APT_REPOSITORY.md#publication) consumes a release `.deb` and `SHA256SUMS`, retains packages from the live signed index, signs regenerated repository metadata, and deploys the website and archive together to GitHub Pages. It runs for qualifying pushes to `master`, release-published events, and manual dispatches. Because the tag-driven release workflow publishes with `GITHUB_TOKEN`, maintainers must still explicitly dispatch APT publication after that workflow finishes; its release event does not automatically start another workflow.

Publication requires the configured signing secrets and GitHub Pages deployment permissions. The workflow stops if it cannot authenticate and recover every package version from the currently published APT index. See the guide for prerequisites, publication steps, test coverage, and key rotation.

## Distribution roadmap

Status below is based on this repository's files and workflows as of 2026-10-10, plus the live APT endpoint. Except for APT, external registry listings were not verified; an implementation or CI build is not evidence that a package is publicly available.

| Manager | Verified status | Required files and publication route | Automation and maintenance effort | Validation |
|---|---|---|---|---|
| Arch Linux / AUR | `packaging/arch/PKGBUILD` and `.SRCINFO` exist; CI builds and inspects the package. AUR publication is not verified. | Keep `PKGBUILD` and generated `.SRCINFO` synchronized; publish commits to the AUR package Git repository. | Release/package CI already builds it; AUR updates can be submitted after each tagged release. Medium effort: track source tags, dependencies, and metadata. | `makepkg --printsrcinfo` parity, clean build, `namcap`, then install, upgrade, service, and removal checks on Arch. |
| Signed APT | A signed amd64 repository currently publishes `0.3.0-1`; the redesigned homepage is not yet live. | Debian metadata and maintainer scripts, the existing builder/workflow, and the pinned signing key; publish the combined website and repository artifact through Pages. | Release assets provide the package; qualifying `master` pushes rebuild the site while recovering all published package versions. Medium effort: key custody, package history, and Debian dependency compatibility. | Verify fingerprint and `InRelease`, run the integration test and `apt-get update`, compare all versions and package hashes, and test install/upgrade/removal on supported Debian/Ubuntu releases. |
| Homebrew tap | No formula or tap publication is verified. | Add `Formula/klicky.rb` with source/tag, dependencies, and checksums; publish through a dedicated `homebrew-klicky` tap. | A release job can update the formula and open a tap PR; bottles are optional future work. Medium-high effort: macOS builds, dependencies, and versioned formula maintenance. | `brew audit --strict --online`, `brew test`, and install/uninstall checks on supported macOS architectures. |
| WinGet | No manifest or Windows package is present; Windows is not currently a supported platform. | First provide a supported Windows build and installer, then add versioned WinGet YAML manifests and submit them to `microsoft/winget-pkgs`. | Release automation could generate checksums and submit a manifest PR only after a Windows release exists. High effort: Windows input/audio/service support and installer lifecycle. | Validate the manifest, then install, upgrade, and uninstall in a clean Windows environment; test runtime input and audio on Windows. |
| Scoop | No bucket manifest or Windows package is verified; Windows is not currently supported. | First provide a supported Windows build; add a bucket JSON manifest and install/uninstall scripts in a Klicky bucket. | A release job could update the manifest and open a bucket PR. Medium-high effort: portable layout, shims, and update behavior. | Run Scoop install, update, and uninstall on clean Windows and verify paths, permissions, input, audio, and service behavior. |
| Chocolatey | No `.nuspec`, package scripts, or Windows package is verified; Windows is not currently supported. | First provide a supported Windows installer; add a `.nuspec` and PowerShell install/uninstall scripts, then submit to the Chocolatey Community Repository. | `choco pack` and package tests can run in Windows CI; publication should remain gated on moderation and explicit credentials. High effort: installer lifecycle and Windows support. | Run package verification plus clean install, upgrade, and uninstall tests in a disposable Windows environment. |
| RPM distributions | No RPM spec or repository is present. | Add an RPM `.spec` with files, dependencies, service, udev, and license metadata; select a publisher such as OBS for multiple distributions or COPR for Fedora. | CI can build per target distribution and publish only after spec review. Medium-high effort: distro-specific dependencies and policy differences. | `rpmlint`, clean builds for each target, then install, upgrade, service, and removal tests. |
| Nix | No Nix expression or flake is present. | Add a derivation or `flake.nix` with source hash, Rust build inputs, metadata, and optionally a NixOS service module; submit to nixpkgs or maintain a project flake. | CI can run `nix flake check` and `nix build`; update automation can refresh source hashes. Medium effort: reproducible dependencies and NixOS integration. | Build in the Nix sandbox, run the binary, and test any service module on supported Linux systems. |
