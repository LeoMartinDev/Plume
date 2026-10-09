//! Preview completion deliberately has no injector, clipboard, or history sink.
use crate::{PreviewEvent, SharedRecordings};
use plume_overlay::Feedback;
use std::sync::mpsc::Sender;

pub(crate) fn finish(
    recordings: &SharedRecordings,
    id: u64,
    has_speech: bool,
    result: Result<String, String>,
    output: &Sender<PreviewEvent>,
) -> Result<Feedback, String> {
    // Audio must be gone before publishing a transcript or readiness.
    recordings
        .lock()
        .unwrap()
        .delete(id)
        .map_err(|error| error.to_string())?;
    Ok(match result {
        Ok(text) if has_speech && !text.trim().is_empty() => {
            let _ = output.send(PreviewEvent::Transcript(text));
            Feedback::Success
        }
        Ok(_) => Feedback::NoSpeech,
        Err(error) => Feedback::Error {
            title: "Try again".into(),
            advice: error,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::RecordingStore;

    #[test]
    fn preview_completion_removes_audio_without_saving_history_on_success_silence_or_failure() {
        for (name, speech, result) in [
            ("speech", true, Ok("Hello Plume".to_string())),
            ("silence", false, Ok(String::new())),
            ("failure", true, Err("decoder failed".to_string())),
        ] {
            let root =
                std::env::temp_dir().join(format!("plume-preview-{name}-{}", std::process::id()));
            let mut store = RecordingStore::open(root.clone()).unwrap();
            let mut writer = store.begin_preview().unwrap();
            let id = writer.record.id;
            writer.write(&[0.2; 512], speech).unwrap();
            writer.finish().unwrap();
            let store = store.shared();
            let (tx, rx) = std::sync::mpsc::channel();
            let feedback = finish(&store, id, speech, result, &tx).unwrap();
            assert!(!store.lock().unwrap().has_audio(id));
            assert!(store.lock().unwrap().records().is_empty());
            assert_eq!(std::fs::read_dir(&root).unwrap().count(), 0);
            if name == "speech" {
                assert_eq!(feedback, Feedback::Success);
                assert!(
                    matches!(rx.try_recv(), Ok(PreviewEvent::Transcript(text)) if text == "Hello Plume")
                );
            } else {
                assert!(rx.try_recv().is_err());
                assert!(matches!(
                    feedback,
                    Feedback::NoSpeech | Feedback::Error { .. }
                ));
            }
            std::fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn interrupted_preview_is_deleted_on_restart_and_previous_recordings_survive() {
        let root =
            std::env::temp_dir().join(format!("plume-preview-restart-{}", std::process::id()));
        let mut store = RecordingStore::open(root.clone()).unwrap();
        let mut prior = store.begin().unwrap();
        prior.write(&[0.2; 512], true).unwrap();
        let prior = prior.finish().unwrap();
        let prior_id = prior.id;
        store.promote(prior).unwrap();
        let mut preview = store.begin_preview().unwrap();
        let preview_id = preview.record.id;
        preview.write(&[0.2; 512], true).unwrap();
        drop(preview);
        drop(store);
        let recovered = RecordingStore::open(root.clone()).unwrap();
        assert!(!recovered.has_audio(preview_id));
        assert_eq!(recovered.records().len(), 1);
        assert!(recovered.has_audio(prior_id));
        std::fs::remove_dir_all(root).unwrap();
    }
}
