fn main() {
    plume_logging::init();
    let config = plume_session::Config::from_env().unwrap_or_else(|err| {
        tracing::error!("plume-session: invalid config: {err}");
        std::process::exit(err.exit_code());
    });
    if let Err(err) = plume_session::run(config) {
        tracing::error!("plume-session: startup failed: {err}");
        std::process::exit(err.exit_code());
    }
}
