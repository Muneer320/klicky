#!/usr/bin/env bash
set -euo pipefail

bin_path="$HOME/.local/bin/klicky"
config_dir="${XDG_CONFIG_HOME:-$HOME/.config}/klicky"
unit_path="${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user/klicky.service"

if [[ -x "$bin_path" ]]; then
  "$bin_path" stop
elif [[ -e "$unit_path" ]]; then
  systemctl --user stop klicky.service
fi

if [[ -e "$unit_path" ]]; then
  systemctl --user disable klicky.service
  systemctl --user daemon-reload
fi

if [[ -f /etc/udev/rules.d/70-klicky.rules ]]; then
  sudo rm -f /etc/udev/rules.d/70-klicky.rules
  sudo udevadm control --reload-rules
  sudo udevadm trigger --subsystem-match=input
fi

rm -f "$unit_path" "$bin_path"

if [[ ${1:-} == --purge ]]; then
  rm -rf "$config_dir"
  printf 'Removed Klicky and its configuration.\n'
else
  printf 'Removed Klicky. Configuration remains at %s.\n' "$config_dir"
fi
