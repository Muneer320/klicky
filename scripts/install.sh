#!/usr/bin/env bash
# User-facing bootstrap. Package installs do not require Rust.
set -euo pipefail

source_install=0
menu=1
for argument in "$@"; do
  case "$argument" in
    --source) source_install=1 ;;
    --no-menu) menu=0 ;;
    --help|-h)
      printf 'Install Klicky for your desktop user.\n\n'
      printf 'Usage: bash install.sh [--source] [--no-menu]\n'
      printf 'Linux: signed APT repository or Arch package; --source builds instead.\n'
      printf 'macOS: build from source with Rust and Xcode Command Line Tools.\n'
      printf 'The optional macOS menu bar control is included unless --no-menu is used.\n'
      exit 0 ;;
    *) printf 'Unknown option: %s\n' "$argument" >&2; exit 1 ;;
  esac
done

die() { printf '%s\n' "$*" >&2; exit 1; }
need() { command -v "$1" >/dev/null 2>&1 || die "Missing $1. See https://github.com/Muneer320/klicky/blob/master/docs/INSTALLATION.md"; }
[[ $(id -u) != 0 ]] || die 'Run this as your desktop user, without sudo. Package installation will ask for sudo when needed.'
need curl
platform=$(uname -s)
work_dir=$(mktemp -d "${TMPDIR:-/tmp}/klicky-install.XXXXXXXX")
trap 'rm -rf -- "$work_dir"' EXIT
download() { curl --fail --silent --show-error --location --retry 2 --output "$2" "$1"; }

build_source() {
  need cargo
  need tar
  if [[ $platform == Darwin ]]; then
    need xcrun
    xcrun --find clang >/dev/null 2>&1 || die 'Install Xcode Command Line Tools with: xcode-select --install'
    if [[ $menu == 1 ]]; then need swiftc; fi
  else
    need pkg-config
    pkg-config --exists alsa || die 'Install ALSA development headers: libasound2-dev (Debian/Ubuntu), alsa-lib-devel (Fedora), or alsa-lib (Arch).'
  fi
  printf 'Downloading Klicky source. Building may take a few minutes.\n'
  download 'https://github.com/Muneer320/klicky/archive/refs/heads/master.tar.gz' "$work_dir/source.tar.gz"
  mkdir "$work_dir/source"
  tar -xzf "$work_dir/source.tar.gz" -C "$work_dir/source" --strip-components=1
  if [[ $platform == Darwin ]]; then
    bash "$work_dir/source/scripts/install-macos.sh"
    if [[ $menu == 1 ]]; then bash "$work_dir/source/scripts/install-macos-menu-bar.sh"; fi
    printf '\nAllow ~/.local/bin/klicky in System Settings > Privacy & Security > Accessibility.\n'
    printf 'Then run: ~/.local/bin/klicky service start\n'
  else
    bash "$work_dir/source/scripts/install-linux.sh"
  fi
}

check_source_conflict() {
  if [[ -e "$HOME/.local/bin/klicky" || -L "$HOME/.local/bin/klicky" ]]; then
    die 'An existing ~/.local/bin/klicky would override the package. Remove the old source installation without --purge first; see the installation guide.'
  fi
}

install_apt() {
  need sudo
  need gpg
  need dpkg-query
  need systemctl
  [[ $(dpkg --print-architecture) == amd64 ]] || die 'APT packages are amd64. Use --source for this architecture.'
  libc_version=$(dpkg-query -W -f='${Version}' libc6)
  dpkg --compare-versions "$libc_version" ge 2.39 || die 'This package needs glibc 2.39 or newer (for example Ubuntu 24.04+). Use --source on an older system.'
  check_source_conflict
  archive='https://muneer320.github.io/klicky'
  download "$archive/klicky-archive-keyring.gpg" "$work_dir/keyring.gpg"
  fingerprint=$(gpg --batch --show-keys --with-colons "$work_dir/keyring.gpg" | awk -F: '$1 == "fpr" {print $10; exit}')
  [[ $fingerprint == DBB67AE478D2FFCEC6637B559899E554D3580D8F ]] || die 'The repository signing key does not match. Nothing was trusted.'
  printf 'Types: deb\nURIs: %s\nSuites: stable\nComponents: main\nArchitectures: amd64\nSigned-By: /etc/apt/keyrings/klicky-archive-keyring.gpg\n' "$archive" > "$work_dir/klicky.sources"
  printf 'Adding the signed Klicky repository and installing the package.\n'
  sudo install -d -m 0755 /etc/apt/keyrings
  sudo install -m 0644 "$work_dir/keyring.gpg" /etc/apt/keyrings/klicky-archive-keyring.gpg
  sudo install -m 0644 "$work_dir/klicky.sources" /etc/apt/sources.list.d/klicky.sources
  sudo apt-get update
  sudo apt-get install -y klicky
  systemctl --user enable --now klicky.service
}

install_arch() {
  need sudo
  need sha256sum
  need awk
  need systemctl
  [[ $(uname -m) == x86_64 ]] || die 'Arch packages are x86_64. Use --source for this architecture.'
  check_source_conflict
  # Update with the next published Arch package, not an unpublished source version.
  package='klicky-0.4.0-1-x86_64.pkg.tar.zst'
  release='https://github.com/Muneer320/klicky/releases/download/v0.4.0'
  download "$release/$package" "$work_dir/$package"
  download "$release/SHA256SUMS" "$work_dir/SHA256SUMS"
  awk -v package="$package" '$2 == package || $2 == "*" package {print}' "$work_dir/SHA256SUMS" > "$work_dir/package.sha256"
  [[ $(wc -l < "$work_dir/package.sha256") -eq 1 ]] || die 'The release must contain exactly one checksum for this package.'
  (cd "$work_dir" && sha256sum --check --status package.sha256) || die 'The package checksum does not match.'
  sudo pacman -U --noconfirm "$work_dir/$package"
  systemctl --user enable --now klicky.service
}

case "$platform" in
  Darwin) build_source ;;
  Linux)
    if [[ $source_install == 1 ]]; then build_source
    elif command -v apt-get >/dev/null 2>&1 && command -v dpkg >/dev/null 2>&1; then install_apt
    elif command -v pacman >/dev/null 2>&1; then install_arch
    else build_source
    fi ;;
  *) die 'Use install.ps1 on Windows. This script supports Linux and macOS.' ;;
esac
printf '\nKlicky is installed. Try: klicky list, klicky switch eg-oreo, klicky volume 0.5\n'
