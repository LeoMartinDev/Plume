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

    fn copy_text(&mut self, _text: &str) -> Result<(), BoxError> {
        Err("clipboard copy is not supported by this injector".into())
    }
}
