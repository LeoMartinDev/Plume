#[cfg(target_os = "windows")]
fn main() {
    use std::path::PathBuf;

    println!("cargo:rerun-if-env-changed=WINDIR");

    // The Vulkan backend itself is linked into whisper.cpp. Shipping the
    // loader beside the executable lets the same app start on machines where
    // no graphics driver installed a system-wide loader; whisper.cpp can then
    // discover no Vulkan device and continue with its CPU backend.
    let windows = std::env::var_os("WINDIR").expect("WINDIR is set on Windows");
    let loader = PathBuf::from(windows).join("System32").join("vulkan-1.dll");
    assert!(
        loader.is_file(),
        "the Windows Vulkan loader is required to package stt-app"
    );
    println!("cargo:rerun-if-changed={}", loader.display());

    let out_dir = PathBuf::from(std::env::var_os("OUT_DIR").expect("Cargo sets OUT_DIR"));
    let profile_dir = out_dir
        .ancestors()
        .nth(3)
        .expect("OUT_DIR is inside target/<profile>/build/<package>/out");
    std::fs::copy(&loader, profile_dir.join("vulkan-1.dll"))
        .expect("copy the Vulkan loader beside stt-app");
}

#[cfg(not(target_os = "windows"))]
fn main() {}
