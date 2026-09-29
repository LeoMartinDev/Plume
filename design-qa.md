# Design QA

final result: blocked

## History grouped-list pass — 2026-09-29

- Source visual truth: `C:\Users\leoma\AppData\Local\Temp\codex-clipboard-77ecaee0-67bc-4da7-a7c6-ffaf0ad84a56.png`
- Intended state: Light theme, History page with three recent transcription entries.
- Implementation capture: blocked. The native `stt-app` build cannot start in this environment because `whisper-rs-sys` requires a configured Vulkan SDK.
- Viewport and density: unavailable; no implementation screenshot was produced.

### Intended changes

- Replace the per-entry cards with one shared `ListGroup` card and hairline separators.
- Keep application name and relative date as muted, small metadata.
- Replace text Copy/Delete buttons with 24 px Fluent icon buttons while retaining the same copy and deletion handlers.

### Findings

- [P1] Native visual comparison blocked.
  Evidence: `cargo check -p stt-app` stops in `whisper-rs-sys` before the settings window can be launched, reporting that `VULKAN_SDK` is not configured.
  Fix: rerun the native app after installing/configuring the Vulkan SDK, capture the History page at the supplied reference viewport, and compare the grouped list plus hover states.

### Implementation checklist

- [x] Group entries into one card with dividers.
- [x] Make app name and date visually secondary.
- [x] Use Fluent copy and delete icon buttons.
- [ ] Capture and compare the native History view after the Vulkan build prerequisite is available.

## Visual truth

- Shared card treatment: `C:\Users\leoma\AppData\Local\Temp\codex-clipboard-de240549-b64e-45f3-8019-d191ec490f2b.png`
- Appearance content/state: `C:\Users\leoma\AppData\Local\Temp\codex-clipboard-75097c15-484e-4bfe-844b-c6ab4b91ae43.png`
- Dictation content/state: `C:\Users\leoma\AppData\Local\Temp\codex-clipboard-538f83ed-2d6f-4e8c-8801-21e3c407b8af.png`
- Implementations: `design-qa-captures/model.jpg`, `design-qa-captures/appearance.jpg`, and `design-qa-captures/dictation.jpg`
- Combined evidence: `design-comparison.png`

## Capture context

- Platform: Windows native GPUI window.
- Theme: fixed Dark.
- States: Model, Appearance, and Dictation; no menu or transient interaction open.
- CSS/client viewport: 720 x 280 px at 100% scale.
- Native window capture: 722 x 312 px.
- Source images: 750 x 477 px. Each native window was cropped from `(22, 124)` to `(744, 436)`, yielding 722 x 312 px with no density resampling.
- Implementation captures: 722 x 312 px. No density normalization was required.
- Navigation between the three settings pages was tested. The existing settings controls remain wired; download and shortcut capture were not triggered because they change application state.

## Full-view comparison

- The model card is the visual treatment source: 420 px content width, 10 px radius, themed group fill, hairline border, and one inset separator for its two rows.
- Appearance and Dictation now reuse the same shared card primitive at the same x-position and width.
- Single-row pages intentionally omit an internal separator.
- The shell, sidebar, native titlebar, window size, typography, and control placement remain unchanged as requested.

## Focused comparison

A separate crop was unnecessary: all three cards and their small UI text are legible at 1:1 resolution in `design-comparison.png`.

## Fidelity surfaces

- Fonts and typography: unchanged across all views; title, label, metadata, and control weights retain the existing hierarchy.
- Spacing and layout rhythm: card width, row padding, 16 px title gap, 10 px radius, and control alignment are consistent across all three pages.
- Colors and tokens: the shared `group` token is `#F7F7F7` in Light and `#1E1E1E` in Dark; the existing hairline token defines the border and separator.
- Image quality and assets: no raster assets were added or replaced; existing Fluent SVG navigation icons remain unchanged.
- Copy and content: unchanged.

## Findings

- P0: none.
- P1: none.
- P2: none.
- P3: the Computer Use captures show a cursor highlight on Appearance and Dictation; this is capture-only and is not rendered by the app.

## Comparison history

1. Model established the selected grouped-card treatment.
2. Appearance and Dictation still displayed bare rows, creating a cross-view hierarchy mismatch.
3. Added one shared `settings_group` primitive and a theme-aware `group` token.
4. Recaptured all three views at the same native window size and combined them side by side.
5. The final pass found no actionable P0, P1, or P2 mismatch.

## Implementation checklist

- [x] Reuse the same card primitive on Model, Appearance, and Dictation.
- [x] Preserve existing controls, copy, spacing scale, sidebar, and titlebar.
- [x] Keep the model-only separator and avoid unnecessary dividers on single-row cards.
- [x] Verify the native build and all three page states.
