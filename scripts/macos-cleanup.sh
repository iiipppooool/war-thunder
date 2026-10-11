#!/bin/bash
#
# Undo the mess left by Test Drive builds before #17.
#
# Earlier builds got two things wrong on disk:
#
#   * install_map sorted bundled files by extension, so the three level
#     definitions (Ask3lad_Testdrive.blk, _Air.blk, _Naval.blk) were written
#     to UserMissions/ instead of content/pkg_local/levels/. The game tried to
#     load them as missions, and the levels they describe were missing.
#   * Diagnostic probes (perm_test.txt, .td3_write_probe) were left behind.
#     An empty .blk in UserMissions is a broken entry in the game's list.
#
# This moves the level files where they belong, removes the probes and any
# stray duplicate at the top of UserMissions, then prints the final layout.
# It only touches files these builds created. Backups and the external
# symlink folder are reported, never deleted, unless --purge-backups is given.

set -euo pipefail

GAME_DEFAULT="/Applications/WarThunderLauncher.app/Contents/WarThunder.app/Contents/Resources/game"
GAME="${TD3_GAME_DIR:-$GAME_DEFAULT}"
LEVELS="$GAME/content/pkg_local/levels"
MISSIONS="$GAME/UserMissions"
LEVEL_BLKS=("Ask3lad_Testdrive.blk" "Ask3lad_Testdrive_Air.blk" "Ask3lad_Testdrive_Naval.blk")

die() { printf 'error: %s\n' "$1" >&2; exit 1; }

[ -d "$GAME" ] || die "game folder not found: $GAME
Set TD3_GAME_DIR and re-run."
[ -f "$GAME/aces.vromfs.bin" ] || die "$GAME does not look like a War Thunder folder"

printf '== level definitions ==\n'
mkdir -p "$LEVELS"
for f in "${LEVEL_BLKS[@]}"; do
  stray="$MISSIONS/$f"
  if [ -f "$stray" ]; then
    if [ -f "$LEVELS/$f" ]; then
      rm "$stray"
      printf '  removed duplicate from UserMissions: %s\n' "$f"
    else
      mv "$stray" "$LEVELS/$f"
      printf '  moved to levels/: %s\n' "$f"
    fi
  elif [ -f "$LEVELS/$f" ]; then
    printf '  already correct: %s\n' "$f"
  else
    printf '  MISSING: %s -- press "Repair Map and Missions" in the app\n' "$f"
  fi
done

printf '\n== diagnostic probes ==\n'
found_probe=0
while IFS= read -r p; do
  [ -n "$p" ] || continue
  rm -f "$p"
  printf '  removed %s\n' "${p#"$GAME"/}"
  found_probe=1
done < <(find "$GAME" -maxdepth 4 \( -name 'perm_test.txt' -o -name '.td3_write_probe' \) 2>/dev/null || true)
[ "$found_probe" -eq 0 ] && printf '  none found\n'

printf '\n== stray missions at the top of UserMissions ==\n'
found_stray=0
for p in "$MISSIONS"/ask3lad_testdrive*.blk; do
  [ -f "$p" ] || continue
  # The app writes missions into UserMissions/Ask3lad/; a lowercase copy at
  # the top level is a leftover from manual testing.
  rm "$p"
  printf '  removed duplicate %s\n' "$(basename "$p")"
  found_stray=1
done
[ "$found_stray" -eq 0 ] && printf '  none found\n'

printf '\n== empty .blk files (broken entries in the game list) ==\n'
found_empty=0
while IFS= read -r p; do
  [ -n "$p" ] || continue
  rm -f "$p"
  printf '  removed empty %s\n' "${p#"$GAME"/}"
  found_empty=1
done < <(find "$MISSIONS" -name '*.blk' -size 0 2>/dev/null || true)
[ "$found_empty" -eq 0 ] && printf '  none found\n'

printf '\n== backups left by earlier steps ==\n'
for b in "$MISSIONS.bak" "$GAME/content/pkg_local.bak" "$HOME/My Games/WarThunder/td3" \
         "$HOME/Desktop/td3-data-backup" "$HOME/Desktop/wt-backup"; do
  if [ -e "$b" ]; then
    printf '  %s  (%s)\n' "$b" "$(du -sh "$b" 2>/dev/null | cut -f1)"
    if [ "${1:-}" = "--purge-backups" ]; then
      rm -rf "$b"
      printf '    purged\n'
    fi
  fi
done
[ "${1:-}" = "--purge-backups" ] || printf '  (kept -- re-run with --purge-backups to delete these)\n'

printf '\n== final layout ==\n'
printf 'levels/ (expect 5 files):\n'
ls "$LEVELS" 2>/dev/null | sed 's/^/  /' || printf '  (missing)\n'
printf 'UserMissions/ (expect only the Ask3lad folder):\n'
ls "$MISSIONS" 2>/dev/null | sed 's/^/  /' || printf '  (missing)\n'
printf 'UserMissions/Ask3lad/ (the generated missions):\n'
ls "$MISSIONS/Ask3lad" 2>/dev/null | sed 's/^/  /' || printf '  (none yet -- press Apply)\n'
