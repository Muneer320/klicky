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

Status is based on repository contents and the live APT endpoint checked on 2026-10-10. Except for APT, registry publication has not been independently verified. A manifest, build, or proposed install command is not proof that a package is available. All targets below are goals, not current installation instructions.

| #   | Manager                 | Status and eligibility                                                                                                                                                                                                              | Target command                         | Packaging, publication, automation, maintenance, and validation                                                                                                                                                                                                                                                                                                                                                                                                   |
| --- | ----------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1   | **Arch AUR**            | `packaging/arch/PKGBUILD` and `.SRCINFO` exist and CI builds/inspects the package. AUR publication is not verified. Eligible after confirming the package name is available and keeping metadata synchronized with a tagged source. | `yay -S klicky`                        | Publish `PKGBUILD` and generated `.SRCINFO` to the AUR Git repository. Automate source version/checksum updates and `.SRCINFO` parity checks; medium maintenance. Validate `makepkg --printsrcinfo`, clean build, `namcap`, install/upgrade/removal, service behavior, and physical keyboard input.                                                                                                                                                               |
| 2   | **APT (Debian/Ubuntu)** | The live signed amd64 repository advertises `0.3.0-1`; the redesigned Pages homepage and history-retention workflow are not yet deployed. Eligible now for continued repository hardening and supported-distribution testing.       | `sudo apt install klicky`              | Keep Debian metadata, maintainer scripts, builder, workflow, and signing-key operations aligned. Recover all authenticated versions and retain packages when publishing. Automate signed artifact validation; medium maintenance for key custody, history, and distro dependencies. Test signature/fingerprint, fresh installation, upgrades from every retained version, package indexes/hashes, service setup, and removal on supported Debian/Ubuntu versions. |
| 3   | **Homebrew Tap**        | No formula or tap publication is verified. Klicky has macOS support; a formula and release route still need implementation and testing.                                                                                             | `brew install Muneer320/tap/klicky`    | Create and maintain `Formula/klicky.rb` in the owner's tap, with source tag, checksum, dependencies, and macOS build/install steps. CI can update the formula and open a tap PR; medium-high maintenance for macOS dependencies and version updates. Validate `brew audit --strict --online`, `brew test`, and install/upgrade/uninstall on supported macOS architectures.                                                                                        |
| 4   | **Homebrew Core**       | Not submitted or published. Consider only after the tap formula is reliable and the project meets current Homebrew Core acceptance and maintenance requirements.                                                                    | `brew install klicky`                  | Submit an upstream formula PR; follow Homebrew's review and policy rather than duplicating tap-specific automation. High ongoing review/compatibility expectations. Validate the formula with Homebrew audit/test and installation lifecycle checks on supported macOS systems.                                                                                                                                                                                   |
| 5   | **Fedora COPR / RPM**   | No RPM spec or COPR package is present or verified. Eligible after creating a correct RPM package and testing supported Fedora releases.                                                                                            | `sudo dnf install klicky`              | Add an RPM `.spec` for binary, dependencies, systemd user service, udev rule, sounds, docs, and license. Build/publish through Fedora COPR; automate per-release builds after review. Medium-high maintenance across Fedora versions. Validate `rpmlint`, clean builds, install/upgrade/removal, service, udev, and input/audio behavior.                                                                                                                         |
| 6   | **openSUSE / OBS**      | No OBS project or RPM build configuration is present or verified. Eligible after RPM packaging works and openSUSE-specific dependencies/policies are checked.                                                                       | `sudo zypper install klicky`           | Add OBS project metadata and distribution-specific spec adjustments; configure OBS builds and publish only after successful target builds. Medium-high maintenance across Leap/Tumbleweed targets. Validate clean OBS builds, package lint, installation/upgrade/removal, service, udev, and runtime behavior on each supported target.                                                                                                                           |
| 7   | **Nixpkgs**             | No Nix derivation, flake, or nixpkgs submission is present or verified. Eligible after a reproducible build and package/service design are established.                                                                             | `nix profile install nixpkgs#klicky`   | Add a derivation with source hash, Rust build inputs, metadata, and optionally a NixOS service module; submit upstream if appropriate, otherwise maintain a project flake. Automate `nix flake check` and build checks. Medium maintenance for reproducibility and upstream review. Validate sandboxed builds, runtime behavior, and any service module.                                                                                                          |
| 8   | **Snap Store**          | No `snapcraft.yaml`, Snap, or store listing is present or verified. Evaluate whether confinement can safely support global keyboard input, audio, services, and device permissions before committing to this route.                 | `sudo snap install klicky`             | If suitable, add Snapcraft metadata and confinement declarations, then use release automation for review-channel uploads and promote only after testing. High maintenance if required interfaces/confinement differ by distro. Validate Snapcraft builds, confinement, refresh/rollback, install/removal, and real input/audio behavior.                                                                                                                          |
| 9   | **Flathub / Flatpak**   | No Flatpak manifest or Flathub listing is present or verified. Evaluate desktop integration and whether Flatpak permissions can support the global input listener; do not assume suitability. App ID is not established.            | `flatpak install flathub <APP_ID>`     | If suitable, add a Flatpak manifest and submit it to Flathub under an agreed reverse-DNS app ID. Automate manifest builds and checks, but keep publication/review gated. Medium-high maintenance for permissions, portals, and desktop integration. Validate sandbox permissions, install/update/removal, and actual keyboard/audio behavior.                                                                                                                     |
| 10  | **Cargo / crates.io**   | No crates.io publication is verified. Evaluate whether `cargo install klicky` is useful given native system dependencies, sound assets, and service/udev setup.                                                                     | `cargo install klicky`                 | If useful, verify crate naming/metadata, included assets, build dependencies, and install documentation before publishing a crate. CI can package-test the crate; medium maintenance for Rust compatibility and crate contents. Validate `cargo package --list`, isolated `cargo install`, executable behavior, and clearly document what installation does not configure.                                                                                        |
| 11  | **WinGet**              | No Windows compatibility or Windows release artifact is available or verified. This is conditional on implementing and validating actual Windows support and producing a supported installer/release artifact.                      | `winget install --id Muneer320.Klicky` | Only after that eligibility gate, add versioned WinGet manifests with installer URLs and hashes and submit to `microsoft/winget-pkgs`. Automate manifest PRs from verified Windows releases. High effort including runtime support. Test manifest validation and clean install/upgrade/uninstall on supported Windows versions.                                                                                                                                   |
| 12  | **Scoop**               | No Windows support, Scoop manifest, or Windows artifact is available or verified. Conditional on the same Windows compatibility and release-artifact gate.                                                                          | `scoop install klicky`                 | After eligibility, add a bucket manifest with verified URLs/hashes and install/uninstall behavior; automate updates from Windows releases. Medium-high maintenance for portable layout, shims, and updates. Validate Scoop install/update/uninstall and runtime support on clean Windows systems.                                                                                                                                                                 |
| 13  | **Chocolatey**          | No Windows support, `.nuspec`, package scripts, or Windows artifact is available or verified. Conditional on validated Windows support and release artifacts.                                                                       | `choco install klicky`                 | After eligibility, add a `.nuspec` and PowerShell install/uninstall scripts and submit to the Chocolatey Community Repository. Automate package build/tests; gate publication on package review and credentials. High maintenance for installer lifecycle and Windows compatibility. Test package verification plus clean install/upgrade/uninstall and runtime behavior on Windows.                                                                              |

### Proposed milestones

- **M1: Arch + APT**: verify the AUR entry and package lifecycle; harden the signed APT workflow and test fresh installs and upgrades while retaining history.
- **M2: macOS**: publish and maintain the Homebrew Tap formula, then evaluate and submit to Homebrew Core if eligible.
- **M3: Linux expansion**: add Fedora COPR/RPM, openSUSE/OBS, and Nixpkgs packaging with distribution-specific validation.
- **M4: Universal distribution**: evaluate Snap and Flatpak suitability, especially confinement, global input, and desktop integration.
- **M5: Rust ecosystem**: evaluate whether a crates.io package is useful and complete; publish only if crate contents and installation expectations are sound.
- **M6: Windows compatibility and release artifacts**: implement and validate Windows support and produce supported release artifacts before planning Windows package publication.
- **M7: Windows distribution**: after M6, package for WinGet, Scoop, and Chocolatey and validate each install lifecycle.
