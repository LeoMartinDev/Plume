//! Deterministic overlay preview; no microphone, model or insertion required.
//! cargo run -p plume-overlay --example feedback_preview -- failure
use plume_core::Dictation;
use plume_overlay::{Bubble, Feedback};
use std::{sync::mpsc, time::Duration};

fn main() {
    let mode = std::env::args().nth(1).unwrap_or_else(|| "cycle".into());
    let (bubbles, rx) = mpsc::channel();
    let (levels, level_rx) = mpsc::channel();
    std::thread::spawn(move || {
        if mode == "no-speech" || mode == "success" {
            // Repeat short terminal animations so they can be inspected easily.
            loop {
                let mut dictation = Dictation::new();
                dictation.hold();
                bubbles.send(Bubble::from_dictation(&dictation)).unwrap();
                std::thread::sleep(Duration::from_secs(2));
                if mode == "success" {
                    bubbles
                        .send(Bubble::feedback(0, Feedback::Inserting))
                        .unwrap();
                    std::thread::sleep(Duration::from_millis(500));
                }
                bubbles
                    .send(Bubble::feedback(
                        0,
                        if mode == "no-speech" {
                            Feedback::NoSpeech
                        } else {
                            Feedback::Success
                        },
                    ))
                    .unwrap();
                std::thread::sleep(Duration::from_secs(2));
            }
        }
        if mode == "failure" || mode == "copied" {
            bubbles
                .send(Bubble::feedback(
                    0,
                    Feedback::InsertionFailed {
                        text: "This transcription remains available after an insertion failure."
                            .into(),
                        copied: mode == "copied",
                    },
                ))
                .unwrap();
        } else {
            let mut dictation = Dictation::new();
            dictation.hold();
            bubbles.send(Bubble::from_dictation(&dictation)).unwrap();
            for index in 0..120 {
                let _ = levels.send((index as f32 * 0.2).sin().abs() * 0.5);
                std::thread::sleep(Duration::from_millis(25));
            }
            dictation.release();
            bubbles.send(Bubble::from_dictation(&dictation)).unwrap();
            std::thread::sleep(Duration::from_secs(2));
            bubbles
                .send(Bubble::feedback(0, Feedback::Inserting))
                .unwrap();
            std::thread::sleep(Duration::from_millis(500));
            bubbles
                .send(Bubble::feedback(0, Feedback::Success))
                .unwrap();
            std::thread::sleep(Duration::from_secs(2));
            bubbles
                .send(Bubble::feedback(
                    0,
                    Feedback::InsertionFailed {
                        text: "Preview transcription to recover.".into(),
                        copied: false,
                    },
                ))
                .unwrap();
        }
        // Keep both senders alive while the user tests Copy and dismissal.
        loop {
            std::thread::sleep(Duration::from_secs(60));
        }
    });
    plume_overlay::run_with(rx, level_rx);
}
