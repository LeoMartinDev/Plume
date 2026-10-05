use std::mem;

use crate::{BoxError, Hypothesis, Session, Transcript};

/// Discriminated injection the overlay or a test applies.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Edit {
    Insert(String),
    Replace { old: String, new: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DictationUpdate {
    pub edit: Option<Edit>,
    pub transcript: Option<Transcript>,
}

/// Owns Session and the single current partial used for live correction.
/// Engine types and injector types stay out.
pub struct Dictation {
    session: Session,
}

impl Dictation {
    pub fn new() -> Self {
        Dictation {
            session: Session::new(),
        }
    }

    pub fn session(&self) -> &Session {
        &self.session
    }

    /// Idle -> Recording. Any other state is a no-op, matching Session::hold.
    pub fn hold(&mut self) {
        self.session = mem::take(&mut self.session).hold();
    }

    /// Recording or Streaming -> Finalizing, waiting for the engine's final.
    /// Already closing -> no-op.
    pub fn release(&mut self) {
        self.session = match mem::take(&mut self.session) {
            Session::Recording(recording) => Session::Finalizing(recording.release()),
            Session::Streaming(streaming) => Session::Finalizing(streaming.release()),
            closing => closing,
        };
    }

    pub fn cancel(&mut self) {
        self.session = Session::Cancelled(mem::take(&mut self.session).cancel());
    }

    /// Drive one engine item. File transcribe never calls release. Audio EOF
    /// arrives as Final while still Recording or Streaming. This method
    /// synthesizes Finalizing then finish in that case.
    ///
    /// Cancelled + any hypothesis -> DictationUpdate { edit: None, transcript: None }.
    pub fn on_hypothesis(&mut self, hyp: Hypothesis) -> Result<DictationUpdate, BoxError> {
        let session = mem::take(&mut self.session);
        match (session, hyp) {
            (Session::Cancelled(cancelled), _) => {
                self.session = Session::Cancelled(cancelled);
                Ok(DictationUpdate {
                    edit: None,
                    transcript: None,
                })
            }
            (Session::Idle, _) => {
                self.session = Session::Idle;
                Err("hypothesis received while idle; call hold first".into())
            }
            (Session::Recording(recording), Hypothesis::Partial(partial)) => {
                let edit = Edit::Insert(partial.text.clone());
                self.session = Session::Streaming(recording.on_partial(partial));
                Ok(DictationUpdate {
                    edit: Some(edit),
                    transcript: None,
                })
            }
            (Session::Recording(recording), Hypothesis::Final(transcript)) => {
                let edit = Edit::Insert(transcript.text.clone());
                let (session, transcript) = recording.on_final(transcript);
                self.session = session;
                Ok(DictationUpdate {
                    edit: Some(edit),
                    transcript: Some(transcript),
                })
            }
            (Session::Streaming(streaming), Hypothesis::Partial(partial)) => {
                let old = streaming.latest().text().to_string();
                let new = partial.text.clone();
                self.session = Session::Streaming(streaming.on_partial(partial));
                Ok(DictationUpdate {
                    edit: Some(Edit::Replace { old, new }),
                    transcript: None,
                })
            }
            (Session::Streaming(streaming), Hypothesis::Final(transcript)) => {
                let old = streaming.latest().text().to_string();
                let new = transcript.text.clone();
                let finalizing = streaming.release();
                let (session, transcript) = finalizing.finish(transcript);
                self.session = session;
                Ok(DictationUpdate {
                    edit: Some(Edit::Replace { old, new }),
                    transcript: Some(transcript),
                })
            }
            (Session::Finalizing(finalizing), Hypothesis::Partial(partial)) => {
                let new = partial.text.clone();
                let edit = match finalizing.latest() {
                    Some(latest) => Edit::Replace {
                        old: latest.text().to_string(),
                        new: new.clone(),
                    },
                    None => Edit::Insert(new.clone()),
                };
                self.session = Session::Finalizing(finalizing.on_partial(partial));
                Ok(DictationUpdate {
                    edit: Some(edit),
                    transcript: None,
                })
            }
            (Session::Finalizing(finalizing), Hypothesis::Final(transcript)) => {
                let new = transcript.text.clone();
                let edit = match finalizing.latest() {
                    Some(latest) => Edit::Replace {
                        old: latest.text().to_string(),
                        new: new.clone(),
                    },
                    None => Edit::Insert(new.clone()),
                };
                let (session, transcript) = finalizing.finish(transcript);
                self.session = session;
                Ok(DictationUpdate {
                    edit: Some(edit),
                    transcript: Some(transcript),
                })
            }
        }
    }
}

impl Default for Dictation {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        AsrEngine, AudioChunk, AudioStream, Hypothesis, HypothesisStream, PartialHypothesis,
        SessionState, Transcript,
    };

    struct ScriptedEngine {
        hyps: Vec<Hypothesis>,
    }

    impl AsrEngine for ScriptedEngine {
        fn stream(&self, audio: AudioStream) -> HypothesisStream {
            let _ = audio.count();
            let hyps = self.hyps.clone();
            Box::new(hyps.into_iter().map(Ok))
        }
    }

    #[test]
    fn dictate_bonjour_injects_partials_then_commits_final() {
        let engine = ScriptedEngine {
            hyps: vec![
                Hypothesis::Partial(PartialHypothesis {
                    text: "bonj".into(),
                }),
                Hypothesis::Partial(PartialHypothesis {
                    text: "bonjour".into(),
                }),
                Hypothesis::Final(Transcript {
                    text: "Bonjour.".into(),
                }),
            ],
        };
        let audio: AudioStream = Box::new(std::iter::once(AudioChunk {
            samples: vec![0.0; 1600],
            sample_rate: 16_000,
        }));
        let mut dictation = Dictation::new();
        dictation.hold();
        assert_eq!(dictation.session().state(), SessionState::Recording);

        let mut edits = Vec::new();
        let mut final_text = None;
        for item in engine.stream(audio) {
            let step = dictation.on_hypothesis(item.unwrap()).unwrap();
            if let Some(edit) = step.edit {
                edits.push(edit);
            }
            if let Some(t) = step.transcript {
                final_text = Some(t.text);
            }
        }

        assert_eq!(
            edits,
            vec![
                Edit::Insert("bonj".into()),
                Edit::Replace {
                    old: "bonj".into(),
                    new: "bonjour".into()
                },
                Edit::Replace {
                    old: "bonjour".into(),
                    new: "Bonjour.".into()
                },
            ]
        );
        assert_eq!(final_text.as_deref(), Some("Bonjour."));
        assert_eq!(dictation.session().state(), SessionState::Idle);
    }
}
