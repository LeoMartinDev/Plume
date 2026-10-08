use plume_audio::Mic;
use plume_core::{AudioChunk, AudioStream};
use std::sync::mpsc;
use std::time::{Duration, Instant};
const RELEASE_TAIL: Duration = Duration::from_millis(120);
const MIC_POLL: Duration = Duration::from_millis(10);
pub(crate) enum CaptureCommand {
    Finish,
    Limit,
    Cancel,
}
pub(crate) enum CaptureEvent {
    Ready(Instant),
    Speech(Instant),
    Ended {
        record: Option<Box<crate::Recording>>,
        error: Option<String>,
    },
}

pub(crate) struct RecordedCapture {
    pub commands: mpsc::Sender<CaptureCommand>,
    pub events: mpsc::Receiver<CaptureEvent>,
    pub audio: AudioStream,
}

pub(crate) fn start_recorded(
    writer: crate::recordings::RecordingWriter,
    vad_path: std::path::PathBuf,
    levels: mpsc::SyncSender<f32>,
    cancel: plume_core::CancellationToken,
) -> Result<RecordedCapture, std::io::Error> {
    let (commands, command_rx) = mpsc::channel();
    let (event_tx, events) = mpsc::channel();
    let (audio_tx, audio_rx) = mpsc::channel();
    std::thread::Builder::new()
        .name("plume-recording".into())
        .spawn(move || {
            let mut writer = writer;
            let mut vad = match plume_engine::SpeechDetector::open(&vad_path) {
                Ok(vad) => vad,
                Err(error) => {
                    let _ = event_tx.send(CaptureEvent::Ended {
                        record: None,
                        error: Some(format!("speech detector: {error}")),
                    });
                    return;
                }
            };
            if cancel.is_cancelled() {
                let _ = event_tx.send(CaptureEvent::Ended {
                    record: None,
                    error: None,
                });
                return;
            }
            let mut mic = match Mic::open() {
                Ok(mic) => mic,
                Err(error) => {
                    let _ = event_tx.send(CaptureEvent::Ended {
                        record: None,
                        error: Some(format!("microphone: {error}")),
                    });
                    return;
                }
            };
            let started = Instant::now();
            let _ = event_tx.send(CaptureEvent::Ready(started));
            let mut normalizer = plume_audio::Normalizer::default();
            let mut gate = plume_engine::SpeechGate::default();
            let mut pending = std::collections::VecDeque::<f32>::new();
            let mut finish_at = None;
            let mut error = None;
            let mut received = 0usize;

            {
                let mut publish = |mut chunk: AudioChunk| -> Result<(), plume_core::BoxError> {
                    chunk
                        .samples
                        .truncate((16_000 * 600usize).saturating_sub(received));
                    received += chunk.samples.len();
                    pending.extend(chunk.samples);
                    while pending.len() >= 512 {
                        if cancel.is_cancelled() {
                            return Ok(());
                        }
                        let frame: Vec<_> = pending.drain(..512).collect();
                        let speech = vad.push(&frame)?;
                        writer.write(&frame, speech)?;
                        let chunk = AudioChunk {
                            samples: frame,
                            sample_rate: 16_000,
                        };
                        let _ = levels.try_send(chunk.voice_level());
                        if speech {
                            let _ = event_tx.send(CaptureEvent::Speech(Instant::now()));
                        }
                        if let Some(chunk) = gate.push(chunk, speech) {
                            let _ = audio_tx.send(chunk);
                        }
                    }
                    Ok(())
                };
                loop {
                    if let Some(e) = mic.take_error() {
                        error = Some(format!("microphone: {e}"));
                        break;
                    }
                    if started.elapsed() >= Duration::from_secs(600) {
                        break;
                    }
                    while let Ok(command) = command_rx.try_recv() {
                        match command {
                            CaptureCommand::Cancel => cancel.cancel(),
                            CaptureCommand::Finish => {
                                finish_at = Some(Instant::now() + RELEASE_TAIL)
                            }
                            CaptureCommand::Limit => finish_at = Some(Instant::now()),
                        }
                    }
                    if cancel.is_cancelled() || finish_at.is_some_and(|at| Instant::now() >= at) {
                        break;
                    }
                    match mic.recv_timeout(MIC_POLL) {
                        Ok(chunk) => match normalizer.push(chunk).and_then(&mut publish) {
                            Ok(()) => {}
                            Err(e) => {
                                error = Some(format!("audio capture: {e}"));
                                break;
                            }
                        },
                        Err(mpsc::RecvTimeoutError::Timeout) => {}
                        Err(mpsc::RecvTimeoutError::Disconnected) => {
                            error = Some("microphone disconnected".into());
                            break;
                        }
                    }
                }
                mic.stop();
                if error.is_none() {
                    error = mic.take_error().map(|e| format!("microphone: {e}"));
                }
                if !cancel.is_cancelled() && error.is_none() {
                    while let Ok(chunk) = mic.recv_timeout(Duration::ZERO) {
                        if let Err(e) = normalizer.push(chunk).and_then(&mut publish) {
                            error = Some(e.to_string());
                            break;
                        }
                    }
                    if let Err(e) = publish(normalizer.finish()) {
                        error = Some(e.to_string());
                    }
                }
            }
            if !cancel.is_cancelled() && error.is_none() && !pending.is_empty() {
                let frame: Vec<_> = pending.drain(..).collect();
                let speech = vad.push(&frame).and_then(|_| vad.finish());
                match speech {
                    Ok(speech) => {
                        if let Err(e) = writer.write(&frame, speech) {
                            error = Some(format!("audio storage: {e}"));
                        }
                        if let Some(chunk) = gate.push(
                            AudioChunk {
                                samples: frame,
                                sample_rate: 16_000,
                            },
                            speech,
                        ) {
                            let _ = audio_tx.send(chunk);
                        }
                    }
                    Err(e) => error = Some(format!("speech detector: {e}")),
                }
            }
            drop(audio_tx);
            let record = if cancel.is_cancelled() {
                drop(writer);
                None
            } else {
                match writer.finish() {
                    Ok(r) => Some(Box::new(r)),
                    Err(e) => {
                        error = Some(format!("audio storage: {e}"));
                        None
                    }
                }
            };
            let _ = event_tx.send(CaptureEvent::Ended { record, error });
        })?;
    Ok(RecordedCapture {
        commands,
        events,
        audio: Box::new(audio_rx.into_iter()),
    })
}
