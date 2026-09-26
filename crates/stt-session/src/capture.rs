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
        shared: Arc::new(Mutex::new(Some(tx))),
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
        for chunk in mic {
            if !publish(chunk, &pump, &levels) {
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
