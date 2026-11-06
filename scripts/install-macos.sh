#!/usr/bin/env bash
set -euo pipefail

start=1
case "${1:-}" in
  '') ;;
  --no-start) start=0 ;;
  *) printf 'Usage: install-macos.sh [--no-start]\n' >&2; exit 1 ;;
esac
[[ $# -le 1 ]] || { printf 'Too many arguments.\n' >&2; exit 1; }

if [[ $(uname -s) != Darwin ]]; then
  printf 'This installer supports macOS only.\n' >&2
  exit 1
fi

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
bin_dir="$HOME/.local/bin"
sound_dir="$HOME/Library/Application Support/klicky/sounds"

cd "$repo_root"
cargo build --release

if [[ -x "$bin_dir/klicky" ]]; then
  "$bin_dir/klicky" stop
fi

mkdir -p "$bin_dir" "$sound_dir"
install -m755 target/release/klicky "$bin_dir/klicky"
# BSD cp -n returns failure when a destination already exists.
# Keep custom samples while adding any newly bundled files on upgrade.
find sounds -type f -print0 | while IFS= read -r -d '' source; do
  destination="$sound_dir/${source#sounds/}"
  mkdir -p "$(dirname "$destination")"
  if [[ ! -e $destination && ! -L $destination ]]; then
    cp "$source" "$destination"
  fi
done
if [[ $start == 1 ]]; then "$bin_dir/klicky" service enable; fi

printf '\nKlicky is installed.\n'
if [[ $start == 1 ]]; then printf 'Login startup is enabled.\n'; fi
printf 'Grant Accessibility access to %s in System Settings > Privacy & Security > Accessibility.\n' "$bin_dir/klicky"
printf 'Run %s service status to check it.\n' "$bin_dir/klicky"
