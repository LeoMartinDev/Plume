# UX flow

The current desktop app supports hold-to-talk and hands-free dictation, Esc cancellation during capture or transcription, and final-text insertion. The [glossary](glossary.md) distinguishes current behavior from planned capabilities.

## Background operation

Closing Settings hides the window while the global dictation shortcut, downloads and history continue running. A primary click on the system tray icon reveals and focuses the existing Settings window. A secondary click (including Ctrl-click on macOS) opens the menu with Settings… and Quit Plume. Linux hosts that expose only a menu retain Settings… as a fallback. macOS uses the menu bar without a Dock icon; Windows uses the notification area; Linux X11 uses D-Bus StatusNotifierItem. If tray initialization fails, closing Settings exits normally. The isolated settings preview also exits when closed.

## Dictate with hold-to-talk

1. Focus the target application and place the cursor where the text should go.
2. Hold the global shortcut, `Ctrl+Space` by default. Change it in Settings → Dictation; `Fn` is rejected.
3. Speak. The bubble appears and reacts to microphone levels. Partial hypotheses are not inserted into the target application.
4. Release the shortcut. Capture ends; the bubble switches to three dots while transcription finishes, then a small progress indicator during insertion. On completion, the black bubble gently contracts and fades away in 320 ms. Normal dictation never displays transcript text in the bubble.
5. One operation occupies Plume until insertion finishes. Presses during starting, transcription, insertion or cancellation are ignored; release and press again when ready. Completion indicates dispatch, rather than an acknowledgment from every target application.

## Cancel a session

Press `Esc` during microphone opening, capture, transcription or manual retranscription. Capture stops without the normal 120 ms release tail, the worker aborts and the audio is deleted. The bubble remains busy until microphone closure, worker return and deletion are confirmed. A paste already dispatched is not cancelled. No late result is inserted.

## Insertion and history

Settings → Dictation selects Automatic, Paste or Typing insertion. Clipboard fallback can preserve final text when insertion fails. Settings → History shows insertion results, supports copy/delete/clear actions, and controls retention and maximum entries.

Changing a history limit saves it and immediately prunes older or excess transcriptions. Defaults are 30 days and 500 entries. Each setting offers three finite choices (7/30/90 days and 100/500/5,000 entries) plus Unlimited; the two limits can be disabled independently. Reducing limits deletes entries permanently. Save failures appear in the History page; the displayed list remains intact when history persistence fails.

## Hands-free recording

Press the toggle shortcut, `Ctrl+Shift+Space`, once to start, then press it again to stop. Releasing this shortcut does not stop recording; hold-to-talk gestures cannot control its take. Change either shortcut in Settings → Dictation. The editor temporarily receives configured shortcuts without starting a dictation, and rejects normalized collisions. Older preferences that conflict with the new default keep their shortcuts and disable hands-free with a warning.

Timers begin when the microphone delivers audio. After thirty seconds without speech, hands-free recording displays a reminder that clears on speech. Every take warns at nine minutes and stops for transcription at ten minutes. Speech detection is shared between local engines; releasing a silent take keeps the black bubble at its recording size, gives it a small 220 ms shake, then fades it away within 400 ms and saves no audio. Reduced-motion preferences keep the bubble still until it disappears.

## Audio recovery

The eight latest voiced takes remain available independently of text-history retention, including STT failures. A ninth voiced take evicts the oldest audio. The active recording is a separate temporary PCM16 mono 16 kHz WAV, written progressively. Storage must be writable before microphone opening; a write error stops capture explicitly.

On restart, interrupted voiced takes become recoverable in History without automatic insertion. Retranscribe reads the existing WAV by buffers with the selected model and language frozen at launch, updates the same take and preserves its previous insertion outcome. Recovered text can be read or copied. A new result starts its own text-retention period; the recording's original age and audio eviction order remain unchanged. Delete audio alone, or delete an entry/clear history to remove associated audio.

## Delivery compatibility

Only boundary spaces can change when macOS Accessibility or Windows UI Automation supplies reliable adjacent characters and selection. Unknown context, including X11, receives the exact transcript. A focus change discards the old context and delivers to the currently active target. Known sensitive/read-only fields are refused; Plume never restores an earlier focus.

Paste snapshots the clipboard, stages text, dispatches paste, waits 300 ms, then restores supported content if the native version still belongs to the transaction. A new user copy survives. Empty content, text, HTML with text and images are supported. An unsupported snapshot uses typing in Automatic mode and reports a limitation in explicit Paste mode. After a paste dispatch error, typing is never attempted as a second insertion. Recovery copies intentionally keep the transcription in the clipboard.

## Planned cleanup pass

A local LLM cleanup pass is planned. No cleanup model or setting is connected to the current production dictation flow.

## Planned live correction

The domain layer represents partial-text edits. Inserting and replacing partial hypotheses in the target application is not enabled in the current desktop flow.
