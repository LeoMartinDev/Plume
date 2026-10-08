//! Conservative Accessibility context. Failure or an unsupported editor is Unknown.
use core_foundation::{
    base::{CFGetTypeID, CFHash, CFRelease, CFTypeRef, TCFType},
    string::{CFString, CFStringGetTypeID},
};
use plume_core::{FieldContext, TargetAssessment};
use std::ffi::c_void;
type Element = *const c_void;
#[repr(C)]
struct Range {
    location: isize,
    length: isize,
}
#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXUIElementCreateSystemWide() -> Element;
    fn AXUIElementCopyAttributeValue(
        element: Element,
        attribute: CFTypeRef,
        value: *mut CFTypeRef,
    ) -> i32;
    fn AXUIElementIsAttributeSettable(
        element: Element,
        attribute: CFTypeRef,
        settable: *mut u8,
    ) -> i32;
    fn AXUIElementGetPid(element: Element, pid: *mut i32) -> i32;
    fn AXValueGetValue(value: CFTypeRef, kind: i32, output: *mut c_void) -> u8;
    fn AXValueGetTypeID() -> usize;
}
struct Owned(CFTypeRef);
impl Drop for Owned {
    fn drop(&mut self) {
        unsafe { CFRelease(self.0) }
    }
}
fn attr(element: Element, name: &str) -> Option<Owned> {
    let name = CFString::new(name);
    let mut value = std::ptr::null();
    (unsafe { AXUIElementCopyAttributeValue(element, name.as_CFTypeRef(), &mut value) } == 0
        && !value.is_null())
    .then_some(Owned(value))
}
fn string(value: &Owned) -> Option<String> {
    unsafe {
        (CFGetTypeID(value.0) == CFStringGetTypeID())
            .then(|| CFString::wrap_under_get_rule(value.0.cast()).to_string())
    }
}
fn focused() -> Option<Owned> {
    let system = Owned(unsafe { AXUIElementCreateSystemWide() });
    attr(system.0, "AXFocusedUIElement")
}
pub(crate) fn target() -> (Option<String>, TargetAssessment) {
    let Some(element) = focused() else {
        return (None, TargetAssessment::Unknown);
    };
    (None, assessment(&element))
}
fn assessment(element: &Owned) -> TargetAssessment {
    let role = attr(element.0, "AXRole").and_then(|v| string(&v));
    let subrole = attr(element.0, "AXSubrole").and_then(|v| string(&v));
    if subrole.as_deref() == Some("AXSecureTextField") {
        TargetAssessment::Sensitive
    } else if matches!(
        role.as_deref(),
        Some("AXTextField" | "AXTextArea" | "AXComboBox")
    ) {
        let key = CFString::new("AXValue");
        let mut settable = 0;
        if unsafe { AXUIElementIsAttributeSettable(element.0, key.as_CFTypeRef(), &mut settable) }
            == 0
            && settable == 0
        {
            TargetAssessment::NonEditable
        } else {
            TargetAssessment::Editable
        }
    } else {
        TargetAssessment::Unknown
    }
}
pub(crate) fn context() -> Option<FieldContext> {
    let element = focused()?;
    if assessment(&element) != TargetAssessment::Editable {
        return None;
    }
    let value = attr(element.0, "AXValue")?;
    let text = string(&value)?;
    if text.len() > 1_000_000 {
        return None;
    }
    let selected = attr(element.0, "AXSelectedTextRange")?;
    if unsafe { CFGetTypeID(selected.0) } != unsafe { AXValueGetTypeID() } {
        return None;
    }
    let mut range = Range {
        location: 0,
        length: 0,
    };
    if unsafe { AXValueGetValue(selected.0, 3, (&mut range as *mut Range).cast()) } == 0
        || range.location < 0
        || range.length < 0
    {
        return None;
    }
    let units: Vec<u16> = text.encode_utf16().collect();
    let start = range.location as usize;
    let end = start.checked_add(range.length as usize)?;
    if end > units.len() {
        return None;
    }
    let before = String::from_utf16(&units[..start])
        .ok()?
        .chars()
        .next_back();
    let after = String::from_utf16(&units[end..]).ok()?.chars().next();
    let mut pid = 0;
    unsafe { AXUIElementGetPid(element.0, &mut pid) };
    Some(FieldContext {
        before,
        after,
        has_selection: range.length > 0,
        target_id: format!("{pid}:{}", unsafe { CFHash(element.0) }),
    })
}
