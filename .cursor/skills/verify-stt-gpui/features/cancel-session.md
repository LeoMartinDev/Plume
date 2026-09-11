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

- `control-stt-gpui doctor` reports `overlay=present` and `hotkey=present`.
- The session binary arms `Esc` only while a session runs. v1 has no Esc-cancel on Windows.
- A live cancel proof needs a running session: model pack, display, mic, and a held chord.

- **Wiring.** Run `control-stt-gpui session`. Exit code `0`. The cancel fold test retracts scripted partials with no leak. That is the model proof. It is not an Esc key proof.
- **Report.** Without a live session to discard, mark `cancel-session` with the missing piece (`needs-live-session`, `no-model-pack`, `no-display`, `no-mic`). Keep the transcript as the probe, not as a pass.

## Gotchas

- Sending `Esc` to the harness terminal cancels nothing in a target app.
- `Session::cancel` in unit tests is not the Esc key.
- `overlay-cancel-swallow` in the overlay feature is the scripted model. It is not an Esc key proof.
- Cancel cannot pass until hold-to-talk can start a session you then discard. Toggle is unshipped.
