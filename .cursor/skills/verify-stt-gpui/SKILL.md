---
name: verify-stt-gpui
description: Drive stt-gpui, a local desktop dictation app. Prove the stt-shell CLI and the stt-overlay GPUI bubble. Use when proving a change to the shell, workspace boot, overlay window, hotkey, or dictation flow.
---

# Verify stt-gpui

stt-gpui is a desktop push-to-talk dictation app. Hold a global shortcut, speak, and text streams into the focused app. Audio never leaves the machine.

`stt-shell` is the short-lived CLI. It prints `stt-shell <version>` from `crates/stt-shell/Cargo.toml` and exits. `stt-overlay` is a GPUI bubble window at the bottom center of the screen. It shows the latest partial and a `SessionState` marker. `stt-session` is the resident session binary. It wires the hold hotkey, mic, engine, injector, and overlay bubble into hold-to-talk with Esc to cancel. A live session needs `STT_MODEL_DIR` with a complete model pack, an X11 `DISPLAY`, a mic, and a held chord. `stt-core` is a library. Do not treat `cargo test` as a user path for the shell or for hold-to-talk.

The planned live surface is that bubble, driven by hold-to-talk and toggle shortcuts, with Esc to cancel and an optional local LLM cleanup pass. Those flows live in `docs/spec/ux-flow.md`. Doctor reports `overlay=present` when `crates/stt-overlay` exists, and `hotkey=present` when `crates/stt-hotkey` exists. Either flag means the crate ships, not that a session runs. `stt-session` wires hold-to-talk and Esc-cancel on Linux (v1 has no Esc-cancel on Windows). Toggle and cleanup stay unshipped. A live hold-to-talk pass needs a model pack, a display, a mic, and a real key hold: without all four, report the unmet precondition. Do not invent selectors, screenshots of pixels, or a hold-to-talk pass via unit tests.

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
- `overlay=present` when `crates/stt-overlay/Cargo.toml` exists, otherwise `overlay=absent`
- `hotkey=present` when `crates/stt-hotkey/Cargo.toml` exists, otherwise `hotkey=absent`

`overlay=present` means the bubble crate is in this checkout. It does not mean hold-to-talk works. Drive [overlay-bubble](features/overlay-bubble.md) for the window. `hotkey=present` means the hotkey library ships. It does not mean a shortcut is registered. Neither flag means a live session exists: do not drive hold-to-talk, toggle, cancel, or cleanup as if one did. Those feature files say how to probe the gap. They are not passes.

Never drive an instance this run did not launch.

## Drive

Put the helper on `PATH` for the rest of the session, or call it by repository-relative path.

```bash
export PATH="$PWD/.cursor/skills/verify-stt-gpui/scripts:$PATH"
```

Every user action on the CLI is one invocation of the built `stt-shell` binary:

```bash
control-stt-gpui cli
control-stt-gpui cli -- --help
```

`cli` forwards extra arguments after an optional `--`. Today `stt-shell` ignores flags. `--help` still prints the version line and exits `0`. Assert that line. Do not expect usage text.

The overlay bubble is a separate program. Drive it with `control-stt-gpui overlay`, not with `cli`. Do not bolt screenshots onto the CLI recipe. The session binary has its own drive: `control-stt-gpui session` builds `stt-session`, runs its gate/ledger/fold tests, and probes startup exit codes. It never starts a live session.

There is no ARIA tree, no prompt, and no route. Stable CLI handles are the stdout line `stt-shell <version>` and the process exit code. Stable overlay handles are the window title `stt-overlay` from `xwininfo -name` and the bubble model assertions in `cargo test -p stt-overlay`. Do not click coordinates. Do not send keys to a window. Do not use `ffmpeg` x11grab. Those frames are black on WSLg.

Read `features/README.md`, then the feature file for the path under test. Start from the baseline in that index. Drive every listed entry point, or report the one you could not reach with the command you ran and the unmet precondition.

## Evidence

Proof artifacts go to `/tmp/stt-gpui-verify-artifacts/$RUN_ID/`. Cleanup must not delete that directory.

Each `cli` invocation writes:

- `cli-<stamp>.stdout.txt`
- `cli-<stamp>.stderr.txt`
- `cli-<stamp>.meta.txt` (command, exit code, paths)
- `cli-<stamp>.transcript.txt` (command, stdout, stderr, exit)

Each `overlay` invocation writes under `overlay-<stamp>/`:

- `cargo-test.stdout.txt` and `cargo-test.stderr.txt` (scripted Dictation state)
- `xwininfo-name.txt` when `DISPLAY` is set
- `skip.txt` when `DISPLAY` is unset (window not proven, exit 0)
- `transcript.txt`

Standards:

- Exercise the real user path. For the shell that is running `stt-shell`, not calling `Session` in `stt-core`. For the overlay that is opening `stt-overlay` and reading `xwininfo`, plus the in-process bubble model tests the overlay command runs.
- Capture the command and the resulting stdout, stderr, and exit code, not only a later `cargo test` summary. Overlay state proof is the exception: the overlay command's cargo-test transcript is the state evidence.
- A version line without the matching `Cargo.toml` version is not identity proof. Doctor already checks the toml. The drive transcript must still show the same line.
- `--help` is not a help command. Observe that it still prints the version. Do not treat that as documentation.
- Mocks are not in play. The binary has no network. Local-only is currently "the process printed and exited." Watch for network only when a later engine or updater lands.
- Hotkey, mic, injection, engine, and overlay ship as libraries. `stt-session` wires them into hold-to-talk. `control-stt-gpui session` proves that wiring headlessly: crate tests plus startup probes for exit 4 without a model pack and exit 2 for a bad chord. That is not a live hold-to-talk pass. Settings, toggle, and cleanup are specified and unshipped.
- Window pixels are not proof on this host. Presence plus state is the proof.

Record the feature id and the entry point on every artifact you keep (copy or rename under a feature subdirectory if the default stamp is too generic).

## Cleanup

```bash
.cursor/skills/verify-stt-gpui/scripts/control-stt-gpui cleanup
```

This removes `/tmp/stt-gpui-verify-$RUN_ID/` including the isolated `target/`. It does not kill by process name. The CLI is short-lived, so there is usually no process left. If launch spawned a `cargo build` that is still running for this run id, wait for it or kill that child by the pid you started, not every `stt-shell` on the machine. The overlay command kills the `stt-overlay` pid it started.

After cleanup, confirm `/tmp/stt-gpui-verify-artifacts/$RUN_ID/` still contains the transcripts from the drive. A cleanup that ate the proof failed.

Do not delete workspace `target/` as part of verification cleanup.

## Helpers

`scripts/control-stt-gpui` is executable. Invoke it from the repository root.

```bash
.cursor/skills/verify-stt-gpui/scripts/control-stt-gpui launch
.cursor/skills/verify-stt-gpui/scripts/control-stt-gpui doctor
.cursor/skills/verify-stt-gpui/scripts/control-stt-gpui cli
.cursor/skills/verify-stt-gpui/scripts/control-stt-gpui cli -- --help
.cursor/skills/verify-stt-gpui/scripts/control-stt-gpui overlay
.cursor/skills/verify-stt-gpui/scripts/control-stt-gpui session
.cursor/skills/verify-stt-gpui/scripts/control-stt-gpui cleanup
```

`control-stt-gpui --help` prints harness usage. That is not `stt-shell --help`.
