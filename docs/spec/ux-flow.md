# UX flow

The current desktop app supports hold-to-talk dictation, Esc cancellation and final-text insertion. The [glossary](glossary.md) distinguishes current behavior from planned capabilities.

## Background operation

Closing Settings hides the window while the global dictation shortcut, downloads and history continue running. A primary click on the system tray icon reveals and focuses the existing Settings window. A secondary click (including Ctrl-click on macOS) opens the menu with Settings… and Quit Plume. Linux hosts that expose only a menu retain Settings… as a fallback. macOS uses the menu bar without a Dock icon; Windows uses the notification area; Linux X11 uses D-Bus StatusNotifierItem. If tray initialization fails, closing Settings exits normally. The isolated settings preview also exits when closed.

## Dictate with hold-to-talk

1. Focus the target application and place the cursor where the text should go.
2. Hold the global shortcut, `Ctrl+Space` by default. Change it in Settings → Dictation; `Fn` is rejected.
3. Speak. The bubble appears and reacts to microphone levels. Partial hypotheses are not inserted into the target application.
4. Release the shortcut. Capture ends and the bubble hides. The background decoder finishes, then the final transcript is inserted.
5. If another capture began before earlier results were delivered, its text is delivered in capture order with a separator when needed.

## Cancel a session

Press `Esc` while holding the shortcut. Capture ends and the cancelled job's result is discarded, including output that arrives later. No partial text needs to be removed. After release the capture is already queued for finalization; the current desktop flow does not cancel it through Esc.

## Insertion and history

Settings → Dictation selects Automatic, Paste or Typing insertion. Clipboard fallback can preserve final text when insertion fails. Settings → History shows insertion results, supports copy/delete/clear actions, and controls retention and maximum entries.

Changing a history limit saves it and immediately prunes older or excess transcriptions. Defaults are 30 days and 500 entries. Each setting offers three finite choices (7/30/90 days and 100/500/5,000 entries) plus Unlimited; the two limits can be disabled independently. Reducing limits deletes entries permanently. Save failures appear in the History page; the displayed list remains intact when history persistence fails.

## Planned toggle mode

A toggle shortcut for recording without holding a key is planned. The domain state machine contains toggle transitions, but the desktop runtime does not expose this interaction.

## Planned cleanup pass

A local LLM cleanup pass is planned. No cleanup model or setting is connected to the current production dictation flow.

## Planned live correction

The domain layer represents partial-text edits. Inserting and replacing partial hypotheses in the target application is not enabled in the current desktop flow.
