# Onboarding validation

Automated coverage runs with `cargo test -p plume-app -p plume-session --lib --locked`. It covers legacy preference migration, setup resumption/completion, shortcut validation, idle destination changes, preview completion and crash cleanup without evicting prior recordings. Workspace Clippy checks all targets.

## Native-device acceptance

Use a development app bundle with an isolated config/data root or a disposable user account. Do not reset a user's permissions or delete their existing models to simulate first launch.

1. Fresh preferences: only the dedicated setup window appears. Three model cards sit horizontally, using the existing app colors, typography and controls, with OpenAI/NVIDIA marks, size, speed and accuracy. Nemotron is recommended. The selected card is highlighted without a radio control. The footer has three progress dots (the current step is a pill), no divider and no Back button on the first step. Download remains visible; Next is unavailable until the model and shared speech detector load.
2. Interrupt a download and relaunch. Check resumed progress. Disconnect the network during download, verify the error and retry. A failed or missing model returns setup to Model.
3. Existing preferences without the setup marker: preserve a non-default hold shortcut and Whisper Small. Its current model is displayed separately; continuing does not download a replacement. Other installed models can be selected and loaded without downloading again.
4. Push-to-talk and hands-free appear as two separate cards with a central shortcut control. Configure both. Verify keyboard access, release-to-commit, cancellation, overlap errors and Reset. Typing the configuration shortcut must not start dictation. Return from the permission screen and repeat editing with a suspended session; Reset must release shortcut-edit readiness.
5. A single Permissions card groups Microphone and Accessibility vertically, with actions on the right and an Allowed confirmation once granted. The separate test card below shows the shortcut when ready, capture/transcription status while active, and a scrollable transcript afterward. On macOS: deny microphone permission, grant it later in system settings, and verify live refresh. Repeat for Accessibility. If the event tap requires a relaunch after Input Monitoring changes, verify the guidance and saved progress. On Windows/Linux, verify missing or refused input device and unavailable global shortcuts.
6. After readiness, hold the configured shortcut and speak. The setup screen shows recording, transcription and the final text. No other app receives text, the clipboard is unchanged, and no history entry or saved recording is created. Repeat with silence, Esc and an input device disconnected during capture.
7. Quit during a voiced test, then relaunch. Preview audio and metadata are removed; previous recordings survive. The permission step resumes after revalidating the model.
8. Finish without doing a test. Finish must be disabled while an operation is active or a required permission/service is unavailable. Confirm setup completion is persisted and the native setup window closes immediately from the permissions/test step, without a confirmation screen or delay. Verify normal dictation works in a text editor.
9. Close/reopen setup through the tray while incomplete. Progress survives. After completion the tray opens ordinary settings, with all four models. Without a usable tray, completion reveals settings rather than leaving an inaccessible background app.
10. After completion, make the chosen model unavailable in the disposable environment. Settings must expose the unavailable state and a model repair action, including on the Dictation page.

## Visual preview

`cargo run -p plume-app --example onboarding_preview -- [shortcuts|permissions]` opens a visual-only app with temporary preferences. Ctrl+1/2/3 changes screens; Ctrl+4 verifies the native window handoff to settings. Use it to inspect spacing and keyboard shortcut capture without invoking native permissions, models or microphone capture. Append `--dark` to inspect the dark scheme.
