use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use stt_session::{DictationResult, InsertionMethod};

const RETENTION_SECS: u64 = 30 * 24 * 60 * 60;
const MAX_ENTRIES: usize = 500;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct HistoryEntry {
    pub id: u64,
    pub created_at: u64,
    pub text: String,
    pub application: Option<String>,
    pub method: Option<String>,
    pub status: String,
    pub copied_on_failure: bool,
    pub error: Option<String>,
}

#[derive(Deserialize, Serialize)]
struct HistoryFile {
    version: u32,
    entries: Vec<HistoryEntry>,
}

pub struct HistoryStore {
    path: PathBuf,
    entries: Vec<HistoryEntry>,
    next_id: u64,
}

impl HistoryStore {
    pub fn load(path: PathBuf) -> Result<Self, String> {
        let mut entries = match std::fs::read_to_string(&path) {
            Ok(raw) => {
                serde_json::from_str::<HistoryFile>(&raw)
                    .map_err(|err| format!("history is corrupt: {err}"))?
                    .entries
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(err) => return Err(format!("history read: {err}")),
        };
        prune(&mut entries, now_secs());
        let next_id = entries.iter().map(|entry| entry.id).max().unwrap_or(0) + 1;
        Ok(Self {
            path,
            entries,
            next_id,
        })
    }

    pub fn empty(path: PathBuf) -> Self {
        Self {
            path,
            entries: Vec::new(),
            next_id: 1,
        }
    }

    pub fn entries(&self) -> &[HistoryEntry] {
        &self.entries
    }

    pub fn push(&mut self, result: DictationResult) -> Result<(), String> {
        let (application, method, status, error) = match result.injection {
            Ok(report) => (
                report.application,
                Some(match report.method {
                    InsertionMethod::Clipboard => "Paste".to_string(),
                    InsertionMethod::Typing => "Typing".to_string(),
                }),
                "Inserted".to_string(),
                None,
            ),
            Err(error) => (
                None,
                None,
                if result.copied_on_failure {
                    "Copied"
                } else {
                    "Failed"
                }
                .to_string(),
                Some(error),
            ),
        };
        self.entries.insert(
            0,
            HistoryEntry {
                id: self.next_id,
                created_at: now_secs(),
                text: result.text,
                application,
                method,
                status,
                copied_on_failure: result.copied_on_failure,
                error,
            },
        );
        self.next_id += 1;
        prune(&mut self.entries, now_secs());
        self.save()
    }

    pub fn delete(&mut self, id: u64) -> Result<(), String> {
        self.entries.retain(|entry| entry.id != id);
        self.save()
    }

    pub fn clear(&mut self) -> Result<(), String> {
        self.entries.clear();
        self.save()
    }

    fn save(&self) -> Result<(), String> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(|err| format!("history dir: {err}"))?;
        }
        let body = serde_json::to_vec_pretty(&HistoryFile {
            version: 1,
            entries: self.entries.clone(),
        })
        .map_err(|err| format!("history encode: {err}"))?;
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, body).map_err(|err| format!("history write: {err}"))?;
        replace_file(&tmp, &self.path).map_err(|err| format!("history replace: {err}"))
    }
}

#[cfg(windows)]
fn replace_file(source: &std::path::Path, destination: &std::path::Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{ReplaceFileW, REPLACEFILE_WRITE_THROUGH};

    if !destination.exists() {
        return std::fs::rename(source, destination);
    }
    let destination: Vec<u16> = destination
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();
    let source: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
    let replaced = unsafe {
        ReplaceFileW(
            destination.as_ptr(),
            source.as_ptr(),
            std::ptr::null(),
            REPLACEFILE_WRITE_THROUGH,
            std::ptr::null(),
            std::ptr::null(),
        )
    };
    if replaced == 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(not(windows))]
fn replace_file(source: &std::path::Path, destination: &std::path::Path) -> std::io::Result<()> {
    std::fs::rename(source, destination)
}

pub fn copy_text(text: &str) -> Result<(), String> {
    use arboard::Clipboard;
    let mut clipboard = Clipboard::new().map_err(|err| format!("clipboard: {err}"))?;
    clipboard
        .set_text(text.to_string())
        .map_err(|err| format!("clipboard: {err}"))
}

pub fn age_label(created_at: u64) -> String {
    let age = now_secs().saturating_sub(created_at);
    match age {
        0..=59 => "Just now".to_string(),
        60..=3599 => format!("{}m ago", age / 60),
        3600..=86_399 => format!("{}h ago", age / 3600),
        _ => format!("{}d ago", age / 86_400),
    }
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

fn prune(entries: &mut Vec<HistoryEntry>, now: u64) {
    entries.retain(|entry| now.saturating_sub(entry.created_at) <= RETENTION_SECS);
    entries.truncate(MAX_ENTRIES);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: u64, created_at: u64) -> HistoryEntry {
        HistoryEntry {
            id,
            created_at,
            text: format!("text {id}"),
            application: None,
            method: None,
            status: "Inserted".into(),
            copied_on_failure: false,
            error: None,
        }
    }

    #[test]
    fn prune_drops_expired_and_caps_the_list() {
        let now = 10_000_000;
        let mut entries: Vec<_> = (0..510)
            .map(|id| entry(id, now - id.min(RETENTION_SECS - 1)))
            .collect();
        entries.push(entry(999, now - RETENTION_SECS - 1));
        prune(&mut entries, now);
        assert_eq!(entries.len(), 500);
        assert!(!entries.iter().any(|entry| entry.id == 999));
    }

    #[test]
    fn save_and_load_round_trip() {
        let root = std::env::temp_dir().join(format!(
            "stt-history-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let path = root.join("history.json");
        let mut store = HistoryStore::empty(path.clone());
        store.entries.push(entry(1, now_secs()));
        store.save().unwrap();
        let loaded = HistoryStore::load(path).unwrap();
        assert_eq!(loaded.entries().len(), 1);
        let _ = std::fs::remove_dir_all(root);
    }
}
