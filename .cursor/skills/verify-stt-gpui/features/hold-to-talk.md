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
- `crates/stt-session/Cargo.toml` exists. The session binary wires the hold chord, mic, engine, injector, and bubble.
- A live pass additionally needs `STT_MODEL_DIR` with a complete pack, a `DISPLAY`, a working mic, and a held chord.
- Agent runs cannot hold a key. Without a human driver, report `needs-human-hold` and stop before the live recipe.

- **Wiring.** Run `control-stt-gpui session`. Exit code `0`. The cargo-test transcript covers the audio gate, the target ledger, and scripted hold/partial/final/cancel folds. Startup probes assert exit `4` without `STT_MODEL_DIR`, exit `2` for `STT_HOLD=Fn`, and exit `4` for a missing pack dir. This is the wiring proof. It is not a live hold pass.
- **Live (human only).** Run `STT_MODEL_DIR=<pack> cargo run -p stt-session`. Hold `Ctrl+Space`, speak, release. Words stream into the focused app and refine in place. The bubble mirrors the session state. Do not claim this pass unless you actually drove it.
- **Report.** Without every live precondition, mark `hold-to-talk` with the missing piece (`no-model-pack`, `no-display`, `no-mic`, `needs-human-hold`). Keep the transcripts as the probe, not as a pass.

## Gotchas

- `cargo test -p stt-core` exercises a library state machine. That is not hold-to-talk.
- `control-stt-gpui overlay` proves the bubble window. That is not hold-to-talk.
- Do not screenshot a desktop and call it a hold session.
- Do not send `Ctrl+Space` to the terminal running the harness and call that a global hotkey.
- A green `control-stt-gpui session` run proves wiring, not a live hold. A missing mic still blocks the pass.
- v1 rejects `Fn` at startup. The hold default on this host is `Ctrl+Space`.
