# Klicky Bar

Klicky's sound controls in the Omarchy bar: pause and resume, sound packs, volume, engine start/stop and start at login. Left-click or right-click the keyboard icon to open the controls. Escape or a click outside closes them. The popup supports keyboard focus with Tab and follows your Omarchy theme.

On first setup, choose **Left side**, **Right side**, **Left of clock** or **Right of clock**. Left side places Klicky after the existing left-side widgets, such as workspaces; Right side places it before the existing right-side widgets. On a vertical bar these are **Upper side** and **Lower side**. Klicky remembers the choice in your normal bar configuration. **Move in bar** opens the choices again later. The positions beside the clock use Omarchy's adjacent sections so the clock stays centered. Changing position preserves the other widgets and Klicky's executable setting; it never starts or stops playback.

## Requirements

- Omarchy 4.5/Quattro with its Quickshell bar and native plugin support.
- Klicky 0.6.0 or later with `klicky status --json` schema 1, `pause`, `resume` and `service autostart` commands, plus its user systemd service.
- Bash, GNU coreutils (`timeout`) and util-linux (`flock`), included in Omarchy.

Klicky v0.5.0 does **not** provide these Unix control APIs. The plugin is published separately from the engine. Installing the plugin does not install the engine, change input permissions or enable playback automatically.

## Installation

Install Klicky first using its [Linux installation guide](https://github.com/Muneer320/klicky/blob/master/docs/INSTALLATION.md#linux). If using the one-line installer, pass `--no-omarchy-toggle` to skip the older bar toggle.

If the older toggle is already installed, back up `~/.config/omarchy/shell.json`, then remove only entries with `"id": "klicky"` from its `bar.layout` sections. Keep the other widgets and engine settings. Do not run the full Klicky uninstaller to replace a bar control.

Check that `klicky status --json` works, then add the new bar controls:

```sh
omarchy plugin add https://github.com/Muneer320/klicky-omarchy-bar --enable
```

Choose a position when prompted. Open the keyboard icon to start the engine, pick a sound pack, change volume or pause playback. **Start at login** is optional and independent of whether the engine is running now.

Update the plugin with:

```sh
omarchy plugin update io.github.muneer320.klicky-bar
```

Remove just the bar controls with:

```sh
omarchy plugin remove io.github.muneer320.klicky-bar
```

Removal keeps Klicky itself, its service and your sound settings. See [Klicky's uninstall guide](https://github.com/Muneer320/klicky/blob/master/docs/INSTALLATION.md#uninstall) to remove the engine too.

## Controls

Pause preserves the running engine; Stop engine shuts it down. Start at login changes only the user service's login setting and does not start or stop playback. Volume and pack changes apply through Klicky's CLI. Settings refresh from the engine after each command; unavailable or incompatible engines disable controls instead of displaying saved preferences as live state.

Source builds after v0.7.0 add **Audio output**, **Retry audio** and **Refresh outputs** with the matching running engine. Follow the system default or pin a listed PulseAudio sink, including on PipeWire-Pulse. Labels can repeat; entries remain separate and selection uses durable sink IDs. A missing selected output stays selected and silent, and Retry never clears manual Pause. Reconnect the device or choose another output. Older engines retain the basic playback controls; audio actions require valid live state and advertised capabilities. ALSA-only playback cannot enumerate or select outputs. This widget never changes the system-wide audio device, card profiles or ports.

The widget launches no shell command strings, captures no keyboard events and makes no network requests. Commands run asynchronously with deadlines. Status refreshes every ten seconds, or every two seconds while the popup is open. Audio queries have a five-second deadline and capped responses; output lists are cached until an explicit refresh or a state change. Actions invalidate older audio responses. Reopening the popup refreshes the list, and metadata failures do not overwrite playback settings.

Placement uses the native `omarchy bar move` command, then saves the choice with `omarchy bar set`. A bounded helper completes both steps even when moving recreates the widget. The setup prompt returns only if its saved `placement` setting is removed. The included English catalog falls back to English for other Omarchy languages; additional translations can be added under `i18n/`.

## Development and publication

The source of truth is [Muneer320/klicky](https://github.com/Muneer320/klicky/tree/master/packaging/omarchy-bar). Changes belong there. The plugin-only [publication repository](https://github.com/Muneer320/klicky-omarchy-bar) contains generated, reviewed releases rather than independent development.

From a clean, committed core checkout, export a candidate with:

```sh
python3 scripts/export-omarchy-plugin.py /tmp/klicky-bar-export
```

The destination must not already exist. `SOURCE.json` records the core source commit and file hashes. Exporting does not publish or install anything. Publication and marketplace submission require a reviewed candidate and matching engine release.

### Local Omarchy validation

On the Omarchy desktop, build/install the matching engine from the same reviewed core checkout first, then confirm that `klicky status --json` returns `schema_version: 1`. Back up `~/.config/omarchy/shell.json` before testing. Export into a new directory and validate it:

```sh
omarchy plugin validate /tmp/klicky-bar-export
```

For a local development test, copy that exported folder to the new, unused path `~/.config/omarchy/plugins/io.github.muneer320.klicky-bar`, rescan, and enable it:

```sh
omarchy-shell shell rescanPlugins
omarchy plugin enable io.github.muneer320.klicky-bar --section centerBegin
```

The widget runs `klicky` from the shell's own PATH by default. If a source installation is shadowed by an older packaged binary, set `executable` to the matching CLI's absolute path in the widget's `bar.layout` entry in `~/.config/omarchy/shell.json`:

```json
{
  "id": "io.github.muneer320.klicky-bar",
  "executable": "/home/you/.local/bin/klicky"
}
```

Use your actual home path; `~` and environment variables are not expanded. This setting changes only the CLI used by Klicky Bar. The user service must separately start the same engine version.

Disable the legacy toggle first if it is already installed; keep the backed-up layout for restoration. Check mouse and keyboard operation, both pause states, pack changes, 0%/10%/100% volume, login setting, stopped-engine recovery, outside-click/Escape dismissal, and your usual theme and display scaling. For audio controls, test remembered output/default selection, duplicate labels, missing/reconnected devices, Retry and sleep/wake while paused, and an older/incompatible CLI. Verify no other application's routing changes. Confirm normal and fast typing remain immediate and consistent through laptop speakers. Check service autostart again after a real login. Container tests cannot establish these desktop/audio results.

Remove only the development widget with:

```sh
omarchy plugin remove io.github.muneer320.klicky-bar
```

This uses Omarchy's own confirmation/removal flow and preserves the engine and sound settings. Restore the legacy layout if needed.

The existing legacy Omarchy toggle remains available through `scripts/install-omarchy-toggle.sh`. Use one integration at a time; removing a bar widget preserves the engine, configuration and other bar modules.
