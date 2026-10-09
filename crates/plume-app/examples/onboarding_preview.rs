//! Visual-only onboarding preview, with isolated preferences and no native services.
use gpui::{App, Application};
use plume_app::{
    assets::Assets,
    dirs::AppDirs,
    prefs::{OnboardingStep, Prefs},
    settings::open_onboarding_preview,
};

fn main() {
    plume_logging::init();
    let mut prefs = Prefs::default_fresh();
    prefs.onboarding_step = match std::env::args().nth(1).as_deref() {
        Some("shortcuts") => OnboardingStep::Shortcuts,
        Some("permissions") => OnboardingStep::Permissions,
        _ => OnboardingStep::Model,
    };
    if std::env::args().any(|arg| arg == "--dark") {
        prefs.set_appearance(plume_app::prefs::AppearancePref::Fixed(
            plume_app::prefs::Scheme::Dark,
        ));
    }
    let root =
        std::env::temp_dir().join(format!("plume-onboarding-preview-{}", std::process::id()));
    Application::new()
        .with_assets(Assets)
        .run(move |cx: &mut App| {
            open_onboarding_preview(cx, prefs, AppDirs::from_roots(&root, &root));
            cx.activate(true);
        });
}
