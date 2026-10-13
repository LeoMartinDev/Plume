use crate::{Bubble, Feedback};
use plume_core::SessionState;
use std::collections::VecDeque;
use std::time::{Duration, Instant};

// Long enough to read the pill's label before it fades out.
pub(crate) const NO_SPEECH_DURATION: Duration = Duration::from_millis(1600);
pub(crate) const SUCCESS_DURATION: Duration = Duration::from_millis(900);
// Copied also shows the paste shortcut, so it stays a little longer.
pub(crate) const COPIED_DURATION: Duration = Duration::from_millis(1600);
// A failed insertion whose text is on the clipboard: long enough to switch to
// the right field and paste. The text also stays in History.
pub(crate) const COPIED_NOTICE_DURATION: Duration = Duration::from_secs(15);
// A built-in microphone opens in 100 to 350 ms: the pill then appears straight
// at Listening instead of flashing "Opening mic…" first.
pub(crate) const STARTING_GRACE: Duration = Duration::from_millis(400);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Phase {
    Hidden,
    Starting,
    Cancelling,
    NoSpeech,
    Recording,
    Transcribing,
    Inserting,
    Success,
    Attention,
}

pub(crate) struct Recovery {
    pub text: String,
    pub copied: bool,
    pub copy_failed: bool,
    /// When the text last reached the clipboard.
    pub since: Instant,
}

impl Recovery {
    /// Copied text no longer needs the notice once it had time to be pasted;
    /// text that is not on the clipboard waits for Copy or dismiss.
    fn expired(&self, now: Instant) -> bool {
        self.copied && now.duration_since(self.since) >= COPIED_NOTICE_DURATION
    }
}

pub(crate) struct FeedbackState {
    pub bubble: Bubble,
    pub since: Instant,
    now: Instant,
    pending: Option<Bubble>,
    pub recoveries: VecDeque<Recovery>,
}

impl FeedbackState {
    pub fn new(bubble: Bubble, now: Instant) -> Self {
        Self {
            bubble,
            since: now,
            now,
            pending: None,
            recoveries: VecDeque::new(),
        }
    }

    pub fn receive(&mut self, bubble: Bubble, now: Instant) {
        self.now = now;
        if let Feedback::InsertionFailed { text, copied } = &bubble.feedback {
            self.recoveries.push_back(Recovery {
                text: text.clone(),
                copied: *copied,
                copy_failed: false,
                since: now,
            });
        }
        if bubble.capture_id < self.bubble.capture_id {
            return;
        }
        // Keep the insertion indicator legible even when insertion finishes
        // between two UI polls. New captures always take precedence.
        if bubble.capture_id == self.bubble.capture_id
            && self.bubble.feedback == Feedback::Inserting
            && !matches!(bubble.feedback, Feedback::Session | Feedback::Inserting)
        {
            self.pending = Some(bubble);
        } else {
            self.bubble = bubble;
            self.since = now;
            self.pending = None;
        }
    }

    pub fn tick(&mut self, now: Instant) {
        self.now = now;
        if self.pending.is_some() && now.duration_since(self.since) >= Duration::from_millis(150) {
            self.bubble = self.pending.take().unwrap();
            self.since = now;
        }
        self.recoveries.retain(|recovery| !recovery.expired(now));
        if self
            .lifetime()
            .is_some_and(|lifetime| now.duration_since(self.since) >= lifetime)
        {
            self.bubble.feedback = Feedback::Empty;
        }
    }

    /// How long a terminal state stays before the pill hides itself.
    pub fn lifetime(&self) -> Option<Duration> {
        match self.bubble.feedback {
            Feedback::NoSpeech => Some(NO_SPEECH_DURATION),
            Feedback::Success => Some(SUCCESS_DURATION),
            Feedback::Copied => Some(COPIED_DURATION),
            _ => None,
        }
    }

    pub fn phase(&self) -> Phase {
        match &self.bubble.feedback {
            Feedback::Starting if self.now.duration_since(self.since) < STARTING_GRACE => {
                Phase::Hidden
            }
            Feedback::Starting => Phase::Starting,
            Feedback::Transcribing => Phase::Transcribing,
            Feedback::Cancelling => Phase::Cancelling,
            Feedback::NoSpeech => Phase::NoSpeech,
            Feedback::RecordingNotice { .. } => Phase::Recording,
            Feedback::Session => match self.bubble.state() {
                SessionState::Recording | SessionState::Streaming => Phase::Recording,
                SessionState::Finalizing => Phase::Transcribing,
                _ if !self.recoveries.is_empty() => Phase::Attention,
                _ => Phase::Hidden,
            },
            Feedback::Inserting => Phase::Inserting,
            Feedback::Success | Feedback::Copied => Phase::Success,
            Feedback::Error { .. } => Phase::Attention,
            _ if !self.recoveries.is_empty() => Phase::Attention,
            _ => Phase::Hidden,
        }
    }

    pub fn dismiss(&mut self) {
        self.recoveries.pop_front();
        self.bubble.feedback = Feedback::Empty;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn old_failure_is_recoverable_without_overwriting_new_capture() {
        let now = Instant::now();
        let mut dictation = plume_core::Dictation::new();
        dictation.hold();
        let mut state =
            FeedbackState::new(Bubble::from_dictation(&dictation).with_capture_id(2), now);
        state.receive(
            Bubble::feedback(
                1,
                Feedback::InsertionFailed {
                    text: "Saved text".into(),
                    copied: false,
                },
            ),
            now,
        );
        assert_eq!(state.phase(), Phase::Recording);
        assert_eq!(state.recoveries.front().unwrap().text, "Saved text");
        state.receive(Bubble::feedback(2, Feedback::Empty), now);
        assert_eq!(state.phase(), Phase::Attention);
        state.dismiss();
        assert_eq!(state.phase(), Phase::Hidden);
    }
    #[test]
    fn a_microphone_that_opens_at_once_never_shows_opening() {
        let now = Instant::now();
        let mut state = FeedbackState::new(Bubble::feedback(0, Feedback::Starting), now);
        assert_eq!(state.phase(), Phase::Hidden);
        state.tick(now + STARTING_GRACE);
        assert_eq!(state.phase(), Phase::Starting);
    }

    #[test]
    fn insertion_and_success_have_bounded_display_times() {
        let now = Instant::now();
        let mut state = FeedbackState::new(Bubble::feedback(0, Feedback::Inserting), now);
        state.receive(Bubble::feedback(0, Feedback::Success), now);
        assert_eq!(state.phase(), Phase::Inserting);
        state.tick(now + Duration::from_millis(150));
        assert_eq!(state.phase(), Phase::Success);
        state.tick(now + Duration::from_millis(150) + SUCCESS_DURATION);
        assert_eq!(state.phase(), Phase::Hidden);
    }

    #[test]
    fn copied_lasts_longer_than_inserted() {
        let now = Instant::now();
        let mut state = FeedbackState::new(Bubble::feedback(0, Feedback::Inserting), now);
        state.receive(Bubble::feedback(0, Feedback::Copied), now);
        state.tick(now + Duration::from_millis(150));
        assert_eq!(state.phase(), Phase::Success);
        let shown = now + Duration::from_millis(150);
        state.tick(shown + SUCCESS_DURATION);
        assert_eq!(state.phase(), Phase::Success);
        state.tick(shown + COPIED_DURATION);
        assert_eq!(state.phase(), Phase::Hidden);
    }

    #[test]
    fn release_shows_transcription_and_cancellation_hides_it() {
        let now = Instant::now();
        let mut dictation = plume_core::Dictation::new();
        dictation.hold();
        dictation.release();
        let mut state = FeedbackState::new(Bubble::from_dictation(&dictation), now);
        assert_eq!(state.phase(), Phase::Transcribing);
        dictation.cancel();
        state.receive(Bubble::from_dictation(&dictation), now);
        assert_eq!(state.phase(), Phase::Hidden);
    }

    #[test]
    fn a_copied_failure_hides_itself_but_an_uncopied_one_waits() {
        let now = Instant::now();
        let mut state = FeedbackState::new(Bubble::feedback(0, Feedback::Empty), now);
        for (id, copied) in [(0, true), (1, false)] {
            state.receive(
                Bubble::feedback(
                    id,
                    Feedback::InsertionFailed {
                        text: format!("Text {id}"),
                        copied,
                    },
                ),
                now,
            );
        }
        // A later insertion succeeds; the copied notice comes back after it.
        state.receive(Bubble::feedback(2, Feedback::Success), now);
        state.tick(now + SUCCESS_DURATION);
        assert_eq!(state.phase(), Phase::Attention);
        assert_eq!(state.recoveries.front().unwrap().text, "Text 0");
        state.tick(now + COPIED_NOTICE_DURATION);
        assert_eq!(state.recoveries.front().unwrap().text, "Text 1");
        state.tick(now + COPIED_NOTICE_DURATION * 4);
        assert_eq!(state.phase(), Phase::Attention);
    }

    #[test]
    fn dismissing_one_failure_keeps_the_other_transcription() {
        let now = Instant::now();
        let mut state = FeedbackState::new(Bubble::feedback(0, Feedback::Empty), now);
        for id in 0..2 {
            state.receive(
                Bubble::feedback(
                    id,
                    Feedback::InsertionFailed {
                        text: format!("Text {id}"),
                        copied: false,
                    },
                ),
                now,
            );
        }
        state.dismiss();
        assert_eq!(state.phase(), Phase::Attention);
        assert_eq!(state.recoveries.front().unwrap().text, "Text 1");
        state.dismiss();
        assert_eq!(state.phase(), Phase::Hidden);
    }
}
