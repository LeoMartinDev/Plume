# stt-gpui verification map

This directory is the maintained source for verifying the user-facing behavior of stt-gpui. Read the index before driving the app, then use the matching feature file as the recipe.

## Baseline preconditions

- Work from the repository root of stt-gpui.
- Set `STT_GPUI_VERIFY_RUN_ID` to a unique value when more than one verification run might overlap.
- Run `control-stt-gpui launch` so `stt-shell` is built into `/tmp/stt-gpui-verify-$RUN_ID/target`.
- Run `control-stt-gpui doctor` and require `surface=cli`, `overlay=present`, `hotkey=present`, and stdout `stt-shell <version>` matching `crates/stt-shell/Cargo.toml`.
- Never drive a binary that was not built by this launch, except `stt-overlay` built into that same run target by `control-stt-gpui overlay`.
- Never treat workspace `target/debug/stt-shell` as the instance under test.

## Driving conventions

- Start every recipe from the baseline state unless its preconditions say otherwise.
- The live CLI entry points are running `stt-shell` bare and the `transcribe` subcommand.
- The overlay entry point is `control-stt-gpui overlay`. It is not `cli`.
- Treat every command as literal. Keep quoted names and flags unchanged.
- Run the shell through `control-stt-gpui cli`.
- Put the helper on `PATH` with `export PATH="$PWD/.cursor/skills/verify-stt-gpui/scripts:$PATH"` or use the repository-relative path.
- Hold-to-talk and Esc-cancel ship in `stt-session` on Linux (no Esc-cancel on Windows in v1). A live pass needs a model pack, a display, a mic, and a held chord. Toggle and cleanup are specified but unshipped. Probe each path, then report unreachable pieces. Do not pass a live gesture through `cargo test`.

## Proof and skip reporting

- Capture the user action and the resulting stdout, stderr, and exit code, not only the final line.
- CLI proof includes the command, stdout, stderr, and exit code under `/tmp/stt-gpui-verify-artifacts/$RUN_ID/`.
- Overlay proof includes the cargo-test transcript and, when `DISPLAY` is set, `xwininfo-name.txt`. When `DISPLAY` is unset, keep `skip.txt` and do not call that a window pass.
- A version line is identity proof for `shell-identity` only.
- The same version line on toggle or cleanup is evidence the path is unshipped.
- Record the feature ID and entry point used with every artifact.
- Report an unreachable path with the attempted command and the unmet precondition.
- Do not report a skipped entry point as verified through a different path.

## Feature entry contract

Each feature file starts with an H1 title and one paragraph describing the user-visible behavior. It then uses exactly four H2 sections in this order.

1. `Sub-features` lists short IDs with one line for each behavior.
2. `How to get to it (user POV)` lists every user entry point.
3. `Driving it with control-stt-gpui` starts with `Preconditions:` and uses labeled bullets that pair each user action with an exact command and observable result.
4. `Gotchas` lists traps that can waste or invalidate a verification run.

Keep implementation details out of the map. Name only user paths, stable handles, required state, commands, and observable proof.

## Features

- [Shell identity](./shell-identity.md) covers CLI identity.
- [Transcribe a file](./transcribe-file.md) covers local file transcription. Passes with `STT_MODEL_DIR` set.
- [Overlay bubble](./overlay-bubble.md) covers the GPUI window and the scripted Dictation bubble model.
- [Hold-to-talk](./hold-to-talk.md) covers the dictation hold gesture. Wired in `stt-session`. A live pass needs a model pack, a display, a mic, and a held chord.
- [Toggle mode](./toggle-mode.md) covers long dictation without a held key. Unreachable: v1 ships hold only.
- [Cancel a session](./cancel-session.md) covers Esc discarding in-flight text. Wired on Linux; a live proof needs a running session. No Esc-cancel on Windows in v1.
- [Cleanup pass](./cleanup-pass.md) covers the optional local LLM rewrite. Unreachable until settings and the cleanup pass exist.
