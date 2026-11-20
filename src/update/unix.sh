#!/bin/sh
# Executed in place of the CLI, so package guards no longer see a Klicky process.
set -u
manager=$1
launcher=$2
was_running=$3
was_paused=$4
menu_running=0
menu_job="gui/$(id -u)/dev.klicky.homebrew-menu"
if [ "$manager" = homebrew ] &&
   launchctl print "$menu_job" 2>/dev/null | grep -Eq '^[[:space:]]*pid = [0-9]+'; then
  menu_running=1
fi

restore_playback() {
  result=$?
  trap - EXIT
  if [ "$was_running" = 1 ]; then
    if ! "$launcher" service start; then
      printf 'Klicky did not restart. Check its service status.\n' >&2
      exit 1
    fi
    if [ "$was_paused" = 1 ] && ! "$launcher" pause; then
      printf 'Klicky restarted but could not restore Pause.\n' >&2
      exit 1
    fi
  fi
  exit "$result"
}

trap restore_playback EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
trap 'exit 129' HUP
"$launcher" service stop || exit 1
"$launcher" stop || exit 1

case "$manager" in
  homebrew)
    brew update && brew upgrade muneer320/tap/klicky || exit $?
    # Its ProgramArguments use the stable opt launcher. Restarting the loaded
    # job picks up the new wrapper without enabling login startup or an opt-out.
    if [ "$menu_running" = 1 ]; then launchctl kickstart -k "$menu_job"; fi
    ;;
  apt) sudo apt-get update && sudo apt-get install --only-upgrade klicky ;;
  pacman) sudo pacman -Syu ;;
  *) printf 'Unrecognized update owner; no package was changed.\n' >&2; exit 1 ;;
esac
