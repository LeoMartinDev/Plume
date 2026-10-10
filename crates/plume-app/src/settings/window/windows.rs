use gpui::{App, Pixels, Size, Window};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use windows_sys::Win32::{
    Foundation::{HWND, RECT},
    Graphics::Gdi::{GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST},
    UI::{
        HiDpi::{AdjustWindowRectExForDpi, GetDpiForWindow},
        WindowsAndMessaging::{
            GetWindowLongW, IsWindow, SetWindowLongW, SetWindowPos, GWL_EXSTYLE, GWL_STYLE,
            SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOZORDER, WS_MAXIMIZEBOX, WS_THICKFRAME,
        },
    },
};

pub(super) fn set_fixed_size(window: &Window, dimensions: Size<Pixels>, cx: &App) {
    let Ok(handle) = HasWindowHandle::window_handle(window) else {
        return;
    };
    let RawWindowHandle::Win32(handle) = handle.as_raw() else {
        return;
    };
    let hwnd = handle.hwnd.get();
    let width = f32::from(dimensions.width);
    let height = f32::from(dimensions.height);
    // GPUI 0.2.2 defers placement for show:false windows until activate().
    // Our tray uses native visibility instead, so initialize their geometry
    // explicitly. Defer synchronous WM_SIZE callbacks until App is unborrowed.
    cx.foreground_executor()
        .spawn(async move {
            // SAFETY: dispatched on the owning UI thread, with HWND validity
            // checked before accessing the window.
            unsafe { set_native_fixed_size(hwnd as HWND, width, height) };
        })
        .detach();
}

unsafe fn set_native_fixed_size(hwnd: HWND, width: f32, height: f32) {
    if IsWindow(hwnd) == 0 {
        return;
    }
    let dpi = GetDpiForWindow(hwnd);
    let scale = dpi as f32 / 96.;
    let style = GetWindowLongW(hwnd, GWL_STYLE) as u32 & !(WS_THICKFRAME | WS_MAXIMIZEBOX);
    let ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;
    let mut rect = RECT {
        left: 0,
        top: 0,
        right: (width * scale).round() as i32,
        bottom: (height * scale).round() as i32,
    };
    if AdjustWindowRectExForDpi(&mut rect, style, 0, ex_style, dpi) == 0 {
        tracing::warn!("plume-app: cannot calculate fixed window size");
        return;
    }
    let mut monitor: MONITORINFO = std::mem::zeroed();
    monitor.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
    if GetMonitorInfoW(
        MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST),
        &mut monitor,
    ) == 0
    {
        tracing::warn!("plume-app: cannot locate settings display");
        return;
    }
    let width = rect.right - rect.left;
    let height = rect.bottom - rect.top;
    let work = monitor.rcWork;
    SetWindowLongW(hwnd, GWL_STYLE, style as i32);
    if SetWindowPos(
        hwnd,
        std::ptr::null_mut(),
        work.left + ((work.right - work.left - width) / 2).max(0),
        work.top + ((work.bottom - work.top - height) / 2).max(0),
        width,
        height,
        SWP_NOACTIVATE | SWP_NOZORDER | SWP_FRAMECHANGED,
    ) == 0
    {
        tracing::warn!("plume-app: cannot initialize fixed settings window");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DestroyWindow, GetClientRect, IsWindowVisible, ShowWindow, CW_USEDEFAULT,
        SW_HIDE, SW_SHOW, WS_OVERLAPPEDWINDOW,
    };

    #[test]
    fn hidden_window_has_fixed_client_size_before_native_tray_show() {
        struct TestWindow(HWND);
        impl Drop for TestWindow {
            fn drop(&mut self) {
                // SAFETY: this test owns the window on the creating thread.
                unsafe { DestroyWindow(self.0) };
            }
        }
        // SAFETY: STATIC is a built-in window class; all calls and cleanup run
        // on the creating thread. No application window is affected.
        unsafe {
            let window = TestWindow(CreateWindowExW(
                0,
                windows_sys::w!("STATIC"),
                windows_sys::w!("Plume fixed settings regression"),
                WS_OVERLAPPEDWINDOW,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null(),
            ));
            assert!(!window.0.is_null());
            for height in [450., 410.] {
                set_native_fixed_size(window.0, 720., height);
                assert_eq!(IsWindowVisible(window.0), 0);
                let scale = GetDpiForWindow(window.0) as f32 / 96.;
                for _ in 0..2 {
                    ShowWindow(window.0, SW_SHOW);
                    let mut client: RECT = std::mem::zeroed();
                    assert_ne!(GetClientRect(window.0, &mut client), 0);
                    assert_eq!(client.right - client.left, (720. * scale).round() as i32);
                    assert_eq!(client.bottom - client.top, (height * scale).round() as i32);
                    assert_eq!(
                        GetWindowLongW(window.0, GWL_STYLE) as u32
                            & (WS_THICKFRAME | WS_MAXIMIZEBOX),
                        0
                    );
                    ShowWindow(window.0, SW_HIDE);
                }
            }
        }
    }
}
