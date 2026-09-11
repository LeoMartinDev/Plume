mod decode;
mod model;
mod wav;

pub use model::{Engine, ModelDir};
use stt_core::{AsrEngine, AudioStream, HypothesisStream};
pub use wav::audio_from_wav;

impl AsrEngine for Engine {
    fn stream(&self, audio: AudioStream) -> HypothesisStream {
        Box::new(decode::NemotronStream::new(self.inner.clone(), audio))
    }
}
