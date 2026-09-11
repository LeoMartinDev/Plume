# Toggle mode

Toggle mode is long dictation without a held key. One press of the toggle shortcut starts recording and shows the bubble. A second press finalizes the transcript and dismisses the bubble.

## Sub-features

- `toggle-start` starts capture on the first toggle press.
- `toggle-stream` streams partial text into the target app with no key held.
- `toggle-stop` finalizes on the second toggle press.

## How to get to it (user POV)

- Click into the app that should receive the text.
- Press the toggle shortcut once to start.
- Speak for as long as needed.
- Press the toggle shortcut again to stop.

## Driving it with control-stt-gpui

Preconditions:

- `control-stt-gpui doctor` reports `overlay=absent` and `hotkey=absent` on this checkout.

- **Probe the binary.** Run `control-stt-gpui cli`. Exit code `0`. Stdout is the version line. The process exits. It does not wait for a second key press.
- **Report.** Mark `toggle-mode` unreachable. Unmet preconditions are `overlay-not-shipped` and `toggle-hotkey-not-shipped`. Keep the transcript as the probe, not as a pass.

## Gotchas

- A process that exits immediately cannot be a toggle session.
- Do not hold the recipe open with `sleep` and call that toggle mode.
- The hold-to-talk probe is not a toggle proof. Report this entry point separately.
