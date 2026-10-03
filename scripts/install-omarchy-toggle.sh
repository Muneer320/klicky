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
    center = layout["center"]
except (json.JSONDecodeError, KeyError, TypeError) as error:
    raise SystemExit(f"Invalid Omarchy shell configuration: {error}")

if not isinstance(center, list) or not all(isinstance(entry, dict) for entry in center):
    raise SystemExit("Invalid Omarchy shell configuration: bar.layout.center must be a list of objects")

center = [entry for entry in center if entry.get("id") != "klicky"]
index = next(
    (position + 1 for position, entry in enumerate(center) if entry.get("id") == "omarchy.indicators"),
    0,
)
center.insert(index, {"id": "klicky", "type": "qml"})
layout["center"] = center
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
