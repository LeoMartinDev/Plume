//! Shared logging setup for executable entry points. Libraries only emit events.

use std::fs::OpenOptions;
use std::sync::Once;
use tracing_subscriber::EnvFilter;

fn default_filter(development: bool) -> &'static str {
    if development {
        "warn,plume_app=debug,plume_audio=debug,plume_engine=debug,plume_session=debug,plume_overlay=debug,plume_hotkey=debug,plume_inject=debug,plume_shell=debug"
    } else {
        "warn"
    }
}

fn filter(value: Option<&str>, development: bool) -> EnvFilter {
    value
        .filter(|value| !value.trim().is_empty())
        .and_then(|value| EnvFilter::try_new(value).ok())
        .unwrap_or_else(|| EnvFilter::new(default_filter(development)))
}

/// Development builds log app diagnostics; release builds log warnings/errors.
/// RUST_LOG overrides either default. PLUME_LOG_FILE optionally appends to a file,
/// including for Windows release builds that have no console. Safe to call twice
/// or when the embedding application has already installed a subscriber.
pub fn init() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        let value = std::env::var("RUST_LOG").ok();
        let subscriber = tracing_subscriber::fmt()
            .with_env_filter(filter(value.as_deref(), cfg!(debug_assertions)))
            .with_ansi(false);
        match std::env::var_os("PLUME_LOG_FILE") {
            Some(path) => match OpenOptions::new().create(true).append(true).open(path) {
                Ok(file) => {
                    let _ = subscriber.with_writer(file).try_init();
                }
                Err(error) => {
                    let _ = subscriber.with_writer(std::io::stderr).try_init();
                    tracing::warn!(%error, "Cannot open PLUME_LOG_FILE; logging to stderr");
                }
            },
            None => {
                let _ = subscriber.with_writer(std::io::stderr).try_init();
            }
        }
    });
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
