# Cancel a session

Cancel discards an in-flight dictation. Pressing `Esc` while a session runs ends it with no final transcript. Text already inserted is removed where the target app allows it.

## Sub-features

- `cancel-esc` ends a running session from `Esc`.
- `cancel-no-final` produces no final transcript.
- `cancel-retract` removes text the session already inserted when the target app allows it.

## How to get to it (user POV)

- Start hold-to-talk or toggle mode so a session is running and the bubble is visible.
- Press `Esc`.

## Driving it with control-stt-gpui

Preconditions:

- `control-stt-gpui doctor` reports `overlay=present` and `hotkey=absent`.
- No live session is running. The overlay is not wired to Esc.

- **Probe the binary.** Run `control-stt-gpui cli`. Exit code `0`. Stdout is the version line. There is no session to cancel.
- **Report.** Mark `cancel-session` unreachable. Unmet precondition is `no-running-session`. Keep the transcript as the probe, not as a pass.

## Gotchas

- Sending `Esc` to the harness terminal cancels nothing in a target app.
- `Session::cancel` in unit tests is not the Esc key.
- `overlay-cancel-swallow` in the overlay feature is the scripted model. It is not an Esc key proof.
- Cancel cannot pass until hold-to-talk or toggle can start a session you then discard.
