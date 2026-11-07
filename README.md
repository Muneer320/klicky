# Klicky

**Mechanical keyboard sounds for Linux, macOS and Windows 11.** Give the keyboard you already own a sound you enjoy, whether you're writing code, taking notes or chatting with friends.

<p align="center"><img src="assets/branding/social-preview.jpg" width="1280" height="640" alt="Klicky enjoying the sounds beneath a violet keycap."></p>

<p align="center"><strong>A little sound for every keystroke.</strong><br><a href="https://klicky.muneer320.tech/#sounds">Listen to the packs</a> · <a href="#install">Install</a> · <a href="#everyday-use">Everyday use</a> · <a href="#help-and-faq">Help</a> · <a href="https://klicky.muneer320.tech/guide.html">Full guide</a></p>

<p align="center"><a href="https://github.com/Muneer320/klicky/actions/workflows/ci.yml"><img src="https://github.com/Muneer320/klicky/actions/workflows/ci.yml/badge.svg" alt="CI status"></a> <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-7435b4" alt="MIT license"></a> <a href="docs/SOUND_PACKS.md"><img src="https://img.shields.io/badge/sound_packs-10-c084fc" alt="10 included sound packs"></a></p>

There's someone under your keys. **Klicky**, our curious little mascot, loves a good typing rhythm and settles down when you do. [Meet Klicky.](docs/CHARACTER.md)

The app runs quietly in the background. Pick a pack, set the volume and carry on typing.

- **Find your sound.** Ten included packs: Cherry MX Black, Blue, Brown and Red in PBT/ABS variants, plus EG Oreo and EG Crystal Purple. You can add your own recordings too.
- **Control it from your desktop.** Windows tray, macOS menu bar, or Linux terminal with an optional Omarchy bar toggle.
- **Keep it local.** No account or subscription. Normal playback stores no typed text and makes no network requests.
- **Built in Rust for responsive playback.** Sounds are decoded when a pack loads and reused while you type.

## Install

**Prefer a package manager?** Klicky is available through these routes:

| Your system | Install with | Setup |
|---|---|---|
| Ubuntu / compatible Debian | **APT** | [Add Klicky's signed repository](docs/APT_REPOSITORY.md) |
| macOS · Apple Silicon | **Homebrew** | [Install from Klicky's tap](docs/INSTALLATION.md#homebrew) |
| Windows 11 · x64 | **Scoop** | [Install from Klicky's bucket](docs/INSTALLATION.md#scoop) |
| Arch / Omarchy · x86_64 | **pacman** | [Download and install the release package](docs/INSTALLATION.md#linux) |

The Arch route uses `pacman -U` with a downloaded package, not an AUR or `pacman -S` listing. Each setup guide includes the required repository or bucket and startup steps.

**Open your system below and copy the command.** The setup helpers install the app and sounds, then start Klicky. Follow any setup or permission prompts. For downloads and more methods, use the [installation picker](https://klicky.muneer320.tech/#install).

<details>
<summary><strong>Linux · Ubuntu / compatible Debian, Arch / Omarchy</strong></summary>

Run in a terminal as your normal desktop user:

```bash
curl -fsSL https://klicky.muneer320.tech/install.sh | bash
```

Uses signed APT packages on **Ubuntu 24.04+ / compatible Debian amd64**, or the release package on **Arch / Omarchy x86_64**. Setup asks for sudo when needed and enables login startup. Have `curl` installed first.

Compatible Omarchy setups include the bar toggle. To skip it, use `bash -s -- --no-omarchy-toggle` at the end of the command. For Fedora, older distributions or other architectures, build from source:

```bash
curl -fsSL https://klicky.muneer320.tech/install.sh | bash -s -- --source
```

[All Linux methods and prerequisites](docs/INSTALLATION.md#linux) · [Signed APT setup](docs/APT_REPOSITORY.md) · [Omarchy integration](docs/OMARCHY.md)

</details>

<details>
<summary><strong>macOS · Apple Silicon and Intel</strong></summary>

Run in Terminal:

```bash
curl -fsSL https://klicky.muneer320.tech/install.sh | bash
```

Builds Klicky from source, prepares missing Rust and Apple build tools, includes the menu bar control and enables login startup. First setup can take several minutes; approve Apple's tools dialog if shown.

Allow **`~/.local/bin/klicky`** in **System Settings → Privacy & Security → Accessibility**. Setup opens the pane and retries while you approve it. To skip the menu helper, use `bash -s -- --no-menu` at the end of the command.

**Already use Homebrew on Apple Silicon?** With Homebrew and Apple's build tools installed:

```bash
brew install muneer320/tap/klicky && brew services start muneer320/tap/klicky && klicky service enable
```

This also builds from source. Grant Accessibility to the executable shown by `brew info klicky`, then run `klicky service start`. Use one installation method; Intel Macs use the installer above.

[macOS setup](docs/INSTALLATION.md#macos) · [Homebrew setup, updates and removal](docs/INSTALLATION.md#homebrew)

</details>

<details>
<summary><strong>Windows 11 · x64</strong></summary>

Run in PowerShell:

```powershell
irm https://klicky.muneer320.tech/install.ps1 | iex
```

Downloads the installer, verifies its SHA-256 and opens setup for your user. Choose whether to start at login; the helper starts Klicky after setup. No Rust, Visual Studio or administrator terminal needed.

Prefer to click a download? Get the [Windows installer](https://github.com/Muneer320/klicky/releases/download/v0.5.0/klicky-0.5.0-windows-x64-setup.exe) or [portable ZIP](https://github.com/Muneer320/klicky/releases/download/v0.5.0/klicky-0.5.0-windows-x64.zip). For the ZIP, extract the whole folder and open `klicky-desktop.exe`.

Already use Scoop? Follow the [Scoop setup guide](docs/INSTALLATION.md#scoop).

[Windows controls and troubleshooting](docs/WINDOWS.md) · [All Windows installation methods](docs/INSTALLATION.md#windows)

</details>

> [!NOTE]
> Windows downloads are unsigned, so SmartScreen may show an unknown-publisher warning. If Windows Security reports a threat, stop setup and follow the [security-block guidance](docs/TROUBLESHOOTING.md#windows).

Want to read the helpers first? [Linux/macOS script](https://klicky.muneer320.tech/install.sh) · [Windows script](https://klicky.muneer320.tech/install.ps1)

## Everyday use

Once setup finishes, type in any ordinary app to hear Klicky. Here's where to find the controls:

| System | Where to look |
|---|---|
| **Windows** | Tray menu: choose a pack, change volume, Pause/Resume, retry audio or choose login startup. |
| **macOS** | Menu bar: toggle sounds. Terminal: choose a pack and set volume. |
| **Linux** | Terminal controls below; supported Omarchy setups also have a bar toggle. |

On Linux and macOS, try:

```bash
klicky list                       # List sound packs
klicky switch cherrymx-blue-pbt    # Choose a pack
klicky volume 0.5                 # Set volume to 50% (range: 0 to 1)
klicky status                     # Check current state
```

Source installations use `~/.local/bin/klicky` if your shell doesn't find `klicky`. Windows supports the same commands; Scoop adds them to PATH. With the default Windows installer location, use:

```powershell
& "$env:LOCALAPPDATA\Programs\Klicky\klicky.exe" status
```

<details>
<summary><strong>Start, stop and choose login startup</strong></summary>

```bash
klicky service start     # Start this session
klicky service stop      # Stop now; keep login startup
klicky service enable    # Start now and at login
klicky service disable   # Stop now and disable login startup
```

These commands also work through the Windows CLI. The Windows tray has **Start with Windows**; Windows may independently disable or delay startup. Homebrew manages the menu helper separately; see its [service instructions](docs/INSTALLATION.md#homebrew).

[Full CLI reference](docs/CLI.md)

</details>

Want to customize further? Use the [configuration guide](docs/CONFIGURATION.md) for settings and paths, or [make your own sound pack](docs/SOUND_PACKS.md) from OGG audio and a JSON timing map.

## Help and FAQ

<details>
<summary><strong>Do I need a mechanical keyboard? Does Linux work on Wayland?</strong></summary>

Any ordinary keyboard works; the sound is added in software. Linux reads evdev below Wayland/X11, using a keyboard-only device rule rather than separate desktop-specific capture.

</details>

<details>
<summary><strong>Does it record what I type?</strong></summary>

Normal operation stores no typed text, logs no key names and sends no network requests. Global keyboard permission is sensitive, so grant it only to trusted software. Unix benchmark mode prints mapped key names and is for controlled tests. [Privacy and security](SECURITY.md)

</details>

<details>
<summary><strong>No sound, missing sounds or a delay?</strong></summary>

Check that Klicky is running, isn't paused, and has a nonzero volume. On macOS, check Accessibility permission. Try laptop speakers or wired output to compare: Bluetooth can add noticeable delay.

The [troubleshooting guide](docs/TROUBLESHOOTING.md) covers permissions, startup and audio recovery. If sounds still drop out, [report a bug](https://github.com/Muneer320/klicky/issues/new/choose) with your OS, output device and what happened.

</details>

<details>
<summary><strong>How do I update?</strong></summary>

Use the method you installed with. For helper installations, rerun the install command above. For package managers, follow the [Homebrew](docs/INSTALLATION.md#homebrew) or [Scoop](docs/INSTALLATION.md#scoop) update steps, including stopping Klicky first. Settings and custom sounds are preserved.

</details>

<details>
<summary><strong>How do I uninstall, including desktop controls?</strong></summary>

For installations made with the download helpers:

**Linux/macOS**

```bash
curl -fsSL https://klicky.muneer320.tech/uninstall.sh | bash
```

**Windows installer**

```powershell
irm https://klicky.muneer320.tech/uninstall.ps1 | iex
```

These stop Klicky and remove its installed desktop controls. Settings and custom packs stay unless you choose [purge removal](docs/INSTALLATION.md#uninstall).

Installed with Homebrew or Scoop? Use [package-manager removal](docs/INSTALLATION.md#package-manager-removal) instead. Portable Windows copies have [separate removal steps](docs/INSTALLATION.md#uninstall).

</details>

## Explore the project

Klicky uses native keyboard input on each OS and prepares audio before you type. Linux/macOS use a resident daemon; Windows uses a native tray coordinator. They share sound-pack decoding and cached audio. Read the [architecture guide](docs/ARCHITECTURE.md) for the diagram, module map and platform details, or the [performance guide](docs/BENCHMARKS.md) for measurements and methodology.

| I want to… | Start here |
|---|---|
| Install, configure or troubleshoot | [The website field guide](https://klicky.muneer320.tech/guide.html) |
| Look up commands or configuration | [CLI](docs/CLI.md) · [Configuration](docs/CONFIGURATION.md) |
| Add recordings or desktop integration | [Sound packs](docs/SOUND_PACKS.md) · [Omarchy](docs/OMARCHY.md) |
| Understand or contribute code | [Build, test and contribute](CONTRIBUTING.md) · [Open issues](https://github.com/Muneer320/klicky/issues) |
| Explore the character or use artwork | [Klicky's story](docs/CHARACTER.md) · [Brand assets](docs/BRANDING.md) |
| Check releases and project history | [Releases](https://github.com/Muneer320/klicky/releases) · [Changelog](CHANGELOG.md) |

Klicky grew from [Sauhard Gupta's original macOS project](https://github.com/Sauhard74/klicky). After seeing it on a Mac, [Muneer Alam](https://muneer320.tech/) wanted it on his Omarchy machine, adapted it for Linux, and kept going until Windows joined in. [Read the development story.](https://muneer320.tech/projects/klicky/)

[MIT license](LICENSE) · [Acknowledgements and third-party credits](ACKNOWLEDGEMENTS.md). Third-party notices are included with packaged downloads.
