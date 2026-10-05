use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

use stt_audio::Mic;
use stt_core::{AudioChunk, AudioStream};

pub(crate) struct AudioGate {
    rx: mpsc::Receiver<AudioChunk>,
}

impl AudioGate {
    pub(crate) fn into_audio_stream(self) -> AudioStream {
        Box::new(self.rx.into_iter())
    }
}

#[derive(Clone)]
pub(crate) struct AudioPump {
    shared: Arc<Mutex<PumpState>>,
}

struct PumpState {
    tx: Option<mpsc::Sender<AudioChunk>>,
    finish_at: Option<Instant>,
}

const RELEASE_TAIL: Duration = Duration::from_millis(120);
const MIC_POLL: Duration = Duration::from_millis(10);

impl AudioPump {
    pub(crate) fn close(self) {
        self.shared.lock().unwrap().tx.take();
    }

    /// Let the capture worker stop the device and flush its queued buffers.
    pub(crate) fn finish(self) {
        self.shared.lock().unwrap().finish_at = Some(Instant::now() + RELEASE_TAIL);
    }

    fn stopping(&self) -> bool {
        let state = self.shared.lock().unwrap();
        state.tx.is_none() || state.finish_at.is_some_and(|at| Instant::now() >= at)
    }

    fn send(&self, chunk: AudioChunk) -> bool {
        let slot = self.shared.lock().unwrap();
        match slot.tx.as_ref() {
            Some(tx) => tx.send(chunk).is_ok(),
            None => false,
        }
    }
}

pub(crate) fn open_gate(
    mic: Mic,
    levels: mpsc::SyncSender<f32>,
) -> (AudioGate, AudioPump, MicPump) {
    let (gate, pump) = open_pair();
    let mic_pump = MicPump {
        mic,
        pump: pump.clone(),
        levels,
    };
    (gate, pump, mic_pump)
}

fn publish(chunk: AudioChunk, pump: &AudioPump, levels: &mpsc::SyncSender<f32>) -> bool {
    let _ = levels.try_send(chunk.voice_level());
    pump.send(chunk)
}

fn open_pair() -> (AudioGate, AudioPump) {
    let (tx, rx) = mpsc::channel();
    let pump = AudioPump {
        shared: Arc::new(Mutex::new(PumpState {
            tx: Some(tx),
            finish_at: None,
        })),
    };
    (AudioGate { rx }, pump)
}

pub(crate) struct MicPump {
    mic: Mic,
    pump: AudioPump,
    levels: mpsc::SyncSender<f32>,
}

impl MicPump {
    pub(crate) fn run(self) {
        let Self { mic, pump, levels } = self;
        run_capture(mic, pump, levels);
    }
}

trait CaptureSource {
    fn recv_timeout(&mut self, timeout: Duration) -> Result<AudioChunk, mpsc::RecvTimeoutError>;
    fn stop(&mut self);
}

impl CaptureSource for Mic {
    fn recv_timeout(&mut self, timeout: Duration) -> Result<AudioChunk, mpsc::RecvTimeoutError> {
        Mic::recv_timeout(self, timeout)
    }

    fn stop(&mut self) {
        Mic::stop(self);
    }
}

fn run_capture(mut mic: impl CaptureSource, pump: AudioPump, levels: mpsc::SyncSender<f32>) {
    while !pump.stopping() {
        match mic.recv_timeout(MIC_POLL) {
            Ok(chunk) => {
                if !publish(chunk, &pump, &levels) {
                    break;
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    mic.stop();
    // The callback queue is separate from the decoder queue. Closing the
    // latter first used to discard audio captured just before key release.
    while let Ok(chunk) = mic.recv_timeout(Duration::ZERO) {
        if !publish(chunk, &pump, &levels) {
            break;
        }
    }
    pump.close();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chunk(first: f32) -> AudioChunk {
        AudioChunk {
            samples: vec![first, first + 1.0],
            sample_rate: 16_000,
        }
    }

    struct BufferedMic {
        rx: mpsc::Receiver<AudioChunk>,
        tx: Option<mpsc::Sender<AudioChunk>>,
    }

    impl CaptureSource for BufferedMic {
        fn recv_timeout(
            &mut self,
            timeout: Duration,
        ) -> Result<AudioChunk, mpsc::RecvTimeoutError> {
            self.rx.recv_timeout(timeout)
        }

        fn stop(&mut self) {
            // Simulate a final device callback during shutdown.
            self.tx.take().unwrap().send(chunk(5.0)).unwrap();
        }
    }

    #[test]
    fn release_flushes_microphone_buffers_before_ending_the_decoder_stream() {
        let (gate, pump) = open_pair();
        let (tx, rx) = mpsc::channel();
        tx.send(chunk(1.0)).unwrap();
        tx.send(chunk(3.0)).unwrap();
        let (levels, _) = mpsc::sync_channel(1);
        pump.shared.lock().unwrap().finish_at = Some(Instant::now());
        run_capture(BufferedMic { rx, tx: Some(tx) }, pump, levels);
        assert_eq!(
            gate.into_audio_stream().collect::<Vec<_>>(),
            vec![chunk(1.0), chunk(3.0), chunk(5.0)]
        );
    }

    #[test]
    fn cancellation_discards_pending_microphone_buffers() {
        let (gate, pump) = open_pair();
        let (tx, rx) = mpsc::channel();
        tx.send(chunk(1.0)).unwrap();
        let (levels, _) = mpsc::sync_channel(1);
        pump.clone().close();
        run_capture(BufferedMic { rx, tx: Some(tx) }, pump, levels);
        assert!(gate.into_audio_stream().next().is_none());
    }

    #[test]
    fn release_keeps_audio_delivered_after_key_up() {
        let (gate, pump) = open_pair();
        let (tx, rx) = mpsc::channel();
        let (levels, _) = mpsc::sync_channel(1);
        pump.clone().finish();
        // The device can still be filling a buffer when key-up is received.
        let delayed_tx = tx.clone();
        let callback = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(20));
            delayed_tx.send(chunk(3.0)).unwrap();
        });
        run_capture(BufferedMic { rx, tx: Some(tx) }, pump, levels);
        callback.join().unwrap();
        assert_eq!(
            gate.into_audio_stream().collect::<Vec<_>>(),
            vec![chunk(3.0), chunk(5.0)]
        );
    }

    #[test]
    fn closing_the_pump_ends_the_gate_after_queued_chunks() {
        let (gate, pump) = open_pair();
        let feeder = pump.clone();
        let handle = std::thread::spawn(move || {
            assert!(feeder.send(chunk(1.0)));
            assert!(feeder.send(chunk(3.0)));
            assert!(feeder.send(chunk(5.0)));
        });
        handle.join().unwrap();
        let stale = pump.clone();
        pump.close();
        assert!(!stale.send(chunk(7.0)));
        let got: Vec<AudioChunk> = gate.into_audio_stream().collect();
        assert_eq!(got, vec![chunk(1.0), chunk(3.0), chunk(5.0)]);
    }

    #[test]
    fn dropping_the_pump_ends_the_gate() {
        let (gate, pump) = open_pair();
        drop(pump);
        assert!(gate.into_audio_stream().next().is_none());
    }

    #[test]
    fn a_chunk_keeps_its_samples_and_reports_its_level() {
        let (gate, pump) = open_pair();
        let (level_tx, level_rx) = mpsc::sync_channel(8);
        let loud = AudioChunk {
            samples: vec![0.0625, 0.0625],
            sample_rate: 16_000,
        };
        let silent = AudioChunk {
            samples: vec![0.0, 0.0],
            sample_rate: 16_000,
        };
        assert!(publish(loud.clone(), &pump, &level_tx));
        assert!(publish(silent.clone(), &pump, &level_tx));
        drop(level_tx);
        pump.close();
        assert_eq!(level_rx.recv().unwrap(), 1.0);
        assert_eq!(level_rx.recv().unwrap(), 0.0);
        let got: Vec<AudioChunk> = gate.into_audio_stream().collect();
        assert_eq!(got, vec![loud, silent]);
    }

    #[test]
    fn a_full_level_queue_still_forwards_the_chunk() {
        let (gate, pump) = open_pair();
        let (level_tx, level_rx) = mpsc::sync_channel(1);
        let loud = AudioChunk {
            samples: vec![0.0625],
            sample_rate: 16_000,
        };
        let quiet = AudioChunk {
            samples: vec![0.0],
            sample_rate: 16_000,
        };
        assert!(publish(loud.clone(), &pump, &level_tx));
        assert!(publish(quiet.clone(), &pump, &level_tx));
        pump.close();
        assert_eq!(level_rx.try_recv().unwrap(), 1.0);
        assert!(level_rx.try_recv().is_err());
        let got: Vec<AudioChunk> = gate.into_audio_stream().collect();
        assert_eq!(got, vec![loud, quiet]);
    }

    #[test]
    fn dropping_a_clone_does_not_end_the_gate() {
        let (gate, pump) = open_pair();
        drop(pump.clone());
        assert!(pump.send(chunk(1.0)));
        pump.close();
        let got: Vec<AudioChunk> = gate.into_audio_stream().collect();
        assert_eq!(got, vec![chunk(1.0)]);
    }
}
