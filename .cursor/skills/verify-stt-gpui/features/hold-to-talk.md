# Hold-to-talk

Hold-to-talk is the default dictation gesture. The user holds a global shortcut, speaks, watches words stream into the focused app, and releases to finalize. The bubble at the bottom center of the screen is the only session UI.

## Sub-features

- `hold-start` starts capture when the hold shortcut goes down.
- `hold-stream` inserts partial words into the target app while the key is held.
- `hold-correct` replaces already inserted words as the engine refines them.
- `hold-release` finalizes the transcript and dismisses the bubble.

## How to get to it (user POV)

- Click into the app that should receive the text.
- Hold the global shortcut. The default is `Fn` where the OS reports it, or `Ctrl+Space` elsewhere.
- Speak, then release the shortcut.

## Driving it with control-stt-gpui

Preconditions:

- `control-stt-gpui doctor` reports `overlay=present` and `hotkey=present` on this checkout.
- The overlay window exists. It is not wired to a hold shortcut or a microphone.
- The `stt-hotkey`, `stt-audio`, and `stt-inject` crates ship as libraries. No binary registers a shortcut or opens the mic.
- Do not start this recipe expecting a live session.

- **Probe the binary.** Try to start a session from the CLI. Run `control-stt-gpui cli`. Exit code `0`. Stdout is the version line. No recording starts.
- **Probe flags.** Run `control-stt-gpui cli -- --help`. The same version line. There is no hold, record, or dictate subcommand.
- **Report.** Mark `hold-to-talk` unreachable. Unmet preconditions are `no-session-binary` and `overlay-not-wired-to-session`. Keep the transcripts as the probe, not as a pass.

## Gotchas

- `cargo test -p stt-core` exercises a library state machine. That is not hold-to-talk.
- `control-stt-gpui overlay` proves the bubble window. That is not hold-to-talk.
- Do not screenshot a desktop and call it a hold session.
- Do not send `Ctrl+Space` to the terminal running the harness and call that a global hotkey.
- When a session binary lands, this file is stale. Run `/maintain-verification-skill` before claiming a pass.
