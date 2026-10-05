use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Instant;
use stt_core::{AsrEngine, Hypothesis};

pub(crate) const DECODE_QUEUE_CAPACITY: usize = 8;

pub(crate) struct Decoder<E> {
    jobs: mpsc::SyncSender<DecodeJob<E>>,
    pub(crate) completions: mpsc::Receiver<Completion>,
}

impl<E: AsrEngine + Send + 'static> Decoder<E> {
    pub(crate) fn start() -> std::io::Result<Self> {
        let (jobs, job_rx) = mpsc::sync_channel(DECODE_QUEUE_CAPACITY);
        let (completion_tx, completions) = mpsc::channel();
        std::thread::Builder::new()
            .name("stt-decoder".into())
            .spawn(move || run_decoder(job_rx, completion_tx))?;
        Ok(Self { jobs, completions })
    }
}

impl<E> Decoder<E> {
    pub(crate) fn submit(&self, job: DecodeJob<E>) -> Result<(), String> {
        self.jobs.try_send(job).map_err(|error| match error {
            mpsc::TrySendError::Full(_) => "decode queue is full".into(),
            mpsc::TrySendError::Disconnected(_) => "decode worker is unavailable".into(),
        })
    }
}

#[derive(Debug)]
pub(crate) struct Completion {
    pub(crate) id: u64,
    pub(crate) joins_previous: bool,
    pub(crate) result: Result<String, String>,
}

pub(crate) struct DecodeJob<E> {
    pub(crate) id: u64,
    pub(crate) joins_previous: bool,
    pub(crate) engine: E,
    pub(crate) audio: stt_core::AudioStream,
    pub(crate) cancelled: Arc<AtomicBool>,
}

pub(crate) fn run_decoder<E: AsrEngine>(
    jobs: mpsc::Receiver<DecodeJob<E>>,
    completions: mpsc::Sender<Completion>,
) {
    for job in jobs {
        let started = Instant::now();
        let stats = Arc::new(Mutex::new(AudioStats::default()));
        let observed = stats.clone();
        let audio = Box::new(job.audio.inspect(move |chunk| {
            observed.lock().unwrap().observe(chunk);
        }));
        eprintln!("stt-session: capture={} decoder_started", job.id);
        let mut result = decode_final(&job.engine, audio);
        let cancelled = job.cancelled.load(Ordering::Acquire);
        if cancelled {
            result = Ok(String::new());
        }
        let stats = stats.lock().unwrap();
        let rms = (stats.sum_squares / stats.samples.max(1) as f64).sqrt();
        eprintln!(
            "stt-session: capture={} audio_chunks={} samples={} audio_ms={:.0} rms={rms:.6} peak={:.6} nonfinite={} worker_ms={} cancelled={cancelled} final_chars={} error={:?}",
            job.id,
            stats.chunks,
            stats.samples,
            stats.seconds * 1000.0,
            stats.peak,
            stats.nonfinite,
            started.elapsed().as_millis(),
            result.as_ref().map_or(0, |text| text.chars().count()),
            result.as_ref().err(),
        );
        if completions
            .send(Completion {
                id: job.id,
                joins_previous: job.joins_previous,
                result,
            })
            .is_err()
        {
            return;
        }
    }
}

#[derive(Default)]
struct AudioStats {
    chunks: usize,
    samples: usize,
    seconds: f64,
    sum_squares: f64,
    peak: f32,
    nonfinite: usize,
}

impl AudioStats {
    fn observe(&mut self, chunk: &stt_core::AudioChunk) {
        self.chunks += 1;
        self.samples += chunk.samples.len();
        if chunk.sample_rate > 0 {
            self.seconds += chunk.samples.len() as f64 / f64::from(chunk.sample_rate);
        }
        for &sample in &chunk.samples {
            if sample.is_finite() {
                self.sum_squares += f64::from(sample).powi(2);
                self.peak = self.peak.max(sample.abs());
            } else {
                self.nonfinite += 1;
            }
        }
    }
}

fn decode_final<E: AsrEngine>(engine: &E, audio: stt_core::AudioStream) -> Result<String, String> {
    for item in engine.stream(audio) {
        match item.map_err(|err| err.to_string())? {
            Hypothesis::Final(transcript) => return Ok(transcript.text),
            Hypothesis::Partial(_) => {}
        }
    }
    Err("decode thread ended without a final transcript".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use stt_core::{AudioStream, BoxError, HypothesisStream, PartialHypothesis, Transcript};

    struct ScriptedEngine(Vec<Result<Hypothesis, &'static str>>);
    impl AsrEngine for ScriptedEngine {
        fn stream(&self, _: AudioStream) -> HypothesisStream {
            Box::new(
                self.0
                    .clone()
                    .into_iter()
                    .map(|item| item.map_err(|error| -> BoxError { error.into() })),
            )
        }
    }
    fn final_text(text: &str) -> Result<Hypothesis, &'static str> {
        Ok(Hypothesis::Final(Transcript { text: text.into() }))
    }
    fn job(id: u64, engine: ScriptedEngine, cancelled: bool) -> DecodeJob<ScriptedEngine> {
        DecodeJob {
            id,
            joins_previous: id > 0,
            engine,
            audio: Box::new(std::iter::empty()),
            cancelled: Arc::new(AtomicBool::new(cancelled)),
        }
    }

    #[test]
    fn partials_are_ignored_and_only_the_first_final_is_returned() {
        let engine = ScriptedEngine(vec![
            Ok(Hypothesis::Partial(PartialHypothesis {
                text: "bonj".into(),
            })),
            final_text("Bonjour."),
            final_text("duplicate"),
        ]);
        assert_eq!(
            decode_final(&engine, Box::new(std::iter::empty())).unwrap(),
            "Bonjour."
        );
    }
    #[test]
    fn empty_final_errors_and_missing_final_are_distinguished() {
        assert_eq!(
            decode_final(
                &ScriptedEngine(vec![final_text("")]),
                Box::new(std::iter::empty())
            )
            .unwrap(),
            ""
        );
        assert_eq!(
            decode_final(
                &ScriptedEngine(vec![Err("engine failed")]),
                Box::new(std::iter::empty())
            )
            .unwrap_err(),
            "engine failed"
        );
        assert!(
            decode_final(&ScriptedEngine(vec![]), Box::new(std::iter::empty()))
                .unwrap_err()
                .contains("without a final")
        );
    }
    #[test]
    fn worker_preserves_order_and_swallows_cancelled_results() {
        let (tx, rx) = mpsc::sync_channel(3);
        let (out_tx, out_rx) = mpsc::channel();
        for (id, text, cancelled) in [
            (0, "first", false),
            (1, "cancelled", true),
            (2, "last", false),
        ] {
            tx.send(job(id, ScriptedEngine(vec![final_text(text)]), cancelled))
                .ok()
                .unwrap();
        }
        drop(tx);
        run_decoder(rx, out_tx);
        let values: Vec<_> = out_rx
            .into_iter()
            .map(|completion| (completion.id, completion.result.unwrap()))
            .collect();
        assert_eq!(
            values,
            vec![(0, "first".into()), (1, "".into()), (2, "last".into())]
        );
    }
    #[test]
    fn cancellation_during_decode_swallows_the_final() {
        struct CancellingEngine(Arc<AtomicBool>);
        impl AsrEngine for CancellingEngine {
            fn stream(&self, _: AudioStream) -> HypothesisStream {
                self.0.store(true, Ordering::Release);
                Box::new(std::iter::once(Ok(Hypothesis::Final(Transcript {
                    text: "late".into(),
                }))))
            }
        }
        let cancelled = Arc::new(AtomicBool::new(false));
        let (tx, rx) = mpsc::sync_channel(1);
        let (out_tx, out_rx) = mpsc::channel();
        tx.send(DecodeJob {
            id: 0,
            joins_previous: false,
            engine: CancellingEngine(cancelled.clone()),
            audio: Box::new(std::iter::empty()),
            cancelled,
        })
        .ok()
        .unwrap();
        drop(tx);
        run_decoder(rx, out_tx);
        assert_eq!(out_rx.recv().unwrap().result.unwrap(), "");
    }
    #[test]
    fn bounded_submission_reports_full_and_disconnected_queues() {
        let (jobs, rx) = mpsc::sync_channel(DECODE_QUEUE_CAPACITY);
        let (_, completions) = mpsc::channel();
        let decoder = Decoder { jobs, completions };
        for id in 0..DECODE_QUEUE_CAPACITY {
            decoder
                .submit(job(id as u64, ScriptedEngine(vec![]), false))
                .unwrap();
        }
        assert_eq!(
            decoder
                .submit(job(8, ScriptedEngine(vec![]), false))
                .unwrap_err(),
            "decode queue is full"
        );
        drop(rx);
        assert_eq!(
            decoder
                .submit(job(8, ScriptedEngine(vec![]), false))
                .unwrap_err(),
            "decode worker is unavailable"
        );
    }
}
