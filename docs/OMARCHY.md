# Omarchy integration

Klicky can live in Omarchy's center status area as a native Quickshell module.

The control follows the same behavior as Omarchy's built-in indicators:

- Running: visible at full opacity
- Stopped: hidden when the center bar is idle
- Stopped and hovered: visible at reduced opacity
- Click: start or stop `klicky.service`

## Install

Install Klicky and its systemd user service first:

```bash
./scripts/install-linux.sh
```

Then install the bar module:

```bash
./scripts/install-omarchy-toggle.sh
```

The script:

1. Backs up `~/.config/omarchy/shell.json`.
2. Copies the module to `~/.config/omarchy/bar/modules/klicky.qml`.
3. Adds a `klicky` QML entry after `omarchy.indicators` in the center layout.
4. Reloads the shell configuration.
5. Restarts Omarchy shell so the QML source is reloaded.

It does not modify files under `/usr/share/omarchy`.

## Resulting bar entry

The center layout gains:

```json
{
  "id": "klicky",
  "type": "qml"
}
```

A typical ordering is:

```text
Indicators -> Klicky -> Clock -> Keyboard Layout -> Weather -> Update
```

## Requirements

- Current Omarchy shell with custom QML bar-module support
- `klicky.service` installed as a systemd user unit
- Nerd Font glyph support from the Omarchy theme

## Troubleshooting

Check the service:

```bash
systemctl --user status klicky.service
```

Check the shell:

```bash
export OMARCHY_PATH=${OMARCHY_PATH:-/usr/share/omarchy}
omarchy-shell shell ping
```

Reload after editing the QML file:

```bash
export OMARCHY_PATH=${OMARCHY_PATH:-/usr/share/omarchy}
omarchy-restart-shell
```

Inspect recent QML errors:

```bash
journalctl --user --since "5 minutes ago" | grep -E "klicky.qml|QQml|Loader"
```

## Remove the toggle

Remove the `klicky` entry from `bar.layout.center` in:

```text
~/.config/omarchy/shell.json
```

Then remove the module and restart the shell:

```bash
rm -f ~/.config/omarchy/bar/modules/klicky.qml
export OMARCHY_PATH=${OMARCHY_PATH:-/usr/share/omarchy}
omarchy-restart-shell
```

Removing the bar control does not uninstall or stop Klicky.
