use plume_core::{AsrEngine, AudioStream, CancellationToken, Hypothesis};
use std::sync::mpsc;

pub(crate) struct Decoder<E> {
    jobs: mpsc::SyncSender<DecodeJob<E>>,
    pub completions: mpsc::Receiver<Completion>,
}
pub(crate) struct DecodeJob<E> {
    pub id: u64,
    pub engine: E,
    pub audio: AudioStream,
    pub cancelled: CancellationToken,
}
#[derive(Debug)]
pub(crate) enum DecodeOutcome {
    Final(String),
    Failed(String),
    Cancelled,
}
#[derive(Debug)]
pub(crate) struct Completion {
    pub id: u64,
    pub outcome: DecodeOutcome,
}
impl<E: AsrEngine + Send + 'static> Decoder<E> {
    pub fn start() -> std::io::Result<Self> {
        let (jobs, rx) = mpsc::sync_channel(1);
        let (tx, completions) = mpsc::channel();
        std::thread::Builder::new()
            .name("plume-decoder".into())
            .spawn(move || {
                for job in rx {
                    let job: DecodeJob<E> = job;
                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        decode_final(&job.engine, job.audio, job.cancelled.clone())
                    }))
                    .unwrap_or_else(|_| Err("transcription worker failed".into()));
                    let outcome = if job.cancelled.is_cancelled() {
                        DecodeOutcome::Cancelled
                    } else {
                        match result {
                            Ok(t) => DecodeOutcome::Final(t),
                            Err(e) => DecodeOutcome::Failed(e),
                        }
                    };
                    if tx
                        .send(Completion {
                            id: job.id,
                            outcome,
                        })
                        .is_err()
                    {
                        break;
                    }
                }
            })?;
        Ok(Self { jobs, completions })
    }
}
impl<E> Decoder<E> {
    pub fn submit(&self, job: DecodeJob<E>) -> Result<(), String> {
        self.jobs
            .try_send(job)
            .map_err(|_| "transcription worker unavailable or busy".into())
    }
}
fn decode_final<E: AsrEngine>(
    engine: &E,
    audio: AudioStream,
    cancel: CancellationToken,
) -> Result<String, String> {
    for item in engine.stream_with_control(audio, cancel.clone()) {
        if cancel.is_cancelled() {
            return Err("cancelled".into());
        }
        if let Hypothesis::Final(t) = item.map_err(|e| e.to_string())? {
            return Ok(t.text);
        }
    }
    Err("transcription ended without a final result".into())
}
#[cfg(test)]
mod tests {
    use super::*;
    struct Script;
    impl AsrEngine for Script {
        fn stream(&self, _: AudioStream) -> plume_core::HypothesisStream {
            Box::new(
                [
                    Ok(Hypothesis::Partial(plume_core::PartialHypothesis {
                        text: "partial".into(),
                    })),
                    Ok(Hypothesis::Final(plume_core::Transcript {
                        text: "final".into(),
                    })),
                ]
                .into_iter(),
            )
        }
    }
    #[test]
    fn only_final_is_delivered() {
        assert_eq!(
            decode_final(
                &Script,
                Box::new(std::iter::empty()),
                CancellationToken::default()
            )
            .unwrap(),
            "final"
        );
    }
    #[test]
    fn cancelled_job_cannot_deliver_text() {
        let worker = Decoder::start().unwrap();
        let cancel = CancellationToken::default();
        cancel.cancel();
        worker
            .submit(DecodeJob {
                id: 1,
                engine: Script,
                audio: Box::new(std::iter::empty()),
                cancelled: cancel,
            })
            .unwrap();
        assert!(matches!(
            worker.completions.recv().unwrap().outcome,
            DecodeOutcome::Cancelled
        ));
    }
}
