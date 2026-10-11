//! Mission writing, backups and restore.
//!
//! The app's safety promise is that every Apply is reversible: before a file is
//! written its previous contents are captured, and `restore_backup` puts them
//! back (deleting files that did not exist before). That promise is implemented
//! here.
//!
//! All paths coming from the frontend are War-Thunder-relative with forward
//! slashes (e.g. `UserMissions/ask3lad_testdrive.blk`). They are attacker-
//! adjacent input as far as this module is concerned, so every one of them goes
//! through [`safe_join`], which guarantees the result stays inside the game
//! folder.

use serde::{Deserialize, Serialize};
use std::path::{Component, Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Change {
    pub path: String,
    pub after: String,
    #[serde(default)]
    pub before: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupFile {
    pub path: String,
    pub existed: bool,
    /// Previous contents, present only when `existed` is true.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Backup {
    pub id: i64,
    pub wt: String,
    pub label: String,
    pub at: String,
    pub files: Vec<BackupFile>,
}

#[derive(Debug, thiserror::Error)]
pub enum ApplyError {
    #[error("path escapes the War Thunder folder: {0}")]
    Escape(String),
    #[error("not a War Thunder folder: {0}")]
    NotGameDir(String),
    #[error("backup {0} not found")]
    NoBackup(i64),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

/// Join `rel` onto `root`, refusing anything that would escape `root`.
///
/// Rejects absolute paths, `..` components, Windows drive prefixes and, on
/// macOS, anything that resolves outside `root` through a symlink.
pub fn safe_join(root: &Path, rel: &str) -> Result<PathBuf, ApplyError> {
    let esc = || ApplyError::Escape(rel.to_string());
    if rel.is_empty() {
        return Err(esc());
    }
    // Normalise separators: the frontend may emit either.
    let rel = rel.replace('\\', "/");
    // Reject absolute, UNC and drive-letter forms outright.
    if rel.starts_with('/') || rel.starts_with("//") {
        return Err(esc());
    }
    if rel.len() >= 2 && rel.as_bytes()[1] == b':' {
        return Err(esc());
    }
    let mut out = root.to_path_buf();
    for part in Path::new(&rel).components() {
        match part {
            Component::Normal(p) => out.push(p),
            // Everything else (RootDir, Prefix, ParentDir, CurDir) is refused
            // rather than normalised away, so no input can climb out.
            _ => return Err(esc()),
        }
    }
    // Defence in depth: if the parent chain already exists, make sure its real
    // location is still inside root (catches a pre-existing symlink).
    if let Some(parent) = out.parent() {
        if let Ok(real_parent) = parent.canonicalize() {
            let real_root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
            if !real_parent.starts_with(&real_root) {
                return Err(esc());
            }
        }
    }
    Ok(out)
}

fn write_file(target: &Path, contents: &str) -> Result<(), ApplyError> {
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(target, contents)?;
    Ok(())
}

/// Apply `changes` to the game folder, returning a [`Backup`] describing how to
/// undo them. The backup is built *before* anything is written.
pub fn apply(
    wt: &Path,
    label: &str,
    changes: &[Change],
    id: i64,
    now: String,
) -> Result<Backup, ApplyError> {
    if !crate::wt::is_wt_dir(wt) {
        return Err(ApplyError::NotGameDir(wt.display().to_string()));
    }

    // Resolve and validate every target first: a bad path must abort the whole
    // apply rather than leave a half-written mission set behind.
    let mut planned: Vec<(PathBuf, &Change)> = Vec::with_capacity(changes.len());
    for c in changes {
        planned.push((safe_join(wt, &c.path)?, c));
    }

    let mut files = Vec::with_capacity(planned.len());
    for (target, c) in &planned {
        let existed = target.exists();
        let before = if existed {
            // Fall back to the frontend's `before` if the file is not valid
            // UTF-8, rather than failing the apply.
            std::fs::read_to_string(target)
                .ok()
                .or_else(|| c.before.clone())
        } else {
            None
        };
        files.push(BackupFile {
            path: c.path.clone(),
            existed,
            before,
        });
    }

    for (target, c) in &planned {
        write_file(target, &c.after)?;
    }

    Ok(Backup {
        id,
        wt: wt.display().to_string(),
        label: label.to_string(),
        at: now,
        files,
    })
}

/// Undo a backup. Returns the number of files put back.
pub fn restore(backup: &Backup) -> Result<usize, ApplyError> {
    let wt = PathBuf::from(&backup.wt);
    let mut n = 0usize;
    for f in &backup.files {
        let target = safe_join(&wt, &f.path)?;
        if f.existed {
            if let Some(before) = &f.before {
                write_file(&target, before)?;
                n += 1;
            }
        } else if target.exists() {
            // The file did not exist before the apply, so remove it again.
            std::fs::remove_file(&target)?;
            n += 1;
        }
    }
    Ok(n)
}

/// Install only those files that are not already present. Returns the
/// War-Thunder-relative paths that were written.
pub fn install_missing(wt: &Path, files: &[Change]) -> Result<Vec<String>, ApplyError> {
    if !crate::wt::is_wt_dir(wt) {
        return Err(ApplyError::NotGameDir(wt.display().to_string()));
    }
    let mut written = Vec::new();
    for f in files {
        let target = safe_join(wt, &f.path)?;
        if !target.exists() {
            write_file(&target, &f.after)?;
            written.push(f.path.clone());
        }
    }
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn game(name: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("apply_{name}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(p.join("UserMissions")).unwrap();
        fs::write(p.join("aces.vromfs.bin"), b"x").unwrap();
        p
    }

    fn ch(path: &str, after: &str) -> Change {
        Change {
            path: path.into(),
            after: after.into(),
            before: None,
        }
    }

    #[test]
    fn rejects_traversal_and_absolute_paths() {
        let root = Path::new("/game");
        for bad in [
            "../evil.blk",
            "UserMissions/../../evil.blk",
            "/etc/passwd",
            "C:\\Windows\\system32",
            "..",
            "",
            "./../x",
        ] {
            assert!(safe_join(root, bad).is_err(), "should reject {bad:?}");
        }
    }

    #[test]
    fn accepts_normal_relative_paths() {
        let root = Path::new("/game");
        assert_eq!(
            safe_join(root, "UserMissions/ask3lad_testdrive.blk").unwrap(),
            root.join("UserMissions/ask3lad_testdrive.blk")
        );
        // Backslashes from the Windows-era frontend normalise to subpaths.
        assert_eq!(
            safe_join(root, "UserMissions\\a.blk").unwrap(),
            root.join("UserMissions/a.blk")
        );
    }

    #[test]
    fn apply_then_restore_round_trips_a_new_file() {
        let g = game("newfile");
        let changes = vec![ch("UserMissions/td.blk", "mission v1")];
        let b = apply(&g, "first", &changes, 1, "now".into()).unwrap();

        assert_eq!(
            fs::read_to_string(g.join("UserMissions/td.blk")).unwrap(),
            "mission v1"
        );
        assert_eq!(b.files.len(), 1);
        assert!(!b.files[0].existed);

        // Undo removes it, because it did not exist before.
        assert_eq!(restore(&b).unwrap(), 1);
        assert!(!g.join("UserMissions/td.blk").exists());
    }

    #[test]
    fn apply_then_restore_round_trips_an_overwrite() {
        let g = game("overwrite");
        fs::write(g.join("UserMissions/td.blk"), "original").unwrap();

        let b = apply(
            &g,
            "second",
            &[ch("UserMissions/td.blk", "replaced")],
            2,
            "now".into(),
        )
        .unwrap();
        assert_eq!(
            fs::read_to_string(g.join("UserMissions/td.blk")).unwrap(),
            "replaced"
        );
        assert!(b.files[0].existed);
        assert_eq!(b.files[0].before.as_deref(), Some("original"));

        assert_eq!(restore(&b).unwrap(), 1);
        assert_eq!(
            fs::read_to_string(g.join("UserMissions/td.blk")).unwrap(),
            "original"
        );
    }

    #[test]
    fn a_bad_path_aborts_the_whole_apply() {
        let g = game("abort");
        let changes = vec![
            ch("UserMissions/good.blk", "ok"),
            ch("../escape.blk", "bad"),
        ];
        assert!(apply(&g, "mixed", &changes, 3, "now".into()).is_err());
        // Nothing was written, not even the valid first entry.
        assert!(!g.join("UserMissions/good.blk").exists());
    }

    #[test]
    fn refuses_a_folder_that_is_not_war_thunder() {
        let d = std::env::temp_dir().join(format!("notgame_{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        assert!(apply(&d, "x", &[ch("UserMissions/a.blk", "y")], 1, "now".into()).is_err());
    }

    #[test]
    fn install_missing_skips_existing_files() {
        let g = game("missing");
        fs::write(g.join("UserMissions/present.blk"), "keep me").unwrap();
        let files = vec![
            ch("UserMissions/present.blk", "overwrite?"),
            ch("UserMissions/absent.blk", "new"),
        ];
        let written = install_missing(&g, &files).unwrap();
        assert_eq!(written, vec!["UserMissions/absent.blk".to_string()]);
        // The existing file is left exactly as it was.
        assert_eq!(
            fs::read_to_string(g.join("UserMissions/present.blk")).unwrap(),
            "keep me"
        );
    }

    #[test]
    fn apply_creates_intermediate_directories() {
        let g = game("mkdir");
        apply(
            &g,
            "deep",
            &[ch("content/pkg_local/levels/x.blk", "lvl")],
            4,
            "now".into(),
        )
        .unwrap();
        assert!(g.join("content/pkg_local/levels/x.blk").exists());
    }
}
