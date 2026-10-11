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
pub mod vromfs;
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
    // Surface the failure on stderr too: the webview may swallow a rejected
    // command, leaving a blank screen with no visible cause.
    let msg = err.to_string();
    eprintln!("[testdrive3] command error: {msg}");
    msg
}

pub struct AppState {
    pub data_dir: PathBuf,
    /// The app's bundled resources (database JSON, 512 icons, maps). The
    /// frontend builds `td3file://` URLs into here, so it must be servable.
    pub resource_dir: PathBuf,
    pub exe_dir: PathBuf,
    pub backups: Mutex<Vec<Backup>>,
    /// Cached vromf unpackers; game data is packed, not loose.
    pub vromfs: vromfs::VromfCache,
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

/// Read one game file, or `None` when it is missing or unreadable.
///
/// The frontend's contract is `string | null` per path, so an unreadable path
/// must yield `None` rather than failing the call. Returning an error here
/// rejected the whole batch in [`read_game_files`], and a single odd path --
/// one naming a file inside a vromfs archive, say -- took the entire read
/// down with it, leaving the UI with a rejected promise and a blank screen.
///
/// Paths that escape the game folder are refused (as `None`): reads stay
/// inside the install, exactly as writes do.
/// Copy `src` to `dest` by contents alone.
///
/// `std::fs::copy` uses `copyfile(COPYFILE_ALL)` on macOS, which also copies
/// permissions, ACLs and extended attributes. Writing into an app bundle --
/// which is where War Thunder keeps UserMissions on macOS -- can refuse those
/// metadata operations with EPERM ("Operation not permitted") even when
/// writing the bytes is allowed. Moving only the contents avoids that.
fn copy_contents(src: &Path, dest: &Path) -> std::io::Result<u64> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let bytes = std::fs::read(src)?;
    let len = bytes.len() as u64;
    std::fs::write(dest, bytes)?;
    Ok(len)
}

fn read_from_game(state: &AppState, wt_dir: &str, rel: &str) -> Option<String> {
    let root = Path::new(wt_dir);
    let target = match apply::safe_join(root, rel) {
        Ok(t) => t,
        Err(err) => {
            eprintln!("[testdrive3] refusing to read {rel:?}: {err}");
            return None;
        }
    };
    // Loose file first (mods and user content), then the packed archives,
    // which is where the shipped game data actually lives.
    if let Ok(text) = std::fs::read_to_string(&target) {
        return Some(text);
    }
    state.vromfs.read(root, rel)
}

#[tauri::command]
fn read_game_file(state: State<AppState>, wt: String, path: String) -> Option<String> {
    read_from_game(&state, &wt, &path)
}

#[tauri::command]
fn read_game_files(state: State<AppState>, wt: String, paths: Vec<String>) -> Vec<Option<String>> {
    // One unreadable path yields one null, never a failed batch.
    paths
        .iter()
        .map(|p| read_from_game(&state, &wt, p))
        .collect()
}

/// Flight model for a given aircraft id, from `gamedata/flightmodels`.
#[tauri::command]
fn read_flight_model(state: State<AppState>, wt: String, id: String) -> Option<String> {
    [
        format!("gamedata/flightmodels/fm/{id}.blk"),
        format!("gamedata/flightmodels/{id}.blk"),
    ]
    .iter()
    .find_map(|rel| read_from_game(&state, &wt, rel))
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
            copy_contents(&entry.path(), &dest)
                .map_err(|err| e(format!("copying {name} to {}: {err}", dest.display())))?;
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
    copy_contents(src_path, &dest)
        .map_err(|err| e(format!("copying background to {}: {err}", dest.display())))?;
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

/// Receive a frontend error and print it to stderr.
///
/// A webview failure -- a thrown exception, a rejected promise, a CSP
/// violation -- leaves no trace on the Rust side, so a blank window looks
/// identical to a hang. Forwarding them makes the app self-diagnosing: run it
/// from a terminal and the real cause is printed.
#[tauri::command]
fn log_frontend_error(kind: String, message: String) {
    eprintln!("[testdrive3] FRONTEND {kind}: {message}");
}

/// Installed on every page load; see [`log_frontend_error`].
///
/// Serialises `Error` objects properly (a bare `JSON.stringify(err)` yields
/// `{}`, which threw away the first real error) and wraps the IPC layer so
/// every command call, result and rejection is named. A `null` arriving where
/// the frontend expects an array or string is otherwise invisible.
const ERROR_REPORTER: &str = r#"
(function () {
  if (window.__td3_reporter) return;
  window.__td3_reporter = true;

  function fmt(x) {
    if (x instanceof Error) {
      return (x.name || 'Error') + ': ' + (x.message || '') +
             (x.stack ? '\n' + x.stack : '');
    }
    if (typeof x === 'string') return x;
    if (x === null) return 'null';
    if (x === undefined) return 'undefined';
    try {
      var j = JSON.stringify(x);
      // An Error-like object stringifies to {}; dig out its fields instead.
      if (j === '{}' && (x.message || x.stack)) {
        return (x.name || 'Error') + ': ' + (x.message || '') +
               (x.stack ? '\n' + x.stack : '');
      }
      return j;
    } catch (_) { return String(x); }
  }

  function send(kind, msg) {
    try {
      window.__TAURI_INTERNALS__.__td3_raw_invoke('log_frontend_error',
        { kind: String(kind), message: String(msg).slice(0, 4000) });
    } catch (_) {}
  }

  window.addEventListener('error', function (e) {
    send('error', fmt(e.error || e.message) + ' @ ' + (e.filename || '') +
                  ':' + (e.lineno || 0) + ':' + (e.colno || 0));
  });
  window.addEventListener('unhandledrejection', function (e) {
    send('unhandledrejection', fmt(e.reason));
  });
  var ce = console.error;
  console.error = function () {
    send('console.error',
      Array.prototype.slice.call(arguments).map(fmt).join(' '));
    ce.apply(console, arguments);
  };

  // Wrap invoke so the command that returns the unexpected value is named.
  var internals = window.__TAURI_INTERNALS__;
  if (internals && internals.invoke && !internals.__td3_raw_invoke) {
    internals.__td3_raw_invoke = internals.invoke.bind(internals);
    internals.invoke = function (cmd, args, opts) {
      var raw = internals.__td3_raw_invoke;
      if (cmd === 'log_frontend_error') return raw(cmd, args, opts);
      return raw(cmd, args, opts).then(function (res) {
        var t = res === null ? 'null'
              : Array.isArray(res) ? 'array(' + res.length + ')'
              : typeof res;
        var note = '';
        if (res === null) note = '  <-- NULL';
        if (Array.isArray(res) && res.length && res.every(function (x) { return x === null; })) {
          note = '  <-- all null (' + res.length + ')';
        }
        send('ipc', cmd + ' -> ' + t + note);
        return res;
      }, function (err) {
        send('ipc-reject', cmd + ' !! ' + fmt(err));
        throw err;
      });
    };
  }
  send('ready', 'reporter + ipc logger installed');
})();
"#;

// ------------------------------------------------------------------- run loop

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .on_page_load(|window, _| {
            // Install on both Started and Finished: the guard makes it
            // idempotent, and running early catches failures during boot.
            if let Err(err) = window.eval(ERROR_REPORTER) {
                eprintln!("[testdrive3] could not install error reporter: {err}");
            }
        })
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;
            let exe_dir = std::env::current_exe()
                .ok()
                .and_then(|p| p.parent().map(Path::to_path_buf))
                .unwrap_or_else(|| data_dir.clone());
            let resource_dir = app
                .path()
                .resolve("", tauri::path::BaseDirectory::Resource)
                .unwrap_or_else(|_| data_dir.clone());
            let backups = store::load_backups(&data_dir);
            app.manage(AppState {
                data_dir,
                resource_dir,
                exe_dir,
                backups: Mutex::new(backups),
                vromfs: vromfs::VromfCache::new(),
                pictures_running: Mutex::new(false),
            });
            // TD3_DEVTOOLS=1 opens the inspector at launch, so a frontend
            // error can be read without relying on a right-click menu.
            if std::env::var_os("TD3_DEVTOOLS").is_some() {
                use tauri::Manager as _;
                if let Some(w) = app.get_webview_window("main") {
                    w.open_devtools();
                }
            }
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
            log_frontend_error,
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
/// True when `real` sits inside any of `roots`.
///
/// Split out from the protocol handler so it can be tested without a running
/// app: this is the whole security boundary for `td3file://`.
pub fn path_is_allowed(real: &Path, roots: &[PathBuf]) -> bool {
    roots.iter().any(|r| {
        let r = r.canonicalize().unwrap_or_else(|_| r.clone());
        real.starts_with(&r)
    })
}

fn serve_local_file(state: &AppState, raw_path: &str) -> Result<(Vec<u8>, String), u16> {
    let decoded = urlencoding::decode(raw_path.trim_start_matches('/')).map_err(|_| 400u16)?;
    let path = PathBuf::from(decoded.as_ref());
    let real = path.canonicalize().map_err(|_| 404u16)?;
    // The app's own two directories: downloaded/user content in the data dir,
    // and the bundled database and icons in the resource dir. Serving only the
    // data dir meant every icon and the vehicle database came back 403.
    let roots = [state.data_dir.clone(), state.resource_dir.clone()];
    if !path_is_allowed(&real, &roots) {
        eprintln!(
            "[testdrive3] td3file refused (outside app dirs): {}",
            real.display()
        );
        return Err(403);
    }
    let bytes = std::fs::read(&real).map_err(|_| 404u16)?;
    let mime = mime_guess::from_path(&real)
        .first_or_octet_stream()
        .to_string();
    Ok((bytes, mime))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tmp(name: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("serve_{name}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn serves_from_both_app_directories() {
        let data = tmp("data");
        let res = tmp("res");
        fs::create_dir_all(res.join("database/icons")).unwrap();
        let icon = res.join("database/icons/ap_tank.png");
        fs::write(&icon, b"png").unwrap();
        let bg = data.join("background/background.png");
        fs::create_dir_all(bg.parent().unwrap()).unwrap();
        fs::write(&bg, b"png").unwrap();

        let roots = [data.clone(), res.clone()];
        // A bundled icon must be servable -- serving only the data dir is what
        // made every icon 403 and broke the vehicle view.
        assert!(path_is_allowed(&icon.canonicalize().unwrap(), &roots));
        // So must a user background in the data dir.
        assert!(path_is_allowed(&bg.canonicalize().unwrap(), &roots));
    }

    #[test]
    fn refuses_paths_outside_the_app() {
        let data = tmp("data2");
        let res = tmp("res2");
        let roots = [data, res];
        for outside in ["/etc/hosts", "/usr/bin/env"] {
            let p = Path::new(outside);
            if let Ok(real) = p.canonicalize() {
                assert!(!path_is_allowed(&real, &roots), "must refuse {outside}");
            }
        }
    }
}
