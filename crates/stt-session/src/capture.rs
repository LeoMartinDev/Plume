use std::sync::{mpsc, Arc, Mutex};

use stt_audio::Mic;
use stt_core::{AudioChunk, AudioStream};

pub struct AudioGate {
    rx: mpsc::Receiver<AudioChunk>,
}

impl AudioGate {
    pub fn into_audio_stream(self) -> AudioStream {
        Box::new(self.rx.into_iter())
    }
}

#[derive(Clone)]
pub struct AudioPump {
    shared: Arc<Mutex<Option<mpsc::Sender<AudioChunk>>>>,
}

impl AudioPump {
    pub(crate) fn send(&self, chunk: AudioChunk) -> bool {
        let tx = self.shared.lock().expect("pump slot").clone();
        match tx {
            Some(tx) => tx.send(chunk).is_ok(),
            None => false,
        }
    }

    pub fn close(self) {
        self.shared.lock().expect("pump slot").take();
    }
}

impl Drop for AudioPump {
    fn drop(&mut self) {
        if let Ok(mut slot) = self.shared.lock() {
            slot.take();
        }
    }
}

pub fn open_gate() -> (AudioGate, AudioPump) {
    let (tx, rx) = mpsc::channel();
    (
        AudioGate { rx },
        AudioPump {
            shared: Arc::new(Mutex::new(Some(tx))),
        },
    )
}

pub struct MicPump {
    mic: Mic,
    pump: AudioPump,
}

impl MicPump {
    pub fn new(mic: Mic, pump: AudioPump) -> Self {
        Self { mic, pump }
    }

    pub fn run(self) {
        for chunk in self.mic {
            if !self.pump.send(chunk) {
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chunk(sample: f32) -> AudioChunk {
        AudioChunk {
            samples: vec![sample],
            sample_rate: 16_000,
        }
    }

    #[test]
    fn closing_the_pump_yields_eof_after_queued_chunks() {
        let (gate, pump) = open_gate();
        let first = chunk(0.1);
        let second = chunk(0.2);
        assert!(pump.send(first.clone()));
        assert!(pump.send(second.clone()));
        pump.close();
        let mut stream = gate.into_audio_stream();
        assert_eq!(stream.next(), Some(first));
        assert_eq!(stream.next(), Some(second));
        assert_eq!(stream.next(), None);
    }

    #[test]
    fn dropping_the_pump_is_eof_and_send_after_close_fails() {
        let (gate, pump) = open_gate();
        let clone = pump.clone();
        drop(pump);
        assert!(!clone.send(chunk(0.5)));
        drop(clone);
        let mut stream = gate.into_audio_stream();
        assert_eq!(stream.next(), None);
    }
}
