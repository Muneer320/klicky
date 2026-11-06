# Install, update or remove Klicky

[Back to README](../README.md) · [Visual field guide](https://klicky.muneer320.tech/guide.html#installation) · [Troubleshooting](TROUBLESHOOTING.md)

## Windows

**Windows 11 x64.** Run in PowerShell:

```powershell
irm https://klicky.muneer320.tech/install.ps1 | iex
```

The helper downloads and verifies the published installer, opens setup and starts the installed app. For a file download instead, use the [Windows installer](https://github.com/Muneer320/klicky/releases/download/v0.5.0/klicky-0.5.0-windows-x64-setup.exe).

Follow the wizard, choose your login-startup preference, then open Klicky from Start. Choose **Resume** from the tray menu to hear sounds. Rust, Visual Studio and separate sound downloads are not needed. The Windows installer and executables are unsigned.

<details>
<summary><strong>Portable ZIP</strong></summary>

Extract the entire Klicky ZIP into a folder you intend to keep. Open `klicky-desktop.exe`, then use its tray menu. Keep `sounds`, both executables and notices together. If login startup is enabled, leave the folder at the same location.

</details>

<details>
<summary><strong>PowerShell installer for published Windows downloads</strong></summary>

```powershell
irm https://klicky.muneer320.tech/install.ps1 | iex
```

This selects a published Windows installer from the project's GitHub releases, verifies its SHA-256 file and opens the wizard. It uses published assets and does not change Windows security settings. When setup finishes, the helper starts the installed app automatically; no second command is needed. A paused launch selected in the wizard is respected.

Prefer to read it first? [Download the script](https://klicky.muneer320.tech/install.ps1), inspect it and run it in PowerShell.

</details>

[Windows controls, paths and developer builds](WINDOWS.md)

### Scoop

With [Scoop](https://scoop.sh/) installed, run in your normal PowerShell window:

```powershell
scoop bucket add klicky https://github.com/Muneer320/scoop-bucket
scoop install klicky/klicky
klicky service start
```

This installs the complete portable app, adds a Start shortcut and exposes `klicky` on PATH. Enable login startup with `klicky service enable`; installation alone does not start monitoring. Use one installation method: uninstall an existing EXE/WinGet copy first, preserving settings.

For updates, run `klicky service stop`, `scoop update`, `scoop update klicky`, then `klicky service start`. Scoop checks running apps before update hooks, so stopping first is required. [Bucket and lifecycle details](https://github.com/Muneer320/scoop-bucket)

**WinGet:** [submission under review](https://github.com/microsoft/winget-pkgs/pull/473333). Use the installer or Scoop until it is accepted into the public catalogue.

## macOS

Run in Terminal:

```bash
curl -fsSL https://klicky.muneer320.tech/install.sh | bash
```

Setup downloads the source, prepares missing Rust and Apple build tools, installs Klicky and its menu bar control, and starts them. It enables login startup too. Approve Apple's build-tools dialog if shown; the installer waits for it to finish. First setup can take several minutes. Apple may request administrator approval for its tools; Klicky itself is installed for your user.

macOS requires you to allow `~/.local/bin/klicky` in **System Settings → Privacy & Security → Accessibility**. If startup fails, the installer opens that pane and shows the installed file in Finder. Add it with **+**, leave Terminal open, and setup retries for up to ten minutes. If an error persists after permission is granted, consult [troubleshooting](TROUBLESHOOTING.md#macos); setup reports the error rather than claiming success.

To skip the menu bar helper:

```bash
curl -fsSL https://klicky.muneer320.tech/install.sh | bash -s -- --no-menu
```

<details>
<summary><strong>Build from an existing checkout, or skip the menu bar helper</strong></summary>

```bash
git clone https://github.com/Muneer320/klicky.git
cd klicky
./scripts/install-macos.sh
./scripts/install-macos-menu-bar.sh
```

The second script is optional. Existing-checkout builds need Rust 1.87+ and the Xcode Command Line Tools; the download installer prepares missing tools for you.

The installed binary is `~/.local/bin/klicky`; sound packs are in `~/Library/Application Support/klicky/sounds`. Grant Accessibility to the installed binary, rather than the copy under `target/release`.

</details>

### Homebrew

On **Apple Silicon**, with [Homebrew](https://brew.sh/) and Apple's Command Line Tools installed:

```bash
brew install muneer320/tap/klicky && brew services start muneer320/tap/klicky && klicky service enable
```

The project tap builds from source and includes all packs. **Intel Macs use the one-line installer above**: Klicky still supports Intel, while Homebrew has [ended supported Intel coverage and new Intel bottles](https://docs.brew.sh/Support-Tiers#future-macos-support). Homebrew manages the menu helper; Klicky's `service` commands manage the sound daemon. Omit `brew services start muneer320/tap/klicky` to skip the menu.

Allow the **actual executable shown by `brew info klicky`** in Accessibility, then run `klicky service start`. This differs from the source-install path. Remove a previous source installation without purge before switching; do not run both menu helpers.

Update:

```bash
klicky service stop && brew update && brew upgrade muneer320/tap/klicky && brew services restart muneer320/tap/klicky && klicky service start
```

If you opted out of the menu, omit `brew services restart muneer320/tap/klicky` from that line. Settings survive updates; macOS may request Accessibility approval again after a rebuild. [Tap and maintenance details](https://github.com/Muneer320/homebrew-tap)

## Linux

On Ubuntu 24.04+ / compatible Debian **amd64**, or Arch / Omarchy **x86_64**:

```bash
curl -fsSL https://klicky.muneer320.tech/install.sh | bash
```

Use a terminal with `curl`. APT setup installs GnuPG if needed. The installer verifies Klicky's repository key, uses a dedicated `Signed-By` source and installs the package. Arch uses the published package and verifies its release checksum. Both enable and start the systemd user service and include all ten packs. Compatible Omarchy shells also get the bar toggle. Run the command as your desktop user, without prefixing it with `sudo`.

To skip the Omarchy toggle:

```bash
curl -fsSL https://klicky.muneer320.tech/install.sh | bash -s -- --no-omarchy-toggle
```

The toggle needs Omarchy's QML shell and its `shell.json`. Older or different shell setups need their own integration; the installer reports missing requirements without replacing an unrelated bar configuration.

<details>
<summary><strong>Install a package manually</strong></summary>

Copy the command for your distribution. These download the package directly, without opening a release page:

```bash
# Ubuntu / Debian
curl -fL --retry 2 -o klicky.deb \
  https://klicky.muneer320.tech/pool/main/k/klicky/klicky_0.5.0-1_amd64.deb
sudo apt install ./klicky.deb

# Arch / Omarchy
curl -fL --retry 2 -o klicky.pkg.tar.zst \
  https://github.com/Muneer320/klicky/releases/download/v0.5.0/klicky-0.5.0-1-x86_64.pkg.tar.zst
sudo pacman -U ./klicky.pkg.tar.zst
```

The one-command installer above handles repository-key or checksum verification for you. These manual downloads use HTTPS; signed APT is recommended for ongoing Debian/Ubuntu updates.

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

The script builds and installs the binary, packs, keyboard-only udev rule and systemd user service. For an automatic source build, use `curl -fsSL https://klicky.muneer320.tech/install.sh | bash -s -- --source`. It prepares missing Rust and supported distribution build dependencies, then starts the service. The [Omarchy toggle](OMARCHY.md) is included on compatible Omarchy setups unless skipped.

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

## Uninstall

Choose your system. The removal command stops Klicky, disables its login startup and removes the app plus its installed menu/tray/bar controls. **Settings and custom sound packs are kept by default.**

Installed through Homebrew or Scoop? Use [package-manager removal](#package-manager-removal) instead of the download helpers.

<details>
<summary><strong>Linux or macOS</strong></summary>

```bash
curl -fsSL https://klicky.muneer320.tech/uninstall.sh | bash
```

To also delete Klicky's settings and custom sounds:

```bash
curl -fsSL https://klicky.muneer320.tech/uninstall.sh | bash -s -- --purge
```

Linux removal handles the APT/Arch package or source binary, user service, Klicky's device rule and APT source/key. On Omarchy it removes only the Klicky bar entry/module, keeping the rest of your bar configuration. Other signed-in users must stop their packaged copies before package removal.

macOS removal unloads the daemon and menu helper, removes their LaunchAgents and binaries, and removes the installed menu images. Apple Accessibility permission can be removed in System Settings afterward. General-purpose Rust and Apple build tools stay installed.

</details>

<details>
<summary><strong>Windows installer setup</strong></summary>

```powershell
irm https://klicky.muneer320.tech/uninstall.ps1 | iex
```

This opens the registered Klicky uninstaller, which stops the app and removes its installed tray app, Start shortcut and owned login shortcut. Complete the wizard; no second command is needed.

To also remove settings, custom sounds and Klicky installer data:

```powershell
& ([scriptblock]::Create((irm https://klicky.muneer320.tech/uninstall.ps1))) -Purge
```

Use `-WhatIf` in place of `-Purge` to preview without changing anything. If removal is cancelled or fails, personal data is kept. Close remaining portable copies before purging; linked/junction data folders are reported for inspection rather than followed.

Windows **Settings → Apps → Installed apps → Klicky → Uninstall** remains available as well.

</details>

<details>
<summary><strong>Windows portable copy</strong></summary>

In the portable app's tray menu, turn off its login startup and choose Quit, then delete the extracted bundle. Keep unrelated files in the parent folder. The installer-removal helper does not search your drives or delete development/portable folders it cannot identify safely. After closing all copies, its `-Purge` command above can remove the shared Klicky settings too.

</details>

### Package-manager removal

**Homebrew:** run the full line before removing the formula, so both daemon and menu are stopped and unregistered:

```bash
klicky service disable && brew services stop muneer320/tap/klicky && brew uninstall muneer320/tap/klicky
```

Plain `brew uninstall` cannot clean Klicky's independently managed daemon registration. Settings/custom packs remain in `~/Library/Application Support/klicky`.

**Scoop:** removal stops the owned bundle and removes its owned login shortcut:

```powershell
scoop uninstall klicky
```

Settings/custom packs remain in `%LOCALAPPDATA%\Klicky`. Scoop's `--purge` removes its integration state only, not that shared folder. After removing every Klicky installation, delete the settings folder yourself if you also want to erase preferences and custom packs.

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
