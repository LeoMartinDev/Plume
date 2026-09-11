use std::sync::{mpsc, Arc, Mutex};

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

pub(crate) struct MicPump {
    mic: Mic,
    pump: AudioPump,
}

impl MicPump {
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
