#!/usr/bin/env bash
# User-facing bootstrap. Package installs do not require Rust.
set -euo pipefail

source_install=0
menu=1
omarchy=1
for argument in "$@"; do
  case "$argument" in
    --source) source_install=1 ;;
    --no-menu) menu=0 ;;
    --no-omarchy-toggle) omarchy=0 ;;
    --help|-h)
      printf 'Install Klicky for your desktop user.\n\n'
      printf 'Usage: bash install.sh [--source] [--no-menu] [--no-omarchy-toggle]\n'
      printf 'Linux: signed APT repository or Arch package; --source builds instead.\n'
      printf 'macOS: build from source; missing Rust and Apple build tools are installed.\n'
      printf 'The optional macOS menu bar control is included unless --no-menu is used.\n'
      printf 'Compatible Omarchy shells get the bar toggle unless --no-omarchy-toggle is used.\n'
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

fetch_source() {
  [[ ! -d "$work_dir/source" ]] || return 0
  need tar
  printf 'Downloading Klicky source.\n'
  download 'https://github.com/Muneer320/klicky/archive/refs/heads/master.tar.gz' "$work_dir/source.tar.gz"
  mkdir "$work_dir/source"
  tar -xzf "$work_dir/source.tar.gz" -C "$work_dir/source" --strip-components=1
}

prepare_rust() {
  if ! command -v cargo >/dev/null 2>&1 && [[ -x "$HOME/.cargo/bin/cargo" ]]; then
    export PATH="$HOME/.cargo/bin:$PATH"
  fi
  if ! command -v cargo >/dev/null 2>&1; then
    printf 'Installing Rust 1.87 with rustup for your user (minimal profile).\n'
    download 'https://sh.rustup.rs' "$work_dir/rustup-init.sh"
    sh "$work_dir/rustup-init.sh" -y --profile minimal --no-modify-path --default-toolchain 1.87.0
    export PATH="$HOME/.cargo/bin:$PATH"
  fi
  need cargo
  if ! cargo --version | awk '{split($2, v, "."); exit !(v[1] > 1 || (v[1] == 1 && v[2] >= 87))}'; then
    need rustup
    printf 'Installing the required Rust toolchain without changing your default.\n'
    rustup toolchain install 1.87.0 --profile minimal
    export RUSTUP_TOOLCHAIN=1.87.0
  fi
}

prepare_macos() {
  if ! xcrun --find clang >/dev/null 2>&1; then
    printf 'Opening Apple\047s Command Line Tools installer. Approve its dialog; setup continues here when it finishes.\n'
    xcode-select --install || die 'Apple could not open its build-tools installer. See the installation guide.'
    deadline=$((SECONDS + 1800))
    until xcrun --find clang >/dev/null 2>&1; do
      (( SECONDS < deadline )) || die 'Apple build tools were not ready within 30 minutes. Rerun this installer after they finish.'
      sleep 5
    done
  fi
  if [[ $menu == 1 ]]; then need swiftc; fi
}

start_macos() {
  binary="$HOME/.local/bin/klicky"
  if "$binary" service enable > "$work_dir/start.log" 2>&1; then
    cat "$work_dir/start.log"
    return
  fi
  cat "$work_dir/start.log" >&2
  printf '\nAllow %s in System Settings > Privacy & Security > Accessibility.\n' "$binary"
  printf 'Use + to add the installed file shown in Finder. Keep this Terminal open; setup retries for up to 10 minutes.\n'
  open 'x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility' || true
  open -R "$binary" || true
  deadline=$((SECONDS + 600))
  until "$binary" service start > "$work_dir/start.log" 2>&1; do
    if (( SECONDS >= deadline )); then
      cat "$work_dir/start.log" >&2
      die 'Klicky is installed, but startup did not complete. Check the error above; rerun setup or use the menu bar toggle after granting permission.'
    fi
    sleep 5
  done
  cat "$work_dir/start.log"
}

build_source() {
  if [[ $platform == Darwin ]]; then
    prepare_macos
  elif ! command -v cc >/dev/null 2>&1 || ! command -v pkg-config >/dev/null 2>&1 || ! pkg-config --exists alsa; then
    need sudo
    printf 'Installing the compiler and ALSA development dependencies for this source build.\n'
    if command -v apt-get >/dev/null 2>&1; then
      sudo apt-get update
      sudo apt-get install -y build-essential pkg-config libasound2-dev
    elif command -v pacman >/dev/null 2>&1; then
      sudo pacman -S --needed --noconfirm base-devel pkgconf alsa-lib
    elif command -v dnf >/dev/null 2>&1; then
      sudo dnf install -y gcc pkgconf-pkg-config alsa-lib-devel
    else
      die 'Install your compiler, pkg-config and ALSA development headers, then rerun setup.'
    fi
  fi
  prepare_rust
  fetch_source
  printf 'Building Klicky. This may take a few minutes.\n'
  if [[ $platform == Darwin ]]; then
    bash "$work_dir/source/scripts/install-macos.sh" --no-start
    if [[ $menu == 1 ]]; then bash "$work_dir/source/scripts/install-macos-menu-bar.sh"; fi
    start_macos
  else
    bash "$work_dir/source/scripts/install-linux.sh"
    "$HOME/.local/bin/klicky" service start
  fi
}

install_omarchy() {
  [[ $omarchy == 1 ]] || return 0
  if [[ -d "$HOME/.config/omarchy" ]] || command -v omarchy-shell >/dev/null 2>&1; then
    if [[ ! -f "$HOME/.config/omarchy/shell.json" ]] ||
       ! command -v python3 >/dev/null 2>&1 ||
       ! command -v omarchy-shell >/dev/null 2>&1 ||
       ! command -v omarchy-restart-shell >/dev/null 2>&1; then
      printf 'Klicky is running. The bar toggle was skipped because this Omarchy setup does not have the supported QML shell/configuration.\n' >&2
      return 0
    fi
    printf 'Installing the Omarchy bar toggle.\n'
    fetch_source
    bash "$work_dir/source/scripts/install-omarchy-toggle.sh"
  fi
}

check_source_conflict() {
  user_unit="${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user/klicky.service"
  if [[ -e "$HOME/.local/bin/klicky" || -L "$HOME/.local/bin/klicky" || -e $user_unit || -L $user_unit ]]; then
    die 'An existing user-local Klicky installation would override the package. Remove the old source installation without --purge first; see the installation guide.'
  fi
}

stop_packaged_service() {
  printf 'Stopping your Klicky service before package installation. Other signed-in users must stop their packaged copy too.\n'
  if systemctl --user is-active --quiet klicky.service; then
    systemctl --user stop klicky.service
  fi
}

install_apt() {
  need sudo
  need dpkg-query
  need systemctl
  [[ $(dpkg --print-architecture) == amd64 ]] || die 'APT packages are amd64. Use --source for this architecture.'
  libc_version=$(dpkg-query -W -f='${Version}' libc6)
  dpkg --compare-versions "$libc_version" ge 2.39 || die 'This package needs glibc 2.39 or newer (for example Ubuntu 24.04+). Use --source on an older system.'
  check_source_conflict
  if ! command -v gpg >/dev/null 2>&1; then
    printf 'Installing GnuPG to verify the Klicky repository key.\n'
    sudo apt-get update
    sudo apt-get install -y gnupg
  fi
  need gpg
  archive='https://muneer320.github.io/klicky'
  download "$archive/klicky-archive-keyring.gpg" "$work_dir/keyring.gpg"
  key_info=$(gpg --batch --show-keys --with-colons "$work_dir/keyring.gpg")
  primary_keys=$(printf '%s\n' "$key_info" | awk -F: '$1 == "pub" {count++} END {print count + 0}')
  [[ $primary_keys == 1 ]] || die 'The repository keyring must contain exactly one signing key. Nothing was trusted.'
  fingerprint=$(printf '%s\n' "$key_info" | awk -F: '$1 == "fpr" {print $10; exit}')
  [[ $fingerprint == DBB67AE478D2FFCEC6637B559899E554D3580D8F ]] || die 'The repository signing key does not match. Nothing was trusted.'
  printf 'Types: deb\nURIs: %s\nSuites: stable\nComponents: main\nArchitectures: amd64\nSigned-By: /etc/apt/keyrings/klicky-archive-keyring.gpg\n' "$archive" > "$work_dir/klicky.sources"
  printf 'Adding the signed Klicky repository and installing the package.\n'
  sudo install -d -m 0755 /etc/apt/keyrings
  sudo install -m 0644 "$work_dir/keyring.gpg" /etc/apt/keyrings/klicky-archive-keyring.gpg
  sudo install -m 0644 "$work_dir/klicky.sources" /etc/apt/sources.list.d/klicky.sources
  sudo apt-get update
  stop_packaged_service
  sudo apt-get install -y klicky
  systemctl --user enable --now klicky.service
  /usr/bin/klicky service start
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
  stop_packaged_service
  sudo pacman -U --noconfirm "$work_dir/$package"
  systemctl --user enable --now klicky.service
  /usr/bin/klicky service start
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
if [[ $platform == Linux ]]; then install_omarchy; fi
printf '\nKlicky is installed and started. Use its desktop control, or try klicky list and klicky volume 0.5.\n'
printf 'A little sound for every keystroke.\n'
