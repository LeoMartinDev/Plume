use ort::{session::Session, value::Tensor};
use plume_core::BoxError;
use std::path::Path;

/// CPU Silero v5 ONNX, with one state per utterance.
pub struct SpeechDetector {
    session: Session,
    state: Vec<f32>,
    context: Vec<f32>,
    pending: Vec<f32>,
}
impl SpeechDetector {
    pub fn open(path: &Path) -> Result<Self, BoxError> {
        let session = Session::builder()?
            .with_intra_threads(1)
            .map_err(|e| e.to_string())?
            .commit_from_file(path)?;
        for name in ["input", "state", "sr"] {
            if !session.inputs().iter().any(|p| p.name() == name) {
                return Err(format!("unsupported Silero input: {name}").into());
            }
        }
        for name in ["output", "stateN"] {
            if !session.outputs().iter().any(|p| p.name() == name) {
                return Err(format!("unsupported Silero output: {name}").into());
            }
        }
        Ok(Self {
            session,
            state: vec![0.0; 256],
            context: vec![0.0; 64],
            pending: Vec::new(),
        })
    }
    pub fn push(&mut self, samples: &[f32]) -> Result<bool, BoxError> {
        self.pending.extend_from_slice(samples);
        let mut speech = false;
        while self.pending.len() >= 512 {
            let frame: Vec<_> = self.pending.drain(..512).collect();
            speech |= self.frame(&frame)?;
        }
        Ok(speech)
    }
    pub fn finish(&mut self) -> Result<bool, BoxError> {
        if self.pending.is_empty() {
            return Ok(false);
        }
        let mut frame = std::mem::take(&mut self.pending);
        frame.resize(512, 0.0);
        self.frame(&frame)
    }
    fn frame(&mut self, frame: &[f32]) -> Result<bool, BoxError> {
        let mut input = self.context.clone();
        input.extend_from_slice(frame);
        let outputs = self.session.run(ort::inputs![
            "input"=>Tensor::from_array(([1,576],input))?,
            "state"=>Tensor::from_array(([2,1,128],self.state.clone()))?,
            "sr"=>Tensor::from_array(([] as [usize;0],vec![16_000i64]))?
        ])?;
        let (_, prob) = outputs["output"].try_extract_tensor::<f32>()?;
        let (_, state) = outputs["stateN"].try_extract_tensor::<f32>()?;
        if prob.len() != 1 || state.len() != 256 {
            return Err("unsupported Silero output shape".into());
        }
        let speech = prob[0] >= 0.5;
        self.state.copy_from_slice(state);
        self.context.copy_from_slice(&frame[448..]);
        Ok(speech)
    }
}
