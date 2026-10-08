use crate::ops::Stroke;

#[cfg(target_os = "windows")]
use crate::ops::{insert_strokes, replace_strokes};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WinStroke {
    UnicodeDown(u16),
    UnicodeUp(u16),
    BackspaceDown,
    BackspaceUp,
    ReturnDown,
    ReturnUp,
    TabDown,
    TabUp,
    #[cfg(target_os = "windows")]
    ControlDown,
    #[cfg(target_os = "windows")]
    ControlUp,
    #[cfg(target_os = "windows")]
    VDown,
    #[cfg(target_os = "windows")]
    VUp,
}

pub(crate) fn win_strokes(strokes: &[Stroke]) -> Vec<WinStroke> {
    let mut out = Vec::new();
    for stroke in strokes {
        match *stroke {
            Stroke::Backspace => {
                out.push(WinStroke::BackspaceDown);
                out.push(WinStroke::BackspaceUp);
            }
            Stroke::Return => {
                out.push(WinStroke::ReturnDown);
                out.push(WinStroke::ReturnUp);
            }
            Stroke::Tab => {
                out.push(WinStroke::TabDown);
                out.push(WinStroke::TabUp);
            }
            Stroke::Char(c) => {
                let mut buf = [0u16; 2];
                for &unit in &*c.encode_utf16(&mut buf) {
                    out.push(WinStroke::UnicodeDown(unit));
                    out.push(WinStroke::UnicodeUp(unit));
                }
            }
        }
    }
    out
}

#[cfg(target_os = "windows")]
mod sys {
    use std::mem::size_of;
    use std::path::Path;

    use plume_core::BoxError;
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED,
    };
    use windows::Win32::UI::Accessibility::{
        CUIAutomation, IUIAutomation, IUIAutomationValuePattern, UIA_DocumentControlTypeId,
        UIA_EditControlTypeId, UIA_ValuePatternId,
    };
    use windows_sys::Win32::Foundation::{CloseHandle, RECT};
    use windows_sys::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, KEYEVENTF_UNICODE,
        VK_BACK, VK_CONTROL, VK_RETURN, VK_TAB, VK_V,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetClassNameW, GetForegroundWindow, GetGUIThreadInfo, GetWindowLongW,
        GetWindowThreadProcessId, GUITHREADINFO, GWL_STYLE,
    };

    use super::WinStroke;
    use crate::InjectError;

    pub(super) fn post(strokes: &[WinStroke]) -> Result<(), BoxError> {
        if strokes.is_empty() {
            return Ok(());
        }
        let inputs: Vec<INPUT> = strokes.iter().copied().map(to_input).collect();
        let sent = unsafe {
            SendInput(
                inputs.len() as u32,
                inputs.as_ptr(),
                size_of::<INPUT>() as i32,
            )
        };
        if sent as usize != inputs.len() {
            return Err(InjectError::Message(format!(
                "SendInput posted {sent} of {} events",
                inputs.len()
            ))
            .into());
        }
        Ok(())
    }

    fn to_input(stroke: WinStroke) -> INPUT {
        let (vk, scan, flags) = match stroke {
            WinStroke::UnicodeDown(unit) => (0, unit, KEYEVENTF_UNICODE),
            WinStroke::UnicodeUp(unit) => (0, unit, KEYEVENTF_UNICODE | KEYEVENTF_KEYUP),
            WinStroke::BackspaceDown => (VK_BACK, 0, 0),
            WinStroke::BackspaceUp => (VK_BACK, 0, KEYEVENTF_KEYUP),
            WinStroke::ReturnDown => (VK_RETURN, 0, 0),
            WinStroke::ReturnUp => (VK_RETURN, 0, KEYEVENTF_KEYUP),
            WinStroke::TabDown => (VK_TAB, 0, 0),
            WinStroke::TabUp => (VK_TAB, 0, KEYEVENTF_KEYUP),
            WinStroke::ControlDown => (VK_CONTROL, 0, 0),
            WinStroke::ControlUp => (VK_CONTROL, 0, KEYEVENTF_KEYUP),
            WinStroke::VDown => (VK_V, 0, 0),
            WinStroke::VUp => (VK_V, 0, KEYEVENTF_KEYUP),
        };
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: vk,
                    wScan: scan,
                    dwFlags: flags,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        }
    }

    pub(super) fn target_info() -> (Option<String>, plume_core::TargetAssessment) {
        let native = native_target_info();
        if native.1 != plume_core::TargetAssessment::Unknown {
            return native;
        }
        if let Some(info) = uia_target_info() {
            return info;
        }

        native
    }

    /// UI Automation covers browser, Electron, Office and other non-native editors.
    /// It is intentionally limited to metadata: no text or window title is read.
    fn uia_target_info() -> Option<(Option<String>, plume_core::TargetAssessment)> {
        unsafe {
            // UI Automation works in either apartment. RPC_E_CHANGED_MODE only means that the
            // caller already initialized this thread with another apartment model.
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            let automation: IUIAutomation =
                CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER).ok()?;
            let element = automation.GetFocusedElement().ok()?;
            let process_id = element.CurrentProcessId().ok()? as u32;
            let application = process_name(process_id);
            let read_only = element
                .GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId)
                .ok()
                .and_then(|pattern| pattern.CurrentIsReadOnly().ok())
                .is_some_and(|read_only| read_only.as_bool());

            let target = if element.CurrentIsPassword().ok()?.as_bool() {
                plume_core::TargetAssessment::Sensitive
            } else if !element.CurrentIsEnabled().ok()?.as_bool() || read_only {
                plume_core::TargetAssessment::NonEditable
            } else {
                let control_type = element.CurrentControlType().ok()?;
                let focusable = element.CurrentIsKeyboardFocusable().ok()?.as_bool();
                if focusable
                    && (control_type == UIA_EditControlTypeId
                        || control_type == UIA_DocumentControlTypeId)
                {
                    plume_core::TargetAssessment::Editable
                } else {
                    plume_core::TargetAssessment::Unknown
                }
            };
            Some((application, target))
        }
    }

    /// Classic Win32 fallback. Unlike UI Automation, this exposes the read-only style on native
    /// edit controls, so keep it even when UIA is available.
    fn native_target_info() -> (Option<String>, plume_core::TargetAssessment) {
        let foreground = unsafe { GetForegroundWindow() };
        if foreground.is_null() {
            return (None, plume_core::TargetAssessment::Unknown);
        }

        let mut process_id = 0;
        let thread_id = unsafe { GetWindowThreadProcessId(foreground, &mut process_id) };
        let application = process_name(process_id);
        let mut info = GUITHREADINFO {
            cbSize: size_of::<GUITHREADINFO>() as u32,
            flags: 0,
            hwndActive: std::ptr::null_mut(),
            hwndFocus: std::ptr::null_mut(),
            hwndCapture: std::ptr::null_mut(),
            hwndMenuOwner: std::ptr::null_mut(),
            hwndMoveSize: std::ptr::null_mut(),
            hwndCaret: std::ptr::null_mut(),
            rcCaret: RECT {
                left: 0,
                top: 0,
                right: 0,
                bottom: 0,
            },
        };
        if unsafe { GetGUIThreadInfo(thread_id, &mut info) } == 0 || info.hwndFocus.is_null() {
            return (application, plume_core::TargetAssessment::Unknown);
        }

        let mut class = [0u16; 128];
        let len = unsafe { GetClassNameW(info.hwndFocus, class.as_mut_ptr(), class.len() as i32) };
        if len <= 0 {
            return (application, plume_core::TargetAssessment::Unknown);
        }
        let class = String::from_utf16_lossy(&class[..len as usize]).to_ascii_lowercase();
        if !class.contains("edit") {
            return (application, plume_core::TargetAssessment::Unknown);
        }

        let style = unsafe { GetWindowLongW(info.hwndFocus, GWL_STYLE) } as u32;
        const ES_PASSWORD: u32 = 0x0020;
        const ES_READONLY: u32 = 0x0800;
        let target = if style & ES_PASSWORD != 0 {
            plume_core::TargetAssessment::Sensitive
        } else if style & ES_READONLY != 0 {
            plume_core::TargetAssessment::NonEditable
        } else {
            plume_core::TargetAssessment::Editable
        };
        (application, target)
    }

    fn process_name(process_id: u32) -> Option<String> {
        let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, process_id) };
        if handle.is_null() {
            return None;
        }
        let mut path = vec![0u16; 1024];
        let mut len = path.len() as u32;
        let ok = unsafe { QueryFullProcessImageNameW(handle, 0, path.as_mut_ptr(), &mut len) };
        unsafe { CloseHandle(handle) };
        if ok == 0 {
            return None;
        }
        Path::new(&String::from_utf16_lossy(&path[..len as usize]))
            .file_stem()
            .map(|name| name.to_string_lossy().into_owned())
    }
}

#[cfg(target_os = "windows")]
pub(crate) struct WinInjector;

#[cfg(target_os = "windows")]
impl WinInjector {
    pub(crate) fn paste(&mut self) -> Result<(), plume_core::BoxError> {
        sys::post(&[
            WinStroke::ControlDown,
            WinStroke::VDown,
            WinStroke::VUp,
            WinStroke::ControlUp,
        ])
    }

    pub(crate) fn target_info(&self) -> (Option<String>, plume_core::TargetAssessment) {
        sys::target_info()
    }
}

#[cfg(target_os = "windows")]
impl plume_core::TextInjector for WinInjector {
    fn insert(&mut self, text: &str) -> Result<(), plume_core::BoxError> {
        sys::post(&win_strokes(&insert_strokes(text)))
    }

    fn replace_last(&mut self, old: &str, new: &str) -> Result<(), plume_core::BoxError> {
        sys::post(&win_strokes(&replace_strokes(old, new)))
    }
}

#[cfg(windows)]
pub(crate) fn field_context() -> Option<plume_core::FieldContext> {
    use windows::Win32::System::Ole::{
        SafeArrayDestroy, SafeArrayGetElement, SafeArrayGetLBound, SafeArrayGetUBound,
    };
    use windows::Win32::{
        System::Com::{
            CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED,
        },
        UI::Accessibility::*,
    };
    unsafe {
        if sys::target_info().1 != plume_core::TargetAssessment::Editable {
            return None;
        }
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let automation: IUIAutomation =
            CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER).ok()?;
        let element = automation.GetFocusedElement().ok()?;
        if element.CurrentIsPassword().ok()?.as_bool() {
            return None;
        }
        let pattern = element
            .GetCurrentPatternAs::<IUIAutomationTextPattern>(UIA_TextPatternId)
            .ok()?;
        let selections = pattern.GetSelection().ok()?;
        if selections.Length().ok()? != 1 {
            return None;
        }
        let selection = selections.GetElement(0).ok()?;
        let has_selection = selection
            .CompareEndpoints(
                TextPatternRangeEndpoint_Start,
                &selection,
                TextPatternRangeEndpoint_End,
            )
            .ok()?
            != 0;
        let before = selection.Clone().ok()?;
        before
            .MoveEndpointByRange(
                TextPatternRangeEndpoint_End,
                &selection,
                TextPatternRangeEndpoint_Start,
            )
            .ok()?;
        before
            .MoveEndpointByUnit(TextPatternRangeEndpoint_Start, TextUnit_Character, -1)
            .ok()?;
        let before = before.GetText(8).ok()?.to_string().chars().next_back();
        let after = selection.Clone().ok()?;
        after
            .MoveEndpointByRange(
                TextPatternRangeEndpoint_Start,
                &selection,
                TextPatternRangeEndpoint_End,
            )
            .ok()?;
        after
            .MoveEndpointByUnit(TextPatternRangeEndpoint_End, TextUnit_Character, 1)
            .ok()?;
        let after = after.GetText(8).ok()?.to_string().chars().next();
        let ids = element.GetRuntimeId().ok()?;
        if ids.is_null() {
            return None;
        }
        let identity = (|| {
            let low = SafeArrayGetLBound(ids, 1).ok()?;
            let high = SafeArrayGetUBound(ids, 1).ok()?;
            if high - low > 64 {
                return None;
            }
            let mut result = Vec::new();
            for index in low..=high {
                let mut value = 0i32;
                SafeArrayGetElement(ids, &index, (&mut value as *mut i32).cast()).ok()?;
                result.push(value);
            }
            Some(format!("{}:{result:?}", element.CurrentProcessId().ok()?))
        })();
        let _ = SafeArrayDestroy(ids);
        Some(plume_core::FieldContext {
            before,
            after,
            has_selection,
            target_id: identity?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{win_strokes, WinStroke};
    use crate::ops::{insert_strokes, replace_strokes};

    #[test]
    fn insert_emits_unicode_units_down_then_up() {
        assert_eq!(
            win_strokes(&insert_strokes("hi")),
            vec![
                WinStroke::UnicodeDown(u16::from(b'h')),
                WinStroke::UnicodeUp(u16::from(b'h')),
                WinStroke::UnicodeDown(u16::from(b'i')),
                WinStroke::UnicodeUp(u16::from(b'i')),
            ]
        );
    }

    #[test]
    fn insert_splits_supplementary_plane_into_surrogate_pair() {
        // U+1F600 😀 is one char, two UTF-16 units.
        let strokes = win_strokes(&insert_strokes("\u{1F600}"));
        assert_eq!(
            strokes,
            vec![
                WinStroke::UnicodeDown(0xD83D),
                WinStroke::UnicodeUp(0xD83D),
                WinStroke::UnicodeDown(0xDE00),
                WinStroke::UnicodeUp(0xDE00),
            ]
        );
    }

    #[test]
    fn replace_last_emits_backspaces_then_retype() {
        assert_eq!(
            win_strokes(&replace_strokes("hi", "o")),
            vec![
                WinStroke::BackspaceDown,
                WinStroke::BackspaceUp,
                WinStroke::BackspaceDown,
                WinStroke::BackspaceUp,
                WinStroke::UnicodeDown(u16::from(b'o')),
                WinStroke::UnicodeUp(u16::from(b'o')),
            ]
        );
    }

    #[test]
    fn newline_is_return_not_unicode() {
        assert_eq!(
            win_strokes(&insert_strokes("\n")),
            vec![WinStroke::ReturnDown, WinStroke::ReturnUp]
        );
    }
}
