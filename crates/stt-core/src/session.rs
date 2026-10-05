use crate::{PartialHypothesis, Transcript};
use std::fmt;

/// The discriminant of `Session`, for callers that match on the phase without
/// touching the phase's data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionState {
    Idle,
    Recording,
    Streaming,
    Finalizing,
    Cancelled,
}

impl fmt::Display for SessionState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            SessionState::Idle => "idle",
            SessionState::Recording => "recording",
            SessionState::Streaming => "streaming",
            SessionState::Finalizing => "finalizing",
            SessionState::Cancelled => "cancelled",
        };
        f.write_str(name)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PartialTranscript {
    text: String,
}

impl PartialTranscript {
    pub fn text(&self) -> &str {
        &self.text
    }
}

/// The user released or toggled off and a latest partial exists to refine.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Finalizing {
    latest: Option<PartialTranscript>,
}

impl Finalizing {
    pub fn latest(&self) -> Option<&PartialTranscript> {
        self.latest.as_ref()
    }

    pub fn on_partial(self, hypothesis: PartialHypothesis) -> Self {
        Finalizing {
            latest: Some(PartialTranscript {
                text: hypothesis.text,
            }),
        }
    }

    /// The engine returned the final transcript. The session ends in `Idle`
    /// and the transcript goes to the injector.
    pub fn finish(self, transcript: Transcript) -> (Session, Transcript) {
        (Session::Idle, transcript)
    }

    /// Esc during finalization drops the latest partial without injecting text.
    pub fn cancel(self) -> Cancelled {
        Cancelled
    }
}

/// Terminal state after Esc. Nothing carries over, so a cancelled session
/// cannot leak partial text into injection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cancelled;

/// Hold-to-talk capture before any hypothesis arrives.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Recording;

impl Recording {
    /// First partial hypothesis arrived; audio keeps flowing.
    pub fn on_partial(self, hypothesis: PartialHypothesis) -> Streaming {
        Streaming {
            latest: PartialTranscript {
                text: hypothesis.text,
            },
            held: true,
        }
    }

    /// Short utterance. Engine emitted Final before any Partial.
    pub fn on_final(self, transcript: Transcript) -> (Session, Transcript) {
        (Session::Idle, transcript)
    }

    /// Release before any hypothesis still waits for the engine's final text.
    pub fn release(self) -> Finalizing {
        Finalizing { latest: None }
    }

    pub fn cancel(self) -> Cancelled {
        Cancelled
    }
}

/// Partial hypotheses are flowing. `held` records whether the key is still
/// down; it goes away in `Finalizing`, where no key state exists.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Streaming {
    latest: PartialTranscript,
    held: bool,
}

impl Streaming {
    pub fn latest(&self) -> &PartialTranscript {
        &self.latest
    }

    pub fn is_held(&self) -> bool {
        self.held
    }

    pub fn on_partial(mut self, hypothesis: PartialHypothesis) -> Self {
        self.latest.text = hypothesis.text;
        self
    }

    /// Release ends capture and moves the session to `Finalizing`, keeping
    /// the same latest partial.
    pub fn release(self) -> Finalizing {
        Finalizing {
            latest: Some(self.latest),
        }
    }

    pub fn cancel(self) -> Cancelled {
        Cancelled
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Session {
    Idle,
    Recording(Recording),
    Streaming(Streaming),
    Finalizing(Finalizing),
    Cancelled(Cancelled),
}

impl Session {
    pub fn new() -> Self {
        Session::Idle
    }

    pub fn state(&self) -> SessionState {
        match self {
            Session::Idle => SessionState::Idle,
            Session::Recording(_) => SessionState::Recording,
            Session::Streaming(_) => SessionState::Streaming,
            Session::Finalizing(_) => SessionState::Finalizing,
            Session::Cancelled(_) => SessionState::Cancelled,
        }
    }

    /// Hold starts capture from `Idle`. Hold in any other state is a no-op
    /// because the key is already down or the session is closing.
    pub fn hold(self) -> Self {
        match self {
            Session::Idle => Session::Recording(Recording),
            active => active,
        }
    }

    /// Toggle starts capture from `Idle` and stops it from an active capture.
    pub fn toggle(self) -> Self {
        match self {
            Session::Idle => Session::Recording(Recording),
            Session::Recording(recording) => Session::Finalizing(recording.release()),
            Session::Streaming(streaming) => Session::Finalizing(streaming.release()),
            closing => closing,
        }
    }

    /// Esc discards the session from any state, including `Idle`.
    pub fn cancel(self) -> Cancelled {
        match self {
            Session::Recording(recording) => recording.cancel(),
            Session::Streaming(streaming) => streaming.cancel(),
            Session::Finalizing(finalizing) => finalizing.cancel(),
            _ => Cancelled,
        }
    }
}

impl Default for Session {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn partial(text: &str) -> PartialHypothesis {
        PartialHypothesis {
            text: text.to_string(),
        }
    }

    #[test]
    fn hold_release_cancel_drive_the_session() {
        // Hold: idle -> recording.
        let session = Session::new();
        assert_eq!(session.state(), SessionState::Idle);
        let session = session.hold();
        assert_eq!(session.state(), SessionState::Recording);

        // A partial hypothesis moves recording -> streaming.
        let Session::Recording(recording) = session else {
            panic!("hold from idle must start recording");
        };
        let streaming = recording.on_partial(partial("bonj"));
        assert!(streaming.is_held());
        let streaming = streaming.on_partial(partial("bonjour"));
        assert_eq!(streaming.latest().text(), "bonjour");

        let finalizing = streaming.release();
        assert_eq!(
            finalizing.latest().map(PartialTranscript::text),
            Some("bonjour")
        );

        // Esc during finalization: -> cancelled, partials dropped.
        let cancelled = finalizing.cancel();
        assert_eq!(cancelled, Cancelled);
    }

    #[test]
    fn recording_on_final_returns_idle_with_the_transcript() {
        let session = Session::new().hold();
        let Session::Recording(recording) = session else {
            panic!("hold from idle must start recording");
        };
        let (session, transcript) = recording.on_final(Transcript {
            text: "Bonjour.".to_string(),
        });
        assert_eq!(session.state(), SessionState::Idle);
        assert_eq!(transcript.text, "Bonjour.");
    }

    #[test]
    fn release_before_any_hypothesis_waits_for_final() {
        let session = Session::new().hold();
        let Session::Recording(recording) = session else {
            panic!("hold from idle must start recording");
        };
        assert_eq!(recording.release().latest(), None);
    }

    #[test]
    fn hold_during_streaming_is_a_no_op() {
        let session = Session::new().hold();
        let Session::Recording(recording) = session else {
            panic!("hold from idle must start recording");
        };
        let streaming = recording.on_partial(partial("salut"));
        let session = Session::Streaming(streaming).hold();
        let Session::Streaming(streaming) = session else {
            panic!("hold while streaming must not restart the session");
        };
        assert_eq!(streaming.latest().text(), "salut");
    }

    #[test]
    fn toggle_starts_and_stops_capture() {
        let session = Session::new().toggle();
        assert_eq!(session.state(), SessionState::Recording);
        let Session::Recording(recording) = session else {
            panic!("toggle from idle must start recording");
        };
        let streaming = recording.on_partial(partial("hello"));
        let session = Session::Streaming(streaming).toggle();
        assert_eq!(session.state(), SessionState::Finalizing);
    }

    #[test]
    fn esc_cancels_from_any_state() {
        assert_eq!(Session::new().cancel(), Cancelled);
        assert_eq!(Session::new().hold().cancel(), Cancelled);
    }
}
