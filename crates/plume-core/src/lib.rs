use std::error::Error;

mod audio;
mod dictation;
mod hotkey;
mod insertion;
mod session;
mod transcription;

pub type BoxError = Box<dyn Error + Send + Sync>;
pub use audio::AudioChunk;
pub use dictation::{Dictation, DictationUpdate, Edit};
pub use hotkey::{GlobalHotkey, HotkeyEvent};
pub use insertion::{
    InjectionReport, InsertionMethod, InsertionMode, TargetAssessment, TextInjector,
};
pub use session::{
    Cancelled, Finalizing, PartialTranscript, Recording, Session, SessionState, Streaming,
};
pub use transcription::{
    AsrEngine, AudioStream, Hypothesis, HypothesisStream, PartialHypothesis, Transcript,
};
