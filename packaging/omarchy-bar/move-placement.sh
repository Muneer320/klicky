#!/usr/bin/env bash
# The widget may be recreated during a move, so finish outside its lifetime.
set -euo pipefail

[[ $# == 3 ]] || exit 2
case "$1:$2" in
  left:left|right:right|clock-left:centerBegin|clock-right:centerEnd) ;;
  *) exit 2 ;;
esac
[[ $3 =~ ^[0-9]+$ ]] || exit 2
[[ -n ${XDG_RUNTIME_DIR:-} && -d $XDG_RUNTIME_DIR && -O $XDG_RUNTIME_DIR ]] || exit 2
exec 9>"$XDG_RUNTIME_DIR/klicky-bar-placement.lock"
flock -n 9 || exit 0

trap 'notify-send "Klicky Bar" "Could not save bar placement. Open Klicky and try again." || true' ERR
omarchy bar move io.github.muneer320.klicky-bar --section "$2" --index "$3"
# Save only after the native move succeeds; retain executable and other settings.
omarchy bar set io.github.muneer320.klicky-bar placement "$1"
