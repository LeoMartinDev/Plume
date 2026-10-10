mod feedback;
mod frame;
mod pill;
mod window;

use plume_core::{Dictation, Session, SessionState};

pub use window::{attach, prepare_display, run, run_with, WINDOW_TITLE};

/// Pill theme picked in the app's settings, stored as a GPUI global. Without
/// it, or with `System`, the pill follows the operating system's appearance.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Appearance {
    #[default]
    System,
    Light,
    Dark,
}

impl gpui::Global for Appearance {}

/// Latest partial and session phase shown in the overlay bubble.
///
/// Constructed only from `Dictation` / `Session`, so idle, recording, and
/// cancelled cannot carry leftover partial text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Bubble {
    text: String,
    state: SessionState,
    pub capture_id: u64,
    pub feedback: Feedback,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Feedback {
    Starting,
    Transcribing,
    Cancelling,
    NoSpeech,
    /// `cancel` is the cancel shortcut as typed in settings ("Esc").
    RecordingNotice {
        silence: bool,
        limit: bool,
        cancel: String,
    },
    Session,
    Inserting,
    Success,
    /// Delivered to the clipboard instead of a field.
    Copied,
    Empty,
    InsertionFailed {
        text: String,
        copied: bool,
    },
    Error {
        title: String,
        advice: String,
    },
}

impl Bubble {
    pub fn from_dictation(dictation: &Dictation) -> Self {
        Self::from_session(dictation.session())
    }

    pub fn from_session(session: &Session) -> Self {
        let text = String::new();
        Self {
            text,
            state: session.state(),
            capture_id: 0,
            feedback: Feedback::Session,
        }
    }

    pub fn with_capture_id(mut self, id: u64) -> Self {
        self.capture_id = id;
        self
    }

    pub fn feedback(capture_id: u64, feedback: Feedback) -> Self {
        Self {
            text: String::new(),
            state: SessionState::Idle,
            capture_id,
            feedback,
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn state(&self) -> SessionState {
        self.state
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use plume_core::{Hypothesis, PartialHypothesis, Transcript};

    fn partial(text: &str) -> Hypothesis {
        Hypothesis::Partial(PartialHypothesis {
            text: text.to_string(),
        })
    }

    fn final_hyp(text: &str) -> Hypothesis {
        Hypothesis::Final(Transcript {
            text: text.to_string(),
        })
    }

    #[test]
    fn bubble_never_exposes_partial_text() {
        let mut dictation = Dictation::new();
        let bubble = Bubble::from_dictation(&dictation);
        assert_eq!(bubble.text(), "");
        assert_eq!(bubble.state(), SessionState::Idle);

        dictation.hold();
        let bubble = Bubble::from_dictation(&dictation);
        assert_eq!(bubble.text(), "");
        assert_eq!(bubble.state(), SessionState::Recording);

        dictation.on_hypothesis(partial("bonj")).unwrap();
        let bubble = Bubble::from_dictation(&dictation);
        assert_eq!(bubble.text(), "");
        assert_eq!(bubble.state(), SessionState::Streaming);

        dictation.on_hypothesis(partial("bonjour")).unwrap();
        let bubble = Bubble::from_dictation(&dictation);
        assert_eq!(bubble.text(), "");
        assert_eq!(bubble.state(), SessionState::Streaming);

        dictation.release();
        let bubble = Bubble::from_dictation(&dictation);
        assert_eq!(bubble.text(), "");
        assert_eq!(bubble.state(), SessionState::Finalizing);

        dictation.on_hypothesis(final_hyp("Bonjour.")).unwrap();
        let bubble = Bubble::from_dictation(&dictation);
        assert_eq!(bubble.text(), "");
        assert_eq!(bubble.state(), SessionState::Idle);
    }

    #[test]
    fn bubble_mirrors_final_while_streaming() {
        let mut dictation = Dictation::new();
        dictation.hold();
        dictation.on_hypothesis(partial("hi")).unwrap();
        dictation.on_hypothesis(final_hyp("Hi.")).unwrap();
        let bubble = Bubble::from_dictation(&dictation);
        assert_eq!(bubble.text(), "");
        assert_eq!(bubble.state(), SessionState::Idle);
    }

    #[test]
    fn cancel_swallows_late_output() {
        let mut dictation = Dictation::new();
        dictation.hold();
        dictation.on_hypothesis(partial("bonj")).unwrap();
        dictation.cancel();
        let bubble = Bubble::from_dictation(&dictation);
        assert_eq!(bubble.text(), "");
        assert_eq!(bubble.state(), SessionState::Cancelled);

        let step = dictation.on_hypothesis(partial("bonjour")).unwrap();
        assert_eq!(step.edit, None);
        assert_eq!(step.transcript, None);
        let bubble = Bubble::from_dictation(&dictation);
        assert_eq!(bubble.text(), "");
        assert_eq!(bubble.state(), SessionState::Cancelled);

        let step = dictation.on_hypothesis(final_hyp("Bonjour.")).unwrap();
        assert_eq!(step.edit, None);
        assert_eq!(step.transcript, None);
        let bubble = Bubble::from_dictation(&dictation);
        assert_eq!(bubble.text(), "");
        assert_eq!(bubble.state(), SessionState::Cancelled);
    }
}
