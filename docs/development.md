# Developing Plume

A local desktop dictation app written in Rust and GPUI. Hold `Ctrl+Space`, speak, then release, or use `Ctrl+Shift+Space` hands-free: the final transcript is inserted into the focused application. `Esc` cancels capture or transcription. Audio stays on the machine; downloading a model requires a network connection.

## Run

```sh
cargo run -p plume-app --bin plume
```

First launch opens the dedicated onboarding window: Model → Shortcuts → Permissions & test. The model must load before shortcut configuration; permissions and device/session readiness must pass before finishing. The optional test routes its transcript into onboarding and deletes its temporary audio, including after a crash. Existing preferences enter onboarding once without losing their model or shortcuts. Settings afterward contains the full model catalogue, shortcut, language, insertion mode, appearance and transcription history. On macOS, global shortcuts and insertion require the appropriate Accessibility/Input Monitoring permissions, and capture requires microphone permission.

For an isolated visual preview, run `cargo run -p plume-app --example onboarding_preview`, optionally with `-- shortcuts` or `-- permissions`. Ctrl+1/2/3 switches preview screens. This preview uses temporary preferences and never downloads models, requests permissions, or starts native dictation. See [onboarding validation](onboarding-validation.md) for device checks.

The app lives in the macOS menu bar under **Plume** (without a Dock icon), the Windows notification area, or the Linux system tray. Closing Settings hides the window while dictation shortcuts, model downloads and transcription history continue running. Left-click the tray icon to reveal and focus the same Settings window. Right-click (or Ctrl-click on macOS) opens the menu with **Settings…** and **Quit Plume**. Some Linux tray hosts expose only the menu; use **Settings…** there. The isolated settings preview still quits when closed.

Linux uses X11 for the app window and D-Bus StatusNotifierItem for the tray, without a GTK dependency. The desktop must provide a StatusNotifier host (for example KDE Plasma, or GNOME with an AppIndicator extension). If tray initialization fails, closing Settings quits normally so the app remains accessible. A Wayland-only session remains unsupported.

For the standalone session runner, provide an installed model directory:

```sh
PLUME_MODEL_DIR=/path/to/model cargo run -p plume-session
```

Its environment configuration is `PLUME_MODEL_DIR` (required), `PLUME_HOLD` (default `Ctrl+Space`), `PLUME_TOGGLE` (default `Ctrl+Shift+Space`, empty disables it) and `PLUME_CANCEL` (default `Esc`). `Fn` shortcuts are rejected.

The CI installs platform prerequisites: ALSA/XCB/XKB development libraries on Linux and the Vulkan SDK on Windows. See [.github/workflows/ci.yml](../.github/workflows/ci.yml) for the exact dependencies. Native integrations exist for macOS, Windows and Linux X11; Wayland support remains planned.

## Architecture

| Component | Responsibility |
| --- | --- |
| `plume-core` | Dependency-free audio/transcript types, contracts and session state machine |
| `plume-audio` | Microphone capture and sample conversion |
| `plume-engine` | Local models, preprocessing and decoding |
| `plume-hotkey` | Global shortcuts and OS-specific listeners |
| `plume-inject` | Insertion into the focused application |
| `plume-session` | Capture coordination, background decoding and ordered delivery |
| `plume-overlay` | Voice-level bubble and native window behavior |
| `plume-ui` | Shared controls, palette and UI refresh interval |
| `plume-app` | Startup, model downloads, preferences, history and settings |

`plume-session::controller` owns one operation through Ready → Starting → Recording(Hold/Toggle) → Transcribing → Inserting → Ready. Cancellation waits for the microphone, decoder and audio deletion before readiness. Native bindings share one hook/service and report action and press/release edges. Busy presses are consumed; no STT queue, capture joining or timing-based separators remain. The model/language snapshot is fixed at launch. Only the final transcript is delivered.

Settings highlights the model connected to the session, not the saved preference. A model being activated shows `Loading…` until startup completes; while another model downloads or loads, the previous session model remains marked `In use`. A default preference without an installed model is not marked as active.

The CPAL callback copies native samples into a bounded queue; conversion, stateful mono/16 kHz normalization, Silero CPU VAD and progressive WAV writes run outside it. Silero receives 512-sample windows, threshold 0.5, no minimum speech duration and a fresh recurrent state. STT starts with 250 ms of pre-roll on the first voiced window and keeps subsequent pauses. Release retains 120 ms of tail; cancellation has no tail. Monotonic timers begin after microphone readiness: 30-second hands-free silence reminder, nine-minute warning and ten-minute capture limit.

`RecordingStore` retains eight voiced audios plus an active partial WAV. Flushes refresh its header each second; versioned JSON metadata is atomically replaced (ReplaceFileW on Windows). Interrupted voiced takes are recovered manually at startup. Text is persisted before insertion. Retry reads buffers through the same decoder and VAD, updates an existing take, preserves its insertion status and never injects. Audio-only deletion and explicit history clearing remove associated files; automatic text retention remains independent. Acknowledged evicted metadata is removed, and expired stored text is cleared independently of retained audio.

Within `plume-session`, `runtime` coordinates the event-driven controller, capture and configuration updates, `decoder` owns the single worker, `recordings` owns durable takes, `delivery` owns final insertion, and `startup` connects the native adapters. Within `plume-app`, preference types are separate from TOML conversion and storage; settings actions are grouped by shortcut, preferences, models and history.

## Application icon and native packaging

The Plume P is used in the settings sidebar and system tray. Windows embeds it in the executable, window and taskbar. Linux settings windows publish it through X11; the desktop launcher uses the installed `plume` icon. macOS sets the running app icon and includes it in the native bundle while retaining menu-bar-only behavior.

Build a native package on each platform with Python 3:

```sh
python3 scripts/package-app.py
```

Outputs are `target/package/Plume.app` on macOS, `target/package/plume-windows` on Windows (executable and Vulkan loader), and `target/package/plume-linux` on Linux (`bin` and `share` directories). `CARGO_TARGET_DIR` is respected. Use `--profile dev --skip-build` to package an existing debug build.

On macOS, open `Plume.app` or copy it to Applications to see the icon in Finder/Launchpad. The local bundle is ad-hoc signed; public distribution requires signing and notarization. On Linux, copy the package's `bin` and `share` contents into a prefix such as `~/.local`, with its `bin` on `PATH`, and refresh the desktop icon cache if needed. On Windows, Explorer and shortcuts use the embedded icon without a separate icon file.

Icon sources and platform exports are kept in `crates/plume-app/assets/brand`. Regenerate the native icons with `node scripts/generate-icons.mjs` (requires `sharp`; macOS also uses `iconutil` for ICNS).

## Configuration and history

The app stores `prefs.toml` under the OS configuration root in `plume`, and `history.json` and model packs under the OS local-data root in `plume`. On macOS both roots normally resolve to `~/Library/Application Support/plume`; on Linux they normally resolve to `~/.config/plume` and `~/.local/share/plume`; on Windows configuration and local data use the respective roaming and local app-data directories.

Example preferences:

```toml
hold = "Ctrl+Space"
toggle = "Ctrl+Shift+Space"
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

Live correction and a local LLM cleanup pass are not connected to the production flow. The [UX flow](spec/ux-flow.md) describes current behavior and the [dictation validation](dictation-validation.md) records automated checks and remaining native acceptance sessions.

A settings preview keeps the sidebar, uses temporary storage and disables model/capture actions. It seeds 150 synthetic history entries; pass a storage directory to reuse preferences across launches:

```sh
cargo run -p plume-app --example settings_preview -- /tmp/plume-settings-preview
```

## Logging

App diagnostics use `tracing`. Development builds default to `debug` for the app's crates and `warn` for dependencies. Release builds (`cargo run --release -p plume-app`) default to warnings and errors. `RUST_LOG` overrides either default at startup; an empty or invalid filter uses the build's default.

Use `RUST_LOG=warn` for quiet development, or `RUST_LOG=warn,plume_app=debug,plume_audio=debug,plume_engine=debug,plume_session=trace,plume_overlay=debug,plume_hotkey=debug,plume_inject=debug` for detailed diagnosis in either build. For example, in PowerShell:

```powershell
$env:RUST_LOG = 'warn,plume_session=trace,plume_audio=debug,plume_engine=debug'
$env:PLUME_LOG_FILE = "$PWD\plume-diagnostic.log"
cargo run --release -p plume-app
```

Logs go to stderr unless `PLUME_LOG_FILE` specifies a file to append to. This also makes diagnostics available in Windows release builds without a console. If the file cannot be opened, logging falls back to stderr. Restart the app after changing the environment variables; unset them to restore defaults.

Session logs contain identifiers, registered dictation shortcuts, recognized shortcut press/release actions, state transitions, durations and errors. Unrelated keystrokes are never logged. They never include audio, transcripts, adjacent-field text or clipboard snapshots. Whisper/GGML log bodies are suppressed because native debug builds can include decoded tokens; the session reports typed engine errors. The version probe remains ordinary output.

## Verification

Release automation requires Node.js 22+ and Git/Cargo. It uses `.mjs` files and
Node built-ins only, with no npm install or Python. Run its integration tests with:

```sh
node --test scripts/*.test.mjs
```

The native packaging integration test is opt-in: set `PLUME_PACKAGE_TEST_BUILD` to
the directory containing built `plume`, `plume-updater` and native runtimes.
It builds a temporary installer without contacting GitHub. Windows requires
Inno Setup 6, `dumpbin` and `VCToolsRedistDir`; Linux requires `patchelf` and
`dpkg-deb`; macOS requires the Xcode command-line tools. Release jobs build and
verify fresh installers on their native runners.

On macOS, `PLUME_DMG_TEST=1 node --test scripts/macos-dmg.test.mjs` checks a
real disk image with a small app fixture. `PLUME_DMG_TEST=1 cargo test --locked
-p plume-updater` also exercises DMG updates, wrong-version rejection and bundle
replacement. These checks use temporary folders and leave installed apps alone.

The DMG's saved Finder layout is `crates/plume-app/packaging/dmg-layout.ds-store`.
Release builds copy it directly and need no Finder automation. To change the
window or icon positions, regenerate it with `python3 scripts/generate-dmg-layout.py`
in a development environment with `ds_store==1.3.3` installed.

Windows CI keeps Cargo artifacts in `C:\t` to avoid long paths. The cache action
uses `cache-directories` for this absolute path, since `workspaces` target paths
are relative to the checkout. Windows debug/test and release installer jobs use
separate cache keys; Rust and dependency changes still invalidate the cache through
`rust-cache`'s automatic keys. The first run for each new key builds a fresh cache.

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
packaging. Three native builds produce Plume and its update helper: Linux x64
(Ubuntu 24.04/glibc), Windows x64 and macOS Apple Silicon (macOS 15).
Windows ships an Inno Setup `.exe` with Start menu shortcuts and an uninstaller.
macOS ships a `.dmg` opening a compact Finder window with Plume.app and an
Applications shortcut. Drag Plume to Applications, eject the image, then launch
the installed app. Linux ships a `.deb`
installing Plume in `/opt/plume` with a desktop entry and icon. Models are
downloaded separately. Public releases contain no ZIP or tar archives.

Packaging retains the Windows Vulkan SDK prerequisite and bundles non-system
runtime libraries recursively, including DirectML, Vulkan loader and MSVC runtime
DLLs when imported. Linux includes ALSA/XCB/XKB and other non-glibc shared
libraries with `$ORIGIN` paths; macOS uses `@loader_path` for bundled dylibs.
Core OS libraries/frameworks and GPU drivers remain supplied by the OS.
Every installer has a `.sha256` checksum. CI installs the Windows package in a
temporary folder or extracts the macOS/Linux payload, audits runtime dependencies
and runs `plume --version` and `plume-updater --help` with an OS-only PATH and no
build-machine library environment variables. Linux runners also install the
package in its system location; macOS runners mount the DMG, check the saved
Finder layout and Applications shortcut, copy the bundle into Applications,
and verify its signature and startup. This
tests startup/linking; microphone, GUI, GPU and model inference still require
manual testing on the intended machines.

Only after all jobs pass does CI create a **draft** GitHub release with the Markdown
notes and all six assets. To rebuild an existing unpublished tag with the latest
workflow fixes, run the CI workflow manually from main with `release_tag` set to
that tag. Installer tooling must already be present in the tag; historical archive
releases cannot be converted by rebuilding their immutable tags. It checks out the original tagged code and restores
the annotated tag before validation; the tag is not moved. The native DMG fixture
comes from the workflow revision, while its installer imports and application
binaries remain those of the tag. Rerunning refreshes that draft and its
assets; a published release is refused. Review the draft before publishing it
manually. Only the repository's automatic `GITHUB_TOKEN` is used; no signing
secrets are required.

**Installers are not signed by a publisher and are not notarized.** macOS applies
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

CI runs builds and tests on macOS, Windows and Linux. Real model fixture tests require `PLUME_MODEL_DIR`; model-opening tests marked ignored require their corresponding model environment variables. Unit tests use simulated engines and injectors to exercise the production decoder and delivery components without native permissions or model downloads.

On a macOS desktop, `cargo run -p plume-overlay --example macos_visibility` attaches the overlay asynchronously after another window, as production does after loading a model. It checks that the bubble appears while the app is inactive without taking focus, stays visible during transcription after release, hides on cancellation, can appear again, and expands into an error card. It also rejects logged GPUI borrow errors and checks that GPUI's viewport tracks native size changes. It needs no microphone permission or model. If the Xcode Metal compiler is unavailable, append `--features gpui/runtime_shaders` to compile shaders at runtime for this check.

To preview the new feedback without a microphone, model or actual insertion, run `cargo run -p plume-overlay --example feedback_preview`. It shows recording, transcription, insertion, success, then an insertion failure card. Use `-- failure` to show the Copy action immediately, or `-- copied` for the automatic clipboard fallback message. These examples use synthetic text and do not write dictation history.

Insertion failures retain their text in the recovery card until dismissed and also go through the normal history storage path. Copy retries clipboard access; a failed copy keeps the text and explains that it can be retried. Further dictation temporarily hides recovery cards without discarding them. Windows uses a non-activating overlay; macOS keeps its non-activating panel. Windows and macOS animation preferences are read at startup; `PLUME_REDUCED_MOTION=1` disables progress animation and card expansion on any platform.

Before accepting this change, test dictation into another app, release, cancellation, repeated recordings, and tray reopening. In the isolated preview, test Copy, Ctrl+V (Cmd+V on macOS), dismissal, and the copied fallback message. Native focus and visual behavior still need verification on each supported OS.

See [the refactor verification record](readability-verification.md) for the checks performed and native checks still pending.

## Installer updates

Settings → Updates downloads the native installer and verifies its size and SHA-256
before closing Plume. The helper waits for Plume to exit, then runs the Windows
installer in the existing per-user directory, mounts the macOS DMG, or asks
PolicyKit to install the Linux package with dpkg. On macOS the helper copies the
app beside the existing bundle, verifies its signature and version, then replaces
the bundle in its current location and restarts Plume. It keeps the old bundle
until replacement succeeds and restores it if replacement fails. If the app
folder is not writable, install the DMG manually. Models, preferences and history
are outside the installation and remain intact. Linux system installs can request
administrator authentication. Earlier macOS updaters expect `.pkg` assets, so their
users must install the first DMG release manually from GitHub. The previous 0.1.0
updater expects archives and also requires a manual installation on all platforms.
