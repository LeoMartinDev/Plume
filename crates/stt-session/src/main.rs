fn main() {
    let config = stt_session::Config::from_env().unwrap_or_else(|err| {
        eprintln!("stt-session: invalid config: {err}");
        std::process::exit(err.exit_code());
    });
    match stt_session::run(config) {
        Ok(never) => match never {},
        Err(err) => {
            eprintln!("stt-session: startup failed: {err}");
            std::process::exit(err.exit_code());
        }
    }
}
