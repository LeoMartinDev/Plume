# Dictation flow

This guide covers the four flows of a dictation session. Hold-to-talk handles short text. Toggle mode handles long dictation. Esc cancels. The cleanup pass rewrites the result. The [glossary](glossary.md) defines the session states these flows move through.

## Dictate with hold-to-talk

1. Click into the app that receives the text. This is the target app.
2. Hold the global shortcut. The default is `Fn` where the OS reports it, or `Ctrl+Space` elsewhere. Both are configurable in settings.
3. Speak. A bubble appears at the bottom center of the screen. Its animation reacts to your voice level.
4. Watch the text stream word by word into the target app. A word already inserted can change while the engine refines its guess. This is live correction.
5. Release the shortcut. The transcript finalizes and the bubble disappears.

## Dictate a long session with toggle mode

Hold-to-talk tires the hand during long dictation. Toggle mode records without a held key.

1. Press the toggle shortcut once. Recording starts and the bubble appears.
2. Speak for as long as you need. The text streams as in hold-to-talk.
3. Press the toggle shortcut again. The transcript finalizes and the bubble disappears.

## Cancel a session

1. Press `Esc` while a session runs.
2. The session is cancelled and produces no final transcript. Text the session already inserted is removed where the target app allows it.

## Clean up a transcript with the local LLM

The cleanup pass rewrites the final transcript on release. It removes filler words, reformats the text, and adapts the style. It runs on the local machine and stays off until you enable it.

1. Open settings and enable the cleanup pass.
2. Dictate as usual.
3. Release the shortcut. The local LLM rewrites the final transcript, and the cleaned text replaces the raw transcript in the target app.
