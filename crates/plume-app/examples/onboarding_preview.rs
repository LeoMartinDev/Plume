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
        Some("languages") => OnboardingStep::Languages,
        Some("shortcuts") => OnboardingStep::Shortcuts,
        Some("permissions") => OnboardingStep::Permissions,
        _ => OnboardingStep::Model,
    };
    if std::env::args().any(|arg| arg == "--french") {
        prefs.set_interface_language(plume_app::prefs::InterfaceLanguage::FRENCH);
    }
    if let Some(code) = std::env::args()
        .find_map(|arg| arg.strip_prefix("--interface-language=").map(str::to_owned))
    {
        prefs.set_interface_language(
            plume_app::prefs::InterfaceLanguage::from_code(&code)
                .unwrap_or_else(|| panic!("unknown interface language: {code}")),
        );
    }
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
