#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
omarchy_config="$HOME/.config/omarchy"
shell_config="$omarchy_config/shell.json"
module_dir="$omarchy_config/bar/modules"
stamp=$(date +%Y%m%d-%H%M%S)
backup="$shell_config.backup-$stamp"
updated_config=$(mktemp)
trap 'rm -f "$updated_config"' EXIT

if [[ ! -f $shell_config ]]; then
  printf 'Omarchy shell configuration not found: %s\n' "$shell_config" >&2
  exit 1
fi

for command in python3 omarchy-shell omarchy-restart-shell; do
  if ! command -v "$command" >/dev/null 2>&1; then
    printf 'Missing required command: %s\n' "$command" >&2
    exit 1
  fi
done

SHELL_CONFIG="$shell_config" UPDATED_CONFIG="$updated_config" python3 - <<'PY'
import json
import os
from pathlib import Path

source = Path(os.environ["SHELL_CONFIG"])
target = Path(os.environ["UPDATED_CONFIG"])

try:
    data = json.loads(source.read_text())
    bar = data["bar"]
    layout = bar["layout"]
    center = layout.setdefault("centerBegin", [])
except (json.JSONDecodeError, KeyError, TypeError) as error:
    raise SystemExit(f"Invalid Omarchy shell configuration: {error}")

if not isinstance(layout, dict) or any(
    not isinstance(items, list) or not all(isinstance(entry, dict) for entry in items)
    for items in layout.values()
):
    raise SystemExit("Invalid Omarchy shell configuration: layout sections must be lists of objects")

if any(entry.get("id") == "io.github.muneer320.klicky-bar" for items in layout.values() for entry in items):
    raise SystemExit("Klicky Bar is already configured. Keep it, or remove it before installing the older toggle.")
for section, items in layout.items():
    layout[section] = [entry for entry in items if entry.get("id") != "klicky"]
center = layout["centerBegin"]
index = next(
    (position + 1 for position, entry in enumerate(center) if entry.get("id") == "omarchy.indicators"),
    0,
)
center.insert(index, {"id": "klicky", "type": "qml"})
layout["centerBegin"] = center
target.write_text(json.dumps(data, indent=2) + "\n")
PY

cp "$shell_config" "$backup"
install -Dm644 "$repo_root/packaging/omarchy/klicky.qml" "$module_dir/klicky.qml"
cp "$updated_config" "$shell_config"

export OMARCHY_PATH=${OMARCHY_PATH:-/usr/share/omarchy}
if ! omarchy-shell shell reloadConfig >/dev/null || ! omarchy-restart-shell; then
  cp "$backup" "$shell_config"
  printf 'Omarchy reload failed. Restored %s.\n' "$backup" >&2
  exit 1
fi

printf 'Installed the Klicky Omarchy bar toggle.\n'
printf 'Backup: %s\n' "$backup"
