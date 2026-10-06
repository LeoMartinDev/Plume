# Product definition

## The product in one sentence

Plume is a local desktop push-to-talk dictation app. Hold a global shortcut, speak, then release: the final transcript is inserted into the focused app. Native integrations cover macOS, Windows and Linux X11; Wayland remains planned.

## Target user

Anyone who wants local speech-to-text on the desktop, including developers dictating prompts, writers drafting text, and people reducing typing because of strain or injury. They dictate into applications they already use. Recognition stays on the machine without an account or audio upload.

## Language support

French (FR) and English (EN) are the minimum supported languages. Settings offers automatic language detection and explicit French/English selection. The model catalogue records each model's broader language coverage. Recognition quality depends on the selected local model.

## Current capabilities

Hold-to-talk capture, Esc cancellation during capture, background transcription, final-text insertion, clipboard fallback, model selection, configurable history and appearance settings are implemented. Partial hypotheses are not inserted by the desktop runtime.

A separate `stt-shell` command transcribes existing WAV files. It uses the same local engine contracts without the desktop interface.

## Planned capabilities

Toggle recording, live correction, local LLM cleanup and native Wayland integration remain planned. Their presence in historical plans or domain types does not mean they are available in Settings.
