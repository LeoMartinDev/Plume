# Readability refactor verification

Validation performed on 2026-10-05. Native UI checks use the full settings
preview and isolated storage under `/tmp/plume-settings-validation`.

## Automated checks

| Environment | Result |
| --- | --- |
| macOS ARM64 | Workspace build, tests, Clippy with warnings denied and formatting pass. 165 tests pass, 5 are explicitly ignored. |
| Linux ARM64 | Workspace build, tests, Clippy with warnings denied and formatting pass in a Debian Trixie container. 163 tests pass, 5 are explicitly ignored. This Linux run preceded integration of the newer remote commits. |
| Windows x86_64 | Cross-compilation checks pass for `plume-core`, `plume-audio`, `plume-hotkey` and `plume-inject`. Full native workspace validation remains pending. |
| Real Whisper Small on macOS | English and French WAV fixtures pass, using the installed multilingual model and Metal. |
| Documentation | Spec links and required terms pass `scripts/check-spec.mjs`; Git whitespace checks pass. |

The session tests exercise the production decoder and delivery functions with
simulated engines and injectors. They cover partial/final hypotheses, empty
text, engine errors, missing finals, cancellation, queue saturation, ordering,
separators and both successful and failed clipboard fallback.

History and preferences tests cover old files, invalid fields and independent
fallbacks, finite boundaries, custom and unlimited limits, immediate pruning,
startup pruning, transactional writes and retries after failures.

The regular engine suite contains fixture tests that return early when their
Nemotron model is absent. Ignored download/model tests were not all run. X11
live tests require a display; this container does not supply one.

## Settings UI checks

- The sidebar remains visible on Dictation, Model, Appearance and History.
- History menus offer exactly 7/30/90 days and 100/500/5,000 entries, plus
  Unlimited independently for each setting.
- Selecting Unlimited saves the corresponding TOML string.
- A forced preferences write failure retains the previous selection and shows
  the error immediately below the controls. Retrying after restoring write
  access succeeds and clears the error.
- Shortcut capture saves `Ctrl+Alt+k`; changing the theme saves Dark.
- Restart preserves the shortcut, theme and history policy.
- Restart with a maximum of 100 entries prunes the 150 synthetic entries to
  IDs 150 through 51 and persists that result.

Model download, activation and deletion are disabled in the preview. Its
settings targets do not control the native dictation session.

## Remaining native checks

The existing CI matrix still requires a run against these changes on Windows
and Linux x86_64. The local Linux ARM64 container supplements that matrix.

Manual validation in the main application remains necessary for microphone
capture while holding the shortcut, insertion on release, cancellation with
Escape, global shortcut rebinding and changing the active model. Recognizing
fixture audio and editing preview preferences do not verify those OS actions.

## Reproduce

```sh
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --check
node scripts/check-spec.mjs docs/spec
cargo run -p plume-app --example settings_preview -- /tmp/plume-settings-preview
PLUME_WHISPER_MODEL=/path/to/model.bin cargo test --workspace \
  downloaded_whisper_model_transcribes -- --ignored --nocapture
```

The Linux container uses `rust:1-trixie`, CMake, Clang, ALSA/XCB/XKB development
libraries and Clippy/rustfmt. Whisper's CPU detection under Docker on Apple
Silicon required a CMake project include that sets `GGML_NATIVE=OFF` and
`GGML_CPU_ARM_ARCH=armv8-a`. This validation-only adjustment is outside the
repository and does not change the app's default model build settings.
