mod archive;
mod github;
mod install;

pub use github::{check, Release};
pub use install::{apply, installation, prepare, start_helper, InstallPlan, Installation};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const RELEASES_URL: &str = "https://github.com/LeoMartinDev/Plume/releases";
pub type Result<T> = std::result::Result<T, String>;

pub fn target() -> Result<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => Ok("aarch64-apple-darwin"),
        ("windows", "x86_64") => Ok("x86_64-pc-windows-msvc"),
        ("linux", "x86_64") => Ok("x86_64-unknown-linux-gnu"),
        _ => Err("Updates are not available for this platform.".into()),
    }
}

fn io<T>(result: std::io::Result<T>) -> Result<T> {
    result.map_err(|error| error.to_string())
}
