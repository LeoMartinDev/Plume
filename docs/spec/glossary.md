# Glossary

These terms describe current behavior unless explicitly marked planned. The session state machine and partial hypothesis types are shared domain concepts; the desktop runtime uses final-text insertion.

## ASR engine

A local automatic speech recognition engine implementing `AsrEngine`: audio chunks go in, hypotheses come out. The production decoder consumes partial hypotheses and returns the first final transcript.

## Bubble

The overlay shown while capturing speech. It reacts to microphone levels and hides after release or cancellation; background decoding can continue after it hides.

## Cleanup pass — planned

An optional local LLM rewrite of the final transcript. No production cleanup pass is connected today.

## Final transcript

The completed text returned by the engine. The desktop app inserts this text once and records its insertion outcome in history.

## Global hotkey

An OS shortcut received outside the focused app. Hold-to-talk defaults to `Ctrl+Space`; the active capture can be cancelled with `Esc`.

## Hold-to-talk

Recording while a shortcut is held, followed by background finalization after release.

## Live correction — planned

Replacement of text already inserted when a partial hypothesis changes. Domain edit types represent this, but the desktop runtime does not insert partial hypotheses.

## Partial hypothesis

An intermediate best guess at the text. It can change during recognition and is ignored by the final-only desktop insertion flow.

## Session

A capture and its transcription result, from shortcut press through release/finalization or cancellation. Captures may overlap pending background decoding; results are delivered in capture order.

## Session state machine

The dependency-free domain model has five states:

- `idle`: no active domain session.
- `recording`: capture has started.
- `streaming`: the domain model has received partial hypotheses.
- `finalizing`: capture has ended and the domain model awaits final text.
- `cancelled`: the session discards hypotheses.

The desktop capture coordinator publishes recording, finalizing and cancellation snapshots; it does not feed decoder hypotheses back into that model. Its background decoder and delivery components handle final results separately.

## Target app

The focused application receiving final text when delivery occurs.

## Toggle mode — planned

Start and stop capture by successive presses. Domain transitions exist, but the desktop runtime exposes hold-to-talk only.

## History policy

Validated retention and size limits for locally saved transcriptions. Defaults are 30 days and 500 entries. Either limit can independently be Unlimited. Reducing limits prunes immediately; startup and new results also apply the policy.
