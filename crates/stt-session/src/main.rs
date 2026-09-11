fn main() {
    let config = stt_session::Config::from_env().unwrap_or_else(|err| {
        eprintln!("stt-session: invalid config: {err}");
        std::process::exit(err.exit_code());
    });
    if let Err(err) = stt_session::run(config) {
        eprintln!("stt-session: startup failed: {err}");
        std::process::exit(err.exit_code());
    }
}
