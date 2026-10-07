use crate::{Bubble, Feedback};
use plume_core::SessionState;
use std::collections::VecDeque;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Phase {
    Hidden,
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
}

pub(crate) struct FeedbackState {
    pub bubble: Bubble,
    pub since: Instant,
    pending: Option<Bubble>,
    pub recoveries: VecDeque<Recovery>,
}

impl FeedbackState {
    pub fn new(bubble: Bubble, now: Instant) -> Self {
        Self {
            bubble,
            since: now,
            pending: None,
            recoveries: VecDeque::new(),
        }
    }

    pub fn receive(&mut self, bubble: Bubble, now: Instant) {
        if let Feedback::InsertionFailed { text, copied } = &bubble.feedback {
            self.recoveries.push_back(Recovery {
                text: text.clone(),
                copied: *copied,
                copy_failed: false,
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
        if self.pending.is_some() && now.duration_since(self.since) >= Duration::from_millis(150) {
            self.bubble = self.pending.take().unwrap();
            self.since = now;
        }
        if self.bubble.feedback == Feedback::Success
            && now.duration_since(self.since) >= Duration::from_millis(650)
        {
            self.bubble.feedback = Feedback::Empty;
        }
    }

    pub fn phase(&self) -> Phase {
        match &self.bubble.feedback {
            Feedback::Session => match self.bubble.state() {
                SessionState::Recording | SessionState::Streaming => Phase::Recording,
                SessionState::Finalizing => Phase::Transcribing,
                _ if !self.recoveries.is_empty() => Phase::Attention,
                _ => Phase::Hidden,
            },
            Feedback::Inserting => Phase::Inserting,
            Feedback::Success => Phase::Success,
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
    fn insertion_and_success_have_bounded_display_times() {
        let now = Instant::now();
        let mut state = FeedbackState::new(Bubble::feedback(0, Feedback::Inserting), now);
        state.receive(Bubble::feedback(0, Feedback::Success), now);
        assert_eq!(state.phase(), Phase::Inserting);
        state.tick(now + Duration::from_millis(150));
        assert_eq!(state.phase(), Phase::Success);
        state.tick(now + Duration::from_millis(800));
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
