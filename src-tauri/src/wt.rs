//! War Thunder installation discovery and game-folder layout.
//!
//! The Windows build located the game through the registry
//! (`Software\Valve\Steam` -> `SteamPath`), `libraryfolders.vdf`, and a
//! drive-letter sweep (`C:`..`F:` + `\War Thunder`, `\Games\War Thunder`,
//! `\SteamLibrary\steamapps\common\War Thunder`), validating each hit against
//! marker files such as `aces.vromfs.bin`.
//!
//! macOS has no registry and no drive letters, so this module keeps the same
//! shape -- enumerate candidates, then validate by marker -- over the paths a
//! Mac install actually uses. Nothing here is guessed at runtime: a candidate
//! only counts as a game folder if [`is_wt_dir`] finds a marker, and the user
//! can always pick the folder by hand (`check_wt`), exactly as on Windows.

use std::path::{Path, PathBuf};

/// Files/dirs that identify a real War Thunder folder. These are engine data
/// files, not platform binaries, so they are present on a Mac install too.
const MARKERS: &[&str] = &[
    "aces.vromfs.bin",
    "char.vromfs.bin",
    "content/pkg_local",
    "gamedata",
];

/// True when `dir` looks like the root of a War Thunder installation.
pub fn is_wt_dir(dir: &Path) -> bool {
    MARKERS.iter().any(|m| dir.join(m).exists())
}

fn home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

/// Where a game folder can hide relative to a directory the user points at.
///
/// On macOS the Gaijin launcher nests the game two bundles deep:
/// `WarThunderLauncher.app/Contents/WarThunder.app/Contents/Resources/game`,
/// and that `game` directory is the real analogue of the Windows install root
/// (it holds `aces.vromfs.bin` and friends). Someone picking the folder by
/// hand will naturally choose the launcher or the inner app, so accept any
/// level and resolve down to the directory that actually holds the data.
const NESTED: &[&str] = &[
    "Contents/WarThunder.app/Contents/Resources/game",
    "Contents/Resources/game",
    "Contents/MacOS/game",
    "Contents/Resources",
    "game",
];

/// Resolve `dir` to the directory that actually holds the game data, or `None`.
pub fn resolve_wt_dir(dir: &Path) -> Option<PathBuf> {
    if is_wt_dir(dir) {
        return Some(dir.to_path_buf());
    }
    for inner in NESTED {
        let c = dir.join(inner);
        if is_wt_dir(&c) {
            return Some(c);
        }
    }
    None
}

/// Parse Steam's `libraryfolders.vdf` for additional library roots.
///
/// The file is a loose VDF; the Windows build scraped `"path"` keys rather than
/// fully parsing it, and the same approach is used here.
pub fn steam_library_paths(vdf: &Path) -> Vec<PathBuf> {
    let Ok(text) = std::fs::read_to_string(vdf) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        // e.g.   "path"    "/Volumes/Games/SteamLibrary"
        if !line.starts_with("\"path\"") {
            continue;
        }
        let mut parts = line.split('"').filter(|s| !s.trim().is_empty());
        let _key = parts.next();
        if let Some(val) = parts.next() {
            let val = val.trim();
            if !val.is_empty() {
                out.push(PathBuf::from(val.replace("\\\\", "/")));
            }
        }
    }
    out
}

/// Every place a War Thunder install may sit on macOS, most likely first.
pub fn candidate_dirs() -> Vec<PathBuf> {
    let mut c: Vec<PathBuf> = Vec::new();
    let h = home();

    // Steam: default library, then any extra libraries from libraryfolders.vdf.
    if let Some(h) = &h {
        let steam = h.join("Library/Application Support/Steam");
        c.push(steam.join("steamapps/common/War Thunder"));
        for lib in steam_library_paths(&steam.join("steamapps/libraryfolders.vdf")) {
            c.push(lib.join("steamapps/common/War Thunder"));
        }
    }

    // The Gaijin launcher is the normal macOS install: the game lives inside
    // its bundle, so the launcher path resolves down via NESTED.
    for p in [
        "/Applications/WarThunderLauncher.app",
        "/Applications/War Thunder Launcher.app",
        "/Applications/WarThunder.app",
        "/Applications/War Thunder.app",
        "/Applications/War Thunder",
        "/Applications/WarThunder",
    ] {
        c.push(PathBuf::from(p));
    }
    if let Some(h) = &h {
        for p in [
            "Applications/WarThunderLauncher.app",
            "Applications/War Thunder.app",
            "Applications/WarThunder.app",
            "Library/Application Support/WarThunder",
            "Library/Application Support/War Thunder",
            "Documents/My Games/WarThunder",
            "Games/War Thunder",
        ] {
            c.push(h.join(p));
        }
    }

    // External volumes: a game moved to a second drive is common on Macs.
    if let Ok(vols) = std::fs::read_dir("/Volumes") {
        for v in vols.flatten() {
            let v = v.path();
            for p in [
                "War Thunder",
                "WarThunder",
                "Games/War Thunder",
                "SteamLibrary/steamapps/common/War Thunder",
                "steamapps/common/War Thunder",
            ] {
                c.push(v.join(p));
            }
        }
    }
    c
}

/// Validated War Thunder folders, de-duplicated, best match first.
pub fn find_installs() -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    for cand in candidate_dirs() {
        if let Some(dir) = resolve_wt_dir(&cand) {
            let dir = dir.canonicalize().unwrap_or(dir);
            if !out.contains(&dir) {
                out.push(dir);
            }
        }
    }
    out
}

/// `~/My Games/WarThunder` -- where macOS keeps saves and logs, separate from
/// the game data inside the app bundle.
pub fn user_data_dir() -> Option<PathBuf> {
    let d = home()?.join("My Games/WarThunder");
    d.is_dir().then_some(d)
}

/// `<wt>/UserMissions` -- where the game reads custom missions from.
pub fn user_missions(wt: &Path) -> PathBuf {
    wt.join("UserMissions")
}

/// `<wt>/content/pkg_local/levels` -- where the custom map is installed.
pub fn levels_dir(wt: &Path) -> PathBuf {
    wt.join("content/pkg_local/levels")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tmp(name: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("wt_test_{name}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn marker_detects_install() {
        let d = tmp("marker");
        assert!(!is_wt_dir(&d));
        fs::write(d.join("aces.vromfs.bin"), b"x").unwrap();
        assert!(is_wt_dir(&d));
    }

    #[test]
    fn gamedata_dir_counts_as_marker() {
        let d = tmp("gamedata");
        fs::create_dir_all(d.join("gamedata")).unwrap();
        assert!(is_wt_dir(&d));
    }

    #[test]
    fn resolves_into_app_bundle_resources() {
        let d = tmp("bundle");
        let res = d.join("Contents/Resources");
        fs::create_dir_all(&res).unwrap();
        fs::write(res.join("aces.vromfs.bin"), b"x").unwrap();
        // The bundle root itself has no marker...
        assert!(!is_wt_dir(&d));
        // ...but resolving descends into Contents/Resources.
        assert_eq!(resolve_wt_dir(&d).unwrap(), res);
    }

    /// The real macOS layout, as found on a live install: the game sits two
    /// app bundles deep inside the Gaijin launcher, and only that inner
    /// `game` directory holds the vromfs files.
    #[test]
    fn resolves_the_real_macos_launcher_layout() {
        let d = tmp("launcher");
        let launcher = d.join("WarThunderLauncher.app");
        let game = launcher.join("Contents/WarThunder.app/Contents/Resources/game");
        fs::create_dir_all(&game).unwrap();
        for f in ["aces.vromfs.bin", "gui.vromfs.bin", "lang.vromfs.bin"] {
            fs::write(game.join(f), b"x").unwrap();
        }

        // Neither outer level looks like a game folder on its own...
        assert!(!is_wt_dir(&launcher));
        assert!(!is_wt_dir(&launcher.join("Contents")));
        // ...but pointing at the launcher resolves down to the game data,
        // which is what someone browsing by hand will pick.
        assert_eq!(resolve_wt_dir(&launcher).unwrap(), game);
        // Picking the inner app works too.
        assert_eq!(
            resolve_wt_dir(&launcher.join("Contents/WarThunder.app")).unwrap(),
            game
        );
        // And the game dir itself is accepted unchanged.
        assert_eq!(resolve_wt_dir(&game).unwrap(), game);
    }

    /// `~/My Games/WarThunder` holds only saves and logs -- no game data --
    /// so it must NOT be mistaken for the install.
    #[test]
    fn user_data_dir_is_not_a_game_dir() {
        let d = tmp("userdata");
        fs::create_dir_all(d.join("Saves")).unwrap();
        fs::create_dir_all(d.join("_game_logs")).unwrap();
        assert!(!is_wt_dir(&d));
        assert!(resolve_wt_dir(&d).is_none());
    }

    #[test]
    fn resolve_returns_none_without_marker() {
        let d = tmp("empty");
        assert!(resolve_wt_dir(&d).is_none());
    }

    #[test]
    fn parses_steam_library_paths() {
        let d = tmp("vdf");
        let vdf = d.join("libraryfolders.vdf");
        fs::write(
            &vdf,
            r#"
"libraryfolders"
{
    "0"
    {
        "path"        "/Users/me/Library/Application Support/Steam"
    }
    "1"
    {
        "path"        "/Volumes/Games/SteamLibrary"
        "label"       ""
    }
}
"#,
        )
        .unwrap();
        let paths = steam_library_paths(&vdf);
        assert_eq!(
            paths,
            vec![
                PathBuf::from("/Users/me/Library/Application Support/Steam"),
                PathBuf::from("/Volumes/Games/SteamLibrary"),
            ]
        );
    }

    #[test]
    fn missing_vdf_is_not_an_error() {
        assert!(steam_library_paths(Path::new("/nope/libraryfolders.vdf")).is_empty());
    }

    #[test]
    fn layout_helpers() {
        let wt = Path::new("/Applications/War Thunder");
        assert_eq!(user_missions(wt), wt.join("UserMissions"));
        assert_eq!(levels_dir(wt), wt.join("content/pkg_local/levels"));
    }
}
