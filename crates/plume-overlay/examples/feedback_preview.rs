//! Deterministic overlay preview; no microphone, model or insertion required.
//! cargo run -p plume-overlay --example feedback_preview -- failure
//! Modes: cycle (default), no-speech, success, copied-success, error, failure,
//! copied.
//! PLUME_REDUCED_MOTION=1 freezes the animations; PLUME_PREVIEW_THEME=light
//! or dark overrides the system appearance like the app's theme setting.
use plume_core::Dictation;
use plume_overlay::{Appearance, Bubble, Feedback};
use std::{sync::mpsc, time::Duration};

fn main() {
    let mode = std::env::args().nth(1).unwrap_or_else(|| "cycle".into());
    let (bubbles, rx) = mpsc::channel();
    let (levels, level_rx) = mpsc::channel();
    std::thread::spawn(move || {
        if mode == "no-speech" || mode == "success" || mode == "copied-success" {
            // Repeat short terminal animations so they can be inspected easily.
            loop {
                let mut dictation = Dictation::new();
                dictation.hold();
                bubbles.send(Bubble::from_dictation(&dictation)).unwrap();
                std::thread::sleep(Duration::from_secs(2));
                if mode != "no-speech" {
                    bubbles
                        .send(Bubble::feedback(0, Feedback::Inserting))
                        .unwrap();
                    std::thread::sleep(Duration::from_millis(500));
                }
                bubbles
                    .send(Bubble::feedback(
                        0,
                        match mode.as_str() {
                            "no-speech" => Feedback::NoSpeech,
                            "copied-success" => Feedback::Copied,
                            _ => Feedback::Success,
                        },
                    ))
                    .unwrap();
                std::thread::sleep(Duration::from_secs(2));
            }
        }
        if mode == "error" {
            bubbles
                .send(Bubble::feedback(
                    0,
                    Feedback::Error {
                        title: "Recording unavailable".into(),
                        advice: "The microphone could not be opened. Check it in system settings."
                            .into(),
                    },
                ))
                .unwrap();
        } else if mode == "failure" || mode == "copied" {
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
            bubbles
                .send(Bubble::feedback(
                    0,
                    Feedback::RecordingNotice {
                        silence: false,
                        limit: false,
                        cancel: "Esc".into(),
                    },
                ))
                .unwrap();
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
    let appearance = match std::env::var("PLUME_PREVIEW_THEME").as_deref() {
        Ok("light") => Appearance::Light,
        Ok("dark") => Appearance::Dark,
        _ => Appearance::System,
    };
    plume_overlay::prepare_display();
    gpui::Application::new().run(move |cx| {
        cx.set_global(appearance);
        plume_overlay::attach(cx, rx, level_rx);
        cx.activate(true);
    });
}
