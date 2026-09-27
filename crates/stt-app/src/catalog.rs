use std::path::Path;

use stt_engine::{Engine, ModelDir};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ModelId {
    Nemotron35Compact,
    WhisperBase,
    WhisperSmall,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EngineKind {
    Nemotron,
    Whisper,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ModelFile {
    pub remote: &'static str,
    pub local: &'static str,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ModelEntry {
    pub id: ModelId,
    pub name: &'static str,
    pub size: &'static str,
    pub languages: &'static str,
    pub guidance: &'static str,
    pub engine: EngineKind,
    pub repo: &'static str,
    pub revision: &'static str,
    pub files: &'static [ModelFile],
}

const NEMOTRON_FILES: &[ModelFile] = &[
    ModelFile {
        remote: "encoder.onnx",
        local: "encoder.onnx",
    },
    ModelFile {
        remote: "encoder.onnx.data",
        local: "encoder.onnx.data",
    },
    ModelFile {
        remote: "decoder.onnx",
        local: "decoder.onnx",
    },
    ModelFile {
        remote: "decoder.onnx.data",
        local: "decoder.onnx.data",
    },
    ModelFile {
        remote: "joint.onnx",
        local: "joint.onnx",
    },
    ModelFile {
        remote: "joint.onnx.data",
        local: "joint.onnx.data",
    },
    ModelFile {
        remote: "silero_vad.onnx",
        local: "silero_vad.onnx",
    },
    ModelFile {
        remote: "tokenizer.json",
        local: "tokenizer.json",
    },
    ModelFile {
        remote: "vocab.txt",
        local: "vocab.txt",
    },
];

const WHISPER_BASE_FILES: &[ModelFile] = &[ModelFile {
    remote: "ggml-base.bin",
    local: "model.bin",
}];

const WHISPER_SMALL_FILES: &[ModelFile] = &[ModelFile {
    remote: "ggml-small.bin",
    local: "model.bin",
}];

const CATALOG: &[ModelEntry] = &[
    ModelEntry {
        id: ModelId::Nemotron35Compact,
        name: "Nemotron 3.5 Compact",
        size: "793 MB",
        languages: "35 languages",
        guidance: "Shortest wait after speaking",
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
        guidance: "Better accuracy",
        engine: EngineKind::Whisper,
        repo: "ggerganov/whisper.cpp",
        revision: "5359861c739e955e79d9a303bcbc70fb988958b1",
        files: WHISPER_SMALL_FILES,
    },
];

impl ModelId {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Nemotron35Compact => "nemotron-3.5-compact",
            Self::WhisperBase => "whisper-base",
            Self::WhisperSmall => "whisper-small",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "light" | "nemotron-3.5-compact" => Some(Self::Nemotron35Compact),
            "whisper-base" => Some(Self::WhisperBase),
            "whisper-small" => Some(Self::WhisperSmall),
            _ => None,
        }
    }

    pub fn entry(self) -> &'static ModelEntry {
        entries()
            .iter()
            .find(|entry| entry.id == self)
            .expect("catalog ids always have an entry")
    }

    pub fn data_dir(self) -> std::path::PathBuf {
        crate::dirs::AppDirs::resolve().model_dir(self)
    }
}

impl Default for ModelId {
    fn default() -> Self {
        Self::Nemotron35Compact
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_has_named_models_with_choice_metadata() {
        assert_eq!(entries().len(), 3);
        for entry in entries() {
            assert!(!entry.name.is_empty());
            assert!(!entry.size.is_empty());
            assert!(!entry.languages.is_empty());
            assert!(!entry.guidance.is_empty());
            assert!(!entry.files.is_empty());
        }
    }

    #[test]
    fn legacy_light_migrates_but_generic_tiers_do_not_exist() {
        assert_eq!(ModelId::parse("light"), Some(ModelId::Nemotron35Compact));
        assert_eq!(ModelId::parse("medium"), None);
        assert_eq!(ModelId::parse("large"), None);
    }
}
