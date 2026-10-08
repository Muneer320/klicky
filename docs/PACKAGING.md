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

Follow the [APT repository guide](APT_REPOSITORY.md) to verify the dedicated signing-key fingerprint and install the Deb822 source with `Signed-By`. Once configured, use `sudo apt update` and `sudo apt install klicky`. This installs a published release package, not the current source tree. The currently published package is `0.3.0-1`; use `systemctl --user` for its service controls.

[APT publication](APT_REPOSITORY.md#publication) is a separate workflow that consumes a release `.deb` and `SHA256SUMS`, signs repository metadata, and deploys public files to GitHub Pages. It accepts a release-published event or manual dispatch. Because the tag-driven release workflow publishes with `GITHUB_TOKEN`, maintainers must explicitly dispatch APT publication after that workflow finishes; its release event does not automatically start another workflow.

Publication requires the configured signing secrets and GitHub Pages deployment permissions. The current workflow publishes only the selected release package; it does not retain previous repository versions. See the guide for prerequisites, publication steps, test coverage, and key rotation.
