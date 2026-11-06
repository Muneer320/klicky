# Troubleshooting

[Back to README](../README.md) · [Installation](INSTALLATION.md)

Start with three checks: is Klicky running, is its volume above zero, and is the intended speaker/headphone output selected? Resume it if paused. Headphones and laptop speakers are useful comparisons when diagnosing a route-specific problem.

## Windows

<details>
<summary><strong>No sound or some typing sounds are missing</strong></summary>

Check the tray state, pack and volume. Choose **Retry audio** after changing output devices. If the problem continues, run:

```powershell
& "$env:LOCALAPPDATA\Programs\Klicky\klicky.exe" status
& "$env:LOCALAPPDATA\Programs\Klicky\klicky.exe" diagnostics
```

Diagnostics reads aggregate counters from the running app. Compare reports from the same process before and after the problem; it does not record what you type. Share the build, output route, approximate time and report in an issue if useful. Older previews need an update before they support the command. [Counter meanings](WINDOWS.md#diagnostics)

Firmware-only Fn actions and some vendor media keys may not send a supported keyboard event. Random omissions from ordinary typing are worth reporting even if restarting helps.

</details>

<details>
<summary><strong>Klicky takes a while to appear after signing in</strong></summary>

Windows can defer Startup-folder apps while the system is busy. Open Klicky from Start to use it sooner, then Resume if it opens paused. Check **Start with Windows** in the tray and **Settings → Apps → Startup** if it never appears.

[Microsoft explains startup deferral](https://jpwinsup.github.io/blog/2025/11/13/Performance/SlowResponse/AutoRunDelayOnWin11/). Klicky does not change the system-wide setting for other apps.

</details>

<details>
<summary><strong>Installer or update fails</strong></summary>

Close Klicky in other signed-in sessions and retry. Do not force-replace a running bundle. If the installer retained a recovery folder under `%LOCALAPPDATA%\KlickyInstaller\recovery`, keep it until recovery is complete. [Installation and recovery](WINDOWS.md#installation-and-recovery)

</details>

<details>
<summary><strong>Windows Security blocks Klicky</strong></summary>

Stop setup if Windows Security reports a threat. Keep protection enabled; do not add an exclusion or restore a quarantined file just to finish installation. An unsigned-app reputation warning and an antivirus threat detection require different investigations.

Note the Klicky build and detection name, and contact the maintainer through the [security reporting guide](../SECURITY.md). Keep full protection-history logs private. A suspected incorrect detection can be submitted to [Microsoft for analysis](https://www.microsoft.com/en-us/wdsi/filesubmission); it needs confirmation before being treated as a false positive.

</details>

<details>
<summary><strong>Removal reports a missing registration or control executable</strong></summary>

Use the current [Windows removal helper](https://klicky.muneer320.tech/uninstall.ps1) in PowerShell under the account that installed Klicky. It reads the current user's Windows registration directly. Registry access errors are reported rather than treated as an absent installation. Portable copies are removed separately; the helper does not guess ownership or delete an unregistered application folder.

Older preview uninstallers need `klicky.exe` to stop the app and remove login startup. If it is missing, quit Klicky in every signed-in session and repair with the current installer first. The updated installer and uninstaller can recover an inactive, installer-owned bundle without executing the missing CLI. Edited or foreign startup shortcuts are preserved. Check **Windows Security → Protection history** before repair; keep protection enabled and resolve any detection first. Do not restore a quarantined executable just to uninstall. Settings and custom packs remain untouched after a failed removal.

</details>

## macOS

<details>
<summary><strong>No key sounds</strong></summary>

Allow **`~/.local/bin/klicky`** in **System Settings → Privacy & Security → Accessibility**. Permission for a development copy under `target/release` is separate. After changing permission:

```bash
~/.local/bin/klicky service stop
~/.local/bin/klicky service start
~/.local/bin/klicky status
```

Check pack, volume and the files under `~/Library/Application Support/klicky/sounds`. [Media and function keys](function-keys-sound.md) use a separate input path.

</details>

<details>
<summary><strong>Menu bar control is missing, or the app will not start</strong></summary>

```bash
~/.local/bin/klicky service status
launchctl print "gui/$(id -u)/dev.klicky.daemon"
launchctl print "gui/$(id -u)/dev.klicky.menu"
```

Rerun the menu installer if `~/.local/bin/klicky-menu` or its LaunchAgent is missing. To reopen an installed helper:

```bash
launchctl kickstart -k "gui/$(id -u)/dev.klicky.menu"
```

Quitting the helper closes only the menu; it does not stop the daemon. LaunchAgents throttle repeated failed launches to 30 seconds. To see the underlying error directly:

```bash
~/.local/bin/klicky service stop
~/.local/bin/klicky start
```

Use another Terminal for `~/.local/bin/klicky stop`, then `~/.local/bin/klicky service start` when done.

</details>

## Linux

<details>
<summary><strong>No input or sound</strong></summary>

```bash
klicky status
klicky list
systemctl --user status klicky.service
journalctl --user -u klicky.service -n 50 --no-pager
```

For input permission problems, confirm the keyboard-only udev rule was installed, then log out and back in to refresh access. Newly connected keyboards may need `systemctl --user restart klicky.service` because the Unix listener discovers devices at startup. For audio problems, check the selected pack, volume and your ALSA/PipeWire output.

</details>

<details>
<summary><strong>The wrong version runs, or “service” is unknown</strong></summary>

Check `command -v klicky`. A source-installed `~/.local/bin/klicky` can override `/usr/bin/klicky`. Remove the old source installation without `--purge` before switching to a package. Upgrade an older package if `service` is unavailable. [Migration steps](INSTALLATION.md#linux)

A running process is not the same as a responsive daemon. The current status command separately checks process existence and a matching IPC reply. If it reports responsive but you hear nothing, check input permissions and the audio route.

</details>

## Any system

<details>
<summary><strong>Bluetooth feels delayed</strong></summary>

Bluetooth can add transport delay. Compare laptop speakers or wired output. Smaller internal buffers cannot remove delay added by the output device.

</details>

<details>
<summary><strong>A live pack or volume change did not apply</strong></summary>

Check `klicky status` after the change. Use the app controls instead of editing config while it runs. Successful live pack/volume commands confirm the change was loaded, saved and applied. If a response fails or times out, inspect status and logs before retrying. A custom pack with the same name overrides the bundled pack. [Pack format](SOUND_PACKS.md)

</details>

Still stuck? [Open an issue](https://github.com/Muneer320/klicky/issues/new) with your OS, build/version, output device and reproduction steps. Keep passwords, recordings and unrelated logs private.
