//! Prints how Plume judges the focused control, once a second. Click around
//! other apps while it runs; it never types, pastes or reads any text.
//! cargo run -p plume-inject --example focus_probe
//!
//! NoFocus: Plume copies the text instead of pasting it.
use plume_inject::NativeInjector;
use std::time::Duration;

fn main() {
    let injector = NativeInjector::connect().expect("connect to the desktop session");
    let mut last = None;
    loop {
        let current = injector.target_info();
        if last.as_ref() != Some(&current) {
            let (application, target) = &current;
            println!(
                "{:<28} {target:?}",
                application.as_deref().unwrap_or("<unknown app>")
            );
            last = Some(current);
        }
        std::thread::sleep(Duration::from_secs(1));
    }
}
