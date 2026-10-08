use plume_core::BoxError;
use std::ptr;
use windows_sys::Win32::Foundation::GlobalFree;
use windows_sys::Win32::{
    System::{DataExchange::*, Memory::*},
    UI::WindowsAndMessaging::*,
};
struct Lock;
impl Lock {
    fn open(window: windows_sys::Win32::Foundation::HWND) -> Result<Self, BoxError> {
        for _ in 0..5 {
            if unsafe { OpenClipboard(window) } != 0 {
                return Ok(Self);
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        Err("clipboard is locked by another application".into())
    }
}
impl Drop for Lock {
    fn drop(&mut self) {
        unsafe {
            CloseClipboard();
        }
    }
}
pub struct Snapshot(Vec<(u32, Vec<u8>)>);
pub fn version() -> Result<u64, BoxError> {
    Ok(unsafe { GetClipboardSequenceNumber() } as u64)
}
impl Snapshot {
    pub fn capture() -> Result<Self, BoxError> {
        let _lock = Lock::open(ptr::null_mut())?;
        let mut formats = Vec::new();
        let mut format = 0;
        loop {
            format = unsafe { EnumClipboardFormats(format) };
            if format == 0 {
                break;
            }
            // GDI handles are not HGLOBAL. DIB is sufficient to reconstruct a
            // bitmap; other non-memory formats require the typing fallback.
            if format == 2 {
                if unsafe { IsClipboardFormatAvailable(8) } == 0
                    && unsafe { IsClipboardFormatAvailable(17) } == 0
                {
                    return Err("bitmap clipboard has no preservable DIB".into());
                }
                continue;
            }
            if matches!(format, 3 | 9 | 14) {
                return Err("clipboard object format cannot be preserved".into());
            }
            let handle = unsafe { GetClipboardData(format) };
            if handle.is_null() {
                return Err("clipboard representation cannot be read".into());
            }
            let len = unsafe { GlobalSize(handle) };
            if len == 0 || len > 64 * 1024 * 1024 {
                return Err("clipboard representation cannot be preserved".into());
            }
            let data = unsafe { GlobalLock(handle) };
            if data.is_null() {
                return Err("clipboard representation cannot be locked".into());
            }
            let bytes = unsafe { std::slice::from_raw_parts(data.cast::<u8>(), len) }.to_vec();
            unsafe {
                GlobalUnlock(handle);
            }
            formats.push((format, bytes));
        }
        Ok(Self(formats))
    }
    pub fn restore_if_owned(self, token: u64) -> Result<(), BoxError> {
        // Give clipboard ownership to a window on this thread. NULL ownership
        // after EmptyClipboard does not permit SetClipboardData on Windows.
        let class: Vec<u16> = "STATIC\0".encode_utf16().collect();
        let window = unsafe {
            CreateWindowExW(
                0,
                class.as_ptr(),
                ptr::null(),
                0,
                0,
                0,
                0,
                0,
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null(),
            )
        };
        if window.is_null() {
            return Err("clipboard restoration window failed".into());
        }
        let result = (|| {
            let _lock = Lock::open(window)?;
            if version()? != token {
                return Ok(());
            }
            if unsafe { EmptyClipboard() } == 0 {
                return Err("clipboard clear failed".into());
            }
            for (format, bytes) in self.0 {
                let memory = unsafe { GlobalAlloc(GMEM_MOVEABLE, bytes.len()) };
                if memory.is_null() {
                    return Err("clipboard restoration allocation failed".into());
                }
                let data = unsafe { GlobalLock(memory) };
                if data.is_null() {
                    unsafe {
                        GlobalFree(memory);
                    }
                    return Err("clipboard restoration lock failed".into());
                }
                unsafe {
                    ptr::copy_nonoverlapping(bytes.as_ptr(), data.cast(), bytes.len());
                    GlobalUnlock(memory);
                }
                if unsafe { SetClipboardData(format, memory) }.is_null() {
                    unsafe {
                        GlobalFree(memory);
                    }
                    return Err("clipboard restoration failed".into());
                }
            }
            Ok(())
        })();
        unsafe {
            DestroyWindow(window);
        }
        result
    }
}
