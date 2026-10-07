#!/usr/bin/env bash
set -euo pipefail

if [[ $(uname -s) != Darwin ]]; then
  printf 'This installer supports macOS only.\n' >&2
  exit 1
fi

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
bin_dir="$HOME/.local/bin"
sound_dir="$HOME/Library/Application Support/klicky/sounds"

cd "$repo_root"
cargo build --release
mkdir -p "$bin_dir" "$sound_dir"
install -m755 target/release/klicky "$bin_dir/klicky"
cp -R sounds/. "$sound_dir/"
"$bin_dir/klicky" service enable

printf '\nKlicky is installed and starts at login.\n'
printf 'Grant Accessibility access to %s in System Settings > Privacy & Security > Accessibility.\n' "$bin_dir/klicky"
printf 'Run %s service status to check it.\n' "$bin_dir/klicky"
