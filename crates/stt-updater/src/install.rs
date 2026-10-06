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
    pub target: PathBuf,
    pub staged: PathBuf,
    pub backup: PathBuf,
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
    let parent = installation
        .root
        .parent()
        .ok_or("The installation folder cannot be updated.")?;
    let id = format!("{}-{}", std::process::id(), io_time()?);
    // Sibling staging permits an atomic rename even when the app is on another volume.
    let work = parent.join(format!(".plume-update-{id}"));
    io(fs::create_dir(&work)).map_err(|_| "Plume cannot write to its installation folder. Move it to a writable folder or download the update from GitHub.".to_string())?;
    let result = (|| {
        let archive = work.join(&release.archive_name);
        crate::archive::download(release, &archive, &progress)?;
        let root_name = release
            .archive_name
            .strip_suffix(".tar.gz")
            .or_else(|| release.archive_name.strip_suffix(".zip"))
            .ok_or("Unsupported update archive")?;
        let unpacked =
            crate::archive::extract(&archive, &work.join("extracted"), root_name, cfg!(windows))?;
        let staged = if installation.executable.starts_with("Contents") {
            unpacked.join("Plume.app")
        } else {
            unpacked
        };
        if !staged.join(&installation.executable).is_file() {
            return Err("The update is missing the Plume application.".into());
        }
        let new_helper = staged
            .join(installation.executable.parent().unwrap_or(Path::new("")))
            .join(executable_name("plume-updater"));
        if !new_helper.is_file() {
            return Err("The update is missing its installer.".into());
        }
        if cfg!(target_os = "macos") && installation.executable.starts_with("Contents") {
            let status = io(Command::new("/usr/bin/codesign")
                .args(["--verify", "--deep", "--strict"])
                .arg(&staged)
                .status())?;
            if !status.success() {
                return Err("The downloaded application failed verification.".into());
            }
        }
        let helper_dir = work.join("helper");
        io(fs::create_dir(&helper_dir))?;
        // Run the installer outside both folders being renamed. Windows keeps
        // loaded executables and DLLs locked until their processes exit.
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
        let name = installation
            .root
            .file_name()
            .ok_or("Invalid installation folder")?
            .to_string_lossy();
        let plan = InstallPlan {
            target: installation.root.clone(),
            staged,
            backup: parent.join(format!(".{name}.previous-{id}")),
            executable: installation.executable.clone(),
            parent_pid: std::process::id(),
            work: work.clone(),
        };
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
    .stdin(Stdio::null())
    .stdout(Stdio::null())
    .stderr(Stdio::null())
    .spawn())?;
    Ok(())
}

pub fn apply(plan: &InstallPlan) -> Result<()> {
    validate(plan)?;
    wait_for_exit(plan.parent_pid)?;
    if let Err(error) = replace(plan, |_| launch(plan, None)) {
        // Reopen the restored app with a visible error instead of leaving the
        // user with a closed application and a hidden installer log.
        let _ = launch(plan, Some(&error));
        return Err(error);
    }
    // Keep the previous app for recovery. User data is outside the installation.
    // On Windows the helper itself remains locked; cleanup is best-effort.
    let _ = fs::remove_dir_all(&plan.work);
    Ok(())
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
    if !plan.target.is_absolute()
        || !plan.staged.is_absolute()
        || !plan.work.is_absolute()
        || plan.target.parent() != plan.work.parent()
        || plan.target.parent() != plan.backup.parent()
        || !plan.staged.starts_with(&plan.work)
        || !plan.staged.join(&plan.executable).is_file()
        || plan.backup.exists()
        || !plan.target.is_dir()
        || !plan
            .work
            .file_name()
            .is_some_and(|name| name.to_string_lossy().starts_with(".plume-update-"))
        || plan
            .executable
            .components()
            .any(|part| !matches!(part, std::path::Component::Normal(_)))
        || !plan
            .backup
            .file_name()
            .is_some_and(|name| name.to_string_lossy().contains(".previous-"))
    {
        return Err("Invalid update installation plan.".into());
    }
    Ok(())
}

fn replace(plan: &InstallPlan, launch: impl Fn(&Path) -> Result<()>) -> Result<()> {
    io(fs::rename(&plan.target, &plan.backup))?;
    if let Err(error) = fs::rename(&plan.staged, &plan.target) {
        io(fs::rename(&plan.backup, &plan.target))?;
        return Err(format!(
            "Could not install the update; the previous app was restored: {error}"
        ));
    }
    if let Err(error) = launch(&plan.target.join(&plan.executable)) {
        io(fs::rename(&plan.target, &plan.staged))?;
        io(fs::rename(&plan.backup, &plan.target))?;
        return Err(format!(
            "Could not launch the update; the previous app was restored: {error}"
        ));
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
        fs::create_dir(&root).unwrap();
        let work = root.join(".plume-update-test");
        let staged = work.join("extracted/new");
        let target = root.join("Plume");
        fs::create_dir_all(&staged).unwrap();
        fs::create_dir(&target).unwrap();
        fs::write(target.join("plume"), b"old").unwrap();
        fs::write(staged.join("plume"), b"new").unwrap();
        InstallPlan {
            target,
            staged,
            backup: root.join(".Plume.previous-test"),
            executable: "plume".into(),
            parent_pid: 1,
            work,
        }
    }
    #[test]
    fn replacement_keeps_backup_and_does_not_touch_external_user_data() {
        let plan = fixture();
        let parent = plan.target.parent().unwrap();
        fs::write(parent.join("history.json"), "history").unwrap();
        validate(&plan).unwrap();
        replace(&plan, |exe| {
            assert_eq!(fs::read(exe).unwrap(), b"new");
            Ok(())
        })
        .unwrap();
        assert_eq!(fs::read(plan.backup.join("plume")).unwrap(), b"old");
        assert_eq!(fs::read(parent.join("history.json")).unwrap(), b"history");
        fs::remove_dir_all(parent).unwrap();
    }
    #[test]
    fn failed_restart_rolls_back_to_the_previous_app() {
        let plan = fixture();
        assert!(replace(&plan, |_| Err("launch failed".into())).is_err());
        assert_eq!(fs::read(plan.target.join("plume")).unwrap(), b"old");
        assert!(!plan.backup.exists());
        fs::remove_dir_all(plan.target.parent().unwrap()).unwrap();
    }
    #[test]
    fn rejects_a_stage_outside_the_work_directory() {
        let mut plan = fixture();
        let root = plan.target.parent().unwrap().to_path_buf();
        plan.staged = plan.target.clone();
        assert!(validate(&plan).is_err());
        fs::remove_dir_all(root).unwrap();
    }
}
