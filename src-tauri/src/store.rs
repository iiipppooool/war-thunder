//! On-disk state: settings, backup history and the custom background.
//!
//! The Windows build kept `settings.json` next to its per-user data in
//! `%LOCALAPPDATA%`. The macOS equivalent is
//! `~/Library/Application Support/com.ask3lad.testdrive3`, which Tauri hands us
//! as the app data dir, so nothing here hardcodes a path.

use crate::apply::Backup;
use serde_json::Value;
use std::path::{Path, PathBuf};

/// How many applies to keep undo information for.
const MAX_BACKUPS: usize = 50;

pub fn settings_path(data_dir: &Path) -> PathBuf {
    data_dir.join("settings.json")
}
pub fn backups_path(data_dir: &Path) -> PathBuf {
    data_dir.join("backups.json")
}
pub fn pictures_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("Vehicle_Previews")
}
pub fn background_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("background")
}

/// Load settings, returning `{}` when absent or unreadable.
///
/// A corrupt settings file must not stop the app from starting, so a parse
/// failure degrades to defaults rather than propagating.
pub fn load_settings(data_dir: &Path) -> Value {
    std::fs::read_to_string(settings_path(data_dir))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_else(|| Value::Object(Default::default()))
}

pub fn save_settings(data_dir: &Path, settings: &Value) -> std::io::Result<()> {
    std::fs::create_dir_all(data_dir)?;
    let tmp = settings_path(data_dir).with_extension("json.tmp");
    std::fs::write(
        &tmp,
        serde_json::to_vec_pretty(settings).unwrap_or_default(),
    )?;
    // Atomic replace, so a crash mid-write cannot truncate the settings.
    std::fs::rename(tmp, settings_path(data_dir))
}

pub fn load_backups(data_dir: &Path) -> Vec<Backup> {
    std::fs::read_to_string(backups_path(data_dir))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save_backups(data_dir: &Path, backups: &[Backup]) -> std::io::Result<()> {
    std::fs::create_dir_all(data_dir)?;
    let tmp = backups_path(data_dir).with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec(backups).unwrap_or_default())?;
    std::fs::rename(tmp, backups_path(data_dir))
}

/// Append a backup, newest first, trimming the history to [`MAX_BACKUPS`].
pub fn push_backup(backups: &mut Vec<Backup>, b: Backup) {
    backups.insert(0, b);
    backups.truncate(MAX_BACKUPS);
}

/// Next backup id: one past the highest seen, so ids stay unique across runs.
pub fn next_id(backups: &[Backup]) -> i64 {
    backups.iter().map(|b| b.id).max().unwrap_or(0) + 1
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::apply::Backup;
    use std::fs;

    fn dir(name: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("store_{name}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }

    fn bk(id: i64) -> Backup {
        Backup {
            id,
            wt: "/game".into(),
            label: format!("b{id}"),
            at: "now".into(),
            files: vec![],
        }
    }

    #[test]
    fn settings_round_trip() {
        let d = dir("settings");
        assert_eq!(load_settings(&d), serde_json::json!({}));
        let v = serde_json::json!({ "wtDir": "/Applications/War Thunder", "lang": "en" });
        save_settings(&d, &v).unwrap();
        assert_eq!(load_settings(&d), v);
    }

    #[test]
    fn corrupt_settings_degrade_to_defaults() {
        let d = dir("corrupt");
        fs::write(settings_path(&d), b"{ this is not json").unwrap();
        assert_eq!(load_settings(&d), serde_json::json!({}));
    }

    #[test]
    fn backups_round_trip_and_trim() {
        let d = dir("backups");
        let mut v = Vec::new();
        for i in 1..=60 {
            push_backup(&mut v, bk(i));
        }
        assert_eq!(v.len(), MAX_BACKUPS);
        // Newest first.
        assert_eq!(v[0].id, 60);
        save_backups(&d, &v).unwrap();
        assert_eq!(load_backups(&d).len(), MAX_BACKUPS);
    }

    #[test]
    fn ids_increase_monotonically() {
        assert_eq!(next_id(&[]), 1);
        assert_eq!(next_id(&[bk(1), bk(7), bk(3)]), 8);
    }
}
