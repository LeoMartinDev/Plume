fn main() {
    stt_logging::init();
    let config = stt_session::Config::from_env().unwrap_or_else(|err| {
        tracing::error!("stt-session: invalid config: {err}");
        std::process::exit(err.exit_code());
    });
    if let Err(err) = stt_session::run(config) {
        tracing::error!("stt-session: startup failed: {err}");
        std::process::exit(err.exit_code());
    }
}
