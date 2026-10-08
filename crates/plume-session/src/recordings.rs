use plume_core::BoxError;
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::BufWriter;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

pub const MAX_RECORDINGS: usize = 8;
pub type RecordId = u64;
pub type SharedRecordings = Arc<Mutex<RecordingStore>>;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Recording {
    pub version: u32,
    pub id: RecordId,
    pub created_at: u64,
    pub has_speech: bool,
    pub duration_ms: u64,
    pub status: String,
    pub text: Option<String>,
    pub error: Option<String>,
    #[serde(default)]
    pub history_saved: bool,
    #[serde(default)]
    pub insertion: Option<RecordedInsertion>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RecordedInsertion {
    pub status: String,
    pub method: Option<String>,
    pub application: Option<String>,
    pub error: Option<String>,
    pub copied: bool,
}
pub struct RecordingStore {
    root: PathBuf,
    records: Vec<Recording>,
    next_id: u64,
}
impl RecordingStore {
    pub fn open(root: PathBuf) -> Result<Self, BoxError> {
        fs::create_dir_all(&root)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&root, fs::Permissions::from_mode(0o700))?;
        }
        let mut store = Self {
            root,
            records: Vec::new(),
            next_id: 0,
        };
        // A crash can leave a fully flushed metadata replacement just before
        // its rename. Recover it before deciding whether the audio is empty.
        for entry in fs::read_dir(&store.root)? {
            let path = entry?.path();
            let Some(id) = path
                .file_name()
                .and_then(|name| name.to_str())
                .and_then(|name| name.strip_suffix(".json.tmp"))
                .and_then(|id| id.parse::<u64>().ok())
            else {
                continue;
            };
            if let Ok(record) = serde_json::from_slice::<Recording>(&fs::read(&path)?) {
                if record.id == id && record.version == 1 {
                    replace_file(&path, &store.meta_path(id))?;
                }
            }
        }
        for entry in fs::read_dir(&store.root)? {
            let path = entry?.path();
            if path.extension().is_none_or(|ext| ext != "json") {
                continue;
            }
            let mut record: Recording = match serde_json::from_slice(&fs::read(&path)?) {
                Ok(record) => record,
                Err(_) => {
                    tracing::warn!("recording metadata corrupt");
                    fs::rename(&path, path.with_extension("json.corrupt"))?;
                    continue;
                }
            };
            if record.version != 1 {
                fs::rename(&path, path.with_extension("json.unsupported"))?;
                continue;
            }
            store.next_id = store.next_id.max(record.id);
            let temp = store.partial_path(record.id);
            if store.cancel_path(record.id).exists()
                || record.status == "Cancelled"
                || !record.has_speech
            {
                let _ = fs::remove_file(&temp);
                let _ = fs::remove_file(store.audio_path(record.id));
                fs::remove_file(path)?;
                remove_if_exists(&store.cancel_path(record.id))?;
                continue;
            }
            if temp.exists() {
                // Header is flushed periodically, so uncommitted tail bytes are intentionally ignored.
                let valid = hound::WavReader::open(&temp).is_ok_and(|wav| wav.duration() > 0);
                if valid {
                    fs::rename(&temp, store.audio_path(record.id))?;
                } else {
                    fs::remove_file(&temp)?;
                }
            }
            if record.status == "Recording" || record.status == "Transcribing" {
                record.status = "Interrupted".into();
                record.error = Some("Recording interrupted; retry from History.".into());
                store.save(&record)?;
            }
            store.records.push(record);
        }
        // Reconcile promoted audio even if its metadata write was interrupted.
        for entry in fs::read_dir(&store.root)? {
            let path = entry?.path();
            let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            if let Some(id) = name
                .strip_suffix(".cancelled")
                .and_then(|id| id.parse().ok())
            {
                store.delete(id)?;
                continue;
            }
            if name.ends_with(".partial.wav") {
                remove_if_exists(&path)?;
                continue;
            }
            if path.extension().is_none_or(|ext| ext != "wav") {
                continue;
            }
            let Some(id) = path
                .file_stem()
                .and_then(|name| name.to_str())
                .and_then(|id| id.parse::<u64>().ok())
            else {
                continue;
            };
            if store.cancel_path(id).exists() {
                store.delete(id)?;
                continue;
            }
            if store.records.iter().any(|r| r.id == id) {
                continue;
            }
            let wav = match hound::WavReader::open(&path) {
                Ok(wav) if wav.duration() > 0 => wav,
                _ => {
                    remove_if_exists(&path)?;
                    continue;
                }
            };
            if wav.spec().channels != 1
                || wav.spec().sample_rate != 16_000
                || wav.spec().bits_per_sample != 16
            {
                remove_if_exists(&path)?;
                continue;
            }
            let record = Recording {
                version: 1,
                id,
                created_at: fs::metadata(&path)?
                    .modified()?
                    .duration_since(UNIX_EPOCH)?
                    .as_secs(),
                has_speech: true,
                duration_ms: u64::from(wav.duration()) * 1000 / 16000,
                status: "Interrupted".into(),
                text: None,
                error: Some("Audio metadata was interrupted; retry from History.".into()),
                history_saved: false,
                insertion: None,
            };
            store.next_id = store.next_id.max(id);
            store.save(&record)?;
            store.records.push(record);
        }
        store.enforce_limit()?;
        Ok(store)
    }
    pub fn shared(self) -> SharedRecordings {
        Arc::new(Mutex::new(self))
    }
    pub fn records(&self) -> Vec<Recording> {
        self.records.clone()
    }
    pub fn audio_path(&self, id: RecordId) -> PathBuf {
        self.root.join(format!("{id}.wav"))
    }
    pub fn replay_path(&self, id: RecordId) -> PathBuf {
        if self.audio_path(id).is_file() {
            self.audio_path(id)
        } else {
            self.partial_path(id)
        }
    }
    fn partial_path(&self, id: RecordId) -> PathBuf {
        self.root.join(format!("{id}.partial.wav"))
    }
    fn cancel_path(&self, id: RecordId) -> PathBuf {
        self.root.join(format!("{id}.cancelled"))
    }
    fn meta_path(&self, id: RecordId) -> PathBuf {
        self.root.join(format!("{id}.json"))
    }
    pub fn has_audio(&self, id: RecordId) -> bool {
        self.audio_path(id).is_file() || self.partial_path(id).is_file()
    }
    pub fn begin(&mut self) -> Result<RecordingWriter, BoxError> {
        // A previous storage error may have left a closed temporary take.
        // Resolve it before allowing another temporary file/microphone.
        for entry in fs::read_dir(&self.root)? {
            let path = entry?.path();
            let Some(id) = path
                .file_name()
                .and_then(|s| s.to_str())
                .and_then(|s| s.strip_suffix(".partial.wav"))
                .and_then(|s| s.parse().ok())
            else {
                continue;
            };
            if self.recover_partial(id)?.is_some() {
                self.promote_existing(id)?;
            } else {
                self.delete(id)?;
            }
        }
        self.enforce_limit()?;
        let now = SystemTime::now().duration_since(UNIX_EPOCH)?;
        self.next_id = (now.as_nanos() as u64).max(self.next_id.saturating_add(1));
        let record = Recording {
            version: 1,
            id: self.next_id,
            created_at: now.as_secs(),
            has_speech: false,
            duration_ms: 0,
            status: "Recording".into(),
            text: None,
            error: None,
            history_saved: false,
            insertion: None,
        };
        let file = private_file(&self.partial_path(record.id))?;
        let mut writer = hound::WavWriter::new(
            BufWriter::new(file),
            hound::WavSpec {
                channels: 1,
                sample_rate: 16_000,
                bits_per_sample: 16,
                sample_format: hound::SampleFormat::Int,
            },
        )?;
        writer.flush()?; // Verify writes before opening the microphone.
        if let Err(error) = self.save(&record) {
            drop(writer);
            let _ = fs::remove_file(self.partial_path(record.id));
            return Err(error);
        }
        Ok(RecordingWriter {
            writer: Some(writer),
            record,
            root: self.root.clone(),
            samples: 0,
            last_flush: 0,
        })
    }
    fn save(&self, record: &Recording) -> Result<(), BoxError> {
        save_record(&self.root, record)
    }
    pub fn promote(&mut self, record: Recording) -> Result<(), BoxError> {
        self.save(&record)?;
        let partial = self.partial_path(record.id);
        if partial.exists() {
            fs::rename(partial, self.audio_path(record.id))?;
        }
        self.records.retain(|r| r.id != record.id);
        self.records.push(record);
        self.enforce_limit()
    }
    pub(crate) fn stage(&mut self, record: Recording) -> Result<(), BoxError> {
        self.save(&record)?;
        self.records.retain(|r| r.id != record.id);
        self.records.push(record);
        Ok(())
    }
    pub(crate) fn promote_existing(&mut self, id: RecordId) -> Result<(), BoxError> {
        let record = self
            .records
            .iter()
            .find(|r| r.id == id)
            .cloned()
            .ok_or("recording no longer exists")?;
        self.promote(record)
    }
    fn enforce_limit(&mut self) -> Result<(), BoxError> {
        self.records.sort_by_key(|r| (r.created_at, r.id));
        while self.records.iter().filter(|r| self.has_audio(r.id)).count() > MAX_RECORDINGS {
            let id = self
                .records
                .iter()
                .find(|r| self.has_audio(r.id))
                .unwrap()
                .id;
            self.delete_audio(id)?;
            if self.records.iter().any(|r| r.id == id && r.history_saved) {
                remove_if_exists(&self.meta_path(id))?;
                self.records.retain(|r| r.id != id);
            }
        }
        Ok(())
    }
    pub fn recover_partial(&mut self, id: RecordId) -> Result<Option<Recording>, BoxError> {
        let path = self.meta_path(id);
        if !path.exists() {
            return Ok(None);
        }
        let mut record: Recording = serde_json::from_slice(&fs::read(path)?)?;
        let partial = self.partial_path(id);
        if !record.has_speech || self.cancel_path(id).exists() {
            return Ok(None);
        }
        if partial.exists() {
            let wav = hound::WavReader::open(&partial)?;
            if wav.duration() == 0 {
                return Ok(None);
            }
            record.duration_ms = u64::from(wav.duration()) * 1000 / 16_000;
            drop(wav);
        }
        if !self.has_audio(id) && !self.partial_path(id).exists() {
            return Ok(None);
        }
        record.status = "Interrupted".into();
        self.stage(record.clone())?;
        Ok(Some(record))
    }
    pub fn begin_retry(&mut self, id: RecordId) -> Result<(), BoxError> {
        let index = self
            .records
            .iter()
            .position(|r| r.id == id)
            .ok_or("recording no longer exists")?;
        if !self.has_audio(id) {
            return Err("recording audio is unavailable".into());
        }
        let mut record = self.records[index].clone();
        record.status = "Transcribing".into();
        record.history_saved = false;
        self.save(&record)?;
        self.records[index] = record;
        Ok(())
    }
    pub fn finish_decode(
        &mut self,
        id: RecordId,
        result: &Result<String, String>,
    ) -> Result<(), BoxError> {
        if let Some(index) = self.records.iter().position(|r| r.id == id) {
            let mut record = self.records[index].clone();
            record.history_saved = false;
            match result {
                Ok(text) => {
                    record.text = Some(text.clone());
                    record.error = None;
                    record.status = "Transcribed".into();
                }
                Err(error) => {
                    record.error = Some(error.clone());
                    record.status = "Failed".into();
                }
            }
            self.save(&record)?;
            self.records[index] = record;
        } else {
            return Err("recording no longer exists".into());
        }
        Ok(())
    }
    pub fn finish_insertion(
        &mut self,
        id: RecordId,
        result: &crate::DictationResult,
    ) -> Result<(), BoxError> {
        if let Some(index) = self.records.iter().position(|r| r.id == id) {
            let mut record = self.records[index].clone();
            record.insertion = Some(match &result.injection {
                Ok(report) => RecordedInsertion {
                    status: "Dispatched".into(),
                    method: Some(
                        match report.method {
                            plume_core::InsertionMethod::Clipboard => "Paste",
                            plume_core::InsertionMethod::Typing => "Typing",
                        }
                        .into(),
                    ),
                    application: report.application.clone(),
                    error: None,
                    copied: false,
                },
                Err(error) => RecordedInsertion {
                    status: if result.copied_on_failure {
                        "Copied"
                    } else {
                        "Failed"
                    }
                    .into(),
                    method: None,
                    application: None,
                    error: Some(error.clone()),
                    copied: result.copied_on_failure,
                },
            });
            self.save(&record)?;
            self.records[index] = record;
        }
        Ok(())
    }
    /// The text-history write is durable. Evicted audio metadata can now be removed.
    pub fn acknowledge_history(&mut self, id: RecordId) -> Result<(), BoxError> {
        if let Some(index) = self.records.iter().position(|r| r.id == id) {
            let mut record = self.records[index].clone();
            record.history_saved = true;
            self.save(&record)?;
            self.records[index] = record;
            if !self.has_audio(id) {
                remove_if_exists(&self.meta_path(id))?;
                self.records.remove(index);
            }
        }
        Ok(())
    }
    pub fn retain_history_text(&mut self, ids: &[RecordId]) -> Result<(), BoxError> {
        for index in 0..self.records.len() {
            if self.records[index].history_saved && !ids.contains(&self.records[index].id) {
                let mut record = self.records[index].clone();
                record.text = None;
                record.error = None;
                self.save(&record)?;
                self.records[index] = record;
            }
        }
        Ok(())
    }
    pub fn mark_cancelled(&mut self, id: RecordId) -> Result<(), BoxError> {
        // A separate durable marker cannot be overwritten by the capture writer.
        private_file(&self.cancel_path(id))?.sync_all()?;
        Ok(())
    }
    pub fn delete_audio(&mut self, id: RecordId) -> Result<(), BoxError> {
        remove_if_exists(&self.audio_path(id))?;
        remove_if_exists(&self.partial_path(id))?;
        Ok(())
    }
    pub fn delete(&mut self, id: RecordId) -> Result<(), BoxError> {
        let _ = self.mark_cancelled(id);
        remove_if_exists(&self.audio_path(id))?;
        remove_if_exists(&self.partial_path(id))?;
        remove_if_exists(&self.meta_path(id))?;
        for suffix in ["json.tmp", "json.corrupt", "json.unsupported"] {
            remove_if_exists(&self.root.join(format!("{id}.{suffix}")))?;
        }
        remove_if_exists(&self.cancel_path(id))?;
        self.records.retain(|r| r.id != id);
        Ok(())
    }
    pub fn clear(&mut self) -> Result<(), BoxError> {
        let mut ids: std::collections::BTreeSet<_> = self.records.iter().map(|r| r.id).collect();
        for entry in fs::read_dir(&self.root)? {
            if let Some(id) = entry?
                .file_name()
                .to_str()
                .and_then(|name| name.split('.').next())
                .and_then(|id| id.parse::<u64>().ok())
            {
                ids.insert(id);
            }
        }
        for id in ids {
            self.delete(id)?;
        }
        Ok(())
    }
}
fn private_file(path: &Path) -> std::io::Result<File> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)
}
fn remove_if_exists(path: &Path) -> std::io::Result<()> {
    match fs::remove_file(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        result => result,
    }
}
fn save_record(root: &Path, record: &Recording) -> Result<(), BoxError> {
    let dest = root.join(format!("{}.json", record.id));
    let temp = dest.with_extension("json.tmp");
    let file = private_file(&temp)?;
    serde_json::to_writer(&file, record)?;
    file.sync_all()?;
    // ReplaceFileW opens the replacement exclusively; close our writer first.
    drop(file);
    replace_file(&temp, &dest)?;
    Ok(())
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

pub struct RecordingWriter {
    writer: Option<hound::WavWriter<BufWriter<File>>>,
    pub record: Recording,
    root: PathBuf,
    samples: u64,
    last_flush: u64,
}
impl RecordingWriter {
    pub fn write(&mut self, samples: &[f32], speech: bool) -> Result<(), BoxError> {
        let first = speech && !self.record.has_speech;
        self.record.has_speech |= speech;
        let writer = self.writer.as_mut().ok_or("audio writer closed")?;
        for sample in samples {
            writer.write_sample((sample.clamp(-1.0, 1.0) * 32767.0).round() as i16)?;
        }
        self.samples += samples.len() as u64;
        self.record.duration_ms = self.samples * 1000 / 16_000;
        if first || self.samples - self.last_flush >= 16_000 {
            writer.flush()?;
            save_record(&self.root, &self.record)?;
            self.last_flush = self.samples;
        }
        Ok(())
    }
    pub fn finish(mut self) -> Result<Recording, BoxError> {
        self.writer.take().unwrap().finalize()?;
        let partial = self.root.join(format!("{}.partial.wav", self.record.id));
        if self.record.has_speech {
            self.record.status = "Transcribing".into();
            save_record(&self.root, &self.record)?;
        } else {
            remove_if_exists(&partial)?;
            remove_if_exists(&self.root.join(format!("{}.json", self.record.id)))?;
        }
        Ok(self.record)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn root() -> PathBuf {
        std::env::temp_dir().join(format!(
            "plume-recordings-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }
    #[test]
    fn eight_audios_evicted_but_metadata_preserves_text_and_restart() {
        let root = root();
        let mut store = RecordingStore::open(root.clone()).unwrap();
        let mut ids = Vec::new();
        for _ in 0..9 {
            let mut w = store.begin().unwrap();
            ids.push(w.record.id);
            w.write(&[0.2; 512], true).unwrap();
            let r = w.finish().unwrap();
            store.promote(r).unwrap();
        }
        assert!(!store.has_audio(ids[0]));
        assert!(store.has_audio(ids[8]));
        store.finish_decode(ids[8], &Ok("hello".into())).unwrap();
        let mut store = RecordingStore::open(root.clone()).unwrap();
        assert_eq!(
            store.records().last().unwrap().text.as_deref(),
            Some("hello")
        );
        store.clear().unwrap();
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn empty_and_cancelled_not_retained() {
        let root = root();
        let mut store = RecordingStore::open(root.clone()).unwrap();
        let mut w = store.begin().unwrap();
        w.write(&[0.0; 512], false).unwrap();
        assert!(!w.finish().unwrap().has_speech);
        let w = store.begin().unwrap();
        let id = w.record.id;
        store.mark_cancelled(id).unwrap();
        drop(w);
        store.delete(id).unwrap();
        assert!(RecordingStore::open(root.clone())
            .unwrap()
            .records()
            .is_empty());
        fs::remove_dir_all(root).unwrap();
    }
}

#[cfg(test)]
mod recovery_tests {
    use super::*;
    fn root() -> PathBuf {
        std::env::temp_dir().join(format!(
            "plume-recovery-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }
    #[test]
    fn cancellation_after_stt_persistence_does_not_evict_a_previous_audio() {
        let root = root();
        let mut store = RecordingStore::open(root.clone()).unwrap();
        let mut previous = Vec::new();
        for _ in 0..8 {
            let mut writer = store.begin().unwrap();
            writer.write(&[0.2; 512], true).unwrap();
            let record = writer.finish().unwrap();
            previous.push(record.id);
            store.promote(record).unwrap();
        }
        let mut writer = store.begin().unwrap();
        writer.write(&[0.2; 512], true).unwrap();
        let record = writer.finish().unwrap();
        let id = record.id;
        store.stage(record).unwrap();
        store.finish_decode(id, &Ok("final".into())).unwrap();
        assert!(store.partial_path(id).exists());
        assert!(!store.audio_path(id).exists());
        store.mark_cancelled(id).unwrap();
        store.delete(id).unwrap();
        assert!(previous.iter().all(|id| store.has_audio(*id)));
        assert_eq!(store.records().len(), 8);
        store.clear().unwrap();
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn promotion_keeps_the_result_persisted_before_dispatch() {
        let root = root();
        let mut store = RecordingStore::open(root.clone()).unwrap();
        let mut writer = store.begin().unwrap();
        writer.write(&[0.2; 512], true).unwrap();
        let record = writer.finish().unwrap();
        let id = record.id;
        store.stage(record).unwrap();
        store.finish_decode(id, &Ok("final".into())).unwrap();
        store
            .finish_insertion(
                id,
                &crate::DictationResult {
                    text: "final".into(),
                    record_id: Some(id),
                    audio_available: true,
                    transcription_error: None,
                    recovered: false,
                    copied_on_failure: true,
                    injection: Err("dispatch failed".into()),
                },
            )
            .unwrap();
        store.promote_existing(id).unwrap();
        assert!(!store.partial_path(id).exists());
        drop(store);
        let mut store = RecordingStore::open(root.clone()).unwrap();
        let recovered = &store.records()[0];
        assert_eq!(recovered.text.as_deref(), Some("final"));
        assert_eq!(recovered.insertion.as_ref().unwrap().status, "Copied");
        assert!(store.audio_path(id).exists());
        store.clear().unwrap();
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn crash_before_metadata_rename_recovers_first_speech() {
        let root = root();
        let mut store = RecordingStore::open(root.clone()).unwrap();
        let mut writer = store.begin().unwrap();
        let before = writer.record.clone();
        writer.write(&[0.2; 512], true).unwrap();
        let after = writer.record.clone();
        drop(writer);
        fs::write(
            store.meta_path(before.id),
            serde_json::to_vec(&before).unwrap(),
        )
        .unwrap();
        fs::write(
            root.join(format!("{}.json.tmp", before.id)),
            serde_json::to_vec(&after).unwrap(),
        )
        .unwrap();
        drop(store);
        let mut store = RecordingStore::open(root.clone()).unwrap();
        assert!(store.has_audio(before.id));
        assert_eq!(store.records()[0].status, "Interrupted");
        store.clear().unwrap();
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn corrupt_metadata_keeps_promoted_audio_recoverable_and_clear_removes_orphans() {
        let root = root();
        let mut store = RecordingStore::open(root.clone()).unwrap();
        let mut writer = store.begin().unwrap();
        writer.write(&[0.2; 512], true).unwrap();
        let record = writer.finish().unwrap();
        let id = record.id;
        store.promote(record).unwrap();
        fs::write(store.meta_path(id), b"interrupted metadata").unwrap();
        fs::write(store.cancel_path(id + 1), b"").unwrap();
        drop(store);
        let mut store = RecordingStore::open(root.clone()).unwrap();
        assert!(store.has_audio(id));
        assert_eq!(store.records()[0].status, "Interrupted");
        assert!(!store.cancel_path(id + 1).exists());
        store.clear().unwrap();
        assert_eq!(fs::read_dir(&root).unwrap().count(), 0);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn flushed_interruption_recovers_and_cancel_marker_survives_writer_updates() {
        let root = root();
        let mut store = RecordingStore::open(root.clone()).unwrap();
        let mut writer = store.begin().unwrap();
        let id = writer.record.id;
        writer.write(&[0.2; 16000], true).unwrap();
        drop(writer);
        drop(store);
        let mut store = RecordingStore::open(root.clone()).unwrap();
        assert!(store.has_audio(id));
        assert_eq!(store.records()[0].status, "Interrupted");
        store.clear().unwrap();
        let mut writer = store.begin().unwrap();
        let id = writer.record.id;
        store.mark_cancelled(id).unwrap();
        writer.write(&[0.2; 16000], true).unwrap();
        drop(writer);
        drop(store);
        let store = RecordingStore::open(root.clone()).unwrap();
        assert!(!store.has_audio(id));
        assert!(store.records().is_empty());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn storage_failure_prevents_start_and_progressive_writes_fail_explicitly() {
        let root = root();
        let mut store = RecordingStore::open(root.clone()).unwrap();
        let writer = store.begin().unwrap();
        let temp = root.join(format!("{}.json.tmp", writer.record.id));
        fs::create_dir(temp).unwrap();
        let mut writer = writer;
        assert!(writer.write(&[0.1; 16000], true).is_err());
        drop(writer);
        let blocked = root.join("not-a-directory");
        fs::write(&blocked, b"blocked").unwrap();
        assert!(RecordingStore::open(blocked).is_err());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn retry_updates_same_take_and_preserves_prior_insertion() {
        let root = root();
        let mut store = RecordingStore::open(root.clone()).unwrap();
        let mut writer = store.begin().unwrap();
        writer.write(&[0.2; 512], true).unwrap();
        let record = writer.finish().unwrap();
        let id = record.id;
        store.promote(record).unwrap();
        store.finish_decode(id, &Ok("original".into())).unwrap();
        store.acknowledge_history(id).unwrap();
        let result = crate::DictationResult {
            text: "original".into(),
            record_id: Some(id),
            audio_available: true,
            transcription_error: None,
            recovered: false,
            copied_on_failure: true,
            injection: Err("failed dispatch".into()),
        };
        store.finish_insertion(id, &result).unwrap();
        store.begin_retry(id).unwrap();
        store.finish_decode(id, &Ok("recovered".into())).unwrap();
        assert_eq!(store.records().len(), 1);
        let record = &store.records()[0];
        assert_eq!(record.text.as_deref(), Some("recovered"));
        assert_eq!(record.insertion.as_ref().unwrap().status, "Copied");
        assert!(!record.history_saved);
        store.delete_audio(id).unwrap();
        assert!(!store.has_audio(id));
        assert_eq!(store.records()[0].text.as_deref(), Some("recovered"));
        store.clear().unwrap();
        fs::remove_dir_all(root).unwrap();
    }
}
