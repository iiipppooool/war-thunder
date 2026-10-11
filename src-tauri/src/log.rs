//! Logging to a file as well as stderr.
//!
//! Launched from Finder, stderr goes nowhere, so diagnosing a failure meant
//! asking the user to run the binary from a terminal. That is a lot to ask
//! mid-problem. Everything now also appends to a log file in the app's data
//! directory, which can simply be sent.

use std::io::Write;
use std::path::PathBuf;
use std::sync::OnceLock;

static LOG_PATH: OnceLock<PathBuf> = OnceLock::new();

/// Point logging at `data_dir/testdrive3.log`, starting a fresh file.
pub fn init(data_dir: &std::path::Path) {
    let path = data_dir.join("testdrive3.log");
    // Start each run clean: a stale log is worse than none, because it looks
    // like evidence from the run being diagnosed.
    let _ = std::fs::write(
        &path,
        format!(
            "Ask3lad Test Drive {} -- log started {}\n",
            env!("CARGO_PKG_VERSION"),
            chrono::Utc::now().to_rfc3339()
        ),
    );
    let _ = LOG_PATH.set(path);
}

/// The log file's location, for showing the user.
pub fn path() -> Option<&'static PathBuf> {
    LOG_PATH.get()
}

/// Write one line to stderr and to the log file.
pub fn line(msg: &str) {
    eprintln!("[testdrive3] {msg}");
    if let Some(p) = LOG_PATH.get() {
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(p)
        {
            let _ = writeln!(f, "{} {msg}", chrono::Utc::now().format("%H:%M:%S"));
        }
    }
}

/// `log::line` with formatting, mirroring `eprintln!`.
#[macro_export]
macro_rules! tdlog {
    ($($arg:tt)*) => { $crate::log::line(&format!($($arg)*)) };
}
