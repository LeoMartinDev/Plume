use std::path::Path;
use std::sync::Arc;

use rustfft::num_complex::Complex;
use rustfft::{Fft, FftPlanner};
use stt_core::{AudioStream, BoxError, Hypothesis, PartialHypothesis, Transcript};

use crate::model::OrtInner;

pub(crate) const CHUNK_SAMPLES: usize = 8960;
pub(crate) const SAMPLE_RATE: u32 = 16_000;
pub(crate) const CHUNK_MEL: usize = 56;
pub(crate) const CACHE_MEL: usize = 9;

const N_FFT: usize = 512;
const HOP: usize = 160;
const WIN: usize = 400;
const N_MELS: usize = 128;
const PREEMPH: f32 = 0.97;
const LOG_GUARD: f32 = 5.960_464_5e-8;
const MAX_SYMBOLS: usize = 10;
fn samples_needed(chunk: usize) -> usize {
    (chunk + 1) * CHUNK_SAMPLES
}

fn chunks_for_samples(n: usize) -> usize {
    n.div_ceil(CHUNK_SAMPLES)
}

pub(crate) fn resample_to_16k(samples: &[f32], from: u32) -> Vec<f32> {
    if from == SAMPLE_RATE || from == 0 || samples.len() < 2 {
        return samples.to_vec();
    }
    let ratio = f64::from(from) / f64::from(SAMPLE_RATE);
    let out_len = ((samples.len() as f64 / ratio).round() as usize).max(1);
    (0..out_len)
        .map(|i| {
            let pos = i as f64 * ratio;
            let j = pos.floor() as usize;
            let frac = (pos - j as f64) as f32;
            let a = samples[j.min(samples.len() - 1)];
            let b = samples[(j + 1).min(samples.len() - 1)];
            a + (b - a) * frac
        })
        .collect()
}

fn is_lang_tag(piece: &str) -> bool {
    let bytes = piece.as_bytes();
    if bytes.len() < 4 || bytes[0] != b'<' || bytes[bytes.len() - 1] != b'>' {
        return false;
    }
    let inner = &bytes[1..bytes.len() - 1];
    match inner.len() {
        2 => inner[0].is_ascii_lowercase() && inner[1].is_ascii_lowercase(),
        5 => {
            inner[0].is_ascii_lowercase()
                && inner[1].is_ascii_lowercase()
                && inner[2] == b'-'
                && inner[3].is_ascii_uppercase()
                && inner[4].is_ascii_uppercase()
        }
        _ => false,
    }
}

pub(crate) struct Vocab {
    pieces: Vec<String>,
    emit: Vec<bool>,
    pub blank: usize,
}

impl Vocab {
    pub fn load(path: &Path, joint_dim: usize) -> Result<Self, BoxError> {
        let text = std::fs::read_to_string(path)?;
        let pieces: Vec<String> = text.lines().map(str::to_string).collect();
        if pieces.len() != joint_dim {
            return Err(format!(
                "vocab has {} pieces but the joint emits {joint_dim}",
                pieces.len()
            )
            .into());
        }
        let Some(blank) = pieces.iter().position(|p| p == "<blank>") else {
            return Err("vocab has no <blank> piece".into());
        };
        let emit = pieces
            .iter()
            .enumerate()
            .map(|(i, p)| i != blank && p != "<unk>" && !is_lang_tag(p))
            .collect();
        Ok(Self {
            pieces,
            emit,
            blank,
        })
    }

    pub fn decode(&self, ids: &[usize]) -> String {
        let mut out = String::new();
        for &id in ids {
            if id < self.pieces.len() && self.emit[id] {
                for c in self.pieces[id].chars() {
                    out.push(if c == '\u{2581}' { ' ' } else { c });
                }
            }
        }
        out.trim_start().to_string()
    }
}

const F_SP: f64 = 200.0 / 3.0;
const MIN_LOG_HZ: f64 = 1000.0;
const MIN_LOG_MEL: f64 = MIN_LOG_HZ / F_SP;
const LOG_STEP: f64 = 0.06875177742094912;

fn hz_to_mel(hz: f64) -> f64 {
    if hz < MIN_LOG_HZ {
        hz / F_SP
    } else {
        MIN_LOG_MEL + (hz / MIN_LOG_HZ).ln() / LOG_STEP
    }
}

fn mel_to_hz(mel: f64) -> f64 {
    if mel < MIN_LOG_MEL {
        mel * F_SP
    } else {
        MIN_LOG_HZ * ((mel - MIN_LOG_MEL) * LOG_STEP).exp()
    }
}

fn mel_basis() -> Vec<f32> {
    let bins = N_FFT / 2 + 1;
    let mut basis = vec![0.0f32; N_MELS * bins];
    let mel_min = hz_to_mel(0.0);
    let mel_max = hz_to_mel(f64::from(SAMPLE_RATE) / 2.0);
    let points: Vec<f64> = (0..=N_MELS + 1)
        .map(|i| mel_to_hz(mel_min + (mel_max - mel_min) * i as f64 / (N_MELS + 1) as f64))
        .collect();
    let freqs: Vec<f64> = (0..bins)
        .map(|i| i as f64 * f64::from(SAMPLE_RATE) / N_FFT as f64)
        .collect();
    let widths: Vec<f64> = points.windows(2).map(|w| w[1] - w[0]).collect();
    for m in 0..N_MELS {
        let enorm = 2.0 / (points[m + 2] - points[m]);
        for (k, &freq) in freqs.iter().enumerate() {
            let lower = (freq - points[m]) / widths[m];
            let upper = (points[m + 2] - freq) / widths[m + 1];
            basis[m * bins + k] = (0.0f64.max(lower.min(upper)) * enorm) as f32;
        }
    }
    basis
}

fn hann_window() -> Vec<f32> {
    (0..WIN)
        .map(|i| 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / (WIN as f32 - 1.0)).cos())
        .collect()
}

pub(crate) struct Mel {
    basis: Vec<f32>,
    window: Vec<f32>,
    fft: Arc<dyn Fft<f32>>,
}

impl Mel {
    pub fn new() -> Self {
        let mut planner = FftPlanner::new();
        Self {
            basis: mel_basis(),
            window: hann_window(),
            fft: planner.plan_fft_forward(N_FFT),
        }
    }

    fn chunk_frames(&self, pcm: &[f32], chunk: usize) -> Vec<f32> {
        let bins = N_FFT / 2 + 1;
        // The 400-sample Hann window is centered inside the 512-point FFT.
        // With the FFT's 256-sample centering pad, its first sample therefore
        // starts 200 samples before the frame center, not 256 samples before it.
        let base = chunk as isize * CHUNK_SAMPLES as isize - (WIN / 2) as isize;
        let at = |i: isize| -> f32 {
            if i < 0 || i >= pcm.len() as isize {
                0.0
            } else {
                pcm[i as usize]
            }
        };
        let mut frames = vec![0.0f32; CHUNK_MEL * N_MELS];
        let mut buf = vec![Complex::new(0.0f32, 0.0); N_FFT];
        let mut scratch = vec![Complex::new(0.0f32, 0.0); self.fft.get_inplace_scratch_len()];
        for r in 0..CHUNK_MEL {
            let start = base + (r * HOP) as isize;
            for (n, slot) in buf.iter_mut().enumerate().take(WIN) {
                let i = start + n as isize;
                let y = if i == 0 {
                    at(0)
                } else {
                    at(i) - PREEMPH * at(i - 1)
                };
                *slot = Complex::new(y * self.window[n], 0.0);
            }
            buf[WIN..].fill(Complex::new(0.0, 0.0));
            self.fft.process_with_scratch(&mut buf, &mut scratch);
            for (m, slot) in frames[r * N_MELS..(r + 1) * N_MELS].iter_mut().enumerate() {
                let row = &self.basis[m * bins..(m + 1) * bins];
                let energy: f32 = row
                    .iter()
                    .zip(buf.iter())
                    .map(|(&w, c)| w * (c.re * c.re + c.im * c.im))
                    .sum();
                *slot = (energy + LOG_GUARD).ln();
            }
        }
        frames
    }
}

type DecoderStep = (Vec<f32>, Vec<f32>, Vec<f32>);

pub(crate) struct NemotronStream {
    inner: Arc<OrtInner>,
    audio: AudioStream,
    pcm: Vec<f32>,
    eof: bool,
    next_chunk: usize,
    mel_tail: Vec<f32>,
    cache_channel: Vec<f32>,
    cache_time: Vec<f32>,
    cache_len: i64,
    h: Vec<f32>,
    c: Vec<f32>,
    last_token: i64,
    tokens: Vec<usize>,
    cumulative: String,
    finished: bool,
    lang_id: i64,
}

impl NemotronStream {
    pub fn new(inner: Arc<OrtInner>, audio: AudioStream, lang_id: i64) -> Self {
        let cache_channel = vec![0.0; inner.cache_channel_len];
        let cache_time = vec![0.0; inner.cache_time_len];
        let lstm = vec![0.0; inner.lstm_len];
        Self {
            last_token: inner.vocab.blank as i64,
            inner,
            audio,
            pcm: Vec::new(),
            eof: false,
            next_chunk: 0,
            // Missing feature frames are log-mel silence. Zero here would mean
            // an energy of 1.0 and corrupt the first encoder chunk.
            mel_tail: vec![LOG_GUARD.ln(); CACHE_MEL * N_MELS],
            cache_channel,
            cache_time,
            cache_len: 0,
            h: lstm.clone(),
            c: lstm,
            tokens: Vec::new(),
            cumulative: String::new(),
            finished: false,
            lang_id,
        }
    }

    fn decode_chunk(&mut self) -> Result<String, BoxError> {
        let frames = self.inner.mel.chunk_frames(&self.pcm, self.next_chunk);
        let mut signal = Vec::with_capacity((CACHE_MEL + CHUNK_MEL) * N_MELS);
        signal.extend_from_slice(&self.mel_tail);
        signal.extend_from_slice(&frames);
        self.mel_tail = frames[frames.len() - CACHE_MEL * N_MELS..].to_vec();

        let inner = self.inner.clone();
        let (encoded, encoded_len) = {
            let mut encoder = inner.encoder.lock().map_err(|_| "encoder lock poisoned")?;
            let outputs = encoder.run(ort::inputs![
                "audio_signal" => ort::value::Tensor::from_array(([1, CACHE_MEL + CHUNK_MEL, N_MELS], signal))?,
                "length" => ort::value::Tensor::from_array(([1], vec![(CACHE_MEL + CHUNK_MEL) as i64]))?,
                "cache_last_channel" => ort::value::Tensor::from_array((inner.cache_channel_shape.clone(), self.cache_channel.clone()))?,
                "cache_last_time" => ort::value::Tensor::from_array((inner.cache_time_shape.clone(), self.cache_time.clone()))?,
                "cache_last_channel_len" => ort::value::Tensor::from_array(([1], vec![self.cache_len]))?,
                "lang_id" => ort::value::Tensor::from_array(([1], vec![self.lang_id]))?
            ])?;
            let (_, encoded) = outputs["outputs"].try_extract_tensor::<f32>()?;
            let (_, lens) = outputs["encoded_lengths"].try_extract_tensor::<i64>()?;
            let (_, next_channel) =
                outputs["cache_last_channel_next"].try_extract_tensor::<f32>()?;
            let (_, next_time) = outputs["cache_last_time_next"].try_extract_tensor::<f32>()?;
            let (_, next_len) =
                outputs["cache_last_channel_len_next"].try_extract_tensor::<i64>()?;
            if encoded.len() != inner.enc_out_frames * inner.enc_hidden
                || lens.len() != 1
                || next_channel.len() != self.cache_channel.len()
                || next_time.len() != self.cache_time.len()
                || next_len.len() != 1
            {
                return Err("encoder returned an unexpected tensor shape".into());
            }
            let encoded_len = (lens[0] as usize).min(inner.enc_out_frames);
            let encoded = encoded.to_vec();
            self.cache_channel = next_channel.to_vec();
            self.cache_time = next_time.to_vec();
            self.cache_len = next_len[0];
            (encoded, encoded_len)
        };

        for t in 0..encoded_len {
            let frame = &encoded[t * inner.enc_hidden..(t + 1) * inner.enc_hidden];
            let mut symbols = 0;
            loop {
                let (dec, h_new, c_new) = self.run_decoder()?;
                let logits = self.run_joint(frame, &dec)?;
                let mut best = 0;
                for (i, &v) in logits.iter().enumerate().skip(1) {
                    if v > logits[best] {
                        best = i;
                    }
                }
                if best == inner.vocab.blank {
                    break;
                }
                self.tokens.push(best);
                self.h = h_new;
                self.c = c_new;
                self.last_token = best as i64;
                symbols += 1;
                if symbols >= MAX_SYMBOLS {
                    break;
                }
            }
        }
        Ok(inner.vocab.decode(&self.tokens))
    }

    fn run_decoder(&self) -> Result<DecoderStep, BoxError> {
        let inner = &self.inner;
        let mut decoder = inner.decoder.lock().map_err(|_| "decoder lock poisoned")?;
        let outputs = decoder.run(ort::inputs![
            "targets" => ort::value::Tensor::from_array(([1, 1], vec![self.last_token]))?,
            "h_in" => ort::value::Tensor::from_array((inner.lstm_shape.clone(), self.h.clone()))?,
            "c_in" => ort::value::Tensor::from_array((inner.lstm_shape.clone(), self.c.clone()))?
        ])?;
        let (_, dec) = outputs["decoder_output"].try_extract_tensor::<f32>()?;
        let (_, h) = outputs["h_out"].try_extract_tensor::<f32>()?;
        let (_, c) = outputs["c_out"].try_extract_tensor::<f32>()?;
        if dec.len() != inner.dec_hidden || h.len() != self.h.len() || c.len() != self.c.len() {
            return Err("decoder returned an unexpected tensor shape".into());
        }
        Ok((dec.to_vec(), h.to_vec(), c.to_vec()))
    }

    fn run_joint(&self, frame: &[f32], dec: &[f32]) -> Result<Vec<f32>, BoxError> {
        let inner = &self.inner;
        let mut joint = inner.joint.lock().map_err(|_| "joint lock poisoned")?;
        let outputs = joint.run(ort::inputs![
            "encoder_output" => ort::value::Tensor::from_array(([1, 1, inner.enc_hidden], frame.to_vec()))?,
            "decoder_output" => ort::value::Tensor::from_array(([1, 1, inner.dec_hidden], dec.to_vec()))?
        ])?;
        let (_, logits) = outputs["joint_output"].try_extract_tensor::<f32>()?;
        if logits.len() != inner.joint_dim {
            return Err("joint returned an unexpected tensor shape".into());
        }
        Ok(logits.to_vec())
    }
}

impl Iterator for NemotronStream {
    type Item = Result<Hypothesis, BoxError>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if self.finished {
                return None;
            }
            while !self.eof && self.pcm.len() < samples_needed(self.next_chunk) {
                match self.audio.next() {
                    Some(chunk) => {
                        if chunk.sample_rate == 0 {
                            self.finished = true;
                            return Some(Err("audio chunk has sample rate 0".into()));
                        }
                        self.pcm
                            .extend(resample_to_16k(&chunk.samples, chunk.sample_rate));
                    }
                    None => self.eof = true,
                }
            }
            if self.eof
                && (self.pcm.is_empty() || self.next_chunk >= chunks_for_samples(self.pcm.len()))
            {
                self.finished = true;
                let text = std::mem::take(&mut self.cumulative);
                eprintln!(
                    "stt-engine: nemotron lang_id={} pcm_samples={} decoded_chunks={} tokens={} final_chars={}",
                    self.lang_id,
                    self.pcm.len(),
                    self.next_chunk,
                    self.tokens.len(),
                    text.chars().count(),
                );
                return Some(Ok(Hypothesis::Final(Transcript { text })));
            }
            match self.decode_chunk() {
                Ok(text) => {
                    self.next_chunk += 1;
                    if text != self.cumulative {
                        self.cumulative = text.clone();
                        return Some(Ok(Hypothesis::Partial(PartialHypothesis { text })));
                    }
                }
                Err(e) => {
                    self.finished = true;
                    return Some(Err(e));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunk_boundaries_cover_whole_utterance_stft() {
        assert_eq!(samples_needed(0), 8960);
        assert_eq!(samples_needed(1), 17920);
        assert_eq!(chunks_for_samples(1), 1);
        assert_eq!(chunks_for_samples(8960), 1);
        assert_eq!(chunks_for_samples(8961), 2);
        assert_eq!(chunks_for_samples(16000), 2);
        assert_eq!(chunks_for_samples(17920), 2);
    }

    #[test]
    fn resample_keeps_16k_and_scales_8k() {
        let input = vec![0.0f32, 1.0, 2.0, 3.0];
        assert_eq!(resample_to_16k(&input, 16_000), input);
        let doubled = resample_to_16k(&input, 8_000);
        assert_eq!(doubled.len(), 8);
        assert!((doubled[1] - 0.5).abs() < 1e-6);
        assert!((doubled[7] - 3.0).abs() < 1e-6);
    }

    #[test]
    fn lang_tags_match_reference_shape() {
        assert!(is_lang_tag("<en>"));
        assert!(is_lang_tag("<fr-FR>"));
        assert!(!is_lang_tag("<blank>"));
        assert!(!is_lang_tag("<unk>"));
        assert!(!is_lang_tag("bonjour"));
    }

    #[test]
    fn silence_produces_log_guard_frames() {
        let mel = Mel::new();
        let frames = mel.chunk_frames(&vec![0.0; 9056], 0);
        assert_eq!(frames.len(), CHUNK_MEL * N_MELS);
        for &v in &frames {
            assert!(
                (v - LOG_GUARD.ln()).abs() < 1e-3,
                "silence must be log guard, got {v}"
            );
        }
    }

    #[test]
    fn sine_concentrates_frame_energy() {
        let pcm: Vec<f32> = (0..9056)
            .map(|i| (2.0 * std::f32::consts::PI * 1000.0 * i as f32 / 16_000.0).sin())
            .collect();
        let mel = Mel::new();
        let frames = mel.chunk_frames(&pcm, 0);
        let frame = &frames[28 * N_MELS..29 * N_MELS];
        let max = frame.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
        let mean = frame.iter().sum::<f32>() / N_MELS as f32;
        assert!(max - mean > 2.0, "1kHz sine must peak above the floor");
    }

    #[test]
    fn vocab_decode_strips_tags_and_marks_word_starts() {
        let dir = std::env::temp_dir().join(format!("stt-engine-vocab-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("vocab.txt");
        std::fs::write(&path, "<unk>\n\u{2581}bonjour\n<fr-FR>\n<blank>\n").unwrap();
        let vocab = Vocab::load(&path, 4).unwrap();
        assert_eq!(vocab.blank, 3);
        assert_eq!(vocab.decode(&[1, 2]), "bonjour");
        assert_eq!(vocab.decode(&[0, 3, 9]), "");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
