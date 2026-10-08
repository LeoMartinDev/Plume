const PROGRESS_REPORT_INTERVAL: std::time::Duration = std::time::Duration::from_millis(50);

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Instant;

use plume_engine::Engine;

use crate::catalog::ModelId;
use crate::phase::Progress;

#[derive(Clone, Copy, Debug)]
pub struct PackOffer {
    pub id: ModelId,
}

impl ModelId {
    pub fn offer(self) -> PackOffer {
        PackOffer { id: self }
    }
}

/// Identifies one reconciliation attempt for a particular model.
///
/// A model can be selected more than once while an earlier attempt is still
/// running, so the model id alone is not enough to identify the current work.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DownloadRequest {
    pub model: ModelId,
    pub generation: u64,
}

/// Tracks which request is currently allowed to update the application.
#[derive(Default)]
pub struct DownloadRequestTracker {
    next_generation: u64,
    active: Option<DownloadRequest>,
}

impl DownloadRequestTracker {
    pub fn register(&mut self, model: ModelId) -> DownloadRequest {
        self.next_generation = self.next_generation.wrapping_add(1);
        let request = DownloadRequest {
            model,
            generation: self.next_generation,
        };
        self.active = Some(request);
        request
    }

    pub fn accepts(&self, selected_model: ModelId, request: DownloadRequest) -> bool {
        self.active == Some(request) && selected_model == request.model
    }

    pub fn is_active_for(&self, model: ModelId) -> bool {
        self.active.is_some_and(|request| request.model == model)
    }

    pub fn clear(&mut self) {
        self.active = None;
    }
}

pub enum PackStatus {
    Proven(Engine),
    Incomplete(Staging),
}

pub enum DownloadEvent {
    Progress {
        request: DownloadRequest,
        last: Progress,
    },
    Proven {
        request: DownloadRequest,
        engine: Engine,
    },
    Failed {
        request: DownloadRequest,
        error: DownloadError,
    },
}

impl DownloadEvent {
    pub fn request(&self) -> DownloadRequest {
        match self {
            Self::Progress { request, .. }
            | Self::Proven { request, .. }
            | Self::Failed { request, .. } => *request,
        }
    }
}

#[derive(Debug)]
pub enum DownloadError {
    Io(std::io::Error),
    Open(String),
    Fetch(String),
}

impl std::fmt::Display for DownloadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DownloadError::Io(err) => write!(f, "pack io: {err}"),
            DownloadError::Open(err) => write!(f, "pack open: {err}"),
            DownloadError::Fetch(err) => write!(f, "pack fetch: {err}"),
        }
    }
}

impl std::error::Error for DownloadError {}

impl From<std::io::Error> for DownloadError {
    fn from(err: std::io::Error) -> Self {
        DownloadError::Io(err)
    }
}

/// Dest plus sibling staging directory `{dest}.partial`.
pub struct Staging {
    dest: PathBuf,
    partial: PathBuf,
    offer: PackOffer,
}

impl Staging {
    pub fn started(&self) -> bool {
        self.partial.is_dir()
            && std::fs::read_dir(&self.partial)
                .ok()
                .and_then(|mut entries| entries.next())
                .is_some()
    }

    pub fn resume(&mut self, request: DownloadRequest, events: mpsc::Sender<DownloadEvent>) {
        self.resume_with(&UreqFetch, request, events);
    }

    fn resume_with(
        &mut self,
        fetch: &dyn Fetch,
        request: DownloadRequest,
        events: mpsc::Sender<DownloadEvent>,
    ) {
        debug_assert_eq!(request.model, self.offer.id);
        if let Err(error) = self.resume_inner(fetch, request, &events) {
            let _ = events.send(DownloadEvent::Failed { request, error });
        }
    }

    fn resume_inner(
        &mut self,
        fetch: &dyn Fetch,
        request: DownloadRequest,
        events: &mpsc::Sender<DownloadEvent>,
    ) -> Result<(), DownloadError> {
        if self.dest.is_dir() {
            if let Ok(engine) = try_open(self.offer.id, &self.dest) {
                prepare_vad(fetch, &self.dest, request, events)?;
                let _ = events.send(DownloadEvent::Proven { request, engine });
                return Ok(());
            }
        }
        let entry = self.offer.id.entry();
        let repo = entry.repo;
        let revision = entry.revision;
        let total_bytes = entry.files.iter().map(|file| file.bytes).sum();
        let mut completed_bytes = 0u64;
        let mut downloaded_bytes = 0u64;
        let download_started = Instant::now();
        std::fs::create_dir_all(&self.partial)?;
        for model_file in entry.files {
            let name = model_file.local;
            let done = self.partial.join(name);
            let part = self.partial.join(format!("{name}.part"));
            if done.is_file() {
                let _ = std::fs::remove_file(&part);
                completed_bytes = completed_bytes.saturating_add(model_file.bytes);
                continue;
            }
            if part.exists() {
                std::fs::remove_file(&part)?;
            }
            let url = format!(
                "https://huggingface.co/{repo}/resolve/{revision}/{}",
                model_file.remote
            );
            let events = events.clone();
            let file = name.to_string();
            let base_bytes = completed_bytes;
            let base_downloaded = downloaded_bytes;
            fetch.fetch_to_file(&url, &part, &move |bytes, _| {
                let elapsed = download_started.elapsed().as_secs_f64();
                let bytes_per_second = (elapsed >= 0.1).then(|| {
                    ((base_downloaded.saturating_add(bytes)) as f64 / elapsed).round() as u64
                });
                let _ = events.send(DownloadEvent::Progress {
                    request,
                    last: Progress {
                        file: file.clone(),
                        bytes: base_bytes.saturating_add(bytes),
                        total: Some(total_bytes),
                        bytes_per_second,
                    },
                });
            })?;
            std::fs::rename(&part, &done)?;
            completed_bytes = completed_bytes.saturating_add(model_file.bytes);
            downloaded_bytes = downloaded_bytes.saturating_add(model_file.bytes);
        }
        if self.dest.exists() {
            std::fs::remove_dir_all(&self.dest)?;
        }
        std::fs::rename(&self.partial, &self.dest)?;
        match try_open(self.offer.id, &self.dest) {
            Ok(engine) => {
                prepare_vad(fetch, &self.dest, request, events)?;
                let _ = events.send(DownloadEvent::Proven { request, engine });
                Ok(())
            }
            Err(err) => {
                move_failed(&self.dest);
                Err(err)
            }
        }
    }
}

/// Scan dest, then staging. Never fetches a dest that already opens.
pub fn reconcile(offer: PackOffer, dest: &Path) -> Result<PackStatus, DownloadError> {
    if dest.is_dir() {
        match try_open(offer.id, dest) {
            Ok(engine) => {
                let common = shared_vad_path(dest);
                if !vad_ready(&common) {
                    copy_existing_vad(dest, &common)?;
                }
                if vad_ready(&common) {
                    return Ok(PackStatus::Proven(engine));
                }
                // A valid ASR pack is retained while downloading only the shared VAD.
            }
            Err(_) => move_failed(dest),
        }
    }
    Ok(PackStatus::Incomplete(Staging {
        dest: dest.to_path_buf(),
        partial: sibling_partial(dest),
        offer,
    }))
}

fn shared_vad_path(dest: &Path) -> PathBuf {
    dest.parent()
        .and_then(Path::parent)
        .unwrap_or(dest)
        .join("speech")
        .join("silero_vad.onnx")
}
fn vad_ready(path: &Path) -> bool {
    std::fs::metadata(path).is_ok_and(|metadata| metadata.len() == 2_243_022)
        && plume_engine::SpeechDetector::open(path).is_ok()
}
fn copy_existing_vad(dest: &Path, common: &Path) -> Result<(), DownloadError> {
    let mut candidates = vec![dest.join("silero_vad.onnx")];
    if let Some(packs) = dest.parent() {
        candidates.push(packs.join("light/silero_vad.onnx"));
    }
    for candidate in candidates {
        if vad_ready(&candidate) {
            std::fs::create_dir_all(common.parent().unwrap())?;
            let temp = common.with_extension("onnx.copy.tmp");
            std::fs::copy(candidate, &temp)?;
            crate::file_store::replace_file(&temp, common)?;
            break;
        }
    }
    Ok(())
}
fn prepare_vad(
    fetch: &dyn Fetch,
    dest: &Path,
    request: DownloadRequest,
    events: &mpsc::Sender<DownloadEvent>,
) -> Result<(), DownloadError> {
    let common = shared_vad_path(dest);
    if !vad_ready(&common) {
        copy_existing_vad(dest, &common)?;
    }
    if !vad_ready(&common) {
        std::fs::create_dir_all(common.parent().unwrap())?;
        let entry = ModelId::Nemotron35Compact.entry();
        let url = format!(
            "https://huggingface.co/{}/resolve/{}/silero_vad.onnx",
            entry.repo, entry.revision
        );
        let temp = common.with_extension(format!("onnx.{}.part", request.generation));
        fetch.fetch_to_file(&url, &temp, &|bytes, _| {
            let _ = events.send(DownloadEvent::Progress {
                request,
                last: Progress {
                    file: "Speech detection".into(),
                    bytes,
                    total: Some(2_243_022),
                    bytes_per_second: None,
                },
            });
        })?;
        if std::fs::metadata(&temp)?.len() != 2_243_022 {
            return Err(DownloadError::Fetch("speech detector size mismatch".into()));
        }
        plume_engine::SpeechDetector::open(&temp)
            .map_err(|e| DownloadError::Open(e.to_string()))?;
        crate::file_store::replace_file(&temp, &common)?;
    }
    plume_engine::SpeechDetector::open(&common).map_err(|e| DownloadError::Open(e.to_string()))?;
    Ok(())
}

/// Removes a downloaded model and any interrupted download for the same model.
pub fn remove(_offer: PackOffer, dest: &Path) -> Result<(), DownloadError> {
    remove_path(dest)?;
    remove_path(&sibling_partial(dest))?;
    Ok(())
}

fn remove_path(path: &Path) -> Result<(), DownloadError> {
    if path.is_dir() {
        std::fs::remove_dir_all(path)?;
    } else if path.exists() {
        std::fs::remove_file(path)?;
    }
    Ok(())
}

fn sibling_partial(dest: &Path) -> PathBuf {
    let name = dest
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "pack".to_string());
    dest.with_file_name(format!("{name}.partial"))
}

fn try_open(id: ModelId, dest: &Path) -> Result<Engine, DownloadError> {
    crate::catalog::open(id, dest).map_err(DownloadError::Open)
}

fn move_failed(dest: &Path) {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let name = dest
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "pack".to_string());
    let failed = dest.with_file_name(format!("{name}.failed.{secs}"));
    let _ = std::fs::rename(dest, failed);
}

trait Fetch: Send + Sync {
    fn fetch_to_file(
        &self,
        url: &str,
        dest: &Path,
        progress: &dyn Fn(u64, Option<u64>),
    ) -> Result<(), DownloadError>;
}

struct UreqFetch;

impl Fetch for UreqFetch {
    fn fetch_to_file(
        &self,
        url: &str,
        dest: &Path,
        progress: &dyn Fn(u64, Option<u64>),
    ) -> Result<(), DownloadError> {
        let mut response = ureq::get(url)
            .header("User-Agent", "Plume/0.1")
            .call()
            .map_err(|err| DownloadError::Fetch(err.to_string()))?;
        let total = response.body().content_length();
        let mut reader = response.body_mut().as_reader();
        let mut file = std::fs::File::create(dest)?;
        let mut buf = [0u8; 64 * 1024];
        let mut received = 0u64;
        let mut last_reported = 0u64;
        let mut last_report = Instant::now();
        loop {
            let n = reader.read(&mut buf)?;
            if n == 0 {
                break;
            }
            file.write_all(&buf[..n])?;
            received += n as u64;
            if last_reported == 0 || last_report.elapsed() >= PROGRESS_REPORT_INTERVAL {
                progress(received, total);
                last_reported = received;
                last_report = Instant::now();
            }
        }
        if last_reported != received {
            progress(received, total);
        }
        file.flush()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use plume_engine::ModelDir;
    use std::collections::HashMap;

    struct MapFetch {
        files: HashMap<String, Vec<u8>>,
    }

    impl Fetch for MapFetch {
        fn fetch_to_file(
            &self,
            url: &str,
            dest: &Path,
            progress: &dyn Fn(u64, Option<u64>),
        ) -> Result<(), DownloadError> {
            let name = url
                .rsplit('/')
                .next()
                .ok_or_else(|| DownloadError::Fetch("url has no file name".into()))?;
            let bytes = self
                .files
                .get(name)
                .ok_or_else(|| DownloadError::Fetch(format!("missing {name}")))?;
            std::fs::write(dest, bytes)?;
            progress(bytes.len() as u64, Some(bytes.len() as u64));
            Ok(())
        }
    }

    fn temp_dest(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "plume-app-pack-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("packs").join(ModelId::Nemotron35Compact.as_str())
    }

    fn dummy_files() -> HashMap<String, Vec<u8>> {
        ModelDir::REQUIRED_FILES
            .into_iter()
            .map(|name| (name.to_string(), b"not an onnx graph".to_vec()))
            .collect()
    }

    fn request() -> DownloadRequest {
        DownloadRequest {
            model: ModelId::Nemotron35Compact,
            generation: 1,
        }
    }

    #[test]
    fn corrupt_shared_vad_is_replaced_during_preparation() {
        let Ok(asset) = std::env::var("PLUME_VAD_PATH") else {
            return;
        };
        let dest = temp_dest("vad-repair");
        let common = shared_vad_path(&dest);
        std::fs::create_dir_all(common.parent().unwrap()).unwrap();
        std::fs::write(&common, b"interrupted asset").unwrap();
        let fetch = MapFetch {
            files: [("silero_vad.onnx".into(), std::fs::read(asset).unwrap())].into(),
        };
        let (tx, _rx) = mpsc::channel();
        prepare_vad(&fetch, &dest, request(), &tx).unwrap();
        assert!(vad_ready(&common));
        // Once prepared, no fetch is necessary at the next start.
        prepare_vad(
            &MapFetch {
                files: HashMap::new(),
            },
            &dest,
            request(),
            &tx,
        )
        .unwrap();
        std::fs::remove_dir_all(dest.parent().unwrap().parent().unwrap()).unwrap();
    }

    #[test]
    fn tracker_rejects_an_older_request_after_reselecting_the_same_model() {
        let mut tracker = DownloadRequestTracker::default();
        let first = tracker.register(ModelId::Nemotron35Compact);
        let _other = tracker.register(ModelId::WhisperBase);
        let latest = tracker.register(ModelId::Nemotron35Compact);

        assert!(!tracker.accepts(ModelId::Nemotron35Compact, first));
        assert!(tracker.accepts(ModelId::Nemotron35Compact, latest));
        assert!(tracker.is_active_for(ModelId::Nemotron35Compact));
    }

    #[test]
    fn tracker_clear_rejects_the_active_request() {
        let mut tracker = DownloadRequestTracker::default();
        let request = tracker.register(ModelId::WhisperSmall);
        tracker.clear();

        assert!(!tracker.accepts(ModelId::WhisperSmall, request));
        assert!(!tracker.is_active_for(ModelId::WhisperSmall));
    }

    #[test]
    fn resume_keeps_whole_files_and_refetches_parts() {
        let dest = temp_dest("parts");
        let partial = sibling_partial(&dest);
        std::fs::create_dir_all(&partial).unwrap();
        std::fs::write(partial.join("encoder.onnx"), b"kept").unwrap();
        std::fs::write(partial.join("encoder.onnx.data.part"), b"trunc").unwrap();
        let mut files = dummy_files();
        files.insert("encoder.onnx".into(), b"should not replace".to_vec());
        let fetch = MapFetch { files };
        let mut staging = Staging {
            dest: dest.clone(),
            partial,
            offer: ModelId::Nemotron35Compact.offer(),
        };
        let (tx, rx) = mpsc::channel();
        staging.resume_with(&fetch, request(), tx);
        let events: Vec<_> = rx.iter().collect();
        assert!(
            events
                .iter()
                .any(|ev| matches!(ev, DownloadEvent::Failed { .. })),
            "dummy graphs must fail Engine::open"
        );
        assert!(!events.iter().any(|ev| match ev {
            DownloadEvent::Progress { last, .. } => last.file == "encoder.onnx",
            _ => false,
        }));
        let progress: Vec<_> = events
            .iter()
            .filter_map(|event| match event {
                DownloadEvent::Progress { last, .. } => Some(last.bytes),
                _ => None,
            })
            .collect();
        assert!(
            progress.windows(2).all(|pair| pair[0] <= pair[1]),
            "model progress must never reset between files: {progress:?}"
        );
        let failed = dest.parent().unwrap().read_dir().unwrap().any(|entry| {
            entry
                .ok()
                .map(|e| e.file_name().to_string_lossy().contains("failed"))
                .unwrap_or(false)
        });
        assert!(failed, "open failure must move dest aside");
        let _ = std::fs::remove_dir_all(dest.parent().unwrap().parent().unwrap());
    }

    #[test]
    fn dest_that_already_opens_is_not_fetched() {
        let dest = temp_dest("missing-open");
        std::fs::create_dir_all(&dest).unwrap();
        for name in ModelDir::REQUIRED_FILES {
            std::fs::write(dest.join(name), b"not an onnx graph").unwrap();
        }
        match reconcile(ModelId::Nemotron35Compact.offer(), &dest) {
            Ok(PackStatus::Incomplete(_)) => {}
            Ok(PackStatus::Proven(_)) => panic!("unparsable graphs must not prove"),
            Err(err) => panic!("reconcile should return Incomplete after moving dest, got {err}"),
        }
        assert!(
            !dest.exists(),
            "failed dest must be renamed away so the next resume can fetch"
        );
        let _ = std::fs::remove_dir_all(dest.parent().unwrap().parent().unwrap());
    }

    #[test]
    fn remove_deletes_download_and_partial_directory() {
        let dest = temp_dest("remove");
        let partial = sibling_partial(&dest);
        std::fs::create_dir_all(&dest).unwrap();
        std::fs::create_dir_all(&partial).unwrap();
        std::fs::write(dest.join("model.bin"), b"model").unwrap();
        std::fs::write(partial.join("model.bin.part"), b"partial").unwrap();

        remove(ModelId::Nemotron35Compact.offer(), &dest).unwrap();

        assert!(!dest.exists());
        assert!(!partial.exists());
        let _ = std::fs::remove_dir_all(dest.parent().unwrap().parent().unwrap());
    }

    #[test]
    #[ignore]
    fn fetches_light_vocab_txt() {
        let dest = temp_dest("vocab");
        std::fs::create_dir_all(dest.parent().unwrap()).unwrap();
        let file = dest.parent().unwrap().join("vocab.txt");
        UreqFetch
            .fetch_to_file(
                &format!(
                    "https://huggingface.co/{}/resolve/{}/vocab.txt",
                    ModelId::Nemotron35Compact.entry().repo,
                    ModelId::Nemotron35Compact.entry().revision
                ),
                &file,
                &|_, _| {},
            )
            .expect("huggingface vocab.txt");
        let n = std::fs::metadata(&file).unwrap().len();
        assert!(n > 100, "vocab.txt too small: {n}");
        let _ = std::fs::remove_dir_all(dest.parent().unwrap().parent().unwrap());
    }

    #[test]
    #[ignore]
    fn live_light_pack_opens() {
        let dest = match std::env::var_os("PLUME_KEEP_PACK") {
            Some(dir) => {
                let dest = PathBuf::from(dir).join(ModelId::Nemotron35Compact.as_str());
                if let Some(parent) = dest.parent() {
                    std::fs::create_dir_all(parent).unwrap();
                }
                dest
            }
            None => temp_dest("live-light"),
        };
        match reconcile(ModelId::Nemotron35Compact.offer(), &dest) {
            Ok(PackStatus::Proven(_)) => {}
            Ok(PackStatus::Incomplete(mut staging)) => {
                let (tx, rx) = mpsc::channel();
                std::thread::spawn(move || staging.resume(request(), tx));
                let mut proven = false;
                for event in rx {
                    match event {
                        DownloadEvent::Progress { last, .. } => {
                            eprintln!("live-light {} {} {:?}", last.file, last.bytes, last.total);
                        }
                        DownloadEvent::Proven { .. } => proven = true,
                        DownloadEvent::Failed { error, .. } => panic!("{error}"),
                    }
                }
                assert!(proven, "Light must open after fetch");
            }
            Err(err) => panic!("{err}"),
        }
        if std::env::var_os("PLUME_KEEP_PACK").is_none() {
            let _ = std::fs::remove_dir_all(dest.parent().unwrap().parent().unwrap());
        }
    }
}
