# stt-gpui

A local desktop dictation app written in Rust and GPUI. Hold `Ctrl+Space`, speak, then release: the final transcript is inserted into the focused application. `Esc` cancels an active capture. Audio stays on the machine; downloading a model requires a network connection.

## Run

```sh
cargo run -p stt-app
```

Choose and download a model in Settings → Model. Settings also contains the shortcut, language, insertion mode, appearance and transcription history. On macOS, global shortcuts and insertion require the appropriate Accessibility/Input Monitoring permissions, and capture requires microphone permission.

The app lives in the macOS menu bar under **STT** (without a Dock icon), the Windows notification area, or the Linux system tray. Closing Settings hides the window while dictation shortcuts, model downloads and transcription history continue running. Use the tray's **Settings…** menu to reopen the same window, or **Quit STT** to stop the app. The isolated settings preview still quits when closed.

Linux uses X11 for the app window and D-Bus StatusNotifierItem for the tray, without a GTK dependency. The desktop must provide a StatusNotifier host (for example KDE Plasma, or GNOME with an AppIndicator extension). If tray initialization fails, closing Settings quits normally so the app remains accessible. A Wayland-only session remains unsupported.

For the standalone session runner, provide an installed model directory:

```sh
STT_MODEL_DIR=/path/to/model cargo run -p stt-session
```

Its environment configuration is `STT_MODEL_DIR` (required), `STT_HOLD` (default `Ctrl+Space`) and `STT_CANCEL` (default `Esc`). `Fn` shortcuts are rejected.

To transcribe a WAV file without the desktop interface:

```sh
cargo run -p stt-shell -- transcribe crates/stt-engine/fixtures/fr-bonjour.wav --model-dir /path/to/model
```

The CI installs platform prerequisites: ALSA/XCB/XKB development libraries on Linux and the Vulkan SDK on Windows. See [.github/workflows/ci.yml](.github/workflows/ci.yml) for the exact dependencies. Native integrations exist for macOS, Windows and Linux X11; Wayland support remains planned.

## Architecture

| Component | Responsibility |
| --- | --- |
| `stt-core` | Dependency-free audio/transcript types, contracts and session state machine |
| `stt-audio` | Microphone capture and sample conversion |
| `stt-engine` | Local models, preprocessing and decoding |
| `stt-hotkey` | Global shortcuts and OS-specific listeners |
| `stt-inject` | Insertion into the focused application |
| `stt-session` | Capture coordination, background decoding and ordered delivery |
| `stt-overlay` | Voice-level bubble and native window behavior |
| `stt-ui` | Shared controls, palette and UI refresh interval |
| `stt-app` | Startup, model downloads, preferences, history and settings |
| `stt-shell` | WAV transcription command |

A shortcut press starts capture. Audio flows to a bounded background decoder queue. Releasing ends capture; the decoder returns the final text. `TranscriptDelivery` preserves capture order, inserts separators between overlapping captures, applies the insertion mode and attempts clipboard fallback when configured. Partial hypotheses are not inserted by the desktop session. A cancelled capture returns no text to insert.

Within `stt-session`, `runtime` coordinates capture and configuration updates, `decoder` owns the decoding queue and worker, `delivery` owns ordered insertion, and `startup` connects the native adapters. Within `stt-app`, preference types are separate from TOML conversion and storage; settings actions are grouped by shortcut, preferences, models and history.

## Configuration and history

The app stores `prefs.toml` under the OS configuration root in `stt`, and `history.json` and model packs under the OS local-data root in `stt`. On macOS both roots normally resolve to `~/Library/Application Support/stt`; on Linux they normally resolve to `~/.config/stt` and `~/.local/share/stt`; on Windows configuration and local data use the respective roaming and local app-data directories.

Example preferences:

```toml
hold = "Ctrl+Space"
cancel = "Esc"
model = "nemotron-3.5-compact"
appearance = "auto"
language = "auto"
insertion_mode = "auto"
copy_on_failure = true

[history]
retention_days = 30
max_entries = 500
```

`appearance` accepts `auto`, `light` or `dark`; `language` accepts `auto`, `fr` or `en`; insertion accepts `auto`, `clipboard` or `typing`. Models are `nemotron-3.5-compact`, `whisper-base` `whisper-small` and `whisper-large-v3-turbo`. The old model name `light` still loads.

History settings accept 1–3,650 days and 1–100,000 entries, or `"unlimited"` for either limit independently. Missing keys use 30 days and 500 entries. Invalid history fields individually use their default and produce a warning, preserving other preferences. The UI offers exactly three finite choices per setting: 7/30/90 days and 100/500/5,000 entries, plus Unlimited for each. A valid custom TOML value remains visible on the selector without adding another menu option.

Reducing a limit immediately deletes older or excess entries, retaining the newest ones. Limits also apply on startup and when recording a result. Increasing a limit cannot restore deleted entries. A failed preference save leaves the selected policy unchanged. A failed history write preserves the displayed list and reports an error; the saved policy is retried on the next transcription or restart. Preferences and history use temporary files and native file replacement.

Toggle mode, live correction and a local LLM cleanup pass remain planned features. They are not settings available in the current app. The original implementation plan is available in Git history; the [UX flow](docs/spec/ux-flow.md) describes current behavior.

A settings preview keeps the sidebar, uses temporary storage and disables model/capture actions. It seeds 150 synthetic history entries; pass a storage directory to reuse preferences across launches:

```sh
cargo run -p stt-app --example settings_preview -- /tmp/stt-settings-preview
```

## Verification

```sh
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --check
```

CI runs builds and tests on macOS, Windows and Linux. Real model fixture tests require `STT_MODEL_DIR`; model-opening tests marked ignored require their corresponding model environment variables. Unit tests use simulated engines and injectors to exercise the production decoder and delivery components without native permissions or model downloads.

On a macOS desktop, `cargo run -p stt-overlay --example macos_visibility` checks that the bubble appears while the app is inactive without taking focus, hides on release and cancellation, and can appear again. It needs no microphone permission or model. If the Xcode Metal compiler is unavailable, append `--features gpui/runtime_shaders` to compile shaders at runtime for this check.

See [the refactor verification record](docs/readability-verification.md) for the checks performed and native checks still pending.
