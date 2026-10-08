use std::error::Error;
use std::fmt;
use std::sync::{mpsc, Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, SampleRate, StreamConfig};
use plume_core::{AudioChunk, AudioStream};

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

enum RawSamples {
    F32(Vec<f32>),
    I16(Vec<i16>),
    U16(Vec<u16>),
}
struct RawBuffer {
    samples: RawSamples,
    rate: u32,
    channels: u16,
}
impl RawBuffer {
    fn mono(self) -> AudioChunk {
        let buffer = match &self.samples {
            RawSamples::F32(v) => InputBuffer::F32(v),
            RawSamples::I16(v) => InputBuffer::I16(v),
            RawSamples::U16(v) => InputBuffer::U16(v),
        };
        to_audio_chunk(buffer, self.rate, self.channels)
    }
}
pub struct Mic {
    rx: mpsc::Receiver<RawBuffer>,
    error: Arc<Mutex<Option<String>>>,
    shutdown: Option<mpsc::Sender<()>>,
    thread: Option<JoinHandle<()>>,
}

impl Mic {
    pub fn open() -> Result<Self, CaptureError> {
        let (ready_tx, ready_rx) = mpsc::channel();
        let (chunk_tx, chunk_rx) = mpsc::sync_channel(256);
        let error = Arc::new(Mutex::new(None));
        let stream_error = error.clone();
        let (shutdown_tx, shutdown_rx) = mpsc::channel();
        // cpal::Stream is !Send on every host, so the handle stays on this thread.
        let thread = thread::Builder::new()
            .name("plume-audio-mic".into())
            .spawn(
                move || match bind_stream(chunk_tx, ready_tx.clone(), stream_error) {
                    Ok(stream) => {
                        let _ = shutdown_rx.recv();
                        let _ = stream.pause();
                    }
                    Err(err) => {
                        let _ = ready_tx.send(Err(err));
                    }
                },
            )
            .map_err(|err| CaptureError::Backend(err.to_string()))?;
        // play() only schedules capture on WASAPI. Do not report readiness
        // until the device has actually delivered its first audio buffer.
        match ready_rx.recv_timeout(Duration::from_secs(3)) {
            Ok(Ok(())) => Ok(Self {
                rx: chunk_rx,
                error,
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
        self.rx.recv_timeout(timeout).map(RawBuffer::mono)
    }

    pub fn take_error(&self) -> Option<String> {
        self.error.lock().unwrap().take()
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
        self.rx.recv().ok().map(RawBuffer::mono)
    }
}

impl Drop for Mic {
    fn drop(&mut self) {
        self.stop();
    }
}

fn bind_stream(
    tx: mpsc::SyncSender<RawBuffer>,
    ready: mpsc::Sender<Result<(), CaptureError>>,
    error: Arc<Mutex<Option<String>>>,
) -> Result<cpal::Stream, CaptureError> {
    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or(CaptureError::NoInputDevice)?;
    let (config, sample_format) = preferred_or_default(&device)?;
    tracing::debug!(state = "opening", "microphone requested");
    let stream = build_stream(&device, &config, sample_format, tx, ready, error)?;
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
    tx: mpsc::SyncSender<RawBuffer>,
    ready: mpsc::Sender<Result<(), CaptureError>>,
    error: Arc<Mutex<Option<String>>>,
) -> Result<cpal::Stream, CaptureError> {
    let sample_rate = config.sample_rate.0;
    let channels = config.channels;
    let mut ready = Some(ready);
    let overflow = error.clone();
    let report_error = move |err: cpal::StreamError| {
        *error.lock().unwrap() = Some(err.to_string());
    };
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
                        &overflow,
                    );
                },
                report_error,
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
                        &overflow,
                    );
                },
                report_error,
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
                        &overflow,
                    );
                },
                report_error,
                None,
            )
            .map_err(|err| CaptureError::Backend(err.to_string())),
        other => Err(CaptureError::UnsupportedSampleFormat(format!("{other:?}"))),
    }
}

fn send_buffer(
    tx: &mpsc::SyncSender<RawBuffer>,
    ready: &mut Option<mpsc::Sender<Result<(), CaptureError>>>,
    buffer: InputBuffer<'_>,
    sample_rate: u32,
    channels: u16,
    error: &Arc<Mutex<Option<String>>>,
) {
    let samples = match buffer {
        InputBuffer::F32(v) if !v.is_empty() => RawSamples::F32(v.to_vec()),
        InputBuffer::I16(v) if !v.is_empty() => RawSamples::I16(v.to_vec()),
        InputBuffer::U16(v) if !v.is_empty() => RawSamples::U16(v.to_vec()),
        _ => return,
    };
    match tx.try_send(RawBuffer {
        samples,
        rate: sample_rate,
        channels,
    }) {
        Ok(()) => {
            if let Some(ready) = ready.take() {
                let _ = ready.send(Ok(()));
            }
        }
        Err(mpsc::TrySendError::Full(_)) => {
            *error.lock().unwrap() = Some("microphone buffer overflow".into())
        }
        Err(mpsc::TrySendError::Disconnected(_)) => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn microphone_is_ready_only_after_a_nonempty_buffer_is_queued() {
        let (tx, rx) = mpsc::sync_channel(8);
        let (ready_tx, ready_rx) = mpsc::channel();
        let mut ready = Some(ready_tx);
        let errors = Arc::new(Mutex::new(None));
        send_buffer(&tx, &mut ready, InputBuffer::F32(&[]), 16_000, 1, &errors);
        assert!(ready_rx.try_recv().is_err());
        assert!(rx.try_recv().is_err());
        send_buffer(
            &tx,
            &mut ready,
            InputBuffer::F32(&[0.25]),
            16_000,
            1,
            &errors,
        );
        assert!(ready_rx.try_recv().unwrap().is_ok());
        assert_eq!(rx.try_recv().unwrap().mono().samples, vec![0.25]);
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
