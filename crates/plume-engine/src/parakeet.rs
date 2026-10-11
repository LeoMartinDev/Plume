//! NVIDIA Parakeet TDT (token-and-duration transducer), decoded once the
//! recording ends. The encoder sees the whole utterance; each joint step
//! predicts a token and how many encoder frames to skip.

use std::path::Path;
use std::sync::{Arc, Mutex};

use ort::session::{RunOptions, Session};
use plume_core::{AudioStream, BoxError, CancellationToken};

use crate::decode::{resample_to_16k, Mel, Vocab, HOP, MAX_SYMBOLS, N_MELS};
use crate::model::tensor_shape;

pub const REQUIRED_FILES: [&str; 3] = ["encoder.onnx", "decoder_joint.onnx", "vocab.txt"];

/// Parakeet TDT v3 predicts skips of 0 to 4 encoder frames.
const DURATIONS: usize = 5;

pub(crate) struct ParakeetInner {
    encoder: Mutex<Session>,
    decoder_joint: Mutex<Session>,
    vocab: Vocab,
    mel: Mel,
    enc_hidden: usize,
    state_shape: Vec<i64>,
    state_len: usize,
}

pub(crate) fn open(dir: &Path) -> Result<ParakeetInner, BoxError> {
    if !dir.is_dir() {
        return Err(format!("model dir {} is not a directory", dir.display()).into());
    }
    for name in REQUIRED_FILES {
        if !dir.join(name).is_file() {
            return Err(format!("model dir {} is missing {name}", dir.display()).into());
        }
    }
    let _ = ort::init().commit();
    // Both graphs stay on the CPU. The INT8 encoder's dynamically quantized
    // matmuls ran 2-7x slower under DirectML than on the CPU and changed
    // words, and the joint runs once per token on a single frame, too small
    // to gain from a GPU.
    let encoder = Session::builder()?.commit_from_file(dir.join("encoder.onnx"))?;
    let decoder_joint = Session::builder()?.commit_from_file(dir.join("decoder_joint.onnx"))?;

    let signal = tensor_shape(&encoder, "encoder", "audio_signal")?;
    if signal.len() != 3 || signal[1] != N_MELS as i64 {
        return Err(
            format!("encoder audio_signal is {signal:?}, want [batch, {N_MELS}, time]").into(),
        );
    }
    let enc_out = tensor_shape(&encoder, "encoder", "outputs")?;
    let [_, enc_hidden, _] = enc_out[..] else {
        return Err("encoder outputs port has an unexpected rank".into());
    };
    if enc_hidden <= 0 {
        return Err("encoder hidden size is not static".into());
    }
    let state = tensor_shape(&decoder_joint, "decoder_joint", "input_states_1")?;
    if tensor_shape(&decoder_joint, "decoder_joint", "input_states_2")? != state {
        return Err("decoder_joint state ports disagree".into());
    }
    let [layers, _, hidden] = state[..] else {
        return Err("decoder_joint state port has an unexpected rank".into());
    };
    if layers <= 0 || hidden <= 0 {
        return Err("decoder_joint state shape is not static".into());
    }
    let joint_out = tensor_shape(&decoder_joint, "decoder_joint", "outputs")?;
    let Some(&joint_dim) = joint_out.last() else {
        return Err("decoder_joint output port has an unexpected rank".into());
    };
    let vocab = Vocab::load_indexed(&dir.join("vocab.txt"))?;
    if joint_dim != (vocab.len() + DURATIONS) as i64 {
        return Err(format!(
            "joint emits {joint_dim} logits, want {} tokens and {DURATIONS} durations",
            vocab.len()
        )
        .into());
    }
    Ok(ParakeetInner {
        encoder: Mutex::new(encoder),
        decoder_joint: Mutex::new(decoder_joint),
        vocab,
        mel: Mel::new(),
        enc_hidden: enc_hidden as usize,
        state_shape: vec![layers, 1, hidden],
        state_len: (layers * hidden) as usize,
    })
}

pub(crate) fn transcribe(
    inner: &Arc<ParakeetInner>,
    audio: AudioStream,
    cancel: CancellationToken,
) -> Result<String, BoxError> {
    let options = Arc::new(RunOptions::new()?);
    let abort = options.clone();
    cancel.on_cancel(move || {
        let _ = abort.terminate();
    });
    let cancelled = || -> Result<(), BoxError> {
        if cancel.is_cancelled() {
            Err("transcription cancelled".into())
        } else {
            Ok(())
        }
    };

    let mut pcm = Vec::new();
    for chunk in audio {
        cancelled()?;
        if chunk.sample_rate == 0 {
            return Err("parakeet received audio with sample rate 0".into());
        }
        pcm.extend(resample_to_16k(&chunk.samples, chunk.sample_rate));
    }
    cancelled()?;
    let Some((features, frames, valid)) = features(&inner.mel, &pcm) else {
        return Ok(String::new());
    };

    let (encoded, encoded_frames, encoded_len) = {
        let mut encoder = inner.encoder.lock().map_err(|_| "encoder lock poisoned")?;
        let outputs = encoder.run_with_options(
            ort::inputs![
                "audio_signal" => ort::value::Tensor::from_array(([1, N_MELS, frames], features))?,
                "length" => ort::value::Tensor::from_array(([1], vec![valid as i64]))?
            ],
            &*options,
        )?;
        let (shape, encoded) = outputs["outputs"].try_extract_tensor::<f32>()?;
        let (_, lens) = outputs["encoded_lengths"].try_extract_tensor::<i64>()?;
        let [1, hidden, time] = shape[..] else {
            return Err("encoder returned an unexpected tensor shape".into());
        };
        if hidden as usize != inner.enc_hidden || lens.len() != 1 {
            return Err("encoder returned an unexpected tensor shape".into());
        }
        let time = time as usize;
        (encoded.to_vec(), time, (lens[0].max(0) as usize).min(time))
    };

    let mut decoder = inner
        .decoder_joint
        .lock()
        .map_err(|_| "decoder_joint lock poisoned")?;
    let vocab_size = inner.vocab.len();
    let blank = inner.vocab.blank;
    let mut state1 = vec![0.0f32; inner.state_len];
    let mut state2 = vec![0.0f32; inner.state_len];
    let mut last = blank as i32;
    let mut tokens = Vec::new();
    let mut frame = vec![0.0f32; inner.enc_hidden];
    let mut t = 0;
    let mut emitted = 0;
    while t < encoded_len {
        cancelled()?;
        // Encoder outputs are hidden-major: [1, hidden, time].
        for (d, slot) in frame.iter_mut().enumerate() {
            *slot = encoded[d * encoded_frames + t];
        }
        let outputs = decoder.run_with_options(
            ort::inputs![
                "encoder_outputs" => ort::value::Tensor::from_array(([1, inner.enc_hidden, 1], frame.clone()))?,
                "targets" => ort::value::Tensor::from_array(([1, 1], vec![last]))?,
                "target_length" => ort::value::Tensor::from_array(([1], vec![1i32]))?,
                "input_states_1" => ort::value::Tensor::from_array((inner.state_shape.clone(), state1.clone()))?,
                "input_states_2" => ort::value::Tensor::from_array((inner.state_shape.clone(), state2.clone()))?
            ],
            &*options,
        )?;
        let (_, logits) = outputs["outputs"].try_extract_tensor::<f32>()?;
        if logits.len() != vocab_size + DURATIONS {
            return Err("decoder_joint returned an unexpected tensor shape".into());
        }
        let token = argmax(&logits[..vocab_size]);
        let skip = argmax(&logits[vocab_size..]);
        if token != blank {
            let (_, next1) = outputs["output_states_1"].try_extract_tensor::<f32>()?;
            let (_, next2) = outputs["output_states_2"].try_extract_tensor::<f32>()?;
            if next1.len() != inner.state_len || next2.len() != inner.state_len {
                return Err("decoder_joint returned an unexpected state shape".into());
            }
            state1.copy_from_slice(next1);
            state2.copy_from_slice(next2);
            tokens.push(token);
            last = token as i32;
            emitted += 1;
        }
        if skip > 0 {
            t += skip;
            emitted = 0;
        } else if token == blank || emitted >= MAX_SYMBOLS {
            t += 1;
            emitted = 0;
        }
    }
    Ok(inner.vocab.decode(&tokens).trim_end().to_string())
}

fn argmax(values: &[f32]) -> usize {
    let mut best = 0;
    for (i, &v) in values.iter().enumerate().skip(1) {
        if v > values[best] {
            best = i;
        }
    }
    best
}

/// NeMo features: log-mel frames normalized per mel bin over the valid frames,
/// laid out mel-major (`N_MELS` x `frames`). Returns `None` when the audio is
/// too short to normalize.
fn features(mel: &Mel, pcm: &[f32]) -> Option<(Vec<f32>, usize, usize)> {
    let valid = pcm.len() / HOP;
    if valid < 2 {
        return None;
    }
    // NeMo's centered STFT yields one more frame than it counts as valid.
    let frames = valid + 1;
    let rows = mel.frames(pcm, 0, frames);
    let mut out = vec![0.0f32; N_MELS * frames];
    for m in 0..N_MELS {
        let bin = |t: usize| rows[t * N_MELS + m];
        let mean = (0..valid).map(bin).sum::<f32>() / valid as f32;
        let var = (0..valid).map(|t| (bin(t) - mean).powi(2)).sum::<f32>() / (valid - 1) as f32;
        let scale = 1.0 / (var.sqrt() + 1e-5);
        for t in 0..valid {
            out[m * frames + t] = (bin(t) - mean) * scale;
        }
    }
    Some((out, frames, valid))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn features_are_normalized_per_bin_and_mask_the_last_frame() {
        let pcm: Vec<f32> = (0..16_000)
            .map(|i| (2.0 * std::f32::consts::PI * 440.0 * i as f32 / 16_000.0).sin() * 0.3)
            .collect();
        let (out, frames, valid) = features(&Mel::new(), &pcm).unwrap();
        assert_eq!(valid, 100);
        assert_eq!(frames, 101);
        for m in [0, 20, 127] {
            let row = &out[m * frames..(m + 1) * frames];
            assert_eq!(row[valid], 0.0);
            let mean = row[..valid].iter().sum::<f32>() / valid as f32;
            assert!(mean.abs() < 1e-3, "bin {m} mean {mean}");
        }
    }

    #[test]
    fn audio_shorter_than_two_frames_has_no_features() {
        assert!(features(&Mel::new(), &[0.1; 319]).is_none());
        assert!(features(&Mel::new(), &[]).is_none());
    }

    fn transcribe_fixture(name: &str) -> String {
        use plume_core::{AsrEngine, Hypothesis};
        let model = std::env::var("PLUME_PARAKEET_MODEL").expect("PLUME_PARAKEET_MODEL");
        let engine = crate::Engine::open_parakeet(model).expect("open Parakeet model");
        let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures");
        let audio = crate::audio_from_wav(&fixtures.join(format!("{name}.wav"))).unwrap();
        let text = engine
            .stream(audio)
            .find_map(|item| match item.expect("Parakeet decode") {
                Hypothesis::Final(transcript) => Some(transcript.text),
                Hypothesis::Partial(_) => None,
            })
            .expect("final transcript");
        let expected = std::fs::read_to_string(fixtures.join(format!("{name}.expected.txt")))
            .unwrap()
            .trim()
            .to_string();
        assert_eq!(text.trim_end_matches('.'), expected);
        text
    }

    #[test]
    #[ignore = "requires PLUME_PARAKEET_MODEL pointing to a Parakeet TDT v3 directory"]
    fn downloaded_parakeet_model_transcribes_the_english_fixture() {
        transcribe_fixture("en-hello");
    }

    #[test]
    #[ignore = "requires PLUME_PARAKEET_MODEL pointing to a Parakeet TDT v3 directory"]
    fn downloaded_parakeet_model_transcribes_the_french_fixture() {
        transcribe_fixture("fr-bonjour");
    }

    #[test]
    fn open_reports_missing_files() {
        let dir = std::env::temp_dir().join(format!("plume-parakeet-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("encoder.onnx"), "x").unwrap();
        let err = open(&dir).map(|_| ()).unwrap_err();
        assert!(err.to_string().contains("decoder_joint.onnx"), "got: {err}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
