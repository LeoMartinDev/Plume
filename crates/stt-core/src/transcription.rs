use crate::{AudioChunk, BoxError};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PartialHypothesis {
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Transcript {
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Hypothesis {
    Partial(PartialHypothesis),
    Final(Transcript),
}

/// Pull-based streams keep stt-core dependency-free. Mic capture feeds the
/// input through a channel's iterator; the engine yields hypotheses the same
/// way.
pub type AudioStream = Box<dyn Iterator<Item = AudioChunk> + Send>;
pub type HypothesisStream = Box<dyn Iterator<Item = Result<Hypothesis, BoxError>> + Send>;

/// Local speech-to-text engine. One streaming call per dictation session:
/// audio chunks in, partial hypotheses and one final transcript out.
pub trait AsrEngine {
    fn stream(&self, audio: AudioStream) -> HypothesisStream;
}
