use crate::HotkeyError;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Trigger {
    Space,
    Escape,
    Tab,
    Return,
    Char(char),
    F(u8),
    Fn,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Shortcut {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub super_key: bool,
    pub fn_key: bool,
    pub trigger: Trigger,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) enum KeyId {
    Ctrl,
    Alt,
    Shift,
    Super,
    Fn,
    Trigger(Trigger),
}

impl Shortcut {
    pub(crate) fn parse(raw: &str) -> Result<Self, HotkeyError> {
        let mut ctrl = false;
        let mut alt = false;
        let mut shift = false;
        let mut super_key = false;
        let mut fn_token = false;
        let mut trigger: Option<Trigger> = None;

        let mut saw_part = false;
        for part in raw.split('+') {
            let token = part.trim();
            if token.is_empty() {
                return Err(HotkeyError::InvalidShortcut(raw.to_string()));
            }
            saw_part = true;
            match parse_token(token)? {
                Token::Ctrl => ctrl = true,
                Token::Alt => alt = true,
                Token::Shift => shift = true,
                Token::Super => super_key = true,
                Token::Fn => fn_token = true,
                Token::Trigger(next) => {
                    if trigger.is_some() {
                        return Err(HotkeyError::InvalidShortcut(raw.to_string()));
                    }
                    trigger = Some(next);
                }
            }
        }
        if !saw_part {
            return Err(HotkeyError::InvalidShortcut(raw.to_string()));
        }

        let (fn_key, trigger) = match (fn_token, trigger) {
            (true, Some(trigger)) => (true, trigger),
            (true, None) => (false, Trigger::Fn),
            (false, Some(trigger)) => (false, trigger),
            (false, None) => return Err(HotkeyError::InvalidShortcut(raw.to_string())),
        };

        Ok(Shortcut {
            ctrl,
            alt,
            shift,
            super_key,
            fn_key,
            trigger,
        })
    }
}

enum Token {
    Ctrl,
    Alt,
    Shift,
    Super,
    Fn,
    Trigger(Trigger),
}

fn parse_token(token: &str) -> Result<Token, HotkeyError> {
    let lower = token.to_ascii_lowercase();
    Ok(match lower.as_str() {
        "ctrl" | "control" => Token::Ctrl,
        "alt" | "option" => Token::Alt,
        "shift" => Token::Shift,
        "super" | "meta" | "win" | "cmd" | "command" => Token::Super,
        "fn" => Token::Fn,
        "space" => Token::Trigger(Trigger::Space),
        "esc" | "escape" => Token::Trigger(Trigger::Escape),
        "tab" => Token::Trigger(Trigger::Tab),
        "return" | "enter" => Token::Trigger(Trigger::Return),
        f if f.starts_with('f') && f.len() > 1 && f[1..].bytes().all(|b| b.is_ascii_digit()) => {
            let n: u8 = f[1..]
                .parse()
                .map_err(|_| HotkeyError::InvalidShortcut(token.to_string()))?;
            if !(1..=24).contains(&n) {
                return Err(HotkeyError::InvalidShortcut(token.to_string()));
            }
            Token::Trigger(Trigger::F(n))
        }
        one if one.chars().count() == 1 => {
            let ch = one
                .chars()
                .next()
                .ok_or_else(|| HotkeyError::InvalidShortcut(token.to_string()))?;
            if ch.is_ascii_lowercase() || ch.is_ascii_digit() {
                Token::Trigger(Trigger::Char(ch))
            } else {
                return Err(HotkeyError::InvalidShortcut(token.to_string()));
            }
        }
        _ => return Err(HotkeyError::InvalidShortcut(token.to_string())),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_ctrl_space() {
        let shortcut = Shortcut::parse("Ctrl+Space").unwrap();
        assert_eq!(
            shortcut,
            Shortcut {
                ctrl: true,
                alt: false,
                shift: false,
                super_key: false,
                fn_key: false,
                trigger: Trigger::Space,
            }
        );
    }

    #[test]
    fn parse_fn_alone_and_as_modifier() {
        let fn_only = Shortcut::parse("Fn").unwrap();
        assert_eq!(fn_only.trigger, Trigger::Fn);
        assert!(!fn_only.fn_key);

        let fn_space = Shortcut::parse("fn+space").unwrap();
        assert!(fn_space.fn_key);
        assert_eq!(fn_space.trigger, Trigger::Space);
    }

    #[test]
    fn parse_rejects_empty_and_two_triggers() {
        assert!(Shortcut::parse("").is_err());
        assert!(Shortcut::parse("Ctrl+").is_err());
        assert!(Shortcut::parse("Ctrl+Space+A").is_err());
        assert!(Shortcut::parse("Ctrl+Alt").is_err());
    }

    #[test]
    fn parse_f9_and_letter() {
        let shortcut = Shortcut::parse("Ctrl+Shift+F9").unwrap();
        assert!(shortcut.ctrl && shortcut.shift);
        assert_eq!(shortcut.trigger, Trigger::F(9));
        assert_eq!(
            Shortcut::parse("Alt+a").unwrap().trigger,
            Trigger::Char('a')
        );
    }
}
