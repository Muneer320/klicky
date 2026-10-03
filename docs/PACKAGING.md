# Linux packaging

Klicky publishes native packages for Arch Linux and Debian-compatible distributions.

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

```bash
cargo install cargo-deb --version 3.8.0 --locked
cargo deb
sudo apt install ./target/debian/klicky_*_amd64.deb
systemctl --user enable --now klicky.service
```

Validate a package with:

```bash
lintian --suppress-tags initial-upload-closes-no-bugs \
  target/debian/klicky_*_amd64.deb
```

The suppressed tag requires a real Debian Intent-To-Package bug number and applies only when submitting to Debian itself. Do not fabricate one for GitHub release packages.

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

A downloadable `.deb` can be installed with `apt install ./file.deb`. Supporting `apt install klicky` requires a signed APT repository or acceptance into Debian and Ubuntu repositories.

A future repository must include:

- A dedicated OpenPGP signing key
- Signed `InRelease` metadata
- Versioned package indexes for supported distributions and architectures
- Key rotation and revocation documentation
- Automated installation tests against the published repository

Do not ask users to trust an unsigned repository or pipe remote setup scripts directly into a root shell.
