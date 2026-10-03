#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
bin_dir="$HOME/.local/bin"
config_dir="${XDG_CONFIG_HOME:-$HOME/.config}/klicky"
user_unit_dir="${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user"

if [[ $(uname -s) != Linux ]]; then
  printf 'This installer supports Linux only.\n' >&2
  exit 1
fi

for command in cargo install systemctl udevadm; do
  if ! command -v "$command" >/dev/null 2>&1; then
    printf 'Missing required command: %s\n' "$command" >&2
    exit 1
  fi
done

cd "$repo_root"
cargo build --release

install -Dm755 target/release/klicky "$bin_dir/klicky"
mkdir -p "$config_dir/sounds"
cp -a sounds/. "$config_dir/sounds/"
install -Dm644 packaging/systemd/klicky.service "$user_unit_dir/klicky.service"

if [[ ${KLICKY_SKIP_UDEV:-0} != 1 ]]; then
  sudo install -Dm644 packaging/udev/70-klicky.rules /etc/udev/rules.d/70-klicky.rules
  sudo udevadm control --reload-rules
  sudo udevadm trigger --subsystem-match=input
fi

systemctl --user daemon-reload
systemctl --user reenable klicky.service
systemctl --user restart klicky.service
rm -f "$config_dir/klicky.pid" "$config_dir/klicky.sock"

printf '\nKlicky installed successfully.\n'
printf 'Binary: %s\n' "$bin_dir/klicky"
printf 'Config: %s\n' "$config_dir"
printf 'Service: %s\n' "$user_unit_dir/klicky.service"
printf '\nRun %s status to inspect the daemon.\n' "$bin_dir/klicky"
