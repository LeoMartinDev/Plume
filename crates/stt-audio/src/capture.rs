use std::error::Error;
use std::fmt;
use std::sync::mpsc;
use std::thread::{self, JoinHandle};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, SampleRate, StreamConfig};
use stt_core::{AudioChunk, AudioStream};

use crate::convert::{to_audio_chunk, InputBuffer};

#[derive(Debug)]
pub enum CaptureError {
    NoInputDevice,
    Backend(String),
    UnsupportedSampleFormat(String),
}

impl fmt::Display for CaptureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoInputDevice => f.write_str("no default input device"),
            Self::Backend(message) => f.write_str(message),
            Self::UnsupportedSampleFormat(format) => {
                write!(f, "unsupported sample format {format}")
            }
        }
    }
}

impl Error for CaptureError {}

pub fn default_input_name() -> Result<String, CaptureError> {
    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or(CaptureError::NoInputDevice)?;
    device.name().map_err(|_| CaptureError::NoInputDevice)
}

pub struct Mic {
    rx: mpsc::Receiver<AudioChunk>,
    shutdown: Option<mpsc::Sender<()>>,
    thread: Option<JoinHandle<()>>,
}

impl Mic {
    pub fn open() -> Result<Self, CaptureError> {
        let (ready_tx, ready_rx) = mpsc::channel();
        let (chunk_tx, chunk_rx) = mpsc::channel();
        let (shutdown_tx, shutdown_rx) = mpsc::channel();
        // cpal::Stream is !Send on every host, so the handle stays on this thread.
        let thread = thread::Builder::new()
            .name("stt-audio-mic".into())
            .spawn(move || match bind_stream(chunk_tx) {
                Ok(stream) => {
                    let _ = ready_tx.send(Ok(()));
                    let _ = shutdown_rx.recv();
                    let _ = stream.pause();
                }
                Err(err) => {
                    let _ = ready_tx.send(Err(err));
                }
            })
            .map_err(|err| CaptureError::Backend(err.to_string()))?;
        match ready_rx.recv() {
            Ok(Ok(())) => Ok(Self {
                rx: chunk_rx,
                shutdown: Some(shutdown_tx),
                thread: Some(thread),
            }),
            Ok(Err(err)) => {
                let _ = thread.join();
                Err(err)
            }
            Err(_) => {
                let _ = thread.join();
                Err(CaptureError::Backend(
                    "mic thread exited before the stream was ready".into(),
                ))
            }
        }
    }

    pub fn into_stream(self) -> AudioStream {
        Box::new(self)
    }
}

impl Iterator for Mic {
    type Item = AudioChunk;

    fn next(&mut self) -> Option<Self::Item> {
        self.rx.recv().ok()
    }
}

impl Drop for Mic {
    fn drop(&mut self) {
        self.shutdown.take();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn bind_stream(tx: mpsc::Sender<AudioChunk>) -> Result<cpal::Stream, CaptureError> {
    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or(CaptureError::NoInputDevice)?;
    let (config, sample_format) = preferred_or_default(&device)?;
    let stream = build_stream(&device, &config, sample_format, tx)?;
    stream
        .play()
        .map_err(|err| CaptureError::Backend(err.to_string()))?;
    Ok(stream)
}

fn preferred_or_default(
    device: &cpal::Device,
) -> Result<(StreamConfig, SampleFormat), CaptureError> {
    if let Ok(ranges) = device.supported_input_configs() {
        let preferred = ranges
            .filter(|range| range.channels() == 1 && range.sample_format() == SampleFormat::F32)
            .find_map(|range| range.try_with_sample_rate(SampleRate(16_000)));
        if let Some(supported) = preferred {
            return Ok((supported.config(), supported.sample_format()));
        }
    }
    let supported = device
        .default_input_config()
        .map_err(|err| CaptureError::Backend(err.to_string()))?;
    Ok((supported.config(), supported.sample_format()))
}

fn build_stream(
    device: &cpal::Device,
    config: &StreamConfig,
    sample_format: SampleFormat,
    tx: mpsc::Sender<AudioChunk>,
) -> Result<cpal::Stream, CaptureError> {
    let sample_rate = config.sample_rate.0;
    let channels = config.channels;
    match sample_format {
        SampleFormat::F32 => device
            .build_input_stream(
                config,
                move |data: &[f32], _| {
                    send_buffer(&tx, InputBuffer::F32(data), sample_rate, channels);
                },
                ignore_stream_error,
                None,
            )
            .map_err(|err| CaptureError::Backend(err.to_string())),
        SampleFormat::I16 => device
            .build_input_stream(
                config,
                move |data: &[i16], _| {
                    send_buffer(&tx, InputBuffer::I16(data), sample_rate, channels);
                },
                ignore_stream_error,
                None,
            )
            .map_err(|err| CaptureError::Backend(err.to_string())),
        SampleFormat::U16 => device
            .build_input_stream(
                config,
                move |data: &[u16], _| {
                    send_buffer(&tx, InputBuffer::U16(data), sample_rate, channels);
                },
                ignore_stream_error,
                None,
            )
            .map_err(|err| CaptureError::Backend(err.to_string())),
        other => Err(CaptureError::UnsupportedSampleFormat(format!("{other:?}"))),
    }
}

fn send_buffer(
    tx: &mpsc::Sender<AudioChunk>,
    buffer: InputBuffer<'_>,
    sample_rate: u32,
    channels: u16,
) {
    let chunk = to_audio_chunk(buffer, sample_rate, channels);
    if chunk.samples.is_empty() {
        return;
    }
    let _ = tx.send(chunk);
}

fn ignore_stream_error(_err: cpal::StreamError) {}

#[cfg(test)]
mod tests {
    use super::{default_input_name, CaptureError};

    #[test]
    fn default_input_name_is_graceful_without_a_device() {
        match default_input_name() {
            Ok(name) => assert_ne!(name, ""),
            Err(CaptureError::NoInputDevice) => {}
            Err(err) => panic!("{err}"),
        }
    }
}
