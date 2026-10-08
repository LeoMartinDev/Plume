#[cfg(not(windows))]
use plume_core::BoxError;
#[cfg(target_os = "macos")]
mod native {
    use super::*;
    use core_foundation::{base::TCFType, string::CFString};
    use objc::runtime::Object;
    use objc::{class, msg_send, sel, sel_impl};
    type Id = *mut Object;
    struct Pool(Id);
    impl Pool {
        fn new() -> Self {
            unsafe { Self(msg_send![class!(NSAutoreleasePool), new]) }
        }
    }
    impl Drop for Pool {
        fn drop(&mut self) {
            unsafe {
                let _: () = msg_send![self.0, drain];
            }
        }
    }
    unsafe fn board() -> Id {
        msg_send![class!(NSPasteboard), generalPasteboard]
    }
    pub struct Snapshot(Vec<(String, Vec<u8>)>);
    pub fn version() -> Result<u64, BoxError> {
        unsafe {
            let count: isize = msg_send![board(), changeCount];
            Ok(count as u64)
        }
    }
    impl Snapshot {
        pub fn capture() -> Result<Self, BoxError> {
            let _pool = Pool::new();
            unsafe {
                let board = board();
                let start = version()?;
                let items: Id = msg_send![board, pasteboardItems];
                let count: usize = if items.is_null() {
                    0
                } else {
                    msg_send![items, count]
                };
                if count > 1 {
                    return Err("multiple clipboard items cannot be preserved".into());
                }
                let mut result = Vec::new();
                if count == 1 {
                    let item: Id = msg_send![items,objectAtIndex:0usize];
                    let types: Id = msg_send![item, types];
                    let count: usize = msg_send![types, count];
                    for index in 0..count {
                        let kind: Id = msg_send![types,objectAtIndex:index];
                        let name = CFString::wrap_under_get_rule(kind.cast()).to_string();
                        let data: Id = msg_send![item,dataForType:kind];
                        if data.is_null() {
                            return Err("clipboard contains an unreadable representation".into());
                        }
                        let len: usize = msg_send![data, length];
                        let bytes: *const u8 = msg_send![data, bytes];
                        if len > 64 * 1024 * 1024 {
                            return Err("clipboard representation too large to preserve".into());
                        }
                        let bytes = if len == 0 {
                            Vec::new()
                        } else {
                            std::slice::from_raw_parts(bytes, len).to_vec()
                        };
                        result.push((name, bytes));
                    }
                }
                if start != version()? {
                    return Err("clipboard changed during snapshot".into());
                }
                Ok(Self(result))
            }
        }
        pub fn restore(self) -> Result<(), BoxError> {
            let _pool = Pool::new();
            unsafe {
                let board = board();
                let _: isize = msg_send![board, clearContents];
                let types: Id = msg_send![class!(NSMutableArray), array];
                for (name, _) in &self.0 {
                    let kind = CFString::new(name);
                    let _: () = msg_send![types,addObject:kind.as_CFTypeRef()];
                }
                let _: isize =
                    msg_send![board,declareTypes:types owner:std::ptr::null_mut::<Object>()];
                for (name, bytes) in self.0 {
                    let kind = CFString::new(&name);
                    let data: Id =
                        msg_send![class!(NSData),dataWithBytes:bytes.as_ptr() length:bytes.len()];
                    let success: objc::runtime::BOOL =
                        msg_send![board,setData:data forType:kind.as_CFTypeRef()];
                    if success == objc::runtime::NO {
                        return Err("clipboard restoration failed".into());
                    }
                }
                Ok(())
            }
        }
    }
}
#[cfg(target_os = "linux")]
mod native {
    use super::*;
    pub enum Snapshot {
        Empty,
        Text(String),
        Html(String, String),
        Image(arboard::ImageData<'static>),
    }
    pub fn version() -> Result<u64, BoxError> {
        #[cfg(windows)]
        {
            Ok(
                unsafe { windows_sys::Win32::System::DataExchange::GetClipboardSequenceNumber() }
                    as u64,
            )
        }
        #[cfg(target_os = "linux")]
        {
            crate::x11::clipboard_version()
        }
    }
    impl Snapshot {
        pub fn capture() -> Result<Self, BoxError> {
            let initial = version()?;
            if !crate::x11::clipboard_formats_preservable()? {
                return Err("clipboard formats cannot be preserved".into());
            }
            let mut clipboard = arboard::Clipboard::new()?;
            let result = if let Ok(image) = clipboard.get_image() {
                Self::Image(image.to_owned())
            } else if let Ok(text) = clipboard.get_text() {
                match clipboard.get().html() {
                    Ok(html) => Self::Html(html, text),
                    Err(_) => Self::Text(text),
                }
            } else if empty()? {
                Self::Empty
            } else {
                return Err("clipboard contents cannot be preserved".into());
            };
            if initial != version()? {
                return Err("clipboard changed during snapshot".into());
            }
            Ok(result)
        }
        pub fn restore(self) -> Result<(), BoxError> {
            let mut clipboard = arboard::Clipboard::new()?;
            match self {
                Self::Empty => clipboard.clear()?,
                Self::Text(text) => clipboard.set_text(text)?,
                Self::Html(html, text) => clipboard.set_html(html, Some(text))?,
                Self::Image(image) => clipboard.set_image(image)?,
            };
            Ok(())
        }
    }
    fn empty() -> Result<bool, BoxError> {
        #[cfg(windows)]
        {
            Ok(unsafe { windows_sys::Win32::System::DataExchange::CountClipboardFormats() } == 0)
        }
        #[cfg(target_os = "linux")]
        {
            crate::x11::clipboard_empty()
        }
    }
}
#[cfg(not(windows))]
pub use native::{version, Snapshot};
#[cfg(windows)]
#[path = "clipboard_windows.rs"]
mod windows;
#[cfg(windows)]
pub use windows::{version, Snapshot};

#[cfg(all(test, target_os = "macos"))]
mod native_tests {
    use super::*;
    #[test]
    #[ignore = "changes the system clipboard; run explicitly in native validation"]
    fn preserves_empty_text_html_and_image() {
        let original = Snapshot::capture().unwrap();
        let mut clipboard = arboard::Clipboard::new().unwrap();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            clipboard.clear().unwrap();
            let empty = Snapshot::capture().unwrap();
            clipboard.set_text("temporary").unwrap();
            empty.restore().unwrap();
            assert!(clipboard.get_text().is_err());
            clipboard.set_text("original text").unwrap();
            let text = Snapshot::capture().unwrap();
            clipboard.set_text("temporary").unwrap();
            text.restore().unwrap();
            assert_eq!(clipboard.get_text().unwrap(), "original text");
            clipboard.set_html("<b>hello</b>", Some("hello")).unwrap();
            let html = Snapshot::capture().unwrap();
            clipboard.set_text("temporary").unwrap();
            html.restore().unwrap();
            assert_eq!(clipboard.get_text().unwrap(), "hello");
            assert!(clipboard.get().html().unwrap().contains("<b>hello</b>"));
            clipboard
                .set_image(arboard::ImageData {
                    width: 1,
                    height: 1,
                    bytes: std::borrow::Cow::Owned(vec![100, 150, 200, 255]),
                })
                .unwrap();
            let image = Snapshot::capture().unwrap();
            clipboard.set_text("temporary").unwrap();
            image.restore().unwrap();
            assert_eq!(
                clipboard.get_image().unwrap().bytes.as_ref(),
                &[100, 150, 200, 255]
            );
        }));
        original.restore().unwrap();
        if let Err(error) = result {
            std::panic::resume_unwind(error);
        }
    }
}
