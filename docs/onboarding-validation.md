# Onboarding validation

Automated coverage runs with `cargo test -p plume-app -p plume-session --lib --locked`. It covers legacy preference migration, setup resumption/completion, shortcut validation, idle destination changes, preview completion and crash cleanup without evicting prior recordings. Workspace Clippy checks all targets.

## Native-device acceptance

Use a development app bundle with an isolated config/data root or a disposable user account. Do not reset a user's permissions or delete their existing models to simulate first launch.

1. Fresh preferences: only the dedicated setup window appears, starting with Languages. Choose interface language (Français or English) independently of dictation language (Automatic, Français or English). Interface changes apply immediately, and returning to the step or relaunching preserves both choices. Continue to three horizontal model cards, with OpenAI/NVIDIA marks, size, speed and accuracy. Nemotron is recommended. The selected card is highlighted without a radio control. The footer has four progress dots (the current step is a pill), no divider and no Back button on Languages. Download remains visible; Next is unavailable until the model and shared speech detector load.
2. Interrupt a download and relaunch. Check resumed progress. Disconnect the network during download, verify the error and retry. A failed or missing model returns setup to Model.
3. Existing preferences without the setup marker: preserve a non-default hold shortcut and Whisper Small. Its current model is displayed separately; continuing does not download a replacement. Other installed models can be selected and loaded without downloading again.
4. Push-to-talk and hands-free appear as two separate cards with a central shortcut control. Configure both. Verify keyboard access, release-to-commit, cancellation, overlap errors and Reset. Typing the configuration shortcut must not start dictation. Return from the permission screen and repeat editing with a suspended session; Reset must release shortcut-edit readiness.
5. A single Permissions card groups Microphone and Accessibility vertically, with actions on the right and an Allowed confirmation once granted. The separate test card below shows the shortcut when ready, capture/transcription status while active, and a scrollable transcript afterward. On macOS: deny microphone permission, grant it later in system settings, and verify live refresh. Repeat for Accessibility. If the event tap requires a relaunch after Input Monitoring changes, verify the guidance and saved progress. On Windows/Linux, verify missing or refused input device and unavailable global shortcuts.
6. After readiness, hold the configured shortcut and speak. The setup screen shows recording, transcription and the final text. No other app receives text, the clipboard is unchanged, and no history entry or saved recording is created. Repeat with silence, Esc and an input device disconnected during capture.
7. Quit during a voiced test, then relaunch. Preview audio and metadata are removed; previous recordings survive. The permission step resumes after revalidating the model.
8. Finish without doing a test. Finish must be disabled while an operation is active or a required permission/service is unavailable. Confirm setup completion is persisted and the native setup window closes immediately from the permissions/test step, without a confirmation screen or delay. Verify normal dictation works in a text editor.
9. Close/reopen setup through the tray while incomplete. Progress survives. After completion the tray opens ordinary settings, with all four models. Without a usable tray, completion reveals settings rather than leaving an inaccessible background app.
10. After completion, make the chosen model unavailable in the disposable environment. Settings must expose the unavailable state and a model repair action, including on the Dictation page.

### Microphone prompt focus (macOS)

With microphone permission not yet requested in a disposable account, click Allow
and answer the native prompt. Verify Plume returns to the foreground after both
Allow and Don't Allow, and the permission row updates accordingly. While the
prompt is open, Plume must not steal focus from it. If setup is deliberately closed
through its close button before completion, the callback must not reopen it.
Opening Privacy & Security through Settings must leave System Settings in front.

## Visual preview

`cargo run -p plume-app --example onboarding_preview -- [languages|shortcuts|permissions]` opens a visual-only app with temporary preferences. Ctrl+0 opens Languages; Ctrl+1/2/3 changes the other screens; Ctrl+4 verifies the native window handoff to settings. Use it to inspect spacing and keyboard shortcut capture without invoking native permissions, models or microphone capture. Append `--dark` to inspect the dark scheme, or `--french` to inspect French text.

### Language preferences

In settings, Interface exposes interface language alongside the theme; Models exposes only dictation language. Change dictation language after loading a model and check both ordinary dictation and the onboarding test use the new language. Switching interface language must preserve the dictation language, model, shortcuts and transcript content. Check French labels on all setup steps and the Dictation, Models, Interface, History and About pages, including a populated history list. Release notes retain their published language; system and engine diagnostics retain their original text. Catalogs live in the root `locales/` directory; see its README for editing, adding a language and English fallback.

Both language controls are dropdown selects. Verify opening either closes the other, the current choice is marked, clicking outside or pressing Escape closes the menu, and keyboard arrows followed by Enter select a language. Tab must move to the next control. Long option lists scroll within a maximum height and the popup stays within the window. Dictation language is disabled while a session or model preparation is busy.

Currently all selectable dictation languages are supported by every model. When adding another language or a restricted model, verify incompatible cards are disabled with a language warning, loading and Finish are blocked, and changing to an unsupported language suspends dictation until a compatible model is loaded. The preferred language must remain selected; never silently fall back to English or automatic detection.

On the model screen, append `--preview-download`, `--preview-connecting`, or
`--preview-loading` to inspect simulated progress without downloading anything.
Progress occupies the bottom of the selected model card; the other cards are
dimmed and the grid/footer keep their positions. Verify both light and dark modes.
