# Glossary

These terms describe current behavior unless explicitly marked planned. The session state machine and partial hypothesis types are shared domain concepts; the desktop runtime uses final-text insertion.

## ASR engine

A local automatic speech recognition engine implementing `AsrEngine`: audio chunks go in, hypotheses come out. The production decoder consumes partial hypotheses and returns the first final transcript.

## Bubble

The overlay shown while capturing speech. It reacts to microphone levels and represents microphone opening, capture, transcription, insertion and cancellation. It also shows silence/limit reminders and recovery errors.

## Cleanup pass — planned

An optional local LLM rewrite of the final transcript. No production cleanup pass is connected today.

## Final transcript

The completed text returned by the engine. The desktop app inserts this text once and records its insertion outcome in history.

## Global hotkey

An OS shortcut received outside the focused app. Hold-to-talk defaults to `Ctrl+Space`, hands-free to `Ctrl+Shift+Space`, and capture/transcription cancellation to `Esc`. One native service observes all three actions and physical press/release edges.

## Hold-to-talk

Recording while a shortcut is held, followed by background finalization after release.

## Live correction — planned

Replacement of text already inserted when a partial hypothesis changes. Domain edit types represent this, but the desktop runtime does not insert partial hypotheses.

## Partial hypothesis

An intermediate best guess at the text. It can change during recognition and is ignored by the final-only desktop insertion flow.

## Session

A capture and its transcription result, from shortcut press through release/finalization or cancellation. A take cannot overlap another take, pending decoding, insertion or manual recovery.

## Session state machine

The desktop controller in `plume-session` uses `Ready`, `Starting`, `Recording(Hold|Toggle)`, `Transcribing`, `Inserting` and `Cancelling`. One operation owns the controller until its native resources are released. Manual recovery uses the same worker and never inserts text. The older domain edit/state types remain available, but do not drive the desktop runtime.

## Target app

The focused application receiving final text when delivery occurs.

## Toggle mode

Start and stop hands-free capture by successive presses of its own shortcut. Its release and other start gestures do not stop recording.

## History policy

Validated retention and size limits for locally saved transcriptions. Defaults are 30 days and 500 entries. Either limit can independently be Unlimited. Reducing limits prunes immediately; startup and new results also apply the policy.
