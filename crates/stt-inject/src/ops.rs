/// One keystroke the OS backends turn into native events.
///
/// `replace_last` is always a full backspace replay of `old` plus a retype of
/// `new`. It does not keep a shared prefix.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Stroke {
    Backspace,
    Return,
    Tab,
    Char(char),
}

pub(crate) fn insert_strokes(text: &str) -> Vec<Stroke> {
    text.chars().map(char_stroke).collect()
}

pub(crate) fn replace_strokes(old: &str, new: &str) -> Vec<Stroke> {
    let mut strokes = Vec::with_capacity(old.chars().count() + new.chars().count());
    strokes.extend(std::iter::repeat_n(Stroke::Backspace, old.chars().count()));
    strokes.extend(new.chars().map(char_stroke));
    strokes
}

fn char_stroke(c: char) -> Stroke {
    match c {
        '\n' | '\r' => Stroke::Return,
        '\t' => Stroke::Tab,
        other => Stroke::Char(other),
    }
}

#[cfg(test)]
mod tests {
    use super::{insert_strokes, replace_strokes, Stroke};

    #[test]
    fn insert_maps_control_keys_and_keeps_unicode() {
        assert_eq!(
            insert_strokes("a\n\té"),
            vec![
                Stroke::Char('a'),
                Stroke::Return,
                Stroke::Tab,
                Stroke::Char('é'),
            ]
        );
    }

    #[test]
    fn replace_last_replays_every_old_char_then_retypes() {
        assert_eq!(
            replace_strokes("hello", "help"),
            vec![
                Stroke::Backspace,
                Stroke::Backspace,
                Stroke::Backspace,
                Stroke::Backspace,
                Stroke::Backspace,
                Stroke::Char('h'),
                Stroke::Char('e'),
                Stroke::Char('l'),
                Stroke::Char('p'),
            ]
        );
        assert_eq!(replace_strokes("", "a"), vec![Stroke::Char('a')]);
        assert_eq!(replace_strokes("a", ""), vec![Stroke::Backspace]);
    }
}
