#!/usr/bin/env bash
# Remove Klicky and its desktop controls. Personal data is opt-in via --purge.
set -euo pipefail
purge=0
case "${1:-}" in
  '') ;;
  --purge) purge=1 ;;
  --help|-h) printf 'Usage: bash uninstall.sh [--purge]\nStops/removes Klicky and desktop controls. --purge also deletes settings and custom packs.\n'; exit 0 ;;
  *) printf 'Unknown option: %s\n' "$1" >&2; exit 1 ;;
esac
[[ $# -le 1 ]] || { printf 'Too many arguments.\n' >&2; exit 1; }
die() { printf '%s\n' "$*" >&2; exit 1; }
[[ $(id -u) != 0 ]] || die 'Run as the desktop user, without sudo. Package removal asks for sudo when needed.'
platform=$(uname -s)
[[ $platform == Darwin || $platform == Linux ]] || die 'Use uninstall.ps1 on Windows.'
work_dir=$(mktemp -d "${TMPDIR:-/tmp}/klicky-remove.XXXXXXXX")
trap 'rm -rf -- "$work_dir"' EXIT
local_binary="$HOME/.local/bin/klicky"
[[ ! -L $local_binary ]] || die 'The user-local binary is a symlink. Remove Klicky through the installation that owns its target before retrying.'

remove_omarchy() {
  shell_config="$HOME/.config/omarchy/shell.json"
  module="$HOME/.config/omarchy/bar/modules/klicky.qml"
  if [[ -f $shell_config ]] && { [[ -e $module ]] || grep -q '"id"[[:space:]]*:[[:space:]]*"klicky"' "$shell_config"; }; then
    command -v python3 >/dev/null 2>&1 || die 'Python 3 is needed to remove the Omarchy entry safely.'
    [[ ! -L $shell_config ]] || die 'Omarchy shell.json is a symlink; remove its Klicky entry manually before retrying.'
    cp "$shell_config" "$work_dir/shell-before.json"
    python3 - "$shell_config" "$work_dir/shell-after.json" <<'PY'
import json
import sys
from pathlib import Path

source, target = map(Path, sys.argv[1:])
data = json.loads(source.read_text(encoding="utf-8"))
center = data["bar"]["layout"]["center"]
if not isinstance(center, list) or not all(isinstance(item, dict) for item in center):
    raise SystemExit("Invalid Omarchy center layout; configuration was not changed.")
data["bar"]["layout"]["center"] = [item for item in center if item.get("id") != "klicky"]
target.write_text(json.dumps(data, indent=2) + "\n", encoding="utf-8")
PY
    cp "$work_dir/shell-after.json" "$shell_config"
    if command -v omarchy-shell >/dev/null 2>&1 && command -v omarchy-restart-shell >/dev/null 2>&1; then
      export OMARCHY_PATH=${OMARCHY_PATH:-/usr/share/omarchy}
      if ! omarchy-shell shell reloadConfig >/dev/null || ! omarchy-restart-shell; then
        cp "$work_dir/shell-before.json" "$shell_config"
        die 'Omarchy reload failed; its original configuration was restored. Klicky removal stopped.'
      fi
    fi
  fi
  rm -f -- "$module"
}

unload_agent() {
  label=$1
  if launchctl print "gui/$(id -u)/$label" > "$work_dir/agent.log" 2>&1; then
    launchctl bootout "gui/$(id -u)/$label"
  else
    result=$?
    if [[ $result != 113 ]] || ! grep -q 'Could not find service' "$work_dir/agent.log"; then
      cat "$work_dir/agent.log" >&2
      die "Could not verify the $label LaunchAgent. No files were removed."
    fi
  fi
}

if [[ $platform == Darwin ]]; then
  config_dir="$HOME/Library/Application Support/klicky"
  if [[ -x $local_binary ]]; then "$local_binary" stop; fi
  unload_agent dev.klicky.menu
  unload_agent dev.klicky.daemon
  rm -f -- "$HOME/Library/LaunchAgents/dev.klicky.menu.plist" \
    "$HOME/Library/LaunchAgents/dev.klicky.daemon.plist" \
    "$HOME/.local/bin/klicky-menu" "$local_binary"
  [[ ! -L $config_dir ]] || die 'The Klicky data folder is a symlink; app removal finished, but its target was left untouched.'
  rm -f -- "$config_dir/session.plist"
  if [[ -d "$config_dir/branding" ]]; then
    [[ ! -L "$config_dir/branding" ]] || die 'The branding folder is a symlink; its target was left untouched.'
    rm -f -- "$config_dir/branding/klicky-menu-template.png" "$config_dir/branding/klicky-menu-template@2x.png"
    rmdir "$config_dir/branding" 2>/dev/null || true
  fi
else
  config_dir="${XDG_CONFIG_HOME:-$HOME/.config}/klicky"
  unit="${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user/klicky.service"
  if [[ -x $local_binary ]]; then "$local_binary" stop
  elif [[ -x /usr/bin/klicky ]]; then /usr/bin/klicky stop
  fi
  if command -v systemctl >/dev/null 2>&1 && systemctl --user cat klicky.service >/dev/null 2>&1; then
    systemctl --user disable --now klicky.service
  fi
  remove_omarchy
  if command -v dpkg-query >/dev/null 2>&1 &&
     [[ $(dpkg-query -W -f='${Status}' klicky 2>/dev/null || true) == 'install ok installed' ]]; then
    action=remove
    if [[ $purge == 1 ]]; then action=purge; fi
    sudo apt-get "$action" -y klicky
  elif command -v pacman >/dev/null 2>&1 && pacman -Q klicky >/dev/null 2>&1; then
    sudo pacman -R --noconfirm klicky
  fi
  rm -f -- "$unit" "$local_binary"
  if command -v systemctl >/dev/null 2>&1; then systemctl --user daemon-reload; fi
  if [[ -f /etc/udev/rules.d/70-klicky.rules ]]; then
    sudo rm -f /etc/udev/rules.d/70-klicky.rules
    sudo udevadm control --reload-rules
    sudo udevadm trigger --subsystem-match=input
  fi
  if [[ -f /etc/apt/sources.list.d/klicky.sources || -f /etc/apt/keyrings/klicky-archive-keyring.gpg ]]; then
    sudo rm -f /etc/apt/sources.list.d/klicky.sources /etc/apt/keyrings/klicky-archive-keyring.gpg
  fi
fi

if [[ $purge == 1 ]]; then
  [[ -n $config_dir && $config_dir == */klicky && $config_dir != /klicky ]] || die 'Unexpected data path; configuration was kept.'
  [[ ! -L $config_dir ]] || die 'The Klicky data folder is a symlink; its target was left untouched.'
  rm -rf -- "$config_dir"
  printf 'Removed Klicky, its desktop controls, settings and custom sounds.\n'
else
  printf 'Removed Klicky and its desktop controls. Settings and custom sounds remain at %s.\n' "$config_dir"
fi
