//! Full settings preview with isolated preferences/history and disabled model actions.
use gpui::{App, Application};
use stt_app::{assets::Assets, dirs::AppDirs, prefs::PrefsLoad, settings::open_settings_preview};

fn main() {
    stt_logging::init();
    let root = std::env::args_os()
        .nth(1)
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::env::temp_dir().join(format!("stt-settings-preview-{}", std::process::id()))
        });
    std::fs::create_dir_all(&root).expect("create preview storage");
    eprintln!("Preview data: {}", root.display());
    let history_path = root.join("history.json");
    if !history_path.exists() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let entries: Vec<_> = (1..=150)
            .rev()
            .map(|id| {
                serde_json::json!({
                    "id": id, "created_at": now, "text": format!("Preview transcription {id}"),
                    "application": "Preview", "method": "Typing", "status": "Inserted",
                    "copied_on_failure": false, "error": null,
                })
            })
            .collect();
        std::fs::write(
            &history_path,
            serde_json::to_vec(&serde_json::json!({"version":1,"entries":entries})).unwrap(),
        )
        .unwrap();
    }
    let prefs = match stt_app::prefs::load_at(&root.join("prefs.toml")) {
        PrefsLoad::Fresh(prefs) | PrefsLoad::Loaded(prefs) => prefs,
        PrefsLoad::LoadedWithWarnings { prefs, .. } | PrefsLoad::Quarantined { prefs, .. } => prefs,
    };
    Application::new()
        .with_assets(Assets)
        .run(move |cx: &mut App| {
            open_settings_preview(cx, prefs, AppDirs::from_roots(&root, &root));
            cx.activate(true);
        });
}
