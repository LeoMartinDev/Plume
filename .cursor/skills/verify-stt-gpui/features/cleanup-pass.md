# Cleanup pass

The cleanup pass is an optional local LLM rewrite of the final transcript. It stays off until enabled in settings. On release it removes filler, reformats the text, and replaces the raw transcript in the target app. Audio still never leaves the machine.

## Sub-features

- `cleanup-settings` enables the pass from settings.
- `cleanup-off-default` leaves dictation unchanged while the pass is off.
- `cleanup-rewrite` replaces the raw transcript with the cleaned text on release.

## How to get to it (user POV)

- Open settings and enable the cleanup pass.
- Dictate with hold-to-talk or toggle mode.
- Release or toggle off, then read the text in the target app.

## Driving it with control-stt-gpui

Preconditions:

- `control-stt-gpui doctor` reports `overlay=present`.
- This checkout has no settings UI and no cleanup pass.

- **Probe the binary.** Run `control-stt-gpui cli`. Exit code `0`. Stdout is the version line. There is no settings command and no rewrite.
- **Probe flags.** Run `control-stt-gpui cli -- --help`. Still the version line. No `settings` or `cleanup` flag.
- **Report.** Mark `cleanup-pass` unreachable. Unmet preconditions are `settings-not-shipped` and `cleanup-pass-not-shipped`. Keep the transcripts as the probe, not as a pass.

## Gotchas

- Off by default is not proven until settings exist and a dictation with the pass disabled can be compared to one with it enabled.
- A local LLM is not running in this checkout. Do not call an external model to fake cleanup.
- Network traffic during a claimed cleanup pass would violate local-only. There is no cleanup process to watch yet.
- The overlay window is not a settings UI.
