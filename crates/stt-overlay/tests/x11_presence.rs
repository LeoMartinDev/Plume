use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

fn skip_without_x11() -> bool {
    match std::env::var("DISPLAY") {
        Ok(display) if !display.is_empty() => false,
        _ => {
            eprintln!("skip: no X11 DISPLAY; overlay window presence not proven");
            true
        }
    }
}

fn xwininfo_mapped(title: &str, display: &str) -> Result<String, String> {
    let output = Command::new("xwininfo")
        .args(["-name", title])
        .env("DISPLAY", display)
        .output()
        .map_err(|err| format!("xwininfo spawn failed: {err}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    if output.status.success() && stdout.contains("Map State: IsViewable") {
        Ok(stdout)
    } else {
        Err(format!(
            "xwininfo exit={} stdout={stdout} stderr={stderr}",
            output.status
        ))
    }
}

#[test]
fn overlay_window_is_mapped_on_x11() {
    if skip_without_x11() {
        return;
    }

    let display = std::env::var("DISPLAY").expect("DISPLAY");
    if let Ok(existing) = xwininfo_mapped(stt_overlay::WINDOW_TITLE, &display) {
        panic!(
            "leftover window titled {}:\n{existing}",
            stt_overlay::WINDOW_TITLE
        );
    }

    let bin = env!("CARGO_BIN_EXE_stt-overlay");
    let mut child = Command::new(bin)
        .env("DISPLAY", &display)
        .env_remove("WAYLAND_DISPLAY")
        .env_remove("ZED_HEADLESS")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap_or_else(|err| panic!("spawn {bin}: {err}"));

    let stderr = child.stderr.take().expect("piped stderr");
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let reader = BufReader::new(stderr);
        for line in reader.lines().map_while(Result::ok) {
            if line.contains("window opened") {
                let _ = tx.send(line);
                break;
            }
        }
    });

    let opened = rx.recv_timeout(Duration::from_secs(20));
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut last_err = opened
        .as_ref()
        .err()
        .map(|err| format!("window-opened log: {err}"))
        .unwrap_or_default();
    let mut mapped = None;
    while Instant::now() < deadline {
        match xwininfo_mapped(stt_overlay::WINDOW_TITLE, &display) {
            Ok(info) => {
                mapped = Some(info);
                break;
            }
            Err(err) => last_err = err,
        }
        if child.try_wait().ok().flatten().is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(250));
    }

    let _ = child.kill();
    let waited = child.wait_with_output();
    let mapped = mapped.unwrap_or_else(|| {
        panic!(
            "stt-overlay window not mapped on DISPLAY={display}. last={last_err} opened={opened:?} wait={waited:?}"
        )
    });
    assert!(
        mapped.contains(stt_overlay::WINDOW_TITLE),
        "xwininfo title missing from {mapped}"
    );
}
