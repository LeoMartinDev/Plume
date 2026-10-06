use std::fs;
use std::process::{Command, Stdio};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[test]
fn native_helper_waits_for_parent_exit_before_replacing_the_app() {
    let root = std::env::temp_dir().join(format!(
        "plume-helper-test-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let work = root.join(".plume-update-test");
    let staged = work.join("extracted/new");
    let target = root.join("Plume");
    fs::create_dir_all(&staged).unwrap();
    fs::create_dir(&target).unwrap();
    let executable = format!("plume{}", std::env::consts::EXE_SUFFIX);
    fs::write(target.join(&executable), b"previous app").unwrap();
    // A native binary keeps this test independent of shell executable formats
    // and permissions. Its no-argument exit does not spawn more processes.
    fs::copy(
        env!("CARGO_BIN_EXE_plume-updater"),
        staged.join(&executable),
    )
    .unwrap();
    let mut parent = if cfg!(windows) {
        Command::new("cmd")
            .args(["/C", "ping -n 30 127.0.0.1 >NUL"])
            .stdout(Stdio::null())
            .spawn()
            .unwrap()
    } else {
        Command::new("/bin/sleep").arg("30").spawn().unwrap()
    };
    let plan = stt_updater::InstallPlan {
        target: target.clone(),
        staged,
        backup: root.join(".Plume.previous-test"),
        executable: executable.into(),
        parent_pid: parent.id(),
        work: work.clone(),
    };
    let job = work.join("install.json");
    fs::write(&job, serde_json::to_vec(&plan).unwrap()).unwrap();
    let mut helper = Command::new(env!("CARGO_BIN_EXE_plume-updater"))
        .arg(job)
        .spawn()
        .unwrap();
    std::thread::sleep(Duration::from_millis(200));
    assert_eq!(
        fs::read(target.join(&plan.executable)).unwrap(),
        b"previous app"
    );
    assert!(helper.try_wait().unwrap().is_none());
    parent.kill().unwrap();
    parent.wait().unwrap();
    assert!(helper.wait().unwrap().success());
    assert_eq!(
        fs::read(plan.backup.join(&plan.executable)).unwrap(),
        b"previous app"
    );
    assert_eq!(
        fs::read(target.join(&plan.executable)).unwrap(),
        fs::read(env!("CARGO_BIN_EXE_plume-updater")).unwrap()
    );
    // Windows may briefly retain a handle to the just-started binary.
    for _ in 0..20 {
        if fs::remove_dir_all(&root).is_ok() {
            return;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    fs::remove_dir_all(root).unwrap();
}
