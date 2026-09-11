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

- `control-stt-gpui doctor` reports `overlay=present` and `hotkey=present` on this checkout.
- The overlay window exists. It is not wired to a toggle shortcut.
- The session binary registers the hold chord only. No toggle shortcut exists in v1.

- **Probe the session binary.** Run `control-stt-gpui session`. Exit code `0`. The wiring proof covers hold-to-talk only. No toggle path exists to drive.
- **Report.** Mark `toggle-mode` unreachable. Unmet precondition is `toggle-not-shipped`. Keep the transcript as the probe, not as a pass.

## Gotchas

- A process that exits immediately cannot be a toggle session.
- Do not hold the recipe open with `sleep` and call that toggle mode.
- The overlay window probe is not a toggle proof. Report this entry point separately.
