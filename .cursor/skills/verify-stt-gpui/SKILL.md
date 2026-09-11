---
name: verify-stt-gpui
description: Drive stt-gpui, a local desktop dictation app whose only runnable surface today is the short-lived stt-shell CLI. Use when proving a change to the shell, workspace boot, or a claimed overlay, hotkey, or dictation flow.
---

# Verify stt-gpui

stt-gpui is a desktop push-to-talk dictation app. Hold a global shortcut, speak, and text streams into the focused app. Audio never leaves the machine.

This checkout does not ship that overlay yet. The only user-facing program is `stt-shell`, a short-lived CLI that prints `stt-shell <version>` from `crates/stt-shell/Cargo.toml` and exits. There is no window, no global hotkey, no microphone capture, and no text injection. `stt-core` is a library. Do not treat `cargo test` as a user path.

The planned surface is a native GPUI bubble at the bottom center of the screen, driven by hold-to-talk and toggle shortcuts, with Esc to cancel and an optional local LLM cleanup pass. Those flows live in `docs/spec/ux-flow.md`. Until doctor reports `overlay=present`, they are unreachable. Report the unmet precondition. Do not invent selectors, screenshots of a missing window, or a pass via unit tests.

## Launch

There is no server. Launch means build `stt-shell` once into an isolated target directory, then start each drive as its own process.

From the repository root:

```bash
export STT_GPUI_VERIFY_RUN_ID="${STT_GPUI_VERIFY_RUN_ID:-$(date +%Y%m%dT%H%M%S)-$$}"
.cursor/skills/verify-stt-gpui/scripts/control-stt-gpui launch
```

Ready when stdout contains `ready run_id=` and `surface=cli`. The binary path is printed as `binary=`. A successful launch also writes `/tmp/stt-gpui-verify-current` with that run id.

`CARGO_TARGET_DIR` is `/tmp/stt-gpui-verify-$RUN_ID/target`. Never `cargo clean` a shared `target/` to make verification faster. Never reuse `target/debug/stt-shell` from the workspace tree. Doctor rejects a binary that is not inside the launched run directory.

Concurrent runs are allowed. Give each one its own `STT_GPUI_VERIFY_RUN_ID`. If a current instance already exists and you need a second, set the env var. Do not share one binary across two proofs.

Teardown is `control-stt-gpui cleanup`. It deletes `/tmp/stt-gpui-verify-$RUN_ID/` and leaves evidence at `/tmp/stt-gpui-verify-artifacts/$RUN_ID/`.

## Doctor

Run this first whenever anything looks off, and before the first drive of a session:

```bash
.cursor/skills/verify-stt-gpui/scripts/control-stt-gpui doctor
```

Pass only when every line is `ok` and all of these hold:

- `cargo` and `rustc` are on `PATH`
- the state file for this run id exists
- the binary is executable and lives under `/tmp/stt-gpui-verify-$RUN_ID/`
- stdout of the binary is exactly `stt-shell <version>` matching `crates/stt-shell/Cargo.toml`
- exit code is `0`
- `surface=cli`
- `overlay=absent`
- `hotkey=absent`

If `overlay` is still `absent`, do not drive hold-to-talk, toggle, cancel, or cleanup as if a bubble existed. Those feature files say how to probe the gap. They are not passes.

Never drive an instance this run did not launch.

## Drive

Put the helper on `PATH` for the rest of the session, or call it by repository-relative path.

```bash
export PATH="$PWD/.cursor/skills/verify-stt-gpui/scripts:$PATH"
```

Every user action in this checkout is one invocation of the built binary:

```bash
control-stt-gpui cli
control-stt-gpui cli -- --help
```

`cli` forwards extra arguments after an optional `--`. Today `stt-shell` ignores flags. `--help` still prints the version line and exits `0`. Assert that line. Do not expect usage text.

There is no ARIA tree, no prompt, and no route. Stable handles are the stdout line `stt-shell <version>` and the process exit code. Do not click coordinates. Do not send keys to a window.

Read `features/README.md`, then the feature file for the path under test. Start from the baseline in that index. Drive every listed entry point, or report the one you could not reach with the command you ran and the unmet precondition.

When a later change adds a GPUI window, stop. Run `/maintain-verification-skill` instead of bolting browser or screenshot steps onto this CLI recipe.

## Evidence

Proof artifacts go to `/tmp/stt-gpui-verify-artifacts/$RUN_ID/`. Cleanup must not delete that directory.

Each `cli` invocation writes:

- `cli-<stamp>.stdout.txt`
- `cli-<stamp>.stderr.txt`
- `cli-<stamp>.meta.txt` (command, exit code, paths)
- `cli-<stamp>.transcript.txt` (command, stdout, stderr, exit)

Standards:

- Exercise the real user path. For this checkout that is running `stt-shell`, not calling `Session` in `stt-core` and not using test-only binaries.
- Capture the command and the resulting stdout, stderr, and exit code, not only a later `cargo test` summary.
- A version line without the matching `Cargo.toml` version is not identity proof. Doctor already checks the toml. The drive transcript must still show the same line.
- `--help` is not a help command. Observe that it still prints the version. Do not treat that as documentation.
- Mocks are not in play. The binary has no network. Local-only is currently "the process printed and exited." Watch for network only when a later engine or updater lands.
- Dictation, overlay, hotkey, injection, and settings are specified and unshipped. A probe that prints the version line and exits is evidence the path is absent. It is not evidence the path works.

Record the feature id and the entry point on every artifact you keep (copy or rename under a feature subdirectory if the default stamp is too generic).

## Cleanup

```bash
.cursor/skills/verify-stt-gpui/scripts/control-stt-gpui cleanup
```

This removes `/tmp/stt-gpui-verify-$RUN_ID/` including the isolated `target/`. It does not kill by process name. The CLI is short-lived, so there is usually no process left. If launch spawned a `cargo build` that is still running for this run id, wait for it or kill that child by the pid you started, not every `stt-shell` on the machine.

After cleanup, confirm `/tmp/stt-gpui-verify-artifacts/$RUN_ID/` still contains the transcripts from the drive. A cleanup that ate the proof failed.

Do not delete workspace `target/` as part of verification cleanup.

## Helpers

`scripts/control-stt-gpui` is executable. Invoke it from the repository root.

```bash
.cursor/skills/verify-stt-gpui/scripts/control-stt-gpui launch
.cursor/skills/verify-stt-gpui/scripts/control-stt-gpui doctor
.cursor/skills/verify-stt-gpui/scripts/control-stt-gpui cli
.cursor/skills/verify-stt-gpui/scripts/control-stt-gpui cli -- --help
.cursor/skills/verify-stt-gpui/scripts/control-stt-gpui cleanup
```

`control-stt-gpui --help` prints harness usage. That is not `stt-shell --help`.
