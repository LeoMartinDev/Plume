use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::{io, Release, Result};

#[derive(Clone, Debug)]
pub struct Installation {
    pub root: PathBuf,
    pub executable: PathBuf,
    pub helper: PathBuf,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InstallPlan {
    pub installer: PathBuf,
    pub version: String,
    pub target: PathBuf,
    pub executable: PathBuf,
    pub parent_pid: u32,
    pub work: PathBuf,
}

fn executable_name(name: &str) -> String {
    format!("{name}{}", std::env::consts::EXE_SUFFIX)
}

pub fn installation() -> Result<Installation> {
    locate(&io(std::env::current_exe())?)
}

fn locate(executable: &Path) -> Result<Installation> {
    let executable = io(executable.canonicalize())?;
    let directory = executable
        .parent()
        .ok_or("Could not find Plume's installation folder")?;
    let helper = directory.join(executable_name("plume-updater"));
    if !helper.is_file() {
        return Err("Install a release build to update Plume from the app.".into());
    }
    let (root, relative) = if cfg!(target_os = "macos") && directory.ends_with("Contents/MacOS") {
        let app = directory
            .parent()
            .and_then(Path::parent)
            .ok_or("Invalid app bundle")?;
        if app.extension() != Some(std::ffi::OsStr::new("app"))
            || !app.join("Contents/Info.plist").is_file()
        {
            return Err("Invalid Plume app bundle".into());
        }
        (app.to_path_buf(), PathBuf::from("Contents/MacOS/plume"))
    } else {
        if !directory.join("RUNTIME-LIBRARIES.txt").is_file() {
            return Err("Install a release build to update Plume from the app.".into());
        }
        (
            directory.to_path_buf(),
            PathBuf::from(executable_name("plume")),
        )
    };
    Ok(Installation {
        root,
        executable: relative,
        helper,
    })
}

pub fn prepare(
    release: &Release,
    installation: &Installation,
    progress: impl Fn(u64, u64),
) -> Result<InstallPlan> {
    prepare_installer(release, installation, progress)
}

fn prepare_installer(
    release: &Release,
    installation: &Installation,
    progress: impl Fn(u64, u64),
) -> Result<InstallPlan> {
    let work = std::env::temp_dir().join(format!(
        ".plume-update-{}-{}",
        std::process::id(),
        io_time()?
    ));
    io(fs::create_dir(&work))?;
    let result = (|| {
        let installer = work.join(&release.installer_name);
        crate::download::download(release, &installer, &progress)?;
        let helper_dir = work.join("helper");
        io(fs::create_dir(&helper_dir))?;
        io(fs::copy(
            &installation.helper,
            helper_dir.join(executable_name("plume-updater")),
        ))?;
        for entry in io(fs::read_dir(
            installation
                .helper
                .parent()
                .ok_or("Missing helper folder")?,
        ))? {
            let entry = io(entry)?;
            if entry
                .path()
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("dll"))
            {
                io(fs::copy(entry.path(), helper_dir.join(entry.file_name())))?;
            }
        }
        let plan = InstallPlan {
            installer,
            version: release.version.clone(),
            target: installation.root.clone(),
            executable: installation.executable.clone(),
            parent_pid: std::process::id(),
            work: work.clone(),
        };
        validate(&plan)?;
        io(fs::write(
            work.join("install.json"),
            serde_json::to_vec(&plan).map_err(|e| e.to_string())?,
        ))?;
        Ok(plan)
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(&work);
    }
    result
}

fn io_time() -> Result<u128> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|time| time.as_nanos())
        .map_err(|e| e.to_string())
}

pub fn start_helper(plan: &InstallPlan) -> Result<()> {
    io(Command::new(
        plan.work
            .join("helper")
            .join(executable_name("plume-updater")),
    )
    .arg(plan.work.join("install.json"))
    .current_dir(plan.work.join("helper"))
    .stdin(Stdio::null())
    .stdout(Stdio::null())
    .stderr(Stdio::null())
    .spawn())?;
    Ok(())
}

pub fn apply(plan: &InstallPlan) -> Result<()> {
    apply_with(plan, install_native)
}

fn apply_with(
    plan: &InstallPlan,
    install: impl FnOnce(&InstallPlan, &Path) -> Result<()>,
) -> Result<()> {
    validate(plan)?;
    wait_for_exit(plan.parent_pid)?;
    if let Err(error) = install(plan, &plan.installer) {
        let _ = launch(plan, Some(&error));
        return Err(error);
    }
    let _ = fs::remove_dir_all(&plan.work);
    Ok(())
}

fn install_native(plan: &InstallPlan, installer: &Path) -> Result<()> {
    #[cfg(windows)]
    let status = io(Command::new(installer)
        .args(["/SILENT", "/SUPPRESSMSGBOXES", "/NORESTART", "/SP-"])
        .arg(format!("/DIR={}", plan.target.display()))
        .status())?;
    #[cfg(target_os = "macos")]
    let status = io(Command::new("/usr/bin/open")
        .args(["-W", "-a", "Installer"])
        .arg(installer)
        .status())?;
    #[cfg(target_os = "linux")]
    let status = io(Command::new("/usr/bin/pkexec")
        .args(["/usr/bin/dpkg", "--install"])
        .arg(installer)
        .status())?;
    if !status.success() {
        return Err("The installer did not complete. You can retry from GitHub Releases.".into());
    }
    // Linux packages and macOS packages own a fixed system installation path.
    // Windows installs in the current user's existing installation directory.
    let mut installed = plan.clone();
    if cfg!(target_os = "linux") {
        installed.target = PathBuf::from("/opt/plume");
        installed.executable = PathBuf::from("plume");
    } else if cfg!(target_os = "macos") {
        installed.target = PathBuf::from("/Applications/Plume.app");
        installed.executable = PathBuf::from("Contents/MacOS/plume");
    }
    let output = io(Command::new(installed.target.join(&installed.executable))
        .arg("--version")
        .output())?;
    if !output.status.success()
        || String::from_utf8_lossy(&output.stdout).trim() != format!("Plume {}", plan.version)
    {
        return Err(
            "Installation was cancelled or the installed version could not be verified.".into(),
        );
    }
    launch(&installed, None)
}

fn launch(plan: &InstallPlan, error: Option<&str>) -> Result<()> {
    if cfg!(target_os = "macos") && plan.executable.starts_with("Contents") {
        let mut command = Command::new("/usr/bin/open");
        command.arg(&plan.target);
        if let Some(error) = error {
            command.args(["--args", "--update-error", error]);
        }
        let status = io(command.status())?;
        if !status.success() {
            return Err("Could not restart Plume".into());
        }
    } else {
        let mut command = Command::new(plan.target.join(&plan.executable));
        command.current_dir(&plan.target);
        if let Some(error) = error {
            command.args(["--update-error", error]);
        }
        io(command.spawn())?;
    }
    Ok(())
}

fn validate(plan: &InstallPlan) -> Result<()> {
    let extension = if cfg!(windows) {
        "exe"
    } else if cfg!(target_os = "macos") {
        "pkg"
    } else {
        "deb"
    };
    if !plan.work.is_absolute()
        || semver::Version::parse(&plan.version).is_err()
        || plan.installer.parent() != Some(plan.work.as_path())
        || !plan.installer.is_file()
        || plan.installer.extension() != Some(std::ffi::OsStr::new(extension))
        || !plan.target.is_absolute()
        || !plan.target.is_dir()
        || !plan
            .work
            .file_name()
            .is_some_and(|name| name.to_string_lossy().starts_with(".plume-update-"))
        || plan.executable.as_os_str().is_empty()
        || plan
            .executable
            .components()
            .any(|part| !matches!(part, std::path::Component::Normal(_)))
    {
        return Err("Invalid native installer plan.".into());
    }
    Ok(())
}

fn wait_for_exit(pid: u32) -> Result<()> {
    if pid == std::process::id() || pid == 0 {
        return Err("Invalid parent process".into());
    }
    let deadline = Instant::now() + Duration::from_secs(60);
    while alive(pid)? {
        if Instant::now() >= deadline {
            return Err("Plume did not close; no update was installed.".into());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    Ok(())
}

#[cfg(unix)]
fn alive(pid: u32) -> Result<bool> {
    if pid > i32::MAX as u32 {
        return Err("Invalid parent process".into());
    }
    // SAFETY: kill with signal 0 only checks whether a process exists.
    let result = unsafe { libc::kill(pid as i32, 0) };
    if result == 0 {
        return Ok(true);
    }
    let error = std::io::Error::last_os_error();
    if error.raw_os_error() == Some(libc::ESRCH) {
        Ok(false)
    } else {
        Err(error.to_string())
    }
}

#[cfg(windows)]
fn alive(pid: u32) -> Result<bool> {
    use windows_sys::Win32::{
        Foundation::{CloseHandle, ERROR_INVALID_PARAMETER, STILL_ACTIVE},
        System::Threading::{GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION},
    };
    // SAFETY: the handle is checked and closed within this function.
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle.is_null() {
            let error = std::io::Error::last_os_error();
            return if error.raw_os_error() == Some(ERROR_INVALID_PARAMETER as i32) {
                Ok(false)
            } else {
                Err(error.to_string())
            };
        }
        let mut code = 0;
        let result = GetExitCodeProcess(handle, &mut code);
        CloseHandle(handle);
        if result == 0 {
            Err(std::io::Error::last_os_error().to_string())
        } else {
            Ok(code == STILL_ACTIVE as u32)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> InstallPlan {
        let root = std::env::temp_dir().join(format!(
            "plume-update-test-{}-{}",
            std::process::id(),
            io_time().unwrap()
        ));
        let work = root.join(".plume-update-test");
        let target = root.join("Plume");
        fs::create_dir_all(&work).unwrap();
        fs::create_dir(&target).unwrap();
        let extension = if cfg!(windows) {
            "exe"
        } else if cfg!(target_os = "macos") {
            "pkg"
        } else {
            "deb"
        };
        let installer = work.join(format!("Plume.{extension}"));
        fs::write(&installer, b"installer").unwrap();
        InstallPlan {
            installer,
            version: "0.2.0".into(),
            target,
            executable: "plume".into(),
            parent_pid: 1,
            work,
        }
    }
    #[test]
    fn rejects_installer_outside_download_folder_and_unsafe_executable() {
        let mut plan = fixture();
        validate(&plan).unwrap();
        let root = plan.target.parent().unwrap().to_path_buf();
        plan.installer = plan.target.join("outside.exe");
        assert!(validate(&plan).is_err());
        plan.installer = fs::read_dir(&plan.work)
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        plan.executable = "../other-app".into();
        assert!(validate(&plan).is_err());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn waits_for_parent_before_starting_installer_without_touching_user_data() {
        let mut plan = fixture();
        let root = plan.target.parent().unwrap().to_path_buf();
        fs::write(root.join("history.json"), b"history").unwrap();
        let mut parent = if cfg!(windows) {
            Command::new(
                PathBuf::from(std::env::var_os("SystemRoot").unwrap()).join("System32/ping.exe"),
            )
            .args(["-n", "30", "127.0.0.1"])
            .stdout(Stdio::null())
            .spawn()
            .unwrap()
        } else {
            Command::new("/bin/sleep").arg("30").spawn().unwrap()
        };
        plan.parent_pid = parent.id();
        let marker = root.join("installed");
        let marker_worker = marker.clone();
        let worker = std::thread::spawn(move || {
            apply_with(&plan, |_, installer| {
                assert_eq!(fs::read(installer).unwrap(), b"installer");
                fs::write(marker_worker, b"installed").unwrap();
                Ok(())
            })
        });
        std::thread::sleep(Duration::from_millis(200));
        assert!(!marker.exists());
        parent.kill().unwrap();
        parent.wait().unwrap();
        worker.join().unwrap().unwrap();
        assert!(marker.exists());
        assert_eq!(fs::read(root.join("history.json")).unwrap(), b"history");
        fs::remove_dir_all(root).unwrap();
    }
}
