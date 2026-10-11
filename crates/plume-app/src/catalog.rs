use std::path::Path;

use plume_engine::{Engine, ModelDir};

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum ModelId {
    #[default]
    Nemotron35Compact,
    WhisperBase,
    WhisperSmall,
    WhisperLargeV3Turbo,
    ParakeetTdtV3,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EngineKind {
    Nemotron,
    Whisper,
    Parakeet,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ModelFile {
    pub remote: &'static str,
    pub local: &'static str,
    pub bytes: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ModelEntry {
    pub id: ModelId,
    pub name: &'static str,
    pub size: &'static str,
    pub languages: &'static str,
    pub guidance: &'static str,
    /// 1..=5, 5 = fastest. Relative order on a typical desktop GPU.
    pub speed: u8,
    /// 1..=5, 5 = most accurate for everyday FR dictation.
    pub accuracy: u8,
    /// True when the engine streams partial hypotheses word by word.
    pub streaming: bool,
    pub engine: EngineKind,
    pub repo: &'static str,
    pub revision: &'static str,
    pub files: &'static [ModelFile],
}

const NEMOTRON_FILES: &[ModelFile] = &[
    ModelFile {
        remote: "encoder.onnx",
        local: "encoder.onnx",
        bytes: 2_677_548,
    },
    ModelFile {
        remote: "encoder.onnx.data",
        local: "encoder.onnx.data",
        bytes: 690_089_984,
    },
    ModelFile {
        remote: "decoder.onnx",
        local: "decoder.onnx",
        bytes: 4_696,
    },
    ModelFile {
        remote: "decoder.onnx.data",
        local: "decoder.onnx.data",
        bytes: 59_785_216,
    },
    ModelFile {
        remote: "joint.onnx",
        local: "joint.onnx",
        bytes: 2_136,
    },
    ModelFile {
        remote: "joint.onnx.data",
        local: "joint.onnx.data",
        bytes: 37_830_656,
    },
    ModelFile {
        remote: "silero_vad.onnx",
        local: "silero_vad.onnx",
        bytes: 2_243_022,
    },
    ModelFile {
        remote: "tokenizer.json",
        local: "tokenizer.json",
        bytes: 642_525,
    },
    ModelFile {
        remote: "vocab.txt",
        local: "vocab.txt",
        bytes: 64_024,
    },
];

const WHISPER_BASE_FILES: &[ModelFile] = &[ModelFile {
    remote: "ggml-base.bin",
    local: "model.bin",
    bytes: 147_951_465,
}];

const WHISPER_SMALL_FILES: &[ModelFile] = &[ModelFile {
    remote: "ggml-small.bin",
    local: "model.bin",
    bytes: 487_601_967,
}];

const WHISPER_LARGE_V3_TURBO_FILES: &[ModelFile] = &[ModelFile {
    remote: "ggml-large-v3-turbo.bin",
    local: "model.bin",
    bytes: 1_624_555_275,
}];

const PARAKEET_FILES: &[ModelFile] = &[
    ModelFile {
        remote: "encoder-model.int8.onnx",
        local: "encoder.onnx",
        bytes: 652_183_999,
    },
    ModelFile {
        remote: "decoder_joint-model.int8.onnx",
        local: "decoder_joint.onnx",
        bytes: 18_202_004,
    },
    ModelFile {
        remote: "vocab.txt",
        local: "vocab.txt",
        bytes: 93_939,
    },
];

const CATALOG: &[ModelEntry] = &[
    ModelEntry {
        id: ModelId::Nemotron35Compact,
        name: "Nemotron 3.5 Compact",
        size: "793 MB",
        languages: "35 languages",
        guidance: "Fastest response",
        speed: 5,
        accuracy: 3,
        streaming: true,
        engine: EngineKind::Nemotron,
        repo: "onnx-community/nemotron-3.5-asr-streaming-0.6b-onnx-int4",
        revision: "8364d9e2dd9da23789b480bdbba9e423717e42ee",
        files: NEMOTRON_FILES,
    },
    ModelEntry {
        id: ModelId::WhisperBase,
        name: "Whisper Base",
        size: "142 MB",
        languages: "99 languages",
        guidance: "Smallest download",
        speed: 4,
        accuracy: 2,
        streaming: false,
        engine: EngineKind::Whisper,
        repo: "ggerganov/whisper.cpp",
        revision: "5359861c739e955e79d9a303bcbc70fb988958b1",
        files: WHISPER_BASE_FILES,
    },
    ModelEntry {
        id: ModelId::WhisperSmall,
        name: "Whisper Small",
        size: "466 MB",
        languages: "99 languages",
        guidance: "Balanced",
        speed: 3,
        accuracy: 4,
        streaming: false,
        engine: EngineKind::Whisper,
        repo: "ggerganov/whisper.cpp",
        revision: "5359861c739e955e79d9a303bcbc70fb988958b1",
        files: WHISPER_SMALL_FILES,
    },
    ModelEntry {
        id: ModelId::ParakeetTdtV3,
        name: "Parakeet TDT v3",
        size: "639 MB",
        languages: "25 European languages",
        guidance: "Fast and accurate",
        speed: 4,
        accuracy: 4,
        streaming: false,
        engine: EngineKind::Parakeet,
        repo: "istupakov/parakeet-tdt-0.6b-v3-onnx",
        revision: "8f23f0c03c8761650bdb5b40aaf3e40d2c15f1ce",
        files: PARAKEET_FILES,
    },
    ModelEntry {
        id: ModelId::WhisperLargeV3Turbo,
        name: "Whisper Large-v3-Turbo",
        size: "1.5 GB",
        languages: "99 languages",
        guidance: "Best accuracy · Fr+En",
        speed: 2,
        accuracy: 5,
        streaming: false,
        engine: EngineKind::Whisper,
        repo: "ggerganov/whisper.cpp",
        revision: "5359861c739e955e79d9a303bcbc70fb988958b1",
        files: WHISPER_LARGE_V3_TURBO_FILES,
    },
];

impl ModelId {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Nemotron35Compact => "nemotron-3.5-compact",
            Self::WhisperBase => "whisper-base",
            Self::WhisperSmall => "whisper-small",
            Self::WhisperLargeV3Turbo => "whisper-large-v3-turbo",
            Self::ParakeetTdtV3 => "parakeet-tdt-v3",
        }
    }
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "nemotron-3.5-compact" => Some(Self::Nemotron35Compact),
            "whisper-base" => Some(Self::WhisperBase),
            "whisper-small" => Some(Self::WhisperSmall),
            "whisper-large-v3-turbo" => Some(Self::WhisperLargeV3Turbo),
            "parakeet-tdt-v3" => Some(Self::ParakeetTdtV3),
            _ => None,
        }
    }

    pub fn entry(self) -> &'static ModelEntry {
        entries()
            .iter()
            .find(|entry| entry.id == self)
            .expect("catalog ids always have an entry")
    }

    /// True when the model covers the pinned settings language.
    /// All current packs cover French and English (Nemotron: 35 languages,
    /// Whisper: 99, Parakeet: 25), so this only dims future packs that
    /// are English-only or French-only.
    pub fn supports(self, language: crate::prefs::LanguagePref) -> bool {
        use crate::prefs::LanguagePref;
        match language {
            LanguagePref::Auto => true,
            LanguagePref::French | LanguagePref::English => match self {
                Self::Nemotron35Compact
                | Self::WhisperBase
                | Self::WhisperSmall
                | Self::WhisperLargeV3Turbo
                | Self::ParakeetTdtV3 => true,
            },
        }
    }

    pub fn data_dir(self) -> std::path::PathBuf {
        crate::dirs::AppDirs::resolve().model_dir(self)
    }
}

pub fn entries() -> &'static [ModelEntry] {
    CATALOG
}

pub fn is_complete(id: ModelId, dir: &Path) -> bool {
    id.entry()
        .files
        .iter()
        .all(|file| dir.join(file.local).is_file())
}

pub fn open(id: ModelId, dir: &Path) -> Result<Engine, String> {
    match id.entry().engine {
        EngineKind::Nemotron => {
            let dir = ModelDir::open(dir).map_err(|err| err.to_string())?;
            Engine::open(dir).map_err(|err| err.to_string())
        }
        EngineKind::Whisper => {
            Engine::open_whisper(dir.join("model.bin")).map_err(|err| err.to_string())
        }
        EngineKind::Parakeet => Engine::open_parakeet(dir).map_err(|err| err.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_has_named_models_with_choice_metadata() {
        assert_eq!(entries().len(), 5);
        for entry in entries() {
            assert!(!entry.name.is_empty());
            assert!(!entry.size.is_empty());
            assert!(!entry.languages.is_empty());
            assert!(!entry.guidance.is_empty());
            assert!(!entry.files.is_empty());
            assert!(
                (1..=5).contains(&entry.speed),
                "speed 1..=5 for {}",
                entry.name
            );
            assert!(
                (1..=5).contains(&entry.accuracy),
                "accuracy 1..=5 for {}",
                entry.name
            );
        }
    }

    #[test]
    fn model_ids_round_trip_and_unknown_ids_are_rejected() {
        for entry in entries() {
            assert_eq!(ModelId::parse(entry.id.as_str()), Some(entry.id));
        }
        assert_eq!(ModelId::parse("light"), None);
    }

    #[test]
    fn every_model_covers_french_and_english() {
        use crate::prefs::LanguagePref;
        for entry in entries() {
            assert!(entry.id.supports(LanguagePref::Auto));
            assert!(entry.id.supports(LanguagePref::French));
            assert!(entry.id.supports(LanguagePref::English));
        }
    }
}
