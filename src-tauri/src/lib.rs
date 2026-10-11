//! Ask3lad Test Drive 3.0 - macOS backend.
//!
//! The auto-updater plugin is deliberately NOT registered: it errors during
//! plugin init when `plugins.updater` is absent from tauri.conf.json, which
//! aborts startup. Enabling updates means adding both the config block (with
//! your own minisign key) and the plugin registration together -- see
//! docs/PORTING.md.
//!
//! The frontend is the one shipped in the Windows 3.0.0-alpha.21.1 release,
//! unmodified. It is already platform-aware: it picks the `td3file://localhost/`
//! custom-scheme form on non-Windows and chooses its path separator from the
//! game folder string. So this crate only has to provide the same 25 IPC
//! commands over macOS paths, plus the `td3file` scheme.

pub mod apply;
pub mod store;
pub mod wt;

use apply::{Backup, Change};
use serde::Serialize;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::{AppHandle, Manager, State};

/// Commands return a string error; the frontend surfaces it in a toast.
type R<T> = Result<T, String>;

fn e<E: std::fmt::Display>(err: E) -> String {
    err.to_string()
}

pub struct AppState {
    pub data_dir: PathBuf,
    pub exe_dir: PathBuf,
    pub backups: Mutex<Vec<Backup>>,
    pub pictures_running: Mutex<bool>,
}

#[derive(Serialize)]
pub struct AppPaths {
    pub exe_dir: String,
    pub data_dir: String,
}

// ---------------------------------------------------------------- paths / env

#[tauri::command]
fn app_paths(state: State<AppState>) -> AppPaths {
    AppPaths {
        exe_dir: state.exe_dir.display().to_string(),
        data_dir: state.data_dir.display().to_string(),
    }
}

/// Validated War Thunder folders. The frontend takes the first and offers the
/// rest; if this is empty the user picks the folder with the system dialog.
#[tauri::command]
fn find_wt() -> Vec<String> {
    wt::find_installs()
        .iter()
        .map(|p| p.display().to_string())
        .collect()
}

/// Validate a folder the user picked by hand.
#[tauri::command]
fn check_wt(path: String) -> bool {
    wt::resolve_wt_dir(Path::new(&path)).is_some()
}

#[tauri::command]
fn path_exists(path: String) -> bool {
    Path::new(&path).exists()
}

#[tauri::command]
fn read_text(path: String) -> R<String> {
    std::fs::read_to_string(&path).map_err(e)
}

/// Reveal a path in Finder. The Windows build shelled out to explorer.exe.
#[tauri::command]
fn open_path(path: String) -> R<()> {
    let p = Path::new(&path);
    if !p.exists() {
        return Err(format!("path does not exist: {path}"));
    }
    std::process::Command::new("/usr/bin/open")
        .arg("-R")
        .arg(p)
        .spawn()
        .map(|_| ())
        .map_err(e)
}

/// The bundled vehicle database (`resources/database`).
#[tauri::command]
fn find_database(app: AppHandle) -> R<String> {
    let dir = app
        .path()
        .resolve("database", tauri::path::BaseDirectory::Resource)
        .map_err(e)?;
    if !dir.join("manifest.json").exists() {
        return Err(format!("database missing at {}", dir.display()));
    }
    Ok(dir.display().to_string())
}

/// Downloaded vehicle previews, or `None` when the pack is not installed yet.
#[tauri::command]
fn find_previews(state: State<AppState>) -> Option<String> {
    let d = store::pictures_dir(&state.data_dir);
    d.exists().then(|| d.display().to_string())
}

// ------------------------------------------------------------- game file read

fn read_from_game(wt_dir: &str, rel: &str) -> R<Option<String>> {
    let root = Path::new(wt_dir);
    let target = apply::safe_join(root, rel).map_err(e)?;
    Ok(std::fs::read_to_string(target).ok())
}

#[tauri::command]
fn read_game_file(wt: String, path: String) -> R<Option<String>> {
    read_from_game(&wt, &path)
}

#[tauri::command]
fn read_game_files(wt: String, paths: Vec<String>) -> R<Vec<Option<String>>> {
    paths.iter().map(|p| read_from_game(&wt, p)).collect()
}

/// Flight model for a given aircraft id, from `gamedata/flightmodels`.
#[tauri::command]
fn read_flight_model(wt: String, id: String) -> R<Option<String>> {
    for rel in [
        format!("gamedata/flightmodels/fm/{id}.blk"),
        format!("gamedata/flightmodels/{id}.blk"),
    ] {
        if let Some(text) = read_from_game(&wt, &rel)? {
            return Ok(Some(text));
        }
    }
    Ok(None)
}

// ------------------------------------------------------------ apply / backups

#[tauri::command]
fn apply_changes(
    state: State<AppState>,
    wt: String,
    label: String,
    changes: Vec<Change>,
) -> R<i64> {
    let dir = wt::resolve_wt_dir(Path::new(&wt))
        .ok_or_else(|| format!("not a War Thunder folder: {wt}"))?;
    let mut backups = state.backups.lock().map_err(e)?;
    let id = store::next_id(&backups);
    let now = chrono::Utc::now().to_rfc3339();
    let backup = apply::apply(&dir, &label, &changes, id, now).map_err(e)?;
    store::push_backup(&mut backups, backup);
    store::save_backups(&state.data_dir, &backups).map_err(e)?;
    Ok(id)
}

#[tauri::command]
fn list_backups(state: State<AppState>) -> R<Vec<Backup>> {
    Ok(state.backups.lock().map_err(e)?.clone())
}

#[tauri::command]
fn restore_backup(state: State<AppState>, id: i64) -> R<usize> {
    let mut backups = state.backups.lock().map_err(e)?;
    let idx = backups
        .iter()
        .position(|b| b.id == id)
        .ok_or(format!("backup {id} not found"))?;
    let n = apply::restore(&backups[idx]).map_err(e)?;
    // An undone apply is no longer undoable.
    backups.remove(idx);
    store::save_backups(&state.data_dir, &backups).map_err(e)?;
    Ok(n)
}

#[tauri::command]
fn install_missing(wt: String, files: Vec<Change>) -> R<Vec<String>> {
    let dir = wt::resolve_wt_dir(Path::new(&wt))
        .ok_or_else(|| format!("not a War Thunder folder: {wt}"))?;
    apply::install_missing(&dir, &files).map_err(e)
}

/// Copy the bundled test-drive map and mission templates into the game folder.
///
/// With `repair` set, existing files are overwritten -- this is the
/// "Repair Map and Missions" action.
#[tauri::command]
fn install_map(app: AppHandle, wt: String, repair: bool) -> R<Vec<String>> {
    let dir = wt::resolve_wt_dir(Path::new(&wt))
        .ok_or_else(|| format!("not a War Thunder folder: {wt}"))?;
    let src = app
        .path()
        .resolve("maps", tauri::path::BaseDirectory::Resource)
        .map_err(e)?;
    let levels = wt::levels_dir(&dir);
    let missions = wt::user_missions(&dir);
    std::fs::create_dir_all(&levels).map_err(e)?;
    std::fs::create_dir_all(&missions).map_err(e)?;

    let mut installed = Vec::new();
    for entry in std::fs::read_dir(&src).map_err(e)?.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        let ext = Path::new(&name)
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("");
        // Map payloads go to content/pkg_local/levels, missions to UserMissions.
        let dest_dir = if matches!(ext, "bin" | "png") {
            &levels
        } else {
            &missions
        };
        let dest = dest_dir.join(&name);
        if repair || !dest.exists() {
            std::fs::copy(entry.path(), &dest).map_err(e)?;
            installed.push(name);
        }
    }
    Ok(installed)
}

// --------------------------------------------------------- settings / visuals

#[tauri::command]
fn load_settings(state: State<AppState>) -> Value {
    store::load_settings(&state.data_dir)
}

#[tauri::command]
fn save_settings(state: State<AppState>, settings: Value) -> R<()> {
    store::save_settings(&state.data_dir, &settings).map_err(e)
}

/// Copy a user-chosen image or video into app data and return its new path.
#[tauri::command]
fn copy_background(state: State<AppState>, src: String) -> R<Option<String>> {
    let src_path = Path::new(&src);
    let ext = src_path
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default();
    if !matches!(
        ext.as_str(),
        "png" | "jpg" | "jpeg" | "webp" | "mp4" | "webm"
    ) {
        return Err(format!("unsupported background type: .{ext}"));
    }
    let dir = store::background_dir(&state.data_dir);
    std::fs::create_dir_all(&dir).map_err(e)?;
    // Only one background is kept, so clear any previous one first.
    for old in std::fs::read_dir(&dir).map_err(e)?.flatten() {
        let _ = std::fs::remove_file(old.path());
    }
    let dest = dir.join(format!("background.{ext}"));
    std::fs::copy(src_path, &dest).map_err(e)?;
    Ok(Some(dest.display().to_string()))
}

#[tauri::command]
fn remove_background(state: State<AppState>) -> R<()> {
    let dir = store::background_dir(&state.data_dir);
    if dir.exists() {
        for old in std::fs::read_dir(&dir).map_err(e)?.flatten() {
            let _ = std::fs::remove_file(old.path());
        }
    }
    Ok(())
}

// ------------------------------------------------------------------- pictures

#[derive(Serialize)]
pub struct PicturesStatus {
    pub dir: String,
    pub done: Vec<String>,
    pub running: bool,
}

#[tauri::command]
fn pictures_status(state: State<AppState>) -> R<PicturesStatus> {
    let dir = store::pictures_dir(&state.data_dir);
    let mut done = Vec::new();
    if let Ok(rd) = std::fs::read_dir(&dir) {
        for f in rd.flatten() {
            if let Some(stem) = f.path().file_stem().and_then(|s| s.to_str()) {
                done.push(stem.to_string());
            }
        }
    }
    done.sort();
    Ok(PicturesStatus {
        dir: dir.display().to_string(),
        done,
        running: *state.pictures_running.lock().map_err(e)?,
    })
}

/// Which of `ids` have no preview image on disk yet.
#[tauri::command]
fn pictures_missing(state: State<AppState>, ids: Vec<String>) -> Vec<String> {
    let dir = store::pictures_dir(&state.data_dir);
    ids.into_iter()
        .filter(|id| {
            !["png", "jpg", "jpeg", "webp"]
                .iter()
                .any(|ext| dir.join(format!("{id}.{ext}")).exists())
        })
        .collect()
}

// The preview pack is fetched from the release repo. Downloading is deliberately
// left unimplemented rather than pointed at a guessed URL: see docs/PORTING.md.
#[tauri::command]
fn download_pictures() -> R<()> {
    Err(
        "The vehicle preview pack download is not wired up in the macOS build yet. \
         See docs/PORTING.md."
            .into(),
    )
}

#[tauri::command]
fn download_new_pictures() -> R<()> {
    download_pictures()
}

// ------------------------------------------------------------------- run loop

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;
            let exe_dir = std::env::current_exe()
                .ok()
                .and_then(|p| p.parent().map(Path::to_path_buf))
                .unwrap_or_else(|| data_dir.clone());
            let backups = store::load_backups(&data_dir);
            app.manage(AppState {
                data_dir,
                exe_dir,
                backups: Mutex::new(backups),
                pictures_running: Mutex::new(false),
            });
            Ok(())
        })
        // The frontend builds URLs as `td3file://localhost/<urlencoded abs path>`
        // on macOS to show local images and video (backgrounds, previews).
        .register_uri_scheme_protocol("td3file", |ctx, request| {
            let state = ctx.app_handle().state::<AppState>();
            match serve_local_file(&state, request.uri().path()) {
                Ok((body, mime)) => tauri::http::Response::builder()
                    .status(200)
                    .header("Content-Type", mime)
                    .header("Access-Control-Allow-Origin", "*")
                    .body(body)
                    .unwrap(),
                Err(status) => tauri::http::Response::builder()
                    .status(status)
                    .body(Vec::new())
                    .unwrap(),
            }
        })
        .invoke_handler(tauri::generate_handler![
            app_paths,
            find_wt,
            check_wt,
            path_exists,
            read_text,
            open_path,
            find_database,
            find_previews,
            read_game_file,
            read_game_files,
            read_flight_model,
            apply_changes,
            list_backups,
            restore_backup,
            install_missing,
            install_map,
            load_settings,
            save_settings,
            copy_background,
            remove_background,
            pictures_status,
            pictures_missing,
            download_pictures,
            download_new_pictures,
        ])
        .run(tauri::generate_context!())
        // Report why startup failed instead of aborting: `panic = "abort"`
        // turns a panic here into a bare SIGABRT crash with no explanation.
        .unwrap_or_else(|err| {
            eprintln!("Test Drive 3 failed to start: {err}");
            std::process::exit(1);
        });
}

/// Resolve a `td3file://` request to bytes.
///
/// Only files under the app data dir or the bundled resources are served, so a
/// stray URL in the webview cannot read arbitrary disk.
fn serve_local_file(state: &AppState, raw_path: &str) -> Result<(Vec<u8>, String), u16> {
    let decoded = urlencoding::decode(raw_path.trim_start_matches('/')).map_err(|_| 400u16)?;
    let path = PathBuf::from(decoded.as_ref());
    let real = path.canonicalize().map_err(|_| 404u16)?;
    let allowed = state
        .data_dir
        .canonicalize()
        .unwrap_or_else(|_| state.data_dir.clone());
    if !real.starts_with(&allowed) {
        return Err(403);
    }
    let bytes = std::fs::read(&real).map_err(|_| 404u16)?;
    let mime = mime_guess::from_path(&real)
        .first_or_octet_stream()
        .to_string();
    Ok((bytes, mime))
}
