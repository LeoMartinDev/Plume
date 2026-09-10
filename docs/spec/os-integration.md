# OS integration

This spec names the text-injection and global-hotkey mechanism per OS, how
live correction replaces inserted text, when the clipboard fallback applies,
and how each mechanism maps onto the `TextInjector` and `GlobalHotkey` traits
from `stt-core`. The candidate mechanisms from `platform-matrix.md` are
confirmed below, with the Wayland portal chain resolved. Facts about portal
and compositor support were checked in September 2026.

## macOS

### Text injection

Post `CGEvent` keyboard events carrying Unicode text
(`CGEventKeyboardSetUnicodeString`, posted to `kCGHIDEventTap`). One key
down/up pair per character, surrogate pairs handled by the API. This path
types into every standard text field, including password fields, because the
events are indistinguishable from a hardware keyboard at the HID level.

Crates: `core-graphics`/`core-foundation` (or `objc2` plus CoreGraphics
bindings) for the event API.

### Global hotkey

A Quartz event tap (`CGEventTapCreate` on `kCGSessionEventTap`,
`kCGHeadInsertEventTap`) with a mask covering key down, key up, and flags
changed. The tap callback maps key down to `HotkeyEvent::Pressed` and key up
to `HotkeyEvent::Released`. Auto-repeat is filtered: a key down for a key
already recorded as held is ignored, so holding the shortcut emits exactly
one `Pressed`.

The default hold key `Fn` arrives as flags-changed (keycode 63), not as a
plain key down, which is why flags-changed is in the mask. The Carbon
`RegisterEventHotKey` API is rejected: it reports presses but no usable
release event, so it cannot drive hold-to-talk.

### Permissions

macOS requires accessibility permission (TCC `kTCCServiceAccessibility`,
checked with `AXIsProcessTrustedWithOptions`) for both the event tap and
injection. On first launch without the grant, the app opens the Privacy &
Security pane entry and explains the two missing capabilities. Without the
grant there is no degraded hotkey path: the session can only start from the
tray menu, and even the clipboard fallback cannot simulate `Cmd+V`. The error
state names accessibility explicitly.

## Windows

### Text injection

`SendInput` with `KEYEVENTF_UNICODE`: one down/up pair per UTF-16 code unit,
surrogate pairs sent as two units. This is the only API that injects
arbitrary Unicode reliably. `SendKeys`/journal hooks are rejected (no
reliable Unicode, focus races).

### Global hotkey

A low-level keyboard hook (`SetWindowsHookEx` with `WH_KEYBOARD_LL`,
serviced by a message pump on a dedicated thread). `WM_KEYDOWN` and
`WM_SYSKEYDOWN` map to `Pressed`; `WM_KEYUP` and `WM_SYSKEYUP` map to
`Released`. Repeated `KEYDOWN` frames without an intervening `KEYUP`
(auto-repeat) are ignored while held. `RegisterHotKey` is rejected for the
hold key: it delivers `WM_HOTKEY` on press only, with no release event.

The `Fn` key on most laptop keyboards is consumed by the embedded
controller and never reaches the OS, so `Fn` is documented as
best-effort on Windows and `Ctrl+Space` is the effective default there.

### Permissions

No special permission is required for normal targets. UIPI blocks injection
into elevated (administrator) processes from a non-elevated app: the
injector detects `ERROR_ACCESS_DENIED`-class failures and reports
"target is elevated" with the run-as-administrator remedy, instead of
dropping text silently. The low-level hook itself works without elevation.

## X11

### Text injection

XTEST (`XTestFakeKeyEvent`) with keysym-to-keycode mapping through the
active layout. Characters missing from the layout cannot be faked as keys
and go through the clipboard fallback. `XSendEvent` synthesis is rejected:
too many toolkits and input methods ignore synthetic events.

### Global hotkey

The XRecord extension (`XRecordEnableContext`) reporting raw `KeyPress` and
`KeyRelease` for the configured keycode, independent of focus. XRecord is
passive: unlike `XGrabKey` it takes no grab, so it never steals keys from
other applications and always observes the release. `XGrabKey` is rejected
for the hold key for that reason. RECORD is present on every X server the
app targets; its absence is a fatal startup error naming the extension.

### Permissions

None beyond a working `XAUTHORITY` session: any client of the same X server
can use XTEST and XRecord. (That this also allows keylogging by any X
client is a property of X11, not of this app; the local-only principle
constrains the network, and Wayland is the recommended session where the
compositor supports the portal chain below.)

## Wayland

Wayland has no ambient input capture or injection: every capability arrives
through a compositor protocol or an XDG portal, each probed at runtime with
a fallback chain. GNOME and KDE ship full portal backends; compositors
without one (COSMIC, niri, Jay, Smithay-based) are covered by
`xdg-desktop-portal-generic`, which bridges portal requests to the `ext-*`
and `wlr-*` protocols.

### Text injection

Preference order, probed at startup:

1. `zwp_virtual_keyboard_v1` directly (the `wtype` approach). This is the
   only protocol that uploads a client keymap, hence the only one that
   injects arbitrary Unicode reliably. Requires compositor support and a
   non-sandboxed client: sandboxed apps are denied the protocol, and GNOME
   does not implement it.
2. The RemoteDesktop portal (`org.freedesktop.portal.RemoteDesktop`,
   `NotifyKeyboardKeycode`). Works sandboxed (Flatpak) and on GNOME/KDE,
   but sends keycodes in the compositor's keymap with no client keymap
   upload, so text outside the active layout falls back to the clipboard.
   `libei`/EIS is keycode-only for the same reason and is not a Unicode
   path either.
3. The clipboard fallback (below). Note the Wayland paste loop: simulating
   the paste shortcut itself needs step 1 or 2. With neither available, the
   app sets the clipboard and asks the user to press `Ctrl+V` once; this
   degraded mode is explicit in the bubble, never silent.

### Global hotkey

The GlobalShortcuts portal (`org.freedesktop.portal.GlobalShortcuts`):
`CreateSession` plus `BindShortcuts` over D-Bus, with `Activated`
mapping to `Pressed` and `Deactivated` to `Released`. Backend support in
September 2026: GNOME 48+ (refined in 50), KDE Plasma, Hyprland, and
COSMIC. On wlroots compositors such as Sway the portal may register while
the binding itself must be set in the compositor config. Non-sandboxed
apps must declare an app id matching an installed `.desktop` file or
GNOME rejects the session. The first bind shows a one-time compositor
consent dialog; bindings persist across restarts but every session still
calls `BindShortcuts`.

The emerging `vicinae-hotkey-v1` protocol (Hyprland, niri) offers
client-managed global shortcuts without the portal round-trip. It is
watched, not adopted: the portal stays the default path until the
protocol standardizes.

### Permissions

There is no blanket accessibility grant on Wayland. Each portal mediates
its own consent: the compositor prompts once per shortcut bind, and
RemoteDesktop prompts per session. The app probes `Available`-style checks
for every portal and protocol at startup, picks the first working chain,
and reports the exact missing piece (portal, protocol, or consent) when a
capability is unavailable.

## Live correction

Live correction is `TextInjector::replace_last(old, new)`: `old` is the
text inserted since the last stabilized hypothesis, `new` is the revised
guess. The retained mechanism is backspaces plus re-injection: send one
backspace per grapheme cluster of `old`, then `insert(new)`. This works in
every target without depending on the target's selection shortcuts, and it
needs no screen reading: the injector replays keystrokes from its own
buffer, so password fields and terminals are handled like any other field.

Rules:

- The injector tracks a correction buffer (inserted text, grapheme count)
  and a validity flag. Any focus change or any key typed by the user
  outside the injector invalidates the buffer; a `replace_last` against an
  invalid buffer degrades to a plain `insert(new)` and logs the event.
- Partials arrive faster than screens update. The session coalesces: at
  most one `replace_last` per 100 ms per session, always carrying the
  latest hypothesis, so the text converges instead of flickering.
- Replacements are bounded: beyond 200 graphemes the engine's own
  stabilization must carry the text (a fresh `Final`), because replaying
  long backspace runs fights the target's undo history.
- Terminals, IME composition, and undo pollution are accepted risks,
  documented in the troubleshooting section of the user guide (PR-6
  scope): backspace replays inside an IME composition are deferred until
  the composition commits, and every replayed run is one undo unit where
  the target supports it.

## Clipboard fallback

The fallback applies when native injection cannot carry the text: Wayland
without virtual-keyboard or RemoteDesktop, X11 layouts missing the
characters, elevated Windows targets, and any Unicode outside the active
layout on the portal path. It never applies silently: the bubble shows a
"pasted via clipboard" state while active.

Procedure:

1. Read and stash the current clipboard content (text only; non-text
   content is left untouched and noted in the log).
2. Set the clipboard to the transcript text.
3. Simulate the paste shortcut (`Cmd+V` on macOS, `Ctrl+V` elsewhere)
   through the same injection path as normal typing.
4. After a configurable settle delay (default 800 ms), restore the
   stashed content. Restoration is best-effort and logged.

Risks, stated plainly: the fallback briefly exposes dictated text,
possibly secret, to the clipboard and to clipboard managers/history. There
is no perfect mitigation; the restore delay is kept short, and the
fallback is the last resort after the native paths above. On Wayland the
paste shortcut itself travels the RemoteDesktop portal, and the clipboard
write uses the Clipboard portal or data-control (`ext-data-control-v1`,
`wlr-data-control-v1`); with no portal at all the user presses `Ctrl+V`
manually (see Wayland injection).

## Trait mapping

One implementation pair per OS. Constructors probe their mechanism and
fail fast with a guiding error; every method maps failures to `BoxError`
with a user-actionable message.

| OS | `TextInjector` | `GlobalHotkey` | Probe and notes |
| --- | --- | --- | --- |
| macOS | `MacOSTextInjector`: `insert` posts `CGEvent` Unicode strings; `replace_last` replays backspaces + `insert`. | `MacOSGlobalHotkey`: `register` installs the event tap for the shortcut; `next_event` polls tap callback queue for down/up (+ flags-changed for `Fn`). | Accessibility grant checked at startup; missing grant is a named fatal error with the Settings deep link. |
| Windows | `WindowsTextInjector`: `insert` via `SendInput`/`KEYEVENTF_UNICODE`; `replace_last` via backspaces + `insert`. | `WindowsGlobalHotkey`: `register` installs the `WH_KEYBOARD_LL` hook filtered to the shortcut; `next_event` drains the hook queue (down/sysdown, up/sysup). | Elevated-target failures map to a named error with the run-as-admin remedy. No grant needed otherwise. |
| X11 | `X11TextInjector`: `insert` via XTEST keysym mapping, unmappable text via clipboard fallback; `replace_last` via backspaces + `insert`. | `X11GlobalHotkey`: `register` enables the XRecord context for the keycode; `next_event` drains press/release. | Missing RECORD extension is a named fatal error. No grant needed. |
| Wayland | `WaylandTextInjector`: chain virtual-keyboard, then RemoteDesktop portal, then clipboard fallback; `replace_last` replays through the same chain. | `WaylandGlobalHotkey`: `register` opens the portal session and binds; `next_event` drains Activated/Deactivated. | Each link probed (`Available`); the active chain and the first missing link are logged and shown in settings. |

Shared `BoxError` kinds, one per user-facing remedy: `PermissionDenied`
(open the OS grant pane), `CompositorUnsupported` (name the missing portal
or protocol plus the compositor's remedy), `TargetElevated` (Windows
run-as-admin), `ClipboardBusy` (retry with backoff, then keep the text in
the bubble for manual copy). `GlobalHotkey::register` takes the shortcut
string from settings (`Fn` or `Ctrl+Space` for hold, plus the toggle key);
every implementation debounces auto-repeat internally so the session
state machine sees exactly one `Pressed` per physical hold.
