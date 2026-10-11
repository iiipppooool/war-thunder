# How this port was made

The upstream repository `ask3lad/wt-testdrive3` holds releases only — its README
says so, and the repo contains nothing but `README.md`. There was no source to
port, so the Windows release was taken apart instead.

## What the Windows build turned out to be

`TestDrive3-Setup-3.0.0-alpha.21.1.exe` (47 MB, sha256
`880005913ef2d67a2fe9e9807dd4857394de48d4b256c5d7ac7e46b83d194ac9`) is an NSIS
installer. `nsis_tauri_utils.dll` in its plugin directory gives the app away: it
is a **Tauri 2** app, not Electron. Versions read out of the binary:

| Component | Version |
| --- | --- |
| tauri | 2.12.1 |
| tauri-runtime-wry | 2.12.1 |
| wry | 0.57.0 |
| tao | 0.37.1 |
| tauri-plugin-dialog | 2.8.1 |
| tauri-plugin-updater | 2.13.1 |

That mattered a lot. Tauri compiles the Rust backend to machine code but embeds
the frontend as data, so the two halves recover very differently.

## What was recovered, and how

The installer extracts with 7-Zip 23 (p7zip 16.02 cannot read NSIS). Inside:
the Tauri executable, `WebView2Loader.dll`, `database/`, and `maps/`.

The frontend is five brotli-compressed assets inside the executable. They are
not findable by scanning for brotli streams, so they were located through the
asset table instead: find an asset key string, compute its virtual address,
search for an 8-byte pointer to it, and read the `(key_ptr, key_len, data_ptr,
data_len)` record — stride 32, data stored immediately after each key.

| Asset | Compressed | Decompressed |
| --- | --- | --- |
| `assets/index-uGjlRpS7.js` | 298,436 | 1,437,369 |
| `assets/index-Cnem9IJH.css` | 13,369 | 67,157 |
| `assets/default-BAiWD5T3.png` | 85,983 | 85,978 |
| `assets/tour_user_missions-IrHAGeE-.webp` | 26,709 | 26,718 |
| `index.html` | 288 | 622 |

All of that is in `dist/`, byte-identical to what the Windows build serves.
`resources/` holds the data files straight out of the installer: 3,302 vehicles
(1,239 ground, 1,362 air, 113 heli, 588 naval) built against game patch 2.59,
plus the map and the three mission templates.

The Rust backend could not be recovered — it is compiled x86-64. It was
rewritten from its interface.

## Why the UI needed no changes

The shipped frontend is already cross-platform. Two lines decide it:

```js
a = /Windows/i.test(navigator.userAgent) ? "http://td3file.localhost/" : "td3file://localhost/"
_t = (t, ...a) => [t.replace(/[\\/]+$/, ""), ...a].join(t.includes("\\") ? "\\" : "/")
```

It picks the non-Windows custom-scheme form on macOS, and its path-join helper
takes the separator from the game folder string rather than assuming `\`. So the
port is backend-only: `dist/` is shipped untouched.

## The backend contract

The bundle also ships a mock implementation used by the in-app preview, which
pins down every return shape exactly. 24 commands, reimplemented in
`src-tauri/src/`:

| Command | Returns |
| --- | --- |
| `app_paths` | `{exe_dir, data_dir}` |
| `find_wt` | `string[]` — validated game folders |
| `check_wt(path)` | `bool` |
| `path_exists(path)` / `read_text(path)` | `bool` / `string` |
| `open_path(path)` | reveals in Finder |
| `find_database` / `find_previews` | resource dir / previews dir or null |
| `read_game_file(wt,path)` | `string \| null` |
| `read_game_files(wt,paths)` | `(string \| null)[]` |
| `read_flight_model(wt,id)` | `string \| null` |
| `apply_changes(wt,label,changes)` | backup id |
| `list_backups` / `restore_backup(id)` | `Backup[]` / count restored |
| `install_missing(wt,files)` | paths written |
| `install_map(wt,repair)` | paths written |
| `load_settings` / `save_settings(settings)` | settings JSON |
| `copy_background(src)` / `remove_background` | new path / void |
| `pictures_status` / `pictures_missing(ids)` | status / missing ids |
| `download_pictures` / `download_new_pictures` | **not implemented** |

## Windows-specific logic and its macOS replacement

| Windows | macOS |
| --- | --- |
| `Software\Valve\Steam` → `SteamPath` | `~/Library/Application Support/Steam` |
| `libraryfolders.vdf` `"path"` scrape | same file, same scrape, Mac path |
| drive sweep `C:`–`F:` | `/Applications`, `~/Applications`, `/Volumes/*` |
| validate by `aces.vromfs.bin` | same markers, plus `gamedata`, `char.vromfs.bin`, `content/pkg_local` |
| `%LOCALAPPDATA%\WarThunder` | `~/Library/Application Support/com.ask3lad.testdrive3` |
| `explorer.exe /select` | `/usr/bin/open -R` |
| `http://td3file.localhost/` | `td3file://localhost/` scheme |

macOS can also ship a game inside an `.app`, so `resolve_wt_dir` accepts either
a bundle or its `Contents/Resources` and returns whichever actually holds the
data.

## Verified

- Backend compiles clean: no errors, no warnings, `clippy -D warnings` clean.
- 19 unit tests pass, covering path-traversal refusal (`..`, absolute paths,
  drive letters, symlink escape), apply/undo round-trips for both new and
  overwritten files, abort-on-bad-path leaving nothing written, refusing a
  non-game folder, `install_missing` not clobbering existing files,
  `libraryfolders.vdf` parsing, `.app` bundle resolution, and settings/backup
  persistence including corrupt-file fallback.

## Not verified, and what to test first

1. **The game folder layout on macOS.** This is the big one. No authoritative
   source documents where War Thunder keeps `UserMissions` on macOS, and the
   community threads that ask conflict or go unanswered. The port does not
   guess: it enumerates candidates, validates each by marker file, and falls
   back to the user picking the folder by hand — the same escape hatch the
   Windows build has. Run `find_wt` on a real install first; if it comes back
   empty, add the real path to `candidate_dirs` in `src-tauri/src/wt.rs`.

2. **Backslashes in the mission templates.** Each `.blk` references the map
   twice with a Windows separator:
   `customLevelMap:t="levels\Ask3lad_Testdrive_map.png"`. Whether the Dagor
   engine normalises that on macOS is untested. The author's files are shipped
   byte-identical rather than rewritten on a guess. If the map fails to load,
   that is the first thing to change — swap those two separators and reinstall
   via **Repair Map and Missions**.

3. **Window chrome.** The window is frameless (`decorations: false`) so the
   frontend's own title bar and min/max/close buttons work as they do on
   Windows. That means no macOS traffic lights. If you would rather have native
   ones, set `decorations: true` and `macOS.titleBarStyle` in
   `src-tauri/tauri.conf.json`.

4. **War Thunder on Apple Silicon.** The game ships as x86-64 and runs under
   Rosetta 2; there is no announced native Apple Silicon client. Test Drive
   itself builds universal and runs natively either way, since it only reads and
   writes the game's files.

## Deliberately left undone

- **Preview pack download.** `download_pictures` returns an explanatory error
  instead of calling a guessed URL. The Windows build fetches the pack from the
  release repo, but the exact asset URL and archive layout are not recoverable
  from the binary, and inventing one would fail silently at runtime. Wire it up
  in `src-tauri/src/lib.rs` once the real URL is known. Everything downstream
  (`pictures_status`, `pictures_missing`, the `pictures` progress event, the
  previews directory) is already in place.
- **Code signing and notarisation.** Needs an Apple Developer certificate.
  Without it Gatekeeper blocks first launch; the README documents the
  `xattr -dr com.apple.quarantine` workaround.
- **Auto-updater.** The plugin is **not** registered. Registering it while
  `plugins.updater` is absent from `tauri.conf.json` makes plugin init fail,
  which aborts the app one second into startup with
  `invalid type: null, expected struct Config` -- a bare SIGABRT on macOS.
  The same missing config also fails the bundler. To enable updates, add both
  together: a `plugins.updater` block with your endpoint and minisign public
  key, `"createUpdaterArtifacts": true`, and the
  `tauri_plugin_updater::Builder::new().build()` plugin line in
  `src-tauri/src/lib.rs`. The frontend's update button errors until then.
  CI now runs a headless smoke test that catches this class of startup
  failure, since a bundle can build correctly and still abort on launch.

- **Icon resolution.** The Windows executable only carries 16/32/48 px icons, so
  the macOS icon is upscaled from 48 px and will look soft on a Retina display.
  Drop a 1024×1024 master at `src-tauri/icons/icon.png` and re-run
  `cargo tauri icon` to fix it properly.

## The CSP mistake

Worth recording, because it cost several build cycles. The Windows build's
`tauri.conf.json` is not recoverable from a compiled binary, so the content
security policy here was invented rather than replicated. The policy allowed
`td3file:` under `img-src` and `media-src` but not under `connect-src`, so
every `fetch()` of a `td3file://` URL was blocked -- including the vehicle
database the UI loads when a vehicle is selected. The symptom was a blank
window with no error on the Rust side, because the failure was a CSP
violation inside the webview.

The CSP is now unset (Tauri's default). Re-introducing one is fine, but it
has to be derived from what the frontend actually requests, with the webview
console open to catch violations -- not written from first principles and
shipped. At minimum it needs `td3file:` in `connect-src` as well as
`img-src` and `media-src`.

## macOS will not let the app write where the game reads

The largest platform difference, and the one that cost the most time.

War Thunder on macOS is installed inside the Gaijin launcher's app bundle, and
Test Drive writes to nine folders in there:

    UserMissions/
    content/pkg_local/levels/
    content/pkg_local/gameData/flightModels/{,fm,weaponPresets}/
    content/pkg_local/gameData/units/{tankModels,ships}/userVehicles/
    content/pkg_local/gameData/{Weapons,weapons}/ask3lad/

Since macOS 13, writing inside another application's bundle requires App
Management permission. Without it every write fails with EPERM,
"Operation not permitted" -- even though the folders are owned by the user and
`touch` from Terminal succeeds, because Terminal holds the permission and the
app does not.

Granting it is not a durable fix here. macOS identifies an app for permissions
by its code signature, and these builds are ad-hoc signed, so the grant is
dropped on the next launch or rebuild -- the toggle appears to switch itself
off. **A Developer ID signature is the real fix**, and it also removes the
Gatekeeper warning. Until then `scripts/macos-setup.sh` moves both roots out
of the bundle and symlinks them back, which avoids the permission entirely and
survives rebuilds.

Note for the code: `apply::safe_join` deliberately does not resolve symlinks,
because requiring the resolved path to stay under the game folder would reject
exactly that setup. Traversal is still refused by the component check.

### A process note

This was diagnosed one path at a time -- UserMissions, then pkg_local -- when
one command over the frontend bundle would have listed all nine at the first
failure. Enumerate the class before fixing an instance; the same mistake
appeared with the updater, where fixing the bundler half left the runtime half
broken for the identical reason.
