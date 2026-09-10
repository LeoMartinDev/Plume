# Glossary

Domain terms used across the spec, in alphabetical order.

## AsrEngine

The Rust trait in `stt-core` that abstracts the speech recognition engine. Any engine that implements the trait is interchangeable in settings.

## Bubble

The small overlay at the bottom center of the screen. It appears while a session runs, and its animation reacts to the voice level. The bubble is the only permanent UI element.

## Cleanup pass

An optional rewrite of the final transcript by a local LLM. It runs on release, removes filler words, reformats the text, and adapts the style. It is configured in settings and off by default.

## Final transcript

The complete text of a session after the engine finishes. The target app keeps this text. If the cleanup pass is enabled, the final transcript is its input.

## Global hotkey

The system-wide shortcut that controls a session while any app has focus. Two shortcuts exist. The hold-to-talk key records while held. The toggle key starts and stops recording without a held key.

## Hold-to-talk

The default interaction. The session records while the key is held and finalizes on release.

## Live correction

The replacement of a word already inserted in the target app when the partial hypothesis changes. The visible text self-corrects as the engine refines its guess.

## Partial hypothesis

The engine's current best guess at the transcript while audio is still arriving. It streams word by word into the target app and can change until the session finalizes.

## Session

One dictation run, from the hotkey press to finalization or cancel.

## Session state machine

The five states every session moves through.

- `idle`. No session is active.
- `recording`. The hotkey engaged and audio capture runs.
- `streaming`. Partial hypotheses stream into the target app.
- `finalizing`. The key released and the engine completes the final transcript.
- `cancelled`. Esc ended the session. No final transcript is produced.

A session starts in `idle`, returns to `idle` after finalizing, and ends in `cancelled` after Esc.

## Target app

The focused application that receives the text.

## Toggle mode

The interaction started by the toggle shortcut. Recording runs without a held key until the second press finalizes the session. Used for long dictation.
