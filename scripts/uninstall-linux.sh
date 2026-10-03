#!/usr/bin/env bash
set -euo pipefail

bin_path="$HOME/.local/bin/klicky"
config_dir="${XDG_CONFIG_HOME:-$HOME/.config}/klicky"
unit_path="${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user/klicky.service"

systemctl --user disable --now klicky.service 2>/dev/null || true
rm -f "$unit_path" "$bin_path"
systemctl --user daemon-reload

if [[ -f /etc/udev/rules.d/70-klicky.rules ]]; then
  sudo rm -f /etc/udev/rules.d/70-klicky.rules
  sudo udevadm control --reload-rules
  sudo udevadm trigger --subsystem-match=input
fi

if [[ ${1:-} == --purge ]]; then
  rm -rf "$config_dir"
  printf 'Removed Klicky and its configuration.\n'
else
  printf 'Removed Klicky. Configuration remains at %s.\n' "$config_dir"
fi
