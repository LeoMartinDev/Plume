#[cfg(target_os = "windows")]
fn platform_resources() {
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
fn platform_resources() {}

fn main() {
    embed_locales();
    platform_resources();
}

fn embed_locales() {
    use std::path::PathBuf;
    let root = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap()).join("../../locales");
    println!("cargo:rerun-if-changed={}", root.display());
    let mut files: Vec<_> = std::fs::read_dir(&root)
        .expect("read locales directory")
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "json")
        })
        .collect();
    files.sort();
    assert!(
        files.iter().any(|file| file.file_stem().unwrap() == "en"),
        "locales/en.json is required"
    );
    let mut source = String::from("const LOCALE_SOURCES: &[(&str, &str)] = &[\n");
    for path in files {
        let code = path.file_stem().unwrap().to_str().unwrap();
        assert!(
            !code.is_empty()
                && code
                    .bytes()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-'),
            "locale filenames must be lowercase language codes: {}",
            path.display()
        );
        let raw = std::fs::read_to_string(&path).expect("read locale JSON");
        let json: serde_json::Value = serde_json::from_str(&raw)
            .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
        assert!(
            json["name"]
                .as_str()
                .is_some_and(|name| !name.trim().is_empty()),
            "{} needs a non-empty name",
            path.display()
        );
        assert!(
            json["messages"]
                .as_object()
                .is_some_and(|messages| messages.values().all(|value| value.is_string())),
            "{} needs a messages object containing only strings",
            path.display()
        );
        source.push_str(&format!(
            "    ({code:?}, include_str!({:?})),\n",
            path.to_string_lossy()
        ));
    }
    source.push_str("];\n");
    let output = PathBuf::from(std::env::var_os("OUT_DIR").unwrap()).join("locales.rs");
    std::fs::write(output, source).expect("write bundled locale sources");
}
