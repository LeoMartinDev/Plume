use std::error::Error;
use std::fmt;
use std::sync::mpsc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

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
            .spawn(move || match bind_stream(chunk_tx, ready_tx.clone()) {
                Ok(stream) => {
                    let _ = shutdown_rx.recv();
                    let _ = stream.pause();
                }
                Err(err) => {
                    let _ = ready_tx.send(Err(err));
                }
            })
            .map_err(|err| CaptureError::Backend(err.to_string()))?;
        // play() only schedules capture on WASAPI. Do not report readiness
        // until the device has actually delivered its first audio buffer.
        match ready_rx.recv_timeout(Duration::from_secs(3)) {
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
                drop(shutdown_tx);
                let _ = thread.join();
                Err(CaptureError::Backend(
                    "microphone did not deliver audio within 3 seconds".into(),
                ))
            }
        }
    }

    pub fn into_stream(self) -> AudioStream {
        Box::new(self)
    }

    pub fn recv_timeout(&self, timeout: Duration) -> Result<AudioChunk, mpsc::RecvTimeoutError> {
        self.rx.recv_timeout(timeout)
    }

    /// Stop callbacks before draining the buffers already captured by the device.
    pub fn stop(&mut self) {
        self.shutdown.take();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
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
        self.stop();
    }
}

fn bind_stream(
    tx: mpsc::Sender<AudioChunk>,
    ready: mpsc::Sender<Result<(), CaptureError>>,
) -> Result<cpal::Stream, CaptureError> {
    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or(CaptureError::NoInputDevice)?;
    let (config, sample_format) = preferred_or_default(&device)?;
    tracing::debug!(
        "stt-audio: device={:?} sample_rate={} channels={} format={sample_format:?}",
        device.name().unwrap_or_else(|_| "unknown".into()),
        config.sample_rate.0,
        config.channels,
    );
    let stream = build_stream(&device, &config, sample_format, tx, ready)?;
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
    ready: mpsc::Sender<Result<(), CaptureError>>,
) -> Result<cpal::Stream, CaptureError> {
    let sample_rate = config.sample_rate.0;
    let channels = config.channels;
    let mut ready = Some(ready);
    match sample_format {
        SampleFormat::F32 => device
            .build_input_stream(
                config,
                move |data: &[f32], _| {
                    send_buffer(
                        &tx,
                        &mut ready,
                        InputBuffer::F32(data),
                        sample_rate,
                        channels,
                    );
                },
                ignore_stream_error,
                None,
            )
            .map_err(|err| CaptureError::Backend(err.to_string())),
        SampleFormat::I16 => device
            .build_input_stream(
                config,
                move |data: &[i16], _| {
                    send_buffer(
                        &tx,
                        &mut ready,
                        InputBuffer::I16(data),
                        sample_rate,
                        channels,
                    );
                },
                ignore_stream_error,
                None,
            )
            .map_err(|err| CaptureError::Backend(err.to_string())),
        SampleFormat::U16 => device
            .build_input_stream(
                config,
                move |data: &[u16], _| {
                    send_buffer(
                        &tx,
                        &mut ready,
                        InputBuffer::U16(data),
                        sample_rate,
                        channels,
                    );
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
    ready: &mut Option<mpsc::Sender<Result<(), CaptureError>>>,
    buffer: InputBuffer<'_>,
    sample_rate: u32,
    channels: u16,
) {
    let chunk = to_audio_chunk(buffer, sample_rate, channels);
    if chunk.samples.is_empty() {
        return;
    }
    if tx.send(chunk).is_ok() {
        if let Some(ready) = ready.take() {
            let _ = ready.send(Ok(()));
        }
    }
}

fn ignore_stream_error(err: cpal::StreamError) {
    tracing::warn!("stt-audio: input stream error: {err}");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn microphone_is_ready_only_after_a_nonempty_buffer_is_queued() {
        let (tx, rx) = mpsc::channel();
        let (ready_tx, ready_rx) = mpsc::channel();
        let mut ready = Some(ready_tx);
        send_buffer(&tx, &mut ready, InputBuffer::F32(&[]), 16_000, 1);
        assert!(ready_rx.try_recv().is_err());
        assert!(rx.try_recv().is_err());
        send_buffer(&tx, &mut ready, InputBuffer::F32(&[0.25]), 16_000, 1);
        assert!(ready_rx.try_recv().unwrap().is_ok());
        assert_eq!(rx.try_recv().unwrap().samples, vec![0.25]);
        assert!(ready.is_none());
    }

    #[test]
    fn default_input_name_is_graceful_without_a_device() {
        match default_input_name() {
            Ok(name) => assert_ne!(name, ""),
            Err(CaptureError::NoInputDevice) => {}
            Err(err) => panic!("{err}"),
        }
    }
}
