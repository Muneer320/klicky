# Omarchy controls

## Klicky Bar (Omarchy 4.5+)

[Klicky Bar](https://github.com/Muneer320/klicky-omarchy-bar) provides pause/resume, sound packs, volume, engine start/stop and independent login startup. It follows your theme and remembers its position. Install Klicky 0.6.0 or later first, then:

```bash
omarchy plugin add https://github.com/Muneer320/klicky-omarchy-bar --enable
```

Open the keyboard icon to use the controls. **Move in bar** lets you place it after the left-side widgets, before the right-side widgets, or beside the clock. Tab/Shift+Tab navigate; Escape or a click outside closes the popup.

For installation without the older toggle, pass this flag to the engine installer:

```bash
curl -fsSL https://klicky.muneer320.tech/install.sh | bash -s -- --no-omarchy-toggle
```

Use one bar control at a time. If the older toggle is already present, back up `~/.config/omarchy/shell.json` and remove only entries with `"id": "klicky"` from its `bar.layout` sections before adding Klicky Bar. Keep your other widgets and engine settings.

Update or remove only the plugin:

```bash
omarchy plugin update io.github.muneer320.klicky-bar
omarchy plugin remove io.github.muneer320.klicky-bar
```

Removing the plugin keeps the engine and your sound settings. See [complete uninstallation](INSTALLATION.md#uninstall) to remove Klicky itself. Audio currently follows the system output; per-app output selection remains planned.

See the [plugin reference](../packaging/omarchy-bar/README.md) for the executable override, runtime dependencies and development/export workflow. The main Klicky repository remains the source of truth; the plugin repository contains generated releases.

<details>
<summary><strong>Older start/stop toggle</strong></summary>

The one-command engine installer includes the simple toggle by default on compatible Omarchy shells. It starts/stops `klicky.service` and dims or hides while stopped. For an existing source checkout:

```bash
./scripts/install-omarchy-toggle.sh
```

The script backs up the shell layout, installs `~/.config/omarchy/bar/modules/klicky.qml`, adds one `klicky` entry beside the clock, and reloads the shell. It preserves the centered clock and unrelated widgets. It refuses to add a second control while Klicky Bar is configured.

To remove only this toggle, remove the `klicky` entries from `bar.layout` in `~/.config/omarchy/shell.json`, delete its module file, then restart the shell:

```bash
rm -f ~/.config/omarchy/bar/modules/klicky.qml
omarchy restart shell
```

This does not stop or uninstall the engine. Files under `/usr/share/omarchy` are never modified.

</details>

## Troubleshooting

```bash
klicky status --json
systemctl --user status klicky.service
omarchy-shell shell ping
```

If the shell resolves an older packaged binary before a newer local installation, configure the plugin's [explicit executable path](../packaging/omarchy-bar/README.md#local-omarchy-validation). The user service must run the same engine version. After editing plugin code, use `omarchy restart shell` if a rescan leaves cached code visible.
