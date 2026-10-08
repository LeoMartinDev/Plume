# Platform matrix

One row per platform that ships. Each shipping row names the mechanism for text injection, the global hotkey, and the overlay. Linux ships on X11 only: the session must export `DISPLAY`. A Wayland-only session is refused at startup.

| Platform | Text injection | Global hotkey | Overlay | Tray |
| --- | --- | --- | --- | --- |
| macOS | Clipboard paste with native snapshot/restore, or `CGEvent` Unicode typing; AX context | One Quartz event tap reports all action edges | Borderless non-activating `NSWindow` at floating level | AppKit `NSStatusItem` |
| Windows | Clipboard paste with native snapshot/restore, or `SendInput` Unicode typing; UI Automation context | One `WH_KEYBOARD_LL` hook reports all action edges, including Esc | Topmost layered window with `WS_EX_NOACTIVATE` | Notification area via `tray-icon` |
| Linux X11 | Clipboard paste with snapshot/XFixes ownership, or XTEST typing; unknown context | One connection with passive grabs and XKB detectable autorepeat | Override-redirect window | D-Bus StatusNotifierItem via `ksni` |

## Wayland

Wayland is not a supported platform yet. `plume-hotkey` and `plume-inject` refuse a Wayland-only session before opening a backend. The mechanisms below stay candidates for a later port.

| Platform | Text injection | Global hotkey | Overlay |
| --- | --- | --- | --- |
| Linux Wayland | not shipped (`zwp_virtual_keyboard_v1` is the candidate) | not shipped (XDG GlobalShortcuts is the candidate) | not shipped (`zwlr_layer_shell_v1` is the candidate) |

## Notes

- macOS requires accessibility permission for the event tap and for injection.
- A Linux session with `DISPLAY` set uses the X11 row, including when `WAYLAND_DISPLAY` is also set.
- A Linux session with only `WAYLAND_DISPLAY` fails with the requirement for an X11 `DISPLAY`.
- Automatic insertion prefers a preservable paste transaction, falling back to typing before any clipboard change when a snapshot is unsupported. Explicit Paste reports that limitation. Native receiving-app compatibility still needs interactive validation.
- Native adapters implement the text-injection and multi-binding hotkey contracts. Supported clipboard content is restored after 300 ms only if the transaction still owns it. See [validation](../dictation-validation.md) for measured checks and remaining native sessions.
