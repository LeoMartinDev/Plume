use stt_core::{BoxError, TextInjector};

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Call {
    Insert(String),
    ReplaceLast(String, String),
}

pub(crate) struct RecordingInjector {
    pub calls: Vec<Call>,
}

impl RecordingInjector {
    pub(crate) fn new() -> Self {
        Self { calls: Vec::new() }
    }
}

impl TextInjector for RecordingInjector {
    fn insert(&mut self, text: &str) -> Result<(), BoxError> {
        self.calls.push(Call::Insert(text.to_string()));
        Ok(())
    }

    fn replace_last(&mut self, old: &str, new: &str) -> Result<(), BoxError> {
        self.calls
            .push(Call::ReplaceLast(old.to_string(), new.to_string()));
        Ok(())
    }
}
