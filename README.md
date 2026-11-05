# Klicky

<img src="assets/branding/klicky.svg" width="88" height="88" alt="Klicky keycap and sound mark">

**Mechanical keyboard sounds for Windows, macOS and Linux.**

Give everyday typing a little more character. Klicky plays a sound when you press a key, with **10 included sound packs**, adjustable volume and optional desktop controls. Any keyboard will do.

[Install](#install) · [Choose your sound](#choose-your-sound) · [FAQ](#faq) · [Get help](#get-help) · [Contribute](CONTRIBUTING.md)

[![CI](https://github.com/Muneer320/klicky/actions/workflows/ci.yml/badge.svg)](https://github.com/Muneer320/klicky/actions/workflows/ci.yml)
[![MIT license](https://img.shields.io/badge/license-MIT-blue)](LICENSE)

## Install

Choose your operating system below. The app runs locally, without an account or subscription.

<details>
<summary><strong>Windows 11 · x64</strong></summary>

1. Download the **Windows installer** from [Releases](https://github.com/Muneer320/klicky/releases). Until a public preview is attached there, get the latest **klicky-windows-x64-unsigned-preview** from [Windows build artifacts](https://github.com/Muneer320/klicky/actions/workflows/windows-portable.yml) and extract the downloaded ZIP.
2. Run the file ending in **`-setup.exe`**. Choose whether Klicky should start when you sign in.
3. Open Klicky from Start and choose **Resume** from its tray menu. The same menu changes sounds and volume.

The installer includes the app and all sound packs. **You do not need Rust or Visual Studio.** Windows downloads currently use unsigned preview builds. A portable ZIP is available too; extract the entire bundle and open `klicky-desktop.exe`.

[Windows guide](docs/WINDOWS.md) · [Installation options](docs/INSTALLATION.md#windows)

</details>

<details>
<summary><strong>macOS</strong></summary>

Run in Terminal:

```bash
curl -fsSL https://klicky.muneer320.tech/install.sh | bash
```

It downloads, builds, installs and starts Klicky with its menu bar control, and enables login startup. Missing Rust/build tools are set up for you; approve Apple's tools dialog if it appears. First setup can take several minutes.

macOS requires Accessibility permission for **`~/.local/bin/klicky`**. If startup needs that permission, setup opens the settings pane and retries while you approve it. No second Terminal command is needed. To skip the menu bar helper, add `-s -- --no-menu` after `bash`.

Use the menu bar control to turn sounds on or off. [Manual steps and removal](docs/INSTALLATION.md#macos)

</details>

<details>
<summary><strong>Linux · Ubuntu, Debian, Arch, Omarchy and more</strong></summary>

On **Ubuntu 24.04+ / compatible Debian amd64** or **Arch / Omarchy x86_64**, run:

```bash
curl -fsSL https://klicky.muneer320.tech/install.sh | bash
```

The installer chooses the signed APT repository or published Arch package, includes the sound packs, and enables Klicky at login. It asks for `sudo` for package/device setup; Klicky itself runs as your user. Use a terminal with `curl`; setup installs GnuPG if needed. Compatible Omarchy shells get the bar toggle automatically; add `-s -- --no-omarchy-toggle` after `bash` to skip it.

Already set up the APT repository? Use:

```bash
sudo apt install klicky
```

For **Fedora, older distributions or other architectures**, use the [source-install instructions](docs/INSTALLATION.md#linux-from-source). Klicky works below Wayland and X11; the automatic service setup uses systemd and udev.

[Manual package downloads](https://github.com/Muneer320/klicky/releases) · [APT setup](docs/APT_REPOSITORY.md) · [Installation and removal](docs/INSTALLATION.md#linux)

</details>

Prefer to inspect the installer first? Download [install.sh](https://klicky.muneer320.tech/install.sh), read it, then run it with `bash`. Existing settings and custom sound packs are kept when upgrading or removing the app.

## Choose your sound

Windows has sound-pack and volume controls in the tray. On Linux and macOS, run:

```bash
klicky list                       # See your packs
klicky switch cherrymx-blue-pbt   # Pick a sound
klicky volume 0.5                 # Set volume from 0 to 1
klicky status                    # Check the running app
```

If your shell cannot find `klicky`, use `~/.local/bin/klicky`. On Windows, the CLI is in `%LOCALAPPDATA%\Programs\Klicky`.

<details>
<summary><strong>The 10 included packs</strong></summary>

| Switch | PBT keycaps | ABS keycaps |
|---|---|---|
| Cherry MX Black | `cherrymx-black-pbt` | `cherrymx-black-abs` |
| Cherry MX Blue | `cherrymx-blue-pbt` | `cherrymx-blue-abs` |
| Cherry MX Brown | `cherrymx-brown-pbt` | `cherrymx-brown-abs` |
| Cherry MX Red | `cherrymx-red-pbt` | `cherrymx-red-abs` |

Plus **`eg-oreo`** (the default) and **`eg-crystal-purple`**. Want your own sound? See [custom packs](docs/SOUND_PACKS.md).

</details>

## Make it yours

- **Windows:** use the tray menu for sounds, volume, Pause/Resume and login startup.
- **macOS:** use the menu bar toggle; change sounds and volume in Terminal.
- **Linux:** use the CLI, or add the optional [Omarchy bar toggle](docs/OMARCHY.md).

<details>
<summary><strong>Start, stop and change login startup</strong></summary>

On Linux and macOS:

```bash
klicky service start     # Start now
klicky service stop      # Stop now; keep login startup
klicky service enable    # Start now and at login
klicky service disable   # Stop now and disable login startup
```

On Windows, use the tray's **Start with Windows** submenu. The Windows CLI has the same service commands; see the [Windows guide](docs/WINDOWS.md).

</details>

<details>
<summary><strong>Configuration files and custom packs</strong></summary>

| System | Settings folder |
|---|---|
| Windows | `%LOCALAPPDATA%\Klicky` |
| macOS | `~/Library/Application Support/klicky` |
| Linux | `~/.config/klicky` (or `$XDG_CONFIG_HOME/klicky`) |

Settings live in `config.toml`; custom packs go in its neighboring `sounds` folder. A custom pack takes precedence over a bundled pack with the same name. Use the running app's controls for live changes.

```toml
sound_pack = "eg-oreo"
volume = 0.8
```

[Sound pack format](docs/SOUND_PACKS.md) · [Advanced setup](docs/INSTALLATION.md)

</details>

## FAQ

<details>
<summary><strong>Do I need a mechanical keyboard?</strong></summary>

No. Klicky adds the sound in software, so a laptop or ordinary desktop keyboard works too.

</details>

<details>
<summary><strong>Does it record what I type?</strong></summary>

Normal operation does not store typed text, log key names or send network requests. Klicky reads keyboard events locally to trigger the sounds. macOS asks for Accessibility permission; Linux uses a keyboard-only device rule. Advanced Linux/macOS benchmark mode prints mapped key names, so it is for controlled tests. [Privacy and security](SECURITY.md)

</details>

<details>
<summary><strong>Can I use headphones? Why does Bluetooth feel delayed?</strong></summary>

Yes. Klicky plays through your selected audio output. Bluetooth can add its own delay; try laptop speakers or wired headphones if the sound feels out of sync.

</details>

<details>
<summary><strong>Can I pause it or keep it from starting automatically?</strong></summary>

Yes. Use Pause in the Windows tray, the macOS menu toggle, or `klicky service stop` on Linux/macOS. Windows login startup is opt-in. Use `service disable` or the corresponding desktop control to turn startup off. [Controls](#make-it-yours)

</details>

<details>
<summary><strong>How do I update or uninstall?</strong></summary>

Update through the package manager you used, rerun the source installer, or run a newer Windows installer. On Windows, remove Klicky from **Settings → Apps → Installed apps**. Configuration and custom sounds are kept. [All update and removal steps](docs/INSTALLATION.md#updates-and-removal)

</details>

## Get help

<details>
<summary><strong>No sound, missing sounds, startup or permission problems?</strong></summary>

Check the selected output, pack, volume and whether Klicky is paused. Then open the [troubleshooting guide](docs/TROUBLESHOOTING.md) for your system. It includes the commands to check permissions, services and Windows diagnostic counters.

If the problem continues, [open an issue](https://github.com/Muneer320/klicky/issues/new) with your OS, Klicky build, audio output and what happened. You do not need to share what you typed.

</details>

## Under the hood

Klicky prepares sound samples when a pack loads and reuses one persistent mixer while you type. Native input comes from Linux `evdev`, macOS global input and Windows Raw Input. The Windows tray and optional macOS helper provide desktop controls.

[Architecture](docs/ARCHITECTURE.md) · [Performance and measurement](docs/BENCHMARKS.md) · [Contributing](CONTRIBUTING.md) · [Changelog](CHANGELOG.md)

## Credits

Built on [Sauhard Gupta's original Klicky](https://github.com/Sauhard74/klicky), with subsequent work by Muneer Alam and contributors. See [Acknowledgements](ACKNOWLEDGEMENTS.md). Source and bundled sounds use the [MIT license](LICENSE); third-party notices accompany packaged downloads.
