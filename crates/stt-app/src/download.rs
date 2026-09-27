use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc;

use stt_engine::Engine;

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

pub enum PackStatus {
    Proven(Engine),
    Incomplete(Staging),
}

pub enum DownloadEvent {
    Progress(Progress),
    Proven(Engine),
    Failed(DownloadError),
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

    pub fn resume(&mut self, events: mpsc::Sender<DownloadEvent>) {
        self.resume_with(&UreqFetch, events);
    }

    fn resume_with(&mut self, fetch: &dyn Fetch, events: mpsc::Sender<DownloadEvent>) {
        if let Err(err) = self.resume_inner(fetch, &events) {
            let _ = events.send(DownloadEvent::Failed(err));
        }
    }

    fn resume_inner(
        &mut self,
        fetch: &dyn Fetch,
        events: &mpsc::Sender<DownloadEvent>,
    ) -> Result<(), DownloadError> {
        let entry = self.offer.id.entry();
        let repo = entry.repo;
        let revision = entry.revision;
        std::fs::create_dir_all(&self.partial)?;
        for model_file in entry.files {
            let name = model_file.local;
            let done = self.partial.join(name);
            let part = self.partial.join(format!("{name}.part"));
            if done.is_file() {
                let _ = std::fs::remove_file(&part);
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
            fetch.fetch_to_file(&url, &part, &move |bytes, total| {
                let _ = events.send(DownloadEvent::Progress(Progress {
                    file: file.clone(),
                    bytes,
                    total,
                }));
            })?;
            std::fs::rename(&part, &done)?;
        }
        if self.dest.exists() {
            std::fs::remove_dir_all(&self.dest)?;
        }
        std::fs::rename(&self.partial, &self.dest)?;
        match try_open(self.offer.id, &self.dest) {
            Ok(engine) => {
                let _ = events.send(DownloadEvent::Proven(engine));
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
            Ok(engine) => return Ok(PackStatus::Proven(engine)),
            Err(_) => move_failed(dest),
        }
    }
    Ok(PackStatus::Incomplete(Staging {
        dest: dest.to_path_buf(),
        partial: sibling_partial(dest),
        offer,
    }))
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
            .header("User-Agent", "stt-app/0.1")
            .call()
            .map_err(|err| DownloadError::Fetch(err.to_string()))?;
        let total = response.body().content_length();
        let mut reader = response.body_mut().as_reader();
        let mut file = std::fs::File::create(dest)?;
        let mut buf = [0u8; 64 * 1024];
        let mut received = 0u64;
        loop {
            let n = reader.read(&mut buf)?;
            if n == 0 {
                break;
            }
            file.write_all(&buf[..n])?;
            received += n as u64;
            progress(received, total);
        }
        file.flush()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use stt_engine::ModelDir;

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
            "stt-app-pack-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join(ModelId::Nemotron35Compact.as_str())
    }

    fn dummy_files() -> HashMap<String, Vec<u8>> {
        ModelDir::REQUIRED_FILES
            .into_iter()
            .map(|name| (name.to_string(), b"not an onnx graph".to_vec()))
            .collect()
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
        staging.resume_with(&fetch, tx);
        let events: Vec<_> = rx.iter().collect();
        assert!(
            events
                .iter()
                .any(|ev| matches!(ev, DownloadEvent::Failed(_))),
            "dummy graphs must fail Engine::open"
        );
        assert!(!events.iter().any(|ev| match ev {
            DownloadEvent::Progress(p) => p.file == "encoder.onnx",
            _ => false,
        }));
        let failed = dest.parent().unwrap().read_dir().unwrap().any(|entry| {
            entry
                .ok()
                .map(|e| e.file_name().to_string_lossy().contains("failed"))
                .unwrap_or(false)
        });
        assert!(failed, "open failure must move dest aside");
        let _ = std::fs::remove_dir_all(dest.parent().unwrap());
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
        let _ = std::fs::remove_dir_all(dest.parent().unwrap());
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
        let _ = std::fs::remove_dir_all(dest.parent().unwrap());
    }

    #[test]
    #[ignore]
    fn live_light_pack_opens() {
        let dest = match std::env::var_os("STT_KEEP_PACK") {
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
                std::thread::spawn(move || staging.resume(tx));
                let mut proven = false;
                for event in rx {
                    match event {
                        DownloadEvent::Progress(last) => {
                            eprintln!("live-light {} {} {:?}", last.file, last.bytes, last.total);
                        }
                        DownloadEvent::Proven(_) => proven = true,
                        DownloadEvent::Failed(err) => panic!("{err}"),
                    }
                }
                assert!(proven, "Light must open after fetch");
            }
            Err(err) => panic!("{err}"),
        }
        if std::env::var_os("STT_KEEP_PACK").is_none() {
            let _ = std::fs::remove_dir_all(dest.parent().unwrap());
        }
    }
}
