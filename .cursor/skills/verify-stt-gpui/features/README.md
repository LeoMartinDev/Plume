# stt-gpui verification map

This directory is the maintained source for verifying the user-facing behavior of stt-gpui. Read the index before driving the app, then use the matching feature file as the recipe.

## Baseline preconditions

- Work from the repository root of stt-gpui.
- Set `STT_GPUI_VERIFY_RUN_ID` to a unique value when more than one verification run might overlap.
- Run `control-stt-gpui launch` so `stt-shell` is built into `/tmp/stt-gpui-verify-$RUN_ID/target`.
- Run `control-stt-gpui doctor` and require `surface=cli`, `overlay=absent`, `hotkey=absent`, and stdout `stt-shell <version>` matching `crates/stt-shell/Cargo.toml`.
- Never drive a binary that was not built by this launch.
- Never treat workspace `target/debug/stt-shell` as the instance under test.

## Driving conventions

- Start every recipe from the baseline state unless its preconditions say otherwise.
- The live entry points are running `stt-shell` bare and the `transcribe` subcommand. There is no window or prompt.
- Treat every command as literal. Keep quoted names and flags unchanged.
- Run the binary through `control-stt-gpui cli`.
- Put the helper on `PATH` with `export PATH="$PWD/.cursor/skills/verify-stt-gpui/scripts:$PATH"` or use the repository-relative path.
- Dictation features from `docs/spec/ux-flow.md` are specified and unshipped. Probe them, then report unreachable. Do not pass them through `cargo test`.

## Proof and skip reporting

- Capture the user action and the resulting stdout, stderr, and exit code, not only the final line.
- CLI proof includes the command, stdout, stderr, and exit code under `/tmp/stt-gpui-verify-artifacts/$RUN_ID/`.
- A version line is identity proof for `shell-identity` only.
- The same version line on hold-to-talk, toggle, cancel, or cleanup is evidence the path is absent.
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
- [Hold-to-talk](./hold-to-talk.md) covers the specified dictation hold gesture. Unreachable until an overlay and hotkey exist.
- [Toggle mode](./toggle-mode.md) covers long dictation without a held key. Unreachable until a toggle shortcut exists.
- [Cancel a session](./cancel-session.md) covers Esc discarding in-flight text. Unreachable until a session overlay exists.
- [Cleanup pass](./cleanup-pass.md) covers the optional local LLM rewrite. Unreachable until settings and the cleanup pass exist.
