use std::io::Write;
use std::path::{Path, PathBuf};

use crate::dirs::AppDirs;

#[derive(Debug)]
pub struct AppLock {
    path: PathBuf,
}

#[derive(Debug)]
pub struct AlreadyRunning(pub u32);

impl AppLock {
    pub fn acquire() -> Result<AppLock, AlreadyRunning> {
        acquire_at(&AppDirs::resolve().lock_path())
    }
}

impl Drop for AppLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

pub(crate) fn acquire_at(path: &Path) -> Result<AppLock, AlreadyRunning> {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    match create_new_lock(path) {
        Ok(lock) => Ok(lock),
        Err(AlreadyRunning(pid)) => {
            if pid_alive(pid) {
                Err(AlreadyRunning(pid))
            } else {
                let _ = std::fs::remove_file(path);
                create_new_lock(path)
            }
        }
    }
}

fn create_new_lock(path: &Path) -> Result<AppLock, AlreadyRunning> {
    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
    {
        Ok(mut file) => {
            let pid = std::process::id();
            if file.write_all(format!("{pid}\n").as_bytes()).is_err() {
                drop(file);
                let _ = std::fs::remove_file(path);
                return Err(AlreadyRunning(0));
            }
            Ok(AppLock {
                path: path.to_path_buf(),
            })
        }
        Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {
            Err(AlreadyRunning(read_pid(path).unwrap_or(0)))
        }
        Err(_) => Err(AlreadyRunning(read_pid(path).unwrap_or(0))),
    }
}

fn read_pid(path: &Path) -> Option<u32> {
    let raw = std::fs::read_to_string(path).ok()?;
    raw.trim().parse().ok()
}

fn pid_alive(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    #[cfg(target_os = "linux")]
    {
        std::path::Path::new(&format!("/proc/{pid}")).exists()
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("/bin/kill")
            .args(["-0", &pid.to_string()])
            .status()
            .map(|status| status.success())
            .unwrap_or(true)
    }
    #[cfg(windows)]
    {
        windows_pid_alive(pid)
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
    {
        true
    }
}

#[cfg(windows)]
fn windows_pid_alive(pid: u32) -> bool {
    use std::ffi::c_void;
    extern "system" {
        fn OpenProcess(access: u32, inherit: i32, pid: u32) -> *mut c_void;
        fn GetExitCodeProcess(handle: *mut c_void, exit_code: *mut u32) -> i32;
        fn CloseHandle(handle: *mut c_void) -> i32;
    }
    const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
    const STILL_ACTIVE: u32 = 259;
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle.is_null() {
            return false;
        }
        let mut exit_code = 0;
        let alive = GetExitCodeProcess(handle, &mut exit_code) != 0 && exit_code == STILL_ACTIVE;
        CloseHandle(handle);
        alive
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_lock(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "plume-app-lock-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("lock")
    }

    #[test]
    fn second_acquire_sees_live_pid() {
        let path = temp_lock("live");
        let lock = acquire_at(&path).unwrap();
        let err = acquire_at(&path).unwrap_err();
        assert_eq!(err.0, std::process::id());
        drop(lock);
        let again = acquire_at(&path);
        assert!(again.is_ok());
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn dead_pid_is_replaced() {
        let path = temp_lock("dead");
        std::fs::write(&path, "999999999\n").unwrap();
        let lock = acquire_at(&path).expect("dead pid must yield the lock");
        let written = std::fs::read_to_string(&path).unwrap();
        assert_eq!(written.trim(), std::process::id().to_string());
        drop(lock);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[cfg(windows)]
    #[test]
    fn current_windows_process_is_alive() {
        assert!(pid_alive(std::process::id()));
    }
}
