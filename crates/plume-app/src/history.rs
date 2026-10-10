use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use plume_session::{DictationResult, InsertionMethod};
use serde::{Deserialize, Serialize};

use crate::history_policy::HistoryPolicy;

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct HistoryEntry {
    pub id: u64,
    #[serde(default)]
    pub record_id: Option<u64>,
    #[serde(default)]
    pub transcription_error: Option<String>,
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
    policy: HistoryPolicy,
    load_warning: Option<String>,
}

impl HistoryStore {
    pub fn load(path: PathBuf, policy: HistoryPolicy) -> Result<Self, String> {
        let entries = match std::fs::read_to_string(&path) {
            Ok(raw) => {
                serde_json::from_str::<HistoryFile>(&raw)
                    .map_err(|err| format!("history is corrupt: {err}"))?
                    .entries
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(err) => return Err(format!("history read: {err}")),
        };
        let next_id = entries.iter().map(|entry| entry.id).max().unwrap_or(0) + 1;
        let mut store = Self {
            path,
            entries,
            next_id,
            policy,
            load_warning: None,
        };
        store.load_warning = store.apply_policy(policy).err();
        Ok(store)
    }

    pub fn empty(path: PathBuf, policy: HistoryPolicy) -> Self {
        Self {
            path,
            entries: Vec::new(),
            next_id: 1,
            policy,
            load_warning: None,
        }
    }

    pub fn take_load_warning(&mut self) -> Option<String> {
        self.load_warning.take()
    }

    /// Keep the desired policy even if persisting the pruned list fails, so the
    /// next transcription retries it without discarding the visible entries.
    pub fn apply_policy(&mut self, policy: HistoryPolicy) -> Result<(), String> {
        self.policy = policy;
        let mut entries = self.entries.clone();
        prune(&mut entries, now_secs(), policy);
        self.commit(entries)
    }

    pub fn entries(&self) -> &[HistoryEntry] {
        &self.entries
    }

    pub fn push(&mut self, result: DictationResult) -> Result<(), String> {
        let mut entries = self.entries.clone();
        if result.recovered {
            if let Some(entry) = entries
                .iter_mut()
                .find(|e| e.record_id == result.record_id && result.record_id.is_some())
            {
                if result.transcription_error.is_none() {
                    entry.text = result.text;
                }
                entry.transcription_error = result.transcription_error;
                // The take's age remains in RecordingStore. This new result
                // starts its own text-retention period and remains visible.
                entry.created_at = now_secs();
                prune(&mut entries, now_secs(), self.policy);
                return self.commit(entries);
            }
        }
        let (application, method, status, error) = match result.injection {
            Ok(report) => (
                report.application,
                Some(report.method.label().to_string()),
                if report.method == InsertionMethod::Copied {
                    "Copied"
                } else {
                    "Dispatched"
                }
                .to_string(),
                None,
            ),
            Err(error) => (
                None,
                None,
                if result.recovered && result.transcription_error.is_none() {
                    "Recovered"
                } else if result.copied_on_failure {
                    "Copied"
                } else {
                    "Failed"
                }
                .to_string(),
                Some(error),
            ),
        };
        if let Some(id) = result.record_id {
            entries.retain(|e| e.record_id != Some(id));
        }
        entries.insert(
            0,
            HistoryEntry {
                id: self.next_id,
                record_id: result.record_id,
                transcription_error: result.transcription_error,
                created_at: now_secs(),
                text: result.text,
                application,
                method,
                status,
                copied_on_failure: result.copied_on_failure,
                error,
            },
        );
        prune(&mut entries, now_secs(), self.policy);
        self.commit(entries)?;
        self.next_id += 1;
        Ok(())
    }

    /// Import durable results after a crash, without delivering text to another application.
    pub fn import_recordings(
        &mut self,
        records: &[plume_session::Recording],
    ) -> Result<(), String> {
        let mut entries = self.entries.clone();
        for record in records {
            if record.history_saved {
                continue;
            }
            if let Some(entry) = entries.iter_mut().find(|e| e.record_id == Some(record.id)) {
                if record.error.is_none() {
                    if let Some(text) = &record.text {
                        entry.text = text.clone();
                    }
                }
                entry.transcription_error = record.error.clone();
                entry.created_at = now_secs();
                continue;
            }
            entries.push(HistoryEntry {
                id: self.next_id,
                record_id: Some(record.id),
                transcription_error: record.error.clone(),
                created_at: now_secs(),
                text: record.text.clone().unwrap_or_default(),
                application: record
                    .insertion
                    .as_ref()
                    .and_then(|i| i.application.clone()),
                method: record.insertion.as_ref().and_then(|i| i.method.clone()),
                status: record
                    .insertion
                    .as_ref()
                    .map_or_else(|| "Recovered".into(), |i| i.status.clone()),
                copied_on_failure: record.insertion.as_ref().is_some_and(|i| i.copied),
                error: record.insertion.as_ref().and_then(|i| i.error.clone()),
            });
            self.next_id += 1;
        }
        prune(&mut entries, now_secs(), self.policy);
        self.commit(entries)
    }

    pub fn delete(&mut self, id: u64) -> Result<(), String> {
        let mut entries = self.entries.clone();
        entries.retain(|entry| entry.id != id);
        self.commit(entries)
    }

    pub fn clear(&mut self) -> Result<(), String> {
        self.commit(Vec::new())
    }

    fn commit(&mut self, entries: Vec<HistoryEntry>) -> Result<(), String> {
        self.save_entries(&entries)?;
        self.entries = entries;
        Ok(())
    }

    fn save_entries(&self, entries: &[HistoryEntry]) -> Result<(), String> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(|err| format!("history dir: {err}"))?;
        }
        let body = serde_json::to_vec_pretty(&HistoryFile {
            version: 1,
            entries: entries.to_vec(),
        })
        .map_err(|err| format!("history encode: {err}"))?;
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, body).map_err(|err| format!("history write: {err}"))?;
        crate::file_store::replace_file(&tmp, &self.path)
            .map_err(|err| format!("history replace: {err}"))
    }
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

fn prune(entries: &mut Vec<HistoryEntry>, now: u64, policy: HistoryPolicy) {
    entries.retain(|entry| {
        policy
            .retention_secs()
            .is_none_or(|seconds| now.saturating_sub(entry.created_at) <= seconds)
    });
    entries.sort_by_key(|entry| std::cmp::Reverse((entry.created_at, entry.id)));
    if let Some(limit) = policy.max_entries() {
        entries.truncate(limit);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: u64, created_at: u64) -> HistoryEntry {
        HistoryEntry {
            id,
            record_id: None,
            transcription_error: None,
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
            .map(|id| {
                entry(
                    id,
                    now - id.min(HistoryPolicy::default().retention_secs().unwrap() - 1),
                )
            })
            .collect();
        entries.push(entry(
            999,
            now - HistoryPolicy::default().retention_secs().unwrap() - 1,
        ));
        prune(&mut entries, now, HistoryPolicy::default());
        assert_eq!(entries.len(), 500);
        assert!(!entries.iter().any(|entry| entry.id == 999));
    }

    #[test]
    fn save_and_load_round_trip() {
        let root = std::env::temp_dir().join(format!(
            "plume-history-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let path = root.join("history.json");
        let mut store = HistoryStore::empty(path.clone(), HistoryPolicy::default());
        store.entries.push(entry(1, now_secs()));
        store.save_entries(&store.entries).unwrap();
        let loaded = HistoryStore::load(path, HistoryPolicy::default()).unwrap();
        assert_eq!(loaded.entries().len(), 1);
        let _ = std::fs::remove_dir_all(root);
    }
    fn temp_root() -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "plume-history-policy-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn retention_boundary_is_inclusive_and_limit_keeps_newest_entries() {
        let now = 200_000;
        let policy = HistoryPolicy::new(Some(1), Some(2)).unwrap();
        let mut entries = vec![
            entry(1, now - 86_401),
            entry(2, now - 86_400),
            entry(4, now),
            entry(3, now - 1),
        ];
        prune(
            &mut entries,
            now,
            HistoryPolicy::new(Some(1), Some(10)).unwrap(),
        );
        assert_eq!(
            entries.iter().map(|entry| entry.id).collect::<Vec<_>>(),
            [4, 3, 2]
        );
        prune(&mut entries, now, policy);
        assert_eq!(
            entries.iter().map(|entry| entry.id).collect::<Vec<_>>(),
            [4, 3]
        );
    }

    #[test]
    fn changing_limits_prunes_immediately_and_persists_on_load() {
        let root = temp_root();
        let path = root.join("history.json");
        let mut store = HistoryStore::empty(path.clone(), HistoryPolicy::default());
        store.entries = vec![
            entry(1, now_secs() - 10 * 86_400),
            entry(2, now_secs()),
            entry(3, now_secs()),
        ];
        store.save_entries(&store.entries).unwrap();
        store
            .apply_policy(HistoryPolicy::new(Some(7), Some(1)).unwrap())
            .unwrap();
        assert_eq!(
            store
                .entries()
                .iter()
                .map(|entry| entry.id)
                .collect::<Vec<_>>(),
            [3]
        );
        let loaded = HistoryStore::load(path.clone(), HistoryPolicy::default()).unwrap();
        assert_eq!(loaded.entries().len(), 1);
        let raw: HistoryFile =
            serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        assert_eq!(raw.entries.len(), 1);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn failed_prune_keeps_visible_entries_and_next_push_retries() {
        let root = temp_root();
        let path = root.join("history.json");
        let mut store = HistoryStore::empty(path.clone(), HistoryPolicy::default());
        store.entries = vec![entry(1, now_secs()), entry(2, now_secs())];
        store.next_id = 3;
        store.save_entries(&store.entries).unwrap();
        let original = store.entries.clone();
        std::fs::create_dir(path.with_extension("json.tmp")).unwrap();
        assert!(store
            .apply_policy(HistoryPolicy::new(Some(7), Some(1)).unwrap())
            .is_err());
        assert_eq!(store.entries, original);
        let mut loaded =
            HistoryStore::load(path.clone(), HistoryPolicy::new(Some(7), Some(1)).unwrap())
                .unwrap();
        assert!(loaded.take_load_warning().is_some());
        assert_eq!(loaded.entries, original);
        std::fs::remove_dir(path.with_extension("json.tmp")).unwrap();
        store
            .push(DictationResult {
                record_id: None,
                audio_available: false,
                recovered: false,
                transcription_error: None,
                text: "new".into(),
                injection: Err("failed".into()),
                copied_on_failure: true,
            })
            .unwrap();
        assert_eq!(store.entries.len(), 1);
        assert_eq!(store.entries[0].text, "new");
        assert_eq!(store.entries[0].status, "Copied");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn copied_delivery_is_recorded_as_copied() {
        let root = temp_root();
        let mut store = HistoryStore::empty(root.join("history.json"), HistoryPolicy::default());
        store
            .push(DictationResult {
                record_id: None,
                audio_available: false,
                recovered: false,
                transcription_error: None,
                text: "copied".into(),
                injection: Ok(plume_session::InjectionReport {
                    method: InsertionMethod::Copied,
                    application: None,
                    target: plume_session::TargetAssessment::NoFocus,
                }),
                copied_on_failure: false,
            })
            .unwrap();
        let entry = &store.entries[0];
        assert_eq!(entry.status, "Copied");
        assert_eq!(entry.method.as_deref(), Some("Copy"));
        assert!(!entry.copied_on_failure);
        assert_eq!(entry.error, None);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn startup_prunes_and_writes_existing_history() {
        let root = temp_root();
        let path = root.join("history.json");
        let mut store = HistoryStore::empty(path.clone(), HistoryPolicy::default());
        store.entries = vec![entry(1, now_secs() - 8 * 86_400), entry(2, now_secs())];
        store.save_entries(&store.entries).unwrap();
        let loaded = HistoryStore::load(
            path.clone(),
            HistoryPolicy::new(Some(7), Some(100)).unwrap(),
        )
        .unwrap();
        assert_eq!(loaded.entries.len(), 1);
        let persisted: HistoryFile =
            serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        assert_eq!(persisted.entries.len(), 1);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn unlimited_retention_and_size_are_independent() {
        let now = 100 * 86_400;
        let original = vec![entry(1, 0), entry(2, now)];
        let mut entries = original.clone();
        prune(&mut entries, now, HistoryPolicy::new(None, None).unwrap());
        assert_eq!(entries.len(), 2);
        let mut entries = original.clone();
        prune(
            &mut entries,
            now,
            HistoryPolicy::new(None, Some(1)).unwrap(),
        );
        assert_eq!(
            entries.iter().map(|entry| entry.id).collect::<Vec<_>>(),
            [2]
        );
        let mut entries = original;
        prune(
            &mut entries,
            now,
            HistoryPolicy::new(Some(7), None).unwrap(),
        );
        assert_eq!(
            entries.iter().map(|entry| entry.id).collect::<Vec<_>>(),
            [2]
        );
    }
}

#[cfg(test)]
mod recording_history_tests {
    use super::*;
    #[test]
    fn legacy_entries_default_to_no_audio_and_retry_keeps_insertion_status() {
        let entry:HistoryEntry=serde_json::from_str(r#"{"id":1,"created_at":1,"text":"old","application":null,"method":null,"status":"Inserted","copied_on_failure":false,"error":null}"#).unwrap();
        assert_eq!(entry.record_id, None);
        let root = std::env::temp_dir().join(format!(
            "plume-history-retry-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut store = HistoryStore::empty(root.join("history.json"), HistoryPolicy::default());
        let first = DictationResult {
            text: "old".into(),
            record_id: Some(7),
            audio_available: true,
            transcription_error: None,
            recovered: false,
            copied_on_failure: true,
            injection: Err("dispatch failed".into()),
        };
        store.push(first).unwrap();
        store.entries[0].created_at = 1;
        store
            .push(DictationResult {
                text: "recovered".into(),
                record_id: Some(7),
                audio_available: true,
                transcription_error: None,
                recovered: true,
                copied_on_failure: false,
                injection: Err("not inserted".into()),
            })
            .unwrap();
        let entry = &store.entries()[0];
        assert!(entry.created_at > 1);
        assert_eq!(entry.status, "Copied");
        assert_eq!(entry.text, "recovered");
        assert!(entry.copied_on_failure);
        assert_eq!(store.entries().len(), 1);
        // The original text can expire while its audio remains indefinitely.
        store.clear().unwrap();
        let audio_only: plume_session::Recording = serde_json::from_value(serde_json::json!({
            "version": 1, "id": 7, "created_at": 1, "has_speech": true,
            "duration_ms": 1000, "status": "Transcribed", "text": "new recovery",
            "error": null, "history_saved": false,
            "insertion": { "status": "Copied", "method": null, "application": null,
                "error": "old dispatch failure", "copied": true }
        }))
        .unwrap();
        store.import_recordings(&[audio_only]).unwrap();
        store.apply_policy(HistoryPolicy::default()).unwrap();
        assert_eq!(store.entries()[0].text, "new recovery");
        assert_eq!(store.entries()[0].status, "Copied");
        assert_eq!(store.entries()[0].record_id, Some(7));
        std::fs::remove_dir_all(root).unwrap();
    }
}
