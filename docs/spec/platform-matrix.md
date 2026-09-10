# Platform matrix

One row per platform. Each row names the candidate mechanism for text injection, the global hotkey, and the overlay. Wayland is a first-class row, not a variant of X11. PR-4 verifies these mechanisms and owns the full OS integration spec.

| Platform | Text injection | Global hotkey | Overlay |
| --- | --- | --- | --- |
| macOS | `CGEvent` keyboard events with Unicode text, clipboard paste as fallback | Quartz event tap reports key down and key up | Borderless non-activating `NSWindow` at floating level |
| Windows | `SendInput` with `KEYEVENTF_UNICODE`, clipboard paste as fallback | Low-level keyboard hook `WH_KEYBOARD_LL` reports key down and key up | Topmost layered window with `WS_EX_NOACTIVATE` |
| Linux X11 | XTEST fake key events, clipboard paste as fallback | XRecord extension reports key down and key up | Override-redirect window |
| Linux Wayland | `zwp_virtual_keyboard_v1` where the compositor allows it, clipboard paste as fallback | XDG GlobalShortcuts portal delivers press and release | Layer surface with `zwlr_layer_shell_v1` where the compositor supports it |

## Notes

- macOS requires accessibility permission for the event tap and for injection.
- Wayland capabilities vary by compositor. The portal and the layer-shell protocol both need compositor support.
- The clipboard fallback pastes with the target app's paste shortcut. PR-4 specifies when each fallback applies.
- Each mechanism maps onto the `TextInjector` and `GlobalHotkey` traits from `stt-core`. PR-4 owns that mapping.
