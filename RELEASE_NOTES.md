# Moscow3d — War Thunder Mac Release

Test Drive for macOS. Set up your own Ground, Naval, Air and Helicopter test
drives in War Thunder: pick any vehicle, choose its loadout, targets, map and
weather, press **Apply**, then load the mission from **User Missions** in game.

A macOS port of Test Drive 3.0, built from the 3.0.0-alpha.21.1 Windows
release. Same interface, same 3,302 vehicles, same 12 languages.

## What works

- **Ground, Naval, Air and Helicopter** test drives, confirmed running in game
- Every vehicle from the current patch, with custom ammo, loadouts and targets
- Air Duel, custom maps, weather and spawn options
- Presets and TD- share codes
- Undo Last Apply, Repair Map and Missions, Reset to Default
- Universal build: native on Apple Silicon and Intel

## Requirements

macOS 10.15 or later, and War Thunder installed via the Gaijin launcher.

## Install

**1. Download and open the `.dmg`**, drag the app to Applications.

**2. Clear the Gatekeeper flag.** The app is not code-signed, so macOS blocks
it on first launch — the same warning Windows shows for the unsigned installer.
In Terminal:

```sh
xattr -dr com.apple.quarantine "/Applications/Ask3lad Test Drive 3.app"
```

**3. Run the one-time setup.** On macOS the game lives inside
`/Applications/WarThunderLauncher.app`, and macOS does not let one app write
inside another app's bundle. The setup script moves the two folders Test Drive
writes to outside the bundle and links them back:

```sh
./scripts/macos-setup.sh
```

It keeps backups, verifies both folders are writable, and reverses with
`--undo`.

**4. Launch the app**, confirm your War Thunder folder, and press
**Repair Map and Missions** once to install the map.

## Known limitations

- **Some aircraft cannot be applied.** The Apply button stays disabled for
  certain planes while others work normally — the vehicle database describes
  them differently from the installed game version. Ground, naval and
  helicopter are unaffected.
- **Not code-signed or notarised.** Hence steps 2 and 3. A signed build would
  remove both.
- **Vehicle preview images are not downloaded** in this build.
- The app icon is low resolution.

## Credits

Test Drive, its missions and maps are by **Ask3lad**. Vehicle data from the
War Thunder Datamine by Gszabi99. Localisation by モブ, Pyrenees General,
Zenturion7, WeyLines and Vanilla Wong.

Not affiliated with Gaijin Entertainment. War Thunder is a trademark of
Gaijin Entertainment.
