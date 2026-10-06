#![cfg_attr(windows, windows_subsystem = "windows")]

fn main() {
    let arguments: Vec<_> = std::env::args_os().collect();
    if arguments.len() == 2 && arguments[1] == "--help" {
        println!(
            "Plume updater {} — internal update installer",
            plume_updater::VERSION
        );
        return;
    }
    if arguments.len() != 2 {
        std::process::exit(2);
    }
    let path = std::path::Path::new(&arguments[1]);
    let result = std::fs::read(path)
        .map_err(|error| error.to_string())
        .and_then(|data| {
            serde_json::from_slice::<plume_updater::InstallPlan>(&data)
                .map_err(|error| error.to_string())
        })
        .and_then(|plan| plume_updater::apply(&plan));
    if let Err(error) = result {
        let _ = std::fs::write(path.with_extension("error.txt"), &error);
        eprintln!("Plume update failed: {error}");
        std::process::exit(1);
    }
}
