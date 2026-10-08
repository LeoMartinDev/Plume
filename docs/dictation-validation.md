# Dictation contract validation

The implementation uses one operation for live dictation and recovery. Models,
recordings and clipboard content are local. The existing three-platform CI matrix
continues to build, test, lint and format the workspace.

## Automated checks (8 October 2026)

- `cargo build --workspace --locked`: passed on macOS.
- `cargo test --workspace --locked --no-fail-fast`: 220 passed, 6 ignored.
  Optional model fixtures were also run separately with prepared native assets.
- `cargo clippy --workspace --all-targets --locked -- -D warnings` and
  `cargo fmt --check`: passed.
- `node --test scripts/*.test.mjs`: 27 passed, 3 platform-dependent tests skipped.
- Simulated controller: busy presses, required fresh press, hold/toggle ownership,
  cancellation barriers and simulated 30/540/600-second timers. Shortcut editing
  reserves the idle session and waits for native passthrough/restore acknowledgments.
- Decoder: final-only results and distinct cancellation; native Whisper callback
  and ORT RunOptions termination tested with actual local models.
- Audio: callback-independent 44.1/48/8 kHz resampling, buffered WAV read failures,
  cancellation, first-speech pre-roll and retained pauses.
- Real Silero pinned asset: silence, low-level noise and French/English fixtures.
- Real Whisper Base: English and French fixtures, gated fixture transcription and cancellation during native
  inference; the frozen language remains unchanged when the source engine's
  language changes. Whisper/GGML native logs are suppressed to prevent decoded
  text from appearing in debug builds. Metal requires execution outside the local
  sandbox.
- Real Nemotron pinned pack: French and English exact fixtures, native cancellation
  and preservation of all spoken words through the shared VAD gate. Gating shifts
  chunk boundaries and can change punctuation produced by the model.
- Storage: eight-audio eviction, empty/cancelled exclusion, interrupted recovery,
  interrupted metadata replacement, corrupt metadata/orphan reconciliation,
  persistent cancellation marker, failed writes, same-record retry and preserved
  previous insertion outcome. The active file stays temporary until delivery
  finishes; cancellation after STT persistence does not evict an older recording.
  Promotion retains the final text and insertion result persisted beforehand.
  A corrupt shared VAD is repaired during preparation.
- History/preferences: legacy entries without audio, default-toggle collision
  migration, normalized collision refusal and retry without insertion. Recovery
  after text expiry keeps the previous insertion outcome and remains copyable.
- Insertion: Unicode boundary spacing, unchanged internal text, snapshot fallback,
  user-copy preservation and no typing after paste/restore failure. Cancellation
  is checked immediately before keyboard dispatch, after clipboard staging;
  cancellation restores the snapshot and reports a restoration failure.
- Native macOS clipboard: empty, text, HTML with text and image restored.
- Windows and Linux X11 hotkey/injector modules: cross-compilation and Clippy
  checks with all targets. Shared-trigger repeat/modifier changes have regressions.
- `node scripts/check-spec.mjs docs/spec`: passed.

## Local environment distinctions

Earlier isolated GPUI crate checks without runtime shaders failed because this
machine lacks the Xcode Metal compiler. The application's existing
`gpui/runtime_shaders` dependency feature is unified in a full workspace build,
so standard workspace build/tests/lints pass without an extra feature flag.
The CI matrix is unchanged. The ONNX Runtime download initially failed inside
the network-restricted sandbox; it succeeded with network access. The updater
HTTP test also needs permission to bind a local socket.

An initial native Nemotron cancellation test exposed an iterator that repeatedly
returned its cancellation error. It is corrected to terminate after cancellation;
the native regression test has a bounded result count. A concurrent cold fixture
run exceeded the existing five-second threshold by 37 ms. A serial rerun passed
(English 3,202 ms, French 1,965 ms).

## Native acceptance still requiring interactive hardware sessions

On macOS, Windows and X11, exercise an isolated native editor, browser field and
Electron field. Verify physical hold and toggle shortcuts, Esc during opening,
capture and native STT, busy-press rejection, release tail and focus changes.
Check a new user copy during the 300 ms transaction, selection replacement,
password/read-only refusal and supported clipboard formats. Successful dispatch
is not an acknowledgment from the target editor. Physical shortcuts, permission
prompts and receiving-app behavior are not proved by cross-compilation or audio
fixtures. Windows/X11 desktop sessions are unavailable on this macOS host.
The X11 shortcut service requires XKB detectable autorepeat so server-generated
repeat release/press pairs cannot stop or restart a physical hold.

Native fixture commands (paths point to already prepared local assets):

```sh
PLUME_VAD_PATH=/path/silero_vad.onnx cargo test -p plume-engine --test speech_detection real_silero
PLUME_VAD_PATH=/path/silero_vad.onnx PLUME_WHISPER_PATH=/path/model.bin cargo test -p plume-engine --test speech_detection real_whisper
PLUME_MODEL_DIR=/path/light cargo test -p plume-engine --test fixture_transcribe -- --test-threads=1
PLUME_MODEL_DIR=/path/light PLUME_VAD_PATH=/path/silero_vad.onnx cargo test -p plume-engine --test speech_detection real_nemotron -- --test-threads=1
cargo test -p plume-inject --lib clipboard::native_tests -- --ignored --test-threads=1
```
