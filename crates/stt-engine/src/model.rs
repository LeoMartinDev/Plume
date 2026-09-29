use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex};

use ort::session::{builder::SessionBuilder, Session};
use stt_core::BoxError;
use whisper_rs::{WhisperContext, WhisperContextParameters};

use crate::decode::{Mel, Vocab, CACHE_MEL, CHUNK_MEL};

pub struct ModelDir {
    path: PathBuf,
}

impl ModelDir {
    pub const REQUIRED_FILES: [&'static str; 9] = [
        "encoder.onnx",
        "encoder.onnx.data",
        "decoder.onnx",
        "decoder.onnx.data",
        "joint.onnx",
        "joint.onnx.data",
        "silero_vad.onnx",
        "tokenizer.json",
        "vocab.txt",
    ];

    pub fn open(path: impl AsRef<Path>) -> Result<Self, BoxError> {
        let path = path.as_ref();
        if !path.is_dir() {
            return Err(format!("model dir {} is not a directory", path.display()).into());
        }
        for name in Self::REQUIRED_FILES {
            if !path.join(name).is_file() {
                return Err(format!("model dir {} is missing {name}", path.display()).into());
            }
        }
        Ok(Self {
            path: path.to_path_buf(),
        })
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

fn tensor_shape(session: &Session, port: &str, name: &str) -> Result<Vec<i64>, BoxError> {
    let outlet = session
        .inputs()
        .iter()
        .chain(session.outputs())
        .find(|o| o.name() == name)
        .ok_or_else(|| format!("{port} graph has no {name} port"))?;
    match outlet.dtype() {
        ort::value::ValueType::Tensor { shape, .. } => Ok(shape.to_vec()),
        _ => Err(format!("{port} port {name} is not a tensor").into()),
    }
}

pub(crate) struct OrtInner {
    pub encoder: Mutex<Session>,
    pub decoder: Mutex<Session>,
    pub joint: Mutex<Session>,
    pub vocab: Vocab,
    pub mel: Mel,
    pub cache_channel_shape: Vec<i64>,
    pub cache_channel_len: usize,
    pub cache_time_shape: Vec<i64>,
    pub cache_time_len: usize,
    pub lstm_shape: Vec<i64>,
    pub lstm_len: usize,
    pub enc_out_frames: usize,
    pub enc_hidden: usize,
    pub dec_hidden: usize,
    pub joint_dim: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Language {
    Auto,
    French,
    English,
}

impl Language {
    pub(crate) fn encoder_id(self) -> i64 {
        match self {
            Self::Auto => 101,
            Self::French => 8,
            Self::English => 0,
        }
    }
}

#[derive(Clone)]
pub struct LanguageTarget {
    language: Arc<AtomicI64>,
}

impl LanguageTarget {
    pub fn set(&self, language: Language) {
        self.language
            .store(language.encoder_id(), Ordering::Relaxed);
    }
}

#[derive(Clone)]
pub(crate) enum EngineBackend {
    Nemotron(Arc<OrtInner>),
    Whisper(Arc<WhisperContext>),
}

#[derive(Clone)]
pub struct Engine {
    pub(crate) backend: EngineBackend,
    pub(crate) language: Arc<AtomicI64>,
}

impl Engine {
    pub fn open(dir: ModelDir) -> Result<Self, BoxError> {
        let _ = ort::init().commit();
        let encoder =
            accelerated_session_builder()?.commit_from_file(dir.path().join("encoder.onnx"))?;
        let decoder =
            accelerated_session_builder()?.commit_from_file(dir.path().join("decoder.onnx"))?;
        let joint =
            accelerated_session_builder()?.commit_from_file(dir.path().join("joint.onnx"))?;

        let signal = tensor_shape(&encoder, "encoder", "audio_signal")?;
        let want = vec![1, (CACHE_MEL + CHUNK_MEL) as i64, 128];
        if signal != want {
            return Err(format!("encoder audio_signal is {signal:?}, want {want:?}").into());
        }
        if tensor_shape(&encoder, "encoder", "lang_id")? != vec![1] {
            return Err("encoder lang_id port has an unexpected shape".into());
        }
        let cache_channel_shape = tensor_shape(&encoder, "encoder", "cache_last_channel")?;
        let cache_time_shape = tensor_shape(&encoder, "encoder", "cache_last_time")?;
        let enc_out = tensor_shape(&encoder, "encoder", "outputs")?;
        let lstm_shape = tensor_shape(&decoder, "decoder", "h_in")?;
        let dec_out = tensor_shape(&decoder, "decoder", "decoder_output")?;
        let joint_out = tensor_shape(&joint, "joint", "joint_output")?;

        let [_, enc_out_frames, enc_hidden] = enc_out[..] else {
            return Err("encoder outputs port has an unexpected rank".into());
        };
        let [_, dec_hidden, _] = dec_out[..] else {
            return Err("decoder output port has an unexpected rank".into());
        };
        let Some(&joint_dim) = joint_out.last() else {
            return Err("joint output port has an unexpected rank".into());
        };
        if joint_dim <= 0 {
            return Err("joint output vocab dim is not static".into());
        }
        let vocab = Vocab::load(&dir.path().join("vocab.txt"), joint_dim as usize)?;
        let concrete = |shape: Vec<i64>| -> Result<(Vec<i64>, usize), BoxError> {
            let shape: Vec<i64> = shape
                .into_iter()
                .map(|d| if d == -1 { 1 } else { d })
                .collect();
            if shape.iter().any(|&d| d <= 0) {
                return Err(format!("cache port has a dynamic shape {shape:?}").into());
            }
            let len = shape.iter().map(|&d| d as usize).product();
            Ok((shape, len))
        };
        let (cache_channel_shape, cache_channel_len) = concrete(cache_channel_shape)?;
        let (cache_time_shape, cache_time_len) = concrete(cache_time_shape)?;
        let (lstm_shape, lstm_len) = concrete(lstm_shape)?;
        Ok(Self {
            backend: EngineBackend::Nemotron(Arc::new(OrtInner {
                encoder: Mutex::new(encoder),
                decoder: Mutex::new(decoder),
                joint: Mutex::new(joint),
                vocab,
                mel: Mel::new(),
                cache_channel_len,
                cache_time_len,
                lstm_len,
                cache_channel_shape,
                cache_time_shape,
                lstm_shape,
                enc_out_frames: enc_out_frames as usize,
                enc_hidden: enc_hidden as usize,
                dec_hidden: dec_hidden as usize,
                joint_dim: joint_dim as usize,
            })),
            language: Arc::new(AtomicI64::new(Language::Auto.encoder_id())),
        })
    }

    pub fn open_whisper(path: impl AsRef<Path>) -> Result<Self, BoxError> {
        let path = path.as_ref();
        if !path.is_file() {
            return Err(format!("whisper model {} is not a file", path.display()).into());
        }
        let path = path
            .to_str()
            .ok_or_else(|| format!("whisper model path {} is not UTF-8", path.display()))?;
        let context = WhisperContext::new_with_params(path, WhisperContextParameters::default())?;
        Ok(Self {
            backend: EngineBackend::Whisper(Arc::new(context)),
            language: Arc::new(AtomicI64::new(Language::Auto.encoder_id())),
        })
    }

    pub fn set_language(&self, language: Language) {
        self.language
            .store(language.encoder_id(), Ordering::Relaxed);
    }

    pub fn language_target(&self) -> LanguageTarget {
        LanguageTarget {
            language: self.language.clone(),
        }
    }
}

fn accelerated_session_builder() -> Result<SessionBuilder, ort::Error> {
    let mut builder = Session::builder()?;

    // DirectML requires memory patterns to be disabled. Registration is
    // deliberately best-effort: ORT logs the failure and retains its CPU EP.
    #[cfg(target_os = "windows")]
    {
        builder = builder
            .with_memory_pattern(false)?
            .with_execution_providers([
                #[cfg(feature = "cuda")]
                ort::ep::CUDA::default().build(),
                ort::ep::DirectML::default().build(),
            ])?;
    }

    #[cfg(target_os = "macos")]
    {
        builder = builder.with_execution_providers([
            #[cfg(feature = "cuda")]
            ort::ep::CUDA::default().build(),
            ort::ep::CoreML::default().build(),
        ])?;
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        #[cfg(any(feature = "cuda", feature = "rocm"))]
        {
            builder = builder.with_execution_providers([
                #[cfg(feature = "cuda")]
                ort::ep::CUDA::default().build(),
                #[cfg(feature = "rocm")]
                ort::ep::ROCm::default().build(),
            ])?;
        }
    }

    Ok(builder)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pack_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("stt-engine-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn model_dir_rejects_missing_files() {
        let dir = pack_dir("incomplete");
        std::fs::write(dir.join("encoder.onnx"), "x").unwrap();
        let err = ModelDir::open(&dir).map(|_| ()).unwrap_err();
        assert!(err.to_string().contains("encoder.onnx.data"), "got: {err}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn model_dir_rejects_non_directories() {
        let err = ModelDir::open("/no/such/pack").map(|_| ()).unwrap_err();
        assert!(err.to_string().contains("not a directory"), "got: {err}");
    }

    #[test]
    fn engine_open_rejects_unparsable_graphs() {
        let dir = pack_dir("fake");
        for name in ModelDir::REQUIRED_FILES {
            std::fs::write(dir.join(name), "not an onnx graph").unwrap();
        }
        match Engine::open(ModelDir::open(&dir).unwrap()) {
            Ok(_) => panic!("unparsable graphs must fail to open"),
            Err(e) => assert!(!e.to_string().is_empty()),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn language_ids_match_the_nemotron_prompt_table() {
        assert_eq!(Language::Auto.encoder_id(), 101);
        assert_eq!(Language::French.encoder_id(), 8);
        assert_eq!(Language::English.encoder_id(), 0);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_runtime_exposes_directml() {
        use ort::ep::ExecutionProvider;

        let _ = ort::init().commit();
        assert!(
            ort::ep::DirectML::default().is_available().unwrap(),
            "the packaged ONNX Runtime must include DirectML"
        );
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_whisper_prefers_vulkan_and_can_fall_back_to_cpu() {
        assert!(
            WhisperContextParameters::default().use_gpu,
            "the Windows build must compile Whisper with Vulkan support"
        );
    }
}
