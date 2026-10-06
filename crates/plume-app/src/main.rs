#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

fn main() {
    if std::env::args().nth(1).as_deref() == Some("--version") {
        println!("Plume {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    plume_app::product_main();
}
