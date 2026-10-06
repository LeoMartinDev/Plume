# Developing Plume

A local desktop dictation app written in Rust and GPUI. Hold `Ctrl+Space`, speak, then release: the final transcript is inserted into the focused application. `Esc` cancels an active capture. Audio stays on the machine; downloading a model requires a network connection.

## Run

```sh
cargo run -p stt-app --bin plume
```

Choose and download a model in Settings → Model. Settings also contains the shortcut, language, insertion mode, appearance and transcription history. On macOS, global shortcuts and insertion require the appropriate Accessibility/Input Monitoring permissions, and capture requires microphone permission.

The app lives in the macOS menu bar under **Plume** (without a Dock icon), the Windows notification area, or the Linux system tray. Closing Settings hides the window while dictation shortcuts, model downloads and transcription history continue running. Left-click the tray icon to reveal and focus the same Settings window. Right-click (or Ctrl-click on macOS) opens the menu with **Settings…** and **Quit Plume**. Some Linux tray hosts expose only the menu; use **Settings…** there. The isolated settings preview still quits when closed.

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

The CI installs platform prerequisites: ALSA/XCB/XKB development libraries on Linux and the Vulkan SDK on Windows. See [.github/workflows/ci.yml](../.github/workflows/ci.yml) for the exact dependencies. Native integrations exist for macOS, Windows and Linux X11; Wayland support remains planned.

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

## Application icon and native packaging

The Plume P is used in the settings sidebar and system tray. Windows embeds it in the executable, window and taskbar. Linux settings windows publish it through X11; the desktop launcher uses the installed `plume` icon. macOS sets the running app icon and includes it in the native bundle while retaining menu-bar-only behavior.

Build a native package on each platform with Python 3:

```sh
python3 scripts/package-app.py
```

Outputs are `target/package/Plume.app` on macOS, `target/package/plume-windows` on Windows (executable and Vulkan loader), and `target/package/plume-linux` on Linux (`bin` and `share` directories). `CARGO_TARGET_DIR` is respected. Use `--profile dev --skip-build` to package an existing debug build.

On macOS, open `Plume.app` or copy it to Applications to see the icon in Finder/Launchpad. The local bundle is ad-hoc signed; public distribution requires signing and notarization. On Linux, copy the package's `bin` and `share` contents into a prefix such as `~/.local`, with its `bin` on `PATH`, and refresh the desktop icon cache if needed. On Windows, Explorer and shortcuts use the embedded icon without a separate icon file.

Icon sources and platform exports are kept in `crates/stt-app/assets/brand`, so packaging does not depend on the design-output folder.

## Configuration and history

Plume keeps the legacy `stt` storage folder to preserve existing installations. The app stores `prefs.toml` under the OS configuration root in `stt`, and `history.json` and model packs under the OS local-data root in `stt`. On macOS both roots normally resolve to `~/Library/Application Support/stt`; on Linux they normally resolve to `~/.config/stt` and `~/.local/share/stt`; on Windows configuration and local data use the respective roaming and local app-data directories.

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

Toggle mode, live correction and a local LLM cleanup pass remain planned features. They are not settings available in the current app. The original implementation plan is available in Git history; the [UX flow](spec/ux-flow.md) describes current behavior.

A settings preview keeps the sidebar, uses temporary storage and disables model/capture actions. It seeds 150 synthetic history entries; pass a storage directory to reuse preferences across launches:

```sh
cargo run -p stt-app --example settings_preview -- /tmp/stt-settings-preview
```

## Logging

App diagnostics use `tracing`. Development builds default to `debug` for the app's crates and `warn` for dependencies. Release builds (`cargo run --release -p stt-app`) default to warnings and errors. `RUST_LOG` overrides either default at startup; an empty or invalid filter uses the build's default.

Use `RUST_LOG=warn` for quiet development, or `RUST_LOG=warn,stt_app=debug,stt_audio=debug,stt_engine=debug,stt_session=trace,stt_overlay=debug,stt_hotkey=debug,stt_inject=debug` for detailed diagnosis in either build. For example, in PowerShell:

```powershell
$env:RUST_LOG = 'warn,stt_session=trace,stt_audio=debug,stt_engine=debug'
$env:STT_LOG_FILE = "$PWD\stt-diagnostic.log"
cargo run --release -p stt-app
```

Logs go to stderr unless `STT_LOG_FILE` specifies a file to append to. This also makes diagnostics available in Windows release builds without a console. If the file cannot be opened, logging falls back to stderr. Restart the app after changing the environment variables; unset them to restore defaults.

The per-sample audio statistics (RMS, peak, sample counts and nonfinite samples) run only when `trace` is enabled for `stt_session::decoder`. Otherwise the decoder uses the original audio stream without the diagnostic allocation, mutex or sample scan. Capture/decoder timers run only at `debug` or above. Diagnostics record lengths and timings rather than transcript contents. Native engine libraries may still emit their own messages independently of this filter. CLI results, usage and command errors remain ordinary output.

## Verification

Release automation requires Node.js 22+ and Git/Cargo. It uses `.mjs` files and
Node built-ins only, with no npm install or Python. Run its integration tests with:

```sh
node --test scripts/*.test.mjs
```

The native packaging integration test is opt-in: set `STT_PACKAGE_TEST_BUILD` to
the directory containing already built `plume`/`stt-shell` and native runtimes.
It assembles only a temporary fixture archive and does not contact GitHub. The
release jobs always test their fresh release archives. Local Windows packaging
also needs `dumpbin` on PATH and `VCToolsRedistDir` from a Visual Studio developer
environment; Linux needs `patchelf`, and macOS uses the Xcode command-line tools.

## Releases

All workspace crates share a stable `MAJOR.MINOR.PATCH` version. From a clean
checkout at the repository root, prepare Markdown notes outside the checkout
(an untracked notes file inside the checkout counts as dirty). The script accepts
`--notes-file FILE`, `--notes TEXT`, or `--notes-file -` to read stdin. Notes are
saved verbatim as `releases/vVERSION.md` and must be non-empty.

```sh
# Inspect the plan without changing files, the index, refs, or fetching.
node release.mjs patch --notes-file ../notes.md --dry-run

# On main, increment every crate, commit the release and create an annotated tag.
node release.mjs patch --notes-file ../notes.md
# minor resets patch; major resets minor and patch.
node release.mjs minor --notes "## Changes: new functionality"

# To prepare, commit, tag and push in one operation (unprotected main only):
git switch main
git pull --ff-only origin main
node release.mjs patch --notes-file ../notes.md --push
```

Without `--push`, commits and tags remain local. With `--push`, the script fetches
`origin/main` and requires HEAD to equal it **before** creating the release commit.
It then pushes main and the tag atomically, without forcing. A concurrent main
update or server rejection aborts the push; the local commit/tag remain available
for inspection. After a failed push, inspect the remote before retrying an explicit
`git push --atomic origin main vVERSION`; do not rerun the increment command.
If the commit fails, the original release files and index are restored and new
notes removed. Existing tags, dirty checkouts, mismatched crate/lockfile versions
and empty notes are refused. External dependency versions and checksums are
preserved. Version values may be literal package versions or inherited from
`[workspace.package]`. Tags are created only on `main`.

For **protected main**, use a PR to merge the version and notes first:

```sh
git switch main
git pull --ff-only origin main
git switch -c release/next
node release.mjs patch --notes-file ../notes.md --prepare-only
git add Cargo.toml Cargo.lock crates releases
git commit -m "Prepare next release"
git push -u origin release/next
# Open and merge the PR after CI succeeds, then:
git switch main
git pull --ff-only origin main
node release.mjs tag --dry-run
node release.mjs tag --push
```

`--prepare-only` changes files without a commit or tag and works on a PR branch.
`tag` reads the already merged version and committed notes, creates an annotated
tag at HEAD, and performs no increment or new commit. Its atomic push includes
main (unchanged) and the tag. Tag creation must also be permitted by repository
rules. To check a tag locally, check out its commit and run
`node release.mjs --check-tag vVERSION`.

The CI keeps PR/main validation and runs release jobs only on a canonical
`vMAJOR.MINOR.PATCH` tag. It checks the annotated tag, all versions, Cargo.lock,
committed Markdown notes and that the commit is an ancestor of `origin/main`.
Workspace compilation/tests, Clippy and formatting must succeed before release
packaging. Three native builds produce both `plume` and `stt-shell`: Linux x64
(Ubuntu 24.04/glibc), Windows x64 and macOS Apple Silicon (macOS 15).
These are native archives, not installers; models are downloaded separately.

Packaging retains the Windows Vulkan SDK prerequisite and bundles non-system
runtime libraries recursively, including DirectML, Vulkan loader and MSVC runtime
DLLs when imported. Linux includes ALSA/XCB/XKB and other non-glibc shared
libraries with `$ORIGIN` paths; macOS uses `@loader_path` for bundled dylibs.
Core OS libraries/frameworks and GPU drivers remain supplied by the OS.
Every `.zip` (Windows) or `.tar.gz` (Linux/macOS) has a `.sha256` checksum.
The CI extracts each archive, checks its library paths and runs `stt-shell --help`
with an OS-only PATH and no build-machine library environment variables. This
tests startup/linking; microphone, GUI, GPU and model inference still require
manual testing on the intended machines.

Only after all jobs pass does CI create a **draft** GitHub release with the Markdown
notes and all six assets. To rebuild an existing unpublished tag with the latest
workflow fixes, run the CI workflow manually from main with `release_tag` set to
that tag (for example, `v0.1.0`). It checks out the original tagged code and restores
the annotated tag before validation; the tag is not moved. Rerunning refreshes that draft and its
assets; a published release is refused. Review the draft before publishing it
manually. Only the repository's automatic `GITHUB_TOKEN` is used; no signing
secrets are required.

**Archives are not signed by a publisher and are not notarized.** macOS applies
only local ad-hoc signatures after relocation (needed for Apple Silicon), with no
certificate or verified publisher identity. Gatekeeper/SmartScreen may block or
warn about downloads; review the source/checksums and follow your organization's
policy. Checksums detect corruption; they do not establish publisher identity.

## Rust verification

```sh
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --check
```

CI runs builds and tests on macOS, Windows and Linux. Real model fixture tests require `STT_MODEL_DIR`; model-opening tests marked ignored require their corresponding model environment variables. Unit tests use simulated engines and injectors to exercise the production decoder and delivery components without native permissions or model downloads.

On a macOS desktop, `cargo run -p stt-overlay --example macos_visibility` checks that the bubble appears while the app is inactive without taking focus, hides on release and cancellation, and can appear again. It needs no microphone permission or model. If the Xcode Metal compiler is unavailable, append `--features gpui/runtime_shaders` to compile shaders at runtime for this check.

See [the refactor verification record](readability-verification.md) for the checks performed and native checks still pending.
