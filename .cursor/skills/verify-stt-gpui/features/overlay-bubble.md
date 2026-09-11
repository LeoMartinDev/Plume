# Overlay bubble

The overlay bubble is a GPUI window at the bottom center of the screen. One text line shows the latest partial. One marker mirrors `SessionState`: idle, recording, streaming, finalizing, or cancelled. This checkout opens that window from the `stt-overlay` binary. It does not start a microphone, bind a hotkey, or inject text.

## Sub-features

- `overlay-window` maps a window titled `stt-overlay`.
- `overlay-partial` shows the latest partial after each hypothesis.
- `overlay-state` shows idle, recording, streaming, finalizing, or cancelled.
- `overlay-cancel-swallow` keeps cancelled and empty text when a late partial or final arrives after Esc.

## How to get to it (user POV)

- Run the `stt-overlay` binary from a workspace or isolated build.
- Do not run `stt-shell`. That program still prints a version line and exits.
- Do not press a dictation shortcut. No hotkey is registered.

## Driving it with control-stt-gpui

Preconditions:

- `control-stt-gpui doctor` reports `overlay=present` and `hotkey=absent`.
- Launch already built `stt-shell` into `/tmp/stt-gpui-verify-$RUN_ID/`. Overlay builds into that same target directory.
- Unset `WAYLAND_DISPLAY` and `ZED_HEADLESS` so gpui 0.2.2 uses X11 when `DISPLAY` is set. The overlay command does this.
- Linux CI needs the `libxkbcommon-x11-dev` package so rustc can link `-lxkbcommon-x11`. Do not commit a local `.so` symlink.

- **State.** Run `control-stt-gpui overlay`. Exit code `0`. The cargo-test transcript asserts literal text and state after each scripted hypothesis: idle and empty, recording and empty after hold, `bonj` and streaming, `bonjour` and streaming, `bonjour` and finalizing after release, idle and empty after `Bonjour.` Cancel from `bonj` yields cancelled and empty. A late `bonjour` partial and a late `Bonjour.` final keep cancelled and empty with no edit.
- **Window.** The same command. When `DISPLAY` is set, `overlay-<stamp>/xwininfo-name.txt` contains `Window id:` for `"stt-overlay"` and `Map State: IsViewable`. The overlay process is killed after the probe. Do not capture a screenshot.
- **Skip.** When `DISPLAY` is unset, stdout contains `skip: no X11 DISPLAY; overlay window presence not proven` and the command exits `0`. Model tests still ran. That skip is not a window pass.

## Gotchas

- `ffmpeg` x11grab frames are black on WSLg. Presence plus state is the proof.
- gpui prefers Wayland when `WAYLAND_DISPLAY` is set. Leave it unset for this probe, matching `/tmp/gpui-probe/prove.sh`.
- `stt-shell` printing a version line is not the bubble.
- Hold-to-talk is still unreachable. This window is not wired to a session loop.
- A leftover window titled `stt-overlay` fails the presence test on purpose. Kill it first.
