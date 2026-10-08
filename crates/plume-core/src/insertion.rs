use crate::BoxError;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum InsertionMode {
    #[default]
    Auto,
    Clipboard,
    Typing,
}

impl InsertionMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Clipboard => "clipboard",
            Self::Typing => "typing",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InsertionMethod {
    Clipboard,
    Typing,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TargetAssessment {
    Editable,
    NonEditable,
    Sensitive,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InjectionReport {
    pub method: InsertionMethod,
    pub application: Option<String>,
    pub target: TargetAssessment,
}

/// Inserts dictated text into the focused application. One implementation per
/// operating system.
pub trait TextInjector {
    fn field_context(&self) -> Option<FieldContext> {
        None
    }
    fn insert(&mut self, text: &str) -> Result<(), BoxError>;
    fn replace_last(&mut self, old: &str, new: &str) -> Result<(), BoxError>;

    fn insert_with_mode(
        &mut self,
        text: &str,
        _mode: InsertionMode,
    ) -> Result<InjectionReport, BoxError> {
        self.insert(text)?;
        Ok(InjectionReport {
            method: InsertionMethod::Typing,
            application: None,
            target: TargetAssessment::Unknown,
        })
    }
    fn insert_checked(
        &mut self,
        text: &str,
        mode: InsertionMode,
        before_dispatch: &mut dyn FnMut() -> bool,
    ) -> Result<InjectionReport, BoxError> {
        if !before_dispatch() {
            return Err("dictation cancelled".into());
        }
        self.insert_with_mode(text, mode)
    }

    fn copy_text(&mut self, _text: &str) -> Result<(), BoxError> {
        Err("clipboard copy is not supported by this injector".into())
    }
}

/// A transient context, read at delivery time. Selection is already excluded
/// from the adjacent characters. Native implementations return None if uncertain.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FieldContext {
    pub before: Option<char>,
    pub after: Option<char>,
    pub has_selection: bool,
    pub target_id: String,
}

/// Adjust only the two boundaries; internal transcription is preserved verbatim.
pub fn boundary_spacing(text: &str, context: Option<&FieldContext>) -> String {
    let Some(context) = context else {
        return text.to_owned();
    };
    if text.is_empty() {
        return String::new();
    }
    let mut text = text.to_owned();
    if context.before.is_some_and(char::is_whitespace) {
        text = text.trim_start_matches(char::is_whitespace).to_owned();
    }
    if context.after.is_some_and(char::is_whitespace) {
        text = text.trim_end_matches(char::is_whitespace).to_owned();
    }
    let word = |c: char| {
        c.is_alphanumeric()
            || matches!(c as u32,0x0300..=0x036f|0x1ab0..=0x1aff|0x1dc0..=0x1dff|0x20d0..=0x20ff|0xfe20..=0xfe2f)
    };
    let closing = |c: char| matches!(c, '.' | ',' | ';' | ':' | '!' | '?' | ')' | ']' | '}');
    if context.before.is_some_and(|c| word(c) || closing(c))
        && text.chars().next().is_some_and(word)
    {
        text.insert(0, ' ');
    }
    if context.after.is_some_and(word)
        && text
            .chars()
            .next_back()
            .is_some_and(|c| word(c) || closing(c))
    {
        text.push(' ');
    }
    text
}

#[cfg(test)]
mod boundary_tests {
    use super::*;
    fn context(before: Option<char>, after: Option<char>) -> FieldContext {
        FieldContext {
            before,
            after,
            has_selection: false,
            target_id: "field".into(),
        }
    }
    #[test]
    fn known_boundaries_and_unknown_preserve_internal_text() {
        assert_eq!(
            boundary_spacing("élève  vert", Some(&context(Some('a'), Some('日')))),
            " élève  vert "
        );
        assert_eq!(
            boundary_spacing(" hello ", Some(&context(Some(' '), Some('\n')))),
            "hello"
        );
        for text in ["'ami", "-mot", ", oui", ".", "’s"] {
            assert_eq!(
                boundary_spacing(text, Some(&context(Some('a'), None))),
                text
            );
        }
        assert_eq!(boundary_spacing(" hello  world ", None), " hello  world ");
    }
}
