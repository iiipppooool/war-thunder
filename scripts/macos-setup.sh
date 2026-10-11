#!/bin/bash
#
# Make War Thunder's writable folders reachable from outside its app bundle.
#
# Why this exists: on macOS the game lives inside
# /Applications/WarThunderLauncher.app, and since macOS 13 one app may not
# write inside another app's bundle without App Management permission. That
# permission does not persist for an ad-hoc signed app, so Test Drive cannot
# reliably write the files it needs:
#
#   UserMissions/
#   content/pkg_local/levels/
#   content/pkg_local/gameData/flightModels/{,fm,weaponPresets}/
#   content/pkg_local/gameData/units/{tankModels,ships}/userVehicles/
#   content/pkg_local/gameData/{Weapons,weapons}/ask3lad/
#
# All nine live under two roots: UserMissions and content/pkg_local. This
# script moves both out to ~/My Games/WarThunder/td3 and symlinks them back,
# so the app writes to an ordinary folder and the game reads through the link.
#
# Terminal already holds the permission needed to create the links, which is
# why this runs here rather than from inside the app.
#
# Idempotent, keeps .bak copies, and `--undo` reverses it.

set -euo pipefail

GAME_DEFAULT="/Applications/WarThunderLauncher.app/Contents/WarThunder.app/Contents/Resources/game"
GAME="${TD3_GAME_DIR:-$GAME_DEFAULT}"
EXT="$HOME/My Games/WarThunder/td3"
ROOTS=("UserMissions" "content/pkg_local")

die() { printf 'error: %s\n' "$1" >&2; exit 1; }

[ -d "$GAME" ] || die "game folder not found: $GAME
Set TD3_GAME_DIR to your War Thunder game directory and re-run."
[ -f "$GAME/aces.vromfs.bin" ] || die "$GAME does not look like a War Thunder folder (no aces.vromfs.bin)"

undo() {
  for rel in "${ROOTS[@]}"; do
    target="$GAME/$rel"
    if [ -L "$target" ]; then
      rm "$target"
      printf 'removed link  %s\n' "$rel"
      if [ -d "$target.bak" ]; then
        mv "$target.bak" "$target"
        printf 'restored      %s (from .bak)\n' "$rel"
      fi
    else
      printf 'not linked    %s (nothing to undo)\n' "$rel"
    fi
  done
  printf '\nYour files remain in %s -- delete it by hand if you no longer want them.\n' "$EXT"
}

setup() {
  for rel in "${ROOTS[@]}"; do
    target="$GAME/$rel"
    outside="$EXT/$(basename "$rel")"
    mkdir -p "$outside"

    if [ -L "$target" ]; then
      printf 'already linked  %s -> %s\n' "$rel" "$(readlink "$target")"
      continue
    fi

    if [ -d "$target" ]; then
      # Preserve whatever the game or a previous install left there.
      cp -R "$target/." "$outside/" 2>/dev/null || true
      mv "$target" "$target.bak"
      printf 'moved aside     %s -> %s.bak\n' "$rel" "$rel"
    else
      mkdir -p "$(dirname "$target")"
    fi

    ln -s "$outside" "$target"
    printf 'linked          %s -> %s\n' "$rel" "$outside"
  done

  printf '\nVerifying writes...\n'
  ok=1
  for rel in "${ROOTS[@]}"; do
    probe="$GAME/$rel/.td3_write_probe"
    if touch "$probe" 2>/dev/null; then
      rm -f "$probe"
      printf '  OK      %s is writable\n' "$rel"
    else
      printf '  FAILED  %s is NOT writable\n' "$rel"
      ok=0
    fi
  done

  if [ "$ok" -eq 1 ]; then
    printf '\nDone. Open Test Drive, press "Repair Map and Missions", then Apply.\n'
  else
    printf '\nSomething is still blocking writes. Send the FAILED line above.\n'
    exit 1
  fi
}

case "${1:-}" in
  --undo) undo ;;
  "")     setup ;;
  *)      die "usage: $0 [--undo]" ;;
esac
