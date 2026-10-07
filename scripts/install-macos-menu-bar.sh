#!/usr/bin/env bash
set -euo pipefail

if [[ $(uname -s) != Darwin ]]; then
  printf 'This installer supports macOS only.\n' >&2
  exit 1
fi

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
bin_dir="$HOME/.local/bin"
agent_dir="$HOME/Library/LaunchAgents"
agent="$agent_dir/dev.klicky.menu.plist"
domain="gui/$(id -u)"
label="dev.klicky.menu"

if [[ ! -x "$bin_dir/klicky" ]]; then
  printf 'Install Klicky first with ./scripts/install-macos.sh.\n' >&2
  exit 1
fi
if ! command -v swiftc >/dev/null 2>&1; then
  printf 'swiftc is required. Install the Xcode Command Line Tools with xcode-select --install.\n' >&2
  exit 1
fi

mkdir -p "$bin_dir" "$agent_dir"
swiftc "$repo_root/packaging/macos/KlickyMenu.swift" -o "$bin_dir/klicky-menu"
binary_xml=$(printf '%s' "$bin_dir/klicky-menu" | sed -e 's/\&/\&amp;/g' -e 's/</\&lt;/g' -e 's/>/\&gt;/g' -e 's/"/\&quot;/g' -e "s/'/\&apos;/g")
if launchctl print "$domain/$label" >/dev/null 2>&1; then
  launchctl bootout "$domain/$label"
fi
cat > "$agent" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>Label</key><string>$label</string>
<key>ProgramArguments</key><array><string>$binary_xml</string></array>
<key>RunAtLoad</key><true/>
<key>KeepAlive</key><false/>
</dict></plist>
EOF
chmod 600 "$agent"
launchctl bootstrap "$domain" "$agent"
printf 'Klicky menu bar control installed and set to open at login.\n'
