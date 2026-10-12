//! Shared logging setup for executable entry points. Libraries only emit events.

use std::fs::{self, File, OpenOptions};
use std::path::{Path, PathBuf};
use std::sync::Once;
use tracing_subscriber::fmt::writer::MakeWriterExt;
use tracing_subscriber::EnvFilter;

/// A log file past this size is moved aside at startup, keeping one
/// previous generation, so a long-lived install never grows without bound.
const MAX_LOG_BYTES: u64 = 5 * 1024 * 1024;

fn default_filter(development: bool) -> &'static str {
    if development {
        "warn,plume_app=debug,plume_audio=debug,plume_engine=debug,plume_session=debug,plume_overlay=debug,plume_hotkey=debug,plume_inject=debug,plume_shell=debug"
    } else {
        "warn,plume_app=info,plume_audio=info,plume_engine=info,plume_session=info,plume_overlay=info,plume_hotkey=info,plume_inject=info,plume_shell=info"
    }
}

fn filter(value: Option<&str>, development: bool) -> EnvFilter {
    value
        .filter(|value| !value.trim().is_empty())
        .and_then(|value| EnvFilter::try_new(value).ok())
        .unwrap_or_else(|| EnvFilter::new(default_filter(development)))
}

/// Development builds log app diagnostics; release builds log app milestones,
/// warnings and errors. RUST_LOG overrides either default. Logs go to stderr.
/// PLUME_LOG_FILE optionally appends to a file, including for Windows release
/// builds that have no console. Safe to call twice or when the embedding
/// application has already installed a subscriber.
pub fn init() {
    init_with_file(None);
}

/// Like [`init`], but also appends to `default_file` when PLUME_LOG_FILE is
/// unset, so an app launched from Finder or a desktop entry still leaves a
/// trace. Returns the file actually written, if any.
pub fn init_with_file(default_file: Option<PathBuf>) -> Option<PathBuf> {
    static INIT: Once = Once::new();
    let mut written = None;
    INIT.call_once(|| {
        let value = std::env::var("RUST_LOG").ok();
        let subscriber = tracing_subscriber::fmt()
            .with_env_filter(filter(value.as_deref(), cfg!(debug_assertions)))
            .with_ansi(false);
        let path = std::env::var_os("PLUME_LOG_FILE")
            .map(PathBuf::from)
            .or(default_file);
        let Some(path) = path else {
            let _ = subscriber.with_writer(std::io::stderr).try_init();
            return;
        };
        match open_log(&path) {
            Ok(file) => {
                let _ = subscriber
                    .with_writer(std::io::stderr.and(std::sync::Mutex::new(file)))
                    .try_init();
                written = Some(path);
            }
            Err(error) => {
                let _ = subscriber.with_writer(std::io::stderr).try_init();
                tracing::warn!(%error, path = %path.display(), "Cannot open the log file; logging to stderr");
            }
        }
    });
    written
}

fn open_log(path: &Path) -> std::io::Result<File> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    if fs::metadata(path).is_ok_and(|meta| meta.len() > MAX_LOG_BYTES) {
        let _ = fs::rename(path, path.with_extension("old.log"));
    }
    OpenOptions::new().create(true).append(true).open(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn levels(value: Option<&str>, development: bool) -> (bool, bool, bool, bool) {
        let subscriber = tracing_subscriber::fmt()
            .with_env_filter(filter(value, development))
            .with_writer(std::io::sink)
            .finish();
        tracing::subscriber::with_default(subscriber, || {
            (
                tracing::enabled!(target: "plume_session::decoder", tracing::Level::DEBUG),
                tracing::enabled!(target: "plume_session::decoder", tracing::Level::WARN),
                tracing::enabled!(target: "plume_session::decoder", tracing::Level::TRACE),
                tracing::enabled!(target: "third_party", tracing::Level::DEBUG),
            )
        })
    }

    #[test]
    fn defaults_and_overrides_filter_diagnostics_in_both_build_modes() {
        assert_eq!(levels(None, true), (true, true, false, false));
        assert_eq!(levels(None, false), (false, true, false, false));
        let subscriber = tracing_subscriber::fmt()
            .with_env_filter(filter(None, false))
            .with_writer(std::io::sink)
            .finish();
        tracing::subscriber::with_default(subscriber, || {
            assert!(tracing::enabled!(target: "plume_hotkey::macos", tracing::Level::INFO));
            assert!(!tracing::enabled!(target: "third_party", tracing::Level::INFO));
        });
        assert_eq!(
            levels(Some("warn,plume_session=trace"), false),
            (true, true, true, false)
        );
        assert_eq!(levels(Some("off"), true), (false, false, false, false));
        assert_eq!(levels(Some(""), false), levels(None, false));
        assert_eq!(
            levels(Some("plume_session=invalid"), true),
            levels(None, true)
        );
    }

    #[test]
    fn oversized_log_is_moved_aside_before_appending() {
        let dir = std::env::temp_dir().join(format!("plume-logging-{}", std::process::id()));
        let path = dir.join("nested").join("plume.log");
        let _ = fs::remove_dir_all(&dir);
        drop(open_log(&path).unwrap());
        fs::write(&path, vec![b'x'; MAX_LOG_BYTES as usize + 1]).unwrap();
        drop(open_log(&path).unwrap());
        assert_eq!(fs::metadata(&path).unwrap().len(), 0);
        assert_eq!(
            fs::metadata(path.with_extension("old.log")).unwrap().len(),
            MAX_LOG_BYTES + 1
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn disabled_diagnostic_fields_are_not_evaluated() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let evaluated = AtomicUsize::new(0);
        let subscriber = tracing_subscriber::fmt()
            .with_env_filter(filter(None, false))
            .with_writer(std::io::sink)
            .finish();
        tracing::subscriber::with_default(subscriber, || {
            tracing::debug!(target: "plume_audio", device = evaluated.fetch_add(1, Ordering::Relaxed), "Device");
            tracing::trace!(target: "plume_session::decoder", samples = evaluated.fetch_add(1, Ordering::Relaxed), "Audio");
            assert_eq!(evaluated.load(Ordering::Relaxed), 0);
            tracing::warn!(target: "plume_session", error = evaluated.fetch_add(1, Ordering::Relaxed), "Error");
            assert_eq!(evaluated.load(Ordering::Relaxed), 1);
        });
    }
}
