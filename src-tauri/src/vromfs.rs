//! Reading game files out of War Thunder's `.vromfs.bin` archives.
//!
//! War Thunder ships almost nothing as loose files: `gamedata/flightmodels`,
//! `gamedata/weapons` and the rest live inside `aces.vromfs.bin` and its
//! siblings, as binary BLK encoded with zstd. That is true on Windows as well
//! as macOS -- the Windows build of Test Drive reads them with the `wt_blk`
//! crate, which is what this module uses, at the same version.
//!
//! Opening an archive decodes its whole inner container, which is expensive,
//! so each unpacker is built once and cached for the lifetime of the app.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use wt_blk::vromf::{BlkOutputFormat, File, FileFilter, VromfUnpacker};

/// Archives to search, in the order most likely to hold `gamedata/`.
pub const ARCHIVES: &[&str] = &[
    "aces.vromfs.bin",
    "char.vromfs.bin",
    "game.vromfs.bin",
    "mis.vromfs.bin",
    "wwdata.vromfs.bin",
];

/// Whether to apply BLK `override:` blocks when decoding.
///
/// Override blocks change the decoded content, so this must match however the
/// vehicle database was generated -- otherwise weapon stations read from the
/// game disagree with the database and the app reports the installed game as
/// missing weapons it actually has.
///
/// Defaults to true. Set `TD3_BLK_OVERRIDES=0` to decode without them, which
/// is the knob to try if the app wrongly claims your War Thunder version
/// lacks a weapon or station.
fn apply_overrides() -> bool {
    !matches!(
        std::env::var("TD3_BLK_OVERRIDES").as_deref(),
        Ok("0") | Ok("false") | Ok("no")
    )
}

#[derive(Default)]
pub struct VromfCache {
    /// Archive path -> unpacker. `None` records an archive that failed to
    /// open, so a broken or absent file is not retried on every read.
    open: Mutex<HashMap<PathBuf, Option<Arc<VromfUnpacker>>>>,
}

impl VromfCache {
    pub fn new() -> Self {
        Self::default()
    }

    fn unpacker(&self, archive: &Path) -> Option<Arc<VromfUnpacker>> {
        let mut open = self.open.lock().ok()?;
        if let Some(cached) = open.get(archive) {
            return cached.clone();
        }
        let built = File::new(archive)
            .ok()
            .and_then(|f| VromfUnpacker::from_file(&f, false).ok())
            .map(Arc::new);
        if built.is_none() {
            crate::tdlog!("could not open vromf {}", archive.display());
        }
        open.insert(archive.to_path_buf(), built.clone());
        built
    }

    /// Read `rel` from the archives under `game_dir`, as BLK text.
    ///
    /// Returns `None` when no archive holds that path.
    ///
    /// Paths are tried as given and then lowercased. Dagor stores vromf
    /// entries in lower case, while the vehicle database refers to them in
    /// mixed case (`gameData/Weapons/...`), and `unpack_one` compares paths
    /// byte for byte -- so case matters inside an archive even though macOS's
    /// filesystem ignores it. The Windows build looks these up in lower case
    /// for the same reason.
    pub fn read(&self, game_dir: &Path, rel: &str) -> Option<String> {
        let lowered = rel.to_lowercase();
        let mut candidates: Vec<&str> = vec![rel];
        if lowered != rel {
            candidates.push(&lowered);
        }

        for name in ARCHIVES {
            let archive = game_dir.join(name);
            if !archive.is_file() {
                continue;
            }
            let Some(unpacker) = self.unpacker(&archive) else {
                continue;
            };
            for cand in &candidates {
                let wanted = Path::new(cand);
                match unpacker.unpack_one(
                    wanted,
                    Some(BlkOutputFormat::BlkText),
                    apply_overrides(),
                    FileFilter::All,
                ) {
                    Ok(file) => match String::from_utf8(file.buf().to_vec()) {
                        Ok(text) => return Some(text),
                        Err(err) => {
                            crate::tdlog!("{cand} in {name} is not utf-8: {err}");
                        }
                    },
                    // Not in this archive under this spelling: try the next.
                    Err(_) => continue,
                }
            }
        }
        // Only reached for genuine game-data paths now, so this means a
        // real lookup miss rather than a file the app has yet to create.
        crate::tdlog!("not in any vromf: {rel}");
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_archives_yield_none_without_panicking() {
        let cache = VromfCache::new();
        let dir = std::env::temp_dir().join(format!("vromf_none_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(cache.read(&dir, "gamedata/flightmodels/fm/x.blk"), None);
    }

    #[test]
    fn a_corrupt_archive_is_not_retried_or_fatal() {
        let cache = VromfCache::new();
        let dir = std::env::temp_dir().join(format!("vromf_bad_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("aces.vromfs.bin"), b"not a vromf").unwrap();
        // Twice, to exercise the negative cache path.
        assert_eq!(cache.read(&dir, "gamedata/weapons/x.blk"), None);
        assert_eq!(cache.read(&dir, "gamedata/weapons/x.blk"), None);
    }
}
