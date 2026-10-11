# Test Drive 3.0 for macOS

A macOS port of [Ask3lad Test Drive 3.0](https://github.com/ask3lad/wt-testdrive3),
replicating the Windows release
[`v3.0.0-alpha.21.1`](https://github.com/ask3lad/wt-testdrive3/releases/tag/v3.0.0-alpha.21.1).

Test Drive sets up custom Ground, Naval, Air and Helicopter test drives in War
Thunder: pick a vehicle, choose its loadout, targets, map and weather, press
**Apply**, then load the mission from User Missions in game.

## Status

The app builds as a universal (Apple Silicon + Intel) `.dmg`. The UI is the
shipped 3.0.0-alpha.21.1 frontend, unmodified, so every screen, all 12
languages and all 3,302 vehicles are the ones from the Windows release. The
backend is a new macOS implementation.

**It has not been run against War Thunder on a real Mac yet.** The backend
compiles clean and its logic is unit-tested, but the one thing no amount of
static work can settle is how the game's own folder is laid out on macOS. See
[docs/PORTING.md](docs/PORTING.md) for exactly what is verified, what is
inferred, and what to test first.

## Install

Download the `.dmg` from the releases page and drag the app to Applications.

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
