use std::sync::{mpsc, Arc, Mutex};

use stt_audio::Mic;
use stt_core::{AudioChunk, AudioStream};

/// The engine's audio input for one session. Wraps the receiver side of a
/// closeable channel as the `AudioStream` the engine expects. Yields
/// `None` exactly when the pump closes: EOF the engine turns into `Final`.
pub(crate) struct AudioGate {
    rx: mpsc::Receiver<AudioChunk>,
}

impl AudioGate {
    pub(crate) fn into_audio_stream(self) -> AudioStream {
        Box::new(self.rx.into_iter())
    }
}

/// The mic side of the gate. One sender is shared between the compositor's
/// handle and the pump thread's clone, so `close` (or `Drop`) from either
/// side ends the audio iterator. Idempotent by construction: send-after-close
/// just fails and the pump exits.
#[derive(Clone)]
pub(crate) struct AudioPump {
    shared: Arc<Mutex<Option<mpsc::Sender<AudioChunk>>>>,
}

impl AudioPump {
    pub(crate) fn close(self) {
        self.shared.lock().unwrap().take();
    }

    fn send(&self, chunk: AudioChunk) -> bool {
        let slot = self.shared.lock().unwrap();
        match slot.as_ref() {
            Some(tx) => tx.send(chunk).is_ok(),
            None => false,
        }
    }
}

impl Drop for AudioPump {
    fn drop(&mut self) {
        if let Ok(mut slot) = self.shared.lock() {
            slot.take();
        }
    }
}

/// Open both halves. The `Mic` moves into the pump thread (it is a
/// blocking `recv` iterator; it can never live on the polling thread).
pub(crate) fn open_gate(mic: Mic) -> (AudioGate, AudioPump, MicPump) {
    let (gate, pump) = open_pair();
    let mic_pump = MicPump {
        mic,
        pump: pump.clone(),
    };
    (gate, pump, mic_pump)
}

fn open_pair() -> (AudioGate, AudioPump) {
    let (tx, rx) = mpsc::channel();
    let pump = AudioPump {
        shared: Arc::new(Mutex::new(Some(tx))),
    };
    (AudioGate { rx }, pump)
}

/// Owns the mic until the scope spawns it. Exists so the pump thread's
/// closure is one obvious line, not an inline future-leak.
pub(crate) struct MicPump {
    mic: Mic,
    pump: AudioPump,
}

impl MicPump {
    /// Pump body: forward until the mic ends or the gate closes.
    /// Runs on its own thread inside the session scope.
    pub(crate) fn run(self) {
        let Self { mic, pump } = self;
        for chunk in mic {
            if !pump.send(chunk) {
                break;
            }
        }
    }
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
}
