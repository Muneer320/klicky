#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
omarchy_config="$HOME/.config/omarchy"
shell_config="$omarchy_config/shell.json"
module_dir="$omarchy_config/bar/modules"
stamp=$(date +%Y%m%d-%H%M%S)

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

cp "$shell_config" "$shell_config.backup-$stamp"
install -Dm644 "$repo_root/packaging/omarchy/klicky.qml" "$module_dir/klicky.qml"

SHELL_CONFIG="$shell_config" python3 - <<'PY'
import json
import os
from pathlib import Path

path = Path(os.environ["SHELL_CONFIG"])
data = json.loads(path.read_text())
center = data["bar"]["layout"]["center"]
center = [entry for entry in center if entry.get("id") != "klicky"]
index = next(
    (position + 1 for position, entry in enumerate(center) if entry.get("id") == "omarchy.indicators"),
    0,
)
center.insert(index, {"id": "klicky", "type": "qml"})
data["bar"]["layout"]["center"] = center

temporary = path.with_suffix(".json.tmp")
temporary.write_text(json.dumps(data, indent=2) + "\n")
temporary.replace(path)
PY

export OMARCHY_PATH=${OMARCHY_PATH:-/usr/share/omarchy}
omarchy-shell shell reloadConfig >/dev/null
omarchy-restart-shell

printf 'Installed the Klicky Omarchy bar toggle.\n'
printf 'Backup: %s\n' "$shell_config.backup-$stamp"
