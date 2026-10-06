use std::fs;
use std::process::Command;

#[test]
fn helper_rejects_legacy_archive_plan_without_changing_installation() {
    let root = std::env::temp_dir().join(format!("plume-invalid-plan-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let app = root.join("plume");
    fs::write(&app, b"installed app").unwrap();
    let job = root.join("install.json");
    // Old archive plans cannot invoke folder replacement anymore.
    fs::write(
        &job,
        b"{\"target\":\"/old-installation\",\"staged\":\"/old-stage\"}",
    )
    .unwrap();
    let status = Command::new(env!("CARGO_BIN_EXE_plume-updater"))
        .arg(&job)
        .status()
        .unwrap();
    assert!(!status.success());
    assert_eq!(fs::read(&app).unwrap(), b"installed app");
    assert!(job.with_extension("error.txt").is_file());
    fs::remove_dir_all(root).unwrap();
}
