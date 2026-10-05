# Platform matrix

One row per platform that ships. Each shipping row names the mechanism for text injection, the global hotkey, and the overlay. Linux ships on X11 only: the session must export `DISPLAY`. A Wayland-only session is refused at startup.

| Platform | Text injection | Global hotkey | Overlay | Tray |
| --- | --- | --- | --- | --- |
| macOS | `CGEvent` keyboard events with Unicode text, clipboard paste as fallback | Quartz event tap reports key down and key up | Borderless non-activating `NSWindow` at floating level | AppKit `NSStatusItem` |
| Windows | `SendInput` with `KEYEVENTF_UNICODE`, clipboard paste as fallback | Low-level keyboard hook `WH_KEYBOARD_LL` reports key down and key up | Topmost layered window with `WS_EX_NOACTIVATE` | Notification area via `tray-icon` |
| Linux X11 | XTEST fake key events, clipboard paste as fallback | XRecord extension reports key down and key up | Override-redirect window | D-Bus StatusNotifierItem via `ksni` |

## Wayland

Wayland is not a supported platform yet. `stt-hotkey` and `stt-inject` refuse a Wayland-only session before opening a backend. The mechanisms below stay candidates for a later port.

| Platform | Text injection | Global hotkey | Overlay |
| --- | --- | --- | --- |
| Linux Wayland | not shipped (`zwp_virtual_keyboard_v1` is the candidate) | not shipped (XDG GlobalShortcuts is the candidate) | not shipped (`zwlr_layer_shell_v1` is the candidate) |

## Notes

- macOS requires accessibility permission for the event tap and for injection.
- A Linux session with `DISPLAY` set uses the X11 row, including when `WAYLAND_DISPLAY` is also set.
- A Linux session with only `WAYLAND_DISPLAY` fails with the requirement for an X11 `DISPLAY`.
- The clipboard fallback pastes with the target app's paste shortcut. PR-4 specifies when each fallback applies.
- Each shipping mechanism maps onto the `TextInjector` and `GlobalHotkey` traits from `stt-core`. PR-4 owns that mapping.
