mod transcribe;

enum Command<'a> {
    Version,
    Transcribe(&'a [String]),
    Usage,
}

fn command_from_args(args: &[String]) -> Command<'_> {
    match args.first().map(String::as_str) {
        None | Some("--help") => Command::Version,
        Some("transcribe") => Command::Transcribe(&args[1..]),
        Some(_) => Command::Usage,
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match command_from_args(&args) {
        Command::Version => {
            println!("stt-shell {}", env!("CARGO_PKG_VERSION"));
        }
        Command::Transcribe(rest) => {
            std::process::exit(transcribe::run(rest));
        }
        Command::Usage => {
            eprintln!("{}", transcribe::USAGE);
            std::process::exit(2);
        }
    }
}
