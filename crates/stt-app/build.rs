#[cfg(target_os = "windows")]
fn main() {
    use std::path::PathBuf;

    println!("cargo:rerun-if-changed=assets/brand/plume.rc");
    println!("cargo:rerun-if-changed=assets/brand/plume.ico");
    // GPUI loads resource 1 for the window class; Explorer uses the same icon.
    embed_resource::compile_for("assets/brand/plume.rc", ["plume"], embed_resource::NONE)
        .manifest_required()
        .expect("embed the Plume application icon");

    println!("cargo:rerun-if-env-changed=WINDIR");
    println!("cargo:rerun-if-env-changed=VULKAN_SDK");

    // The Vulkan backend itself is linked into whisper.cpp. Shipping the
    // loader beside the executable lets the same app start on machines where
    // no graphics driver installed a system-wide loader; whisper.cpp can then
    // discover no Vulkan device and continue with its CPU backend.
    let windows = std::env::var_os("WINDIR").expect("WINDIR is set on Windows");
    let system_loader = PathBuf::from(windows).join("System32").join("vulkan-1.dll");
    let loader = std::env::var_os("VULKAN_SDK")
        .map(|sdk| PathBuf::from(sdk).join("Bin").join("vulkan-1.dll"))
        .filter(|file| file.is_file())
        .or_else(|| system_loader.is_file().then_some(system_loader))
        .expect("the Windows Vulkan loader is required to package Plume");
    println!("cargo:rerun-if-changed={}", loader.display());

    let out_dir = PathBuf::from(std::env::var_os("OUT_DIR").expect("Cargo sets OUT_DIR"));
    let profile_dir = out_dir
        .ancestors()
        .nth(3)
        .expect("OUT_DIR is inside target/<profile>/build/<package>/out");
    std::fs::copy(&loader, profile_dir.join("vulkan-1.dll"))
        .expect("copy the Vulkan loader beside Plume");
}

#[cfg(not(target_os = "windows"))]
fn main() {}
