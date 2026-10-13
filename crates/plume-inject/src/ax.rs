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
    fn AXUIElementCreateApplication(pid: i32) -> Element;
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
#[link(name = "AppKit", kind = "framework")]
extern "C" {}
struct Owned(CFTypeRef);
impl Drop for Owned {
    fn drop(&mut self) {
        unsafe { CFRelease(self.0) }
    }
}
fn attr(element: Element, name: &str) -> Option<Owned> {
    let name = CFString::new(name);
    let mut value = std::ptr::null();
    // Lazy on purpose: an eager Owned(null) would be dropped, and CFRelease(NULL) aborts.
    (unsafe { AXUIElementCopyAttributeValue(element, name.as_CFTypeRef(), &mut value) } == 0
        && !value.is_null())
    .then(|| Owned(value))
}
fn string(value: &Owned) -> Option<String> {
    unsafe {
        (CFGetTypeID(value.0) == CFStringGetTypeID())
            .then(|| CFString::wrap_under_get_rule(value.0.cast()).to_string())
    }
}
/// The frontmost app's own Accessibility element and its name. Some Macs
/// answer every system-wide focus query with kAXErrorCannotComplete while the
/// app itself still answers, so focus is asked of the app as a fallback.
fn frontmost_app() -> Option<(Owned, Option<String>)> {
    use objc::{class, msg_send, runtime::Object, sel, sel_impl};
    unsafe {
        let pool: *mut Object = msg_send![class!(NSAutoreleasePool), new];
        let workspace: *mut Object = msg_send![class!(NSWorkspace), sharedWorkspace];
        let app: *mut Object = msg_send![workspace, frontmostApplication];
        let found = (!app.is_null()).then(|| {
            let pid: i32 = msg_send![app, processIdentifier];
            let name: *mut Object = msg_send![app, localizedName];
            let name = (!name.is_null())
                .then(|| CFString::wrap_under_get_rule(name.cast_const().cast()).to_string());
            (Owned(AXUIElementCreateApplication(pid)), name)
        });
        let _: () = msg_send![pool, drain];
        found
    }
}
fn focused_in(app: Option<&Owned>) -> Option<Owned> {
    let system = Owned(unsafe { AXUIElementCreateSystemWide() });
    attr(system.0, "AXFocusedUIElement").or_else(|| attr(app?.0, "AXFocusedUIElement"))
}
fn focused() -> Option<Owned> {
    focused_in(frontmost_app().as_ref().map(|(app, _)| app))
}
pub(crate) fn target() -> (Option<String>, TargetAssessment) {
    let (app, name) = frontmost_app().unzip();
    let name = name.flatten();
    if let Some(element) = focused_in(app.as_ref()) {
        return (name, assessment(&element));
    }
    // Chromium and Electron apps (Claude, Slack, VS Code) may expose no focused
    // element until an assistive app turns their tree on, so a focused window
    // is the best sign left that the user is typing somewhere. No window at
    // all: the desktop, or an app without a window.
    let window = app.and_then(|app| attr(app.0, "AXFocusedWindow"));
    let target = if window.is_some() {
        TargetAssessment::Unknown
    } else {
        TargetAssessment::NoFocus
    };
    (name, target)
}
/// Roles that never take typed or pasted text. Web areas, groups and custom
/// views stay Unknown: terminals and editors draw their text there.
fn never_text(role: &str) -> bool {
    matches!(
        role,
        "AXButton"
            | "AXCheckBox"
            | "AXRadioButton"
            | "AXLink"
            | "AXMenu"
            | "AXMenuBar"
            | "AXMenuBarItem"
            | "AXMenuItem"
            | "AXMenuButton"
            | "AXPopUpButton"
            | "AXImage"
            | "AXTabGroup"
            | "AXToolbar"
            | "AXScrollBar"
            | "AXSlider"
            | "AXList"
            | "AXOutline"
            | "AXBrowser"
            | "AXRow"
            | "AXDisclosureTriangle"
    )
}
fn assessment(element: &Owned) -> TargetAssessment {
    let role = attr(element.0, "AXRole").and_then(|v| string(&v));
    let subrole = attr(element.0, "AXSubrole").and_then(|v| string(&v));
    if subrole.as_deref() == Some("AXSecureTextField") {
        TargetAssessment::Sensitive
    } else if role.as_deref().is_some_and(never_text) {
        TargetAssessment::NoFocus
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
