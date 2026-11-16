# Klicky Bar

Klicky's sound controls in the Omarchy bar: pause and resume, sound packs, volume, engine start/stop and start at login. Left-click or right-click the keyboard icon to open the controls. Escape or a click outside closes them. The popup supports keyboard focus with Tab and follows your Omarchy theme.

On first setup, choose **Left edge**, **Right edge**, **Left of clock** or **Right of clock**. Klicky remembers the choice in your normal bar configuration. **Move in bar** opens the choices again later. The positions beside the clock use Omarchy's adjacent sections so the clock stays centered. Changing position preserves the other widgets and Klicky's executable setting; it never starts or stops playback.

## Requirements

- Omarchy 4.5/Quattro with its Quickshell bar and native plugin support.
- The Klicky engine with `klicky status --json` schema 1, `pause`, `resume` and `service autostart` commands, plus its user systemd service.
- GNU coreutils (`timeout`), included in Omarchy.

The published Klicky v0.5.0 does **not** provide these Unix control APIs. This plugin is a development candidate until a compatible engine release and desktop validation are complete. Installing the plugin does not install the engine, change input permissions or enable playback automatically.

## Controls

Pause preserves the running engine; Stop engine shuts it down. Start at login changes only the user service's login setting and does not start or stop playback. Volume and pack changes apply through Klicky's CLI. Settings refresh from the engine after each command; unavailable or incompatible engines disable controls instead of displaying saved preferences as live state.

Audio uses Klicky's current system-default output. Device selection is a separate planned feature; this widget never changes the system-wide audio device.

The widget launches no shell command strings, captures no keyboard events and makes no network requests. Commands run asynchronously with deadlines. Status refreshes every ten seconds, or every two seconds while the popup is open.

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

Disable the legacy toggle first if it is already installed; keep the backed-up layout for restoration. Check mouse and keyboard operation, both pause states, pack changes, 0%/10%/100% volume, login setting, stopped-engine recovery, outside-click/Escape dismissal, and your usual theme and display scaling. Confirm normal typing remains immediate and consistent through laptop speakers. Check service autostart again after a real login. Container tests cannot establish these desktop/audio results.

Remove only the development widget with:

```sh
omarchy plugin remove io.github.muneer320.klicky-bar
```

This uses Omarchy's own confirmation/removal flow and preserves the engine and sound settings. Restore the legacy layout if needed.

The existing legacy Omarchy toggle remains available through `scripts/install-omarchy-toggle.sh`. Do not install both widgets as a migration shortcut. Native-plugin installation and removal instructions will accompany the qualified release; removing a bar widget must preserve the engine, configuration and other bar modules.
