# Test Drive 3.0 for macOS

A macOS port of [Ask3lad Test Drive 3.0](https://github.com/ask3lad/wt-testdrive3),
replicating the Windows release
[`v3.0.0-alpha.21.1`](https://github.com/ask3lad/wt-testdrive3/releases/tag/v3.0.0-alpha.21.1).

Test Drive sets up custom Ground, Naval, Air and Helicopter test drives in War
Thunder: pick a vehicle, choose its loadout, targets, map and weather, press
**Apply**, then load the mission from User Missions in game.

## Status

Builds as a universal (Apple Silicon + Intel) `.dmg`. The UI is the shipped
3.0.0-alpha.21.1 frontend, unmodified, so every screen, all 12 languages and
all 3,302 vehicles are the ones from the Windows release. The backend is a
new macOS implementation.

**Ground, naval and air test drives are confirmed working end to end on
macOS**: Apply writes the mission, the game lists it under
`campaigns/Ask3lad / chapters/TestDrive`, the custom level loads, and the
vehicle spawns. Reading packed game data out of the `.vromfs.bin` archives
works, as does validation against the installed game.

One known limitation: **some aircraft cannot be applied** -- the Apply button
stays disabled for them while others work normally. The app's own validation
is rejecting those vehicles, which points at the bundled database describing
them differently from the installed game. Under investigation; it affects
individual aircraft, not air test drives as a whole.

Two things remain outstanding, both documented in
[docs/PORTING.md](docs/PORTING.md):

- **Writing into the game folder needs setup**, because War Thunder lives
  inside an app bundle and macOS restricts writes there. See the one-time
  setup below.
- **No Developer ID signature.** This is what stops the app being
  distributable: Gatekeeper warns on every download, and the App Management
  permission the app needs does not persist for an ad-hoc signed build.

## Install

Download the `.dmg` from the releases page and drag the app to Applications.

### One-time setup (required)

macOS will not let one app write inside another app's bundle, and War Thunder
on macOS lives inside `/Applications/WarThunderLauncher.app`. Test Drive needs
to write to nine folders under two roots there, so run this once:

```sh
./scripts/macos-setup.sh
```

It moves `UserMissions` and `content/pkg_local` out to
`~/My Games/WarThunder/td3`, symlinks them back, keeps `.bak` copies of
whatever was there, and verifies both are writable. `--undo` reverses it.

### Cleaning up after a pre-#17 build

Builds before #17 wrote the three level definitions into `UserMissions/`
instead of `content/pkg_local/levels/`, and left diagnostic probe files
behind. Both confuse the game's mission list. To put it right:

```sh
./scripts/macos-cleanup.sh
```

It moves the level files where they belong, removes probes, duplicates and
empty `.blk` files, then prints the final layout. Backups are listed but kept
unless you pass `--purge-backups`.

Granting **App Management** in System Settings is the alternative, but macOS
does not keep that grant for an app without a Developer ID signature, so it
switches itself off again. See [docs/PORTING.md](docs/PORTING.md).

The app is **not code-signed or notarised**, so Gatekeeper blocks it on first
launch — the macOS equivalent of the SmartScreen warning on the Windows build.
Either right-click the app and choose **Open**, then **Open** again, or run:

```sh
xattr -dr com.apple.quarantine "/Applications/Ask3lad Test Drive 3.app"
```

Requires macOS 10.15 or later and War Thunder.

## Build from source

```sh
cargo install tauri-cli --version "^2" --locked
cd src-tauri
cargo test                                        # backend logic
cargo tauri build --target universal-apple-darwin  # .app + .dmg
```

The frontend is prebuilt in `dist/`, so there is no npm step.

## Layout

| Path | What it is |
| --- | --- |
| `dist/` | Frontend from the Windows release, byte-identical |
| `resources/database/` | Vehicle database: 3,302 vehicles, game patch 2.59 |
| `resources/maps/` | Test-drive map and the three mission templates |
| `src-tauri/src/wt.rs` | War Thunder discovery on macOS |
| `src-tauri/src/apply.rs` | Mission writing, backups, undo |
| `src-tauri/src/store.rs` | Settings and backup history |
| `src-tauri/src/lib.rs` | The 24 IPC commands and the `td3file` scheme |
| `docs/PORTING.md` | How the port was made and what is unverified |

## Safety

As on Windows, the app only writes its own mission files, never launches the
game, and backs up every file it touches so **Undo Last Apply** can put things
back. Every path from the UI is validated to stay inside the game folder
(`apply::safe_join`), and the `td3file` scheme only serves files from the app's
own data directory.

## Credits

Test Drive, its missions and maps are by **Ask3lad**. Vehicle data from the War
Thunder Datamine by Gszabi99. Localisation by モブ, Pyrenees General,
Zenturion7, WeyLines and Vanilla Wong. See the upstream
[README](https://github.com/ask3lad/wt-testdrive3) for the full credits.

Not affiliated with Gaijin Entertainment. War Thunder is a trademark of Gaijin
Entertainment.
