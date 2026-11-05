# Install, update or remove Klicky

[Back to README](../README.md) · [Troubleshooting](TROUBLESHOOTING.md)

## Windows

**Windows 11 x64.** Download the installer from [Releases](https://github.com/Muneer320/klicky/releases). Before a public Windows preview is attached there, use the latest [Windows build artifact](https://github.com/Muneer320/klicky/actions/workflows/windows-portable.yml), extract its ZIP, and open the file ending in `-setup.exe`.

Follow the wizard, choose your login-startup preference, then open Klicky from Start. Choose **Resume** from the tray menu to hear sounds. Rust, Visual Studio and separate sound downloads are not needed. Windows downloads currently use unsigned preview builds.

<details>
<summary><strong>Portable ZIP</strong></summary>

Extract the entire Klicky ZIP into a folder you intend to keep. Open `klicky-desktop.exe`, then use its tray menu. Keep `sounds`, both executables and notices together. If login startup is enabled, leave the folder at the same location.

</details>

<details>
<summary><strong>PowerShell installer for published Windows downloads</strong></summary>

```powershell
irm https://raw.githubusercontent.com/Muneer320/klicky/master/scripts/install.ps1 | iex
```

This selects a published Windows installer from the project's GitHub releases, verifies its SHA-256 file and opens the wizard. If none has been published, it stops with the current download guide. It does not download an arbitrary CI build or change Windows security settings.

Prefer to read it first? [Download the script](https://raw.githubusercontent.com/Muneer320/klicky/master/scripts/install.ps1), inspect it and run it in PowerShell.

</details>

[Windows controls, paths and developer builds](WINDOWS.md)

## macOS

The current installer builds from source, then installs the app and menu bar control for your user. Install [Rust 1.87+](https://rustup.rs/) and the Xcode Command Line Tools first:

```bash
xcode-select --install
```

After those are ready:

```bash
curl -fsSL https://raw.githubusercontent.com/Muneer320/klicky/master/scripts/install.sh | bash
```

Allow `~/.local/bin/klicky` in **System Settings → Privacy & Security → Accessibility**, then run `~/.local/bin/klicky service start`. The app and helper open at future logins. This does not need `sudo`.

<details>
<summary><strong>Build from an existing checkout, or skip the menu bar helper</strong></summary>

```bash
git clone https://github.com/Muneer320/klicky.git
cd klicky
./scripts/install-macos.sh
./scripts/install-macos-menu-bar.sh
```

The second script is optional. For the download installer without the helper, save `install.sh` and run `bash install.sh --no-menu`.

The installed binary is `~/.local/bin/klicky`; sound packs are in `~/Library/Application Support/klicky/sounds`. Grant Accessibility to the installed binary, rather than the copy under `target/release`.

</details>

## Linux

On Ubuntu 24.04+ / compatible Debian **amd64**, or Arch / Omarchy **x86_64**:

```bash
curl -fsSL https://raw.githubusercontent.com/Muneer320/klicky/master/scripts/install.sh | bash
```

APT systems need `curl` and GnuPG. The installer verifies Klicky's repository key, uses a dedicated `Signed-By` source and installs the package. Arch uses the published package and verifies its release checksum. Both enable the systemd user service and include all ten packs. Run the command as your desktop user, without prefixing it with `sudo`.

<details>
<summary><strong>Install a package manually</strong></summary>

Download the matching package from [v0.4.0](https://github.com/Muneer320/klicky/releases/tag/v0.4.0). From its download folder:

```bash
# Ubuntu / Debian
sudo apt install ./klicky_0.4.0-1_amd64.deb

# Arch / Omarchy
sudo pacman -U ./klicky-0.4.0-1-x86_64.pkg.tar.zst
```

Use only the command for your distribution. Then start Klicky now and at login:

```bash
systemctl --user enable --now klicky.service
```

The Debian package requires glibc 2.39+, `libasound2t64`, systemd and udev. For repository updates, see [APT setup](APT_REPOSITORY.md).

</details>

<details>
<summary><strong>Moving from a source installation to a package</strong></summary>

A user-local binary or service can override the packaged one. From the old checkout, run:

```bash
./scripts/uninstall-linux.sh
```

Leave off `--purge` to keep settings and custom packs, then install the package. Check `command -v klicky` if your shell still finds the old copy.

</details>

### Linux from source

Use this on Fedora, older distributions, other architectures, or when developing Klicky. Install Rust 1.87+, Git and your distribution's ALSA development package:

```bash
# Ubuntu / Debian
sudo apt install git pkg-config libasound2-dev

# Arch / Omarchy
sudo pacman -S --needed git alsa-lib pkgconf

# Fedora
sudo dnf install git alsa-lib-devel pkgconf-pkg-config
```

Use only your distribution's command. Then:

```bash
git clone https://github.com/Muneer320/klicky.git
cd klicky
./scripts/install-linux.sh
```

The script builds and installs the binary, packs, keyboard-only udev rule and systemd user service. If you already saved the download installer, `bash install.sh --source` uses the same source path. Add the optional [Omarchy toggle](OMARCHY.md) afterward if wanted.

<details>
<summary><strong>Manual Linux setup or a system without systemd</strong></summary>

From a checkout:

```bash
cargo build --release
install -Dm755 target/release/klicky "$HOME/.local/bin/klicky"
mkdir -p "${XDG_CONFIG_HOME:-$HOME/.config}/klicky/sounds"
cp -a sounds/. "${XDG_CONFIG_HOME:-$HOME/.config}/klicky/sounds/"
sudo install -Dm644 packaging/udev/70-klicky.rules /etc/udev/rules.d/70-klicky.rules
sudo udevadm control --reload-rules
sudo udevadm trigger --subsystem-match=input
```

With systemd, also install and enable the user unit:

```bash
install -Dm644 packaging/systemd/klicky.service "${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user/klicky.service"
systemctl --user daemon-reload
systemctl --user enable --now klicky.service
```

Without systemd/udev, provide keyboard-device access and use your session's startup mechanism to run `~/.local/bin/klicky start`. `KLICKY_SKIP_UDEV=1` skips rule installation in the source installer when access is already managed elsewhere. `KLICKY_SYSTEM_SOUNDS_DIR` supplies an additional pack root.

</details>

## Updates and removal

Choose the method you used to install. Settings and custom sounds are kept unless you explicitly purge them.

<details>
<summary><strong>Windows</strong></summary>

Run a newer installer to update. It stops the installed app, updates the files and restores its previous running/pause choice. Remove it through **Settings → Apps → Installed apps → Klicky**.

For a portable copy, remove its login startup from the tray, choose Quit and delete the bundle. Do not replace files while it is running. See [Windows recovery](WINDOWS.md#installation-and-recovery) if an update fails.

</details>

<details>
<summary><strong>Linux packages</strong></summary>

Stop Klicky before updating its package, then start it afterward:

```bash
systemctl --user stop klicky.service
# Update through APT, or install the newer downloaded package.
sudo apt update && sudo apt install --only-upgrade klicky  # APT repository
systemctl --user start klicky.service
```

For a downloaded Arch or Debian package, replace the APT command with the corresponding package-install command above. Other signed-in users must stop their packaged Klicky processes too; the package refuses to replace a running binary. If an update fails, resolve the reported error and restart the service when ready.

To remove:

```bash
systemctl --user disable --now klicky.service
# Ubuntu / Debian
sudo apt remove klicky
# Arch / Omarchy
sudo pacman -R klicky
```

Use only your package manager. [Remove the APT source](APT_REPOSITORY.md) if it is no longer wanted. The optional Omarchy toggle has [separate removal steps](OMARCHY.md).

</details>

<details>
<summary><strong>Linux source install</strong></summary>

Update your checkout and rerun `./scripts/install-linux.sh`. Remove with `./scripts/uninstall-linux.sh`. Add `--purge` only when you also want to delete its user configuration and sounds.

</details>

<details>
<summary><strong>macOS source install</strong></summary>

Rerun the download installer, or update the checkout and rerun both macOS installers. If input stops after replacing the binary, check Accessibility permission for the installed copy.

To remove the app and optional helper while keeping settings and sounds:

```bash
~/.local/bin/klicky service disable
launchctl bootout "gui/$(id -u)/dev.klicky.menu" 2>/dev/null || true
rm -f "$HOME/Library/LaunchAgents/dev.klicky.menu.plist" \
      "$HOME/.local/bin/klicky-menu" \
      "$HOME/.local/bin/klicky"
```

You may remove its Accessibility entry afterward. Settings remain in `~/Library/Application Support/klicky`.

</details>
