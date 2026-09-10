# Overlay

The bubble is the only permanent UI of stt-gpui. It appears at the bottom
center of the screen while a session runs and disappears when the session
ends. This spec records the throwaway prototype variants, the voice-reactive
animation, the position, the states, and the chosen variant with its reason.

## Variants

Three throwaway variants were built behind one switcher in
`overlay-prototype/index.html` (open it in any browser; `?variant=a|b|c`,
`?state=recording|streaming|finalizing|error`, `?static=1` for deterministic
screenshots, `?mic=1` plus the in-page button for a real microphone level).
The prototype is throwaway: it settles the open design question and is not
shipped. Screenshots below were captured with headless Chrome at 1280x800,
frozen mid-streaming with the partial text "bonjour, ceci est une dictée…".

- Variant A, pill: a dark pill with a state dot, a 24-bar voice waveform,
  the current partial hypothesis, and a session timer.
  Screenshot: `overlay-prototype/variant-a.png`.
- Variant B, ring: a circular dial with a level-reactive arc and core, plus
  a small state caption underneath. No transcript text.
  Screenshot: `overlay-prototype/variant-b.png`.
- Variant C, minimal: a translucent caption with a LIVE badge and the
  partial text. No level visualization.
  Screenshot: `overlay-prototype/variant-c.png`.

## Position

Bottom-center of the active screen, 64 px above the bottom edge (120 px in
the static screenshots for legibility). The production overlay never takes
focus: a non-activating floating `NSWindow` on macOS, a `WS_EX_NOACTIVATE`
layered window on Windows, an override-redirect window on X11, and a
`zwlr_layer_shell_v1` surface on Wayland (see `platform-matrix.md`). On
multi-monitor setups it follows the screen holding the focused target app.

## States

The bubble mirrors the session state machine (see `interaction.md`):

| Session | Bubble |
| --- | --- |
| `idle` | Hidden. Nothing renders between sessions. |
| `recording` | Visible, red state dot pulsing, waveform live, no text yet. |
| `streaming` | Visible, green dot, waveform follows the voice, latest partial shown. |
| `finalizing` | Visible, amber dot dimmed, waveform frozen, "finalizing…" replaces the timer. Stays until the transcript (and the optional cleanup pass) lands. |
| `cancelled` | Fades out over 150 ms. No text remains. |
| error | Red dot, short message (e.g. "microphone unavailable"), persists 3 s, then fades. |

A "pasted via clipboard" badge appends to the pill while the clipboard
fallback from `os-integration.md` is active, so degraded injection is never
silent.

## Voice-reactive animation

The level source is the microphone RMS (or a deterministic simulated
oscillator in the prototype). A `requestAnimationFrame` loop samples the
level once per frame and updates the visuals through compositor-only
properties: `transform` (dot scale, bar `scaleY`, ring core scale) and
`opacity`. The single exception is the ring arc's `stroke-dashoffset` in
variant B, which is why the ring scales worse than the bars. No layout or
paint property is touched per frame, so the animation holds its frame
budget by construction.

Perf evidence: no automated number is claimed. Headless Chrome in this
environment completes `--screenshot` runs but never exits `--dump-dom`
against a page with an infinite `requestAnimationFrame` loop (killed by
timeout at 150 s, then 280 s; the captured DOM never contains the report),
so the 60 fps budget rests on construction plus a one-click manual check.
Every per-frame write in the chosen variant is a compositor-only property
(`transform: scale` on the dot and the 24 bars, `opacity` on the timer dim),
the loop samples one level value and touches 26 elements per frame with no
layout or paint work, and opening
`overlay-prototype/index.html?variant=a&fps=1` in any desktop browser prints
`frames=… fps=…` after 2 s of real time. Operator check before locking the
production overlay: confirm the printed fps stays at or above 55.

## Chosen variant

The chosen variant is A, the pill.

Reasons:

1. It is the only variant showing state, voice level, and the current
   partial hypothesis at once. The user sees what will be injected before
   releasing the key, which is the point of live correction.
2. It stays minimal: one pill, bottom-center, readable on light and dark
   backgrounds thanks to the opaque dark surface (variant C's translucency
   fails on busy backgrounds).
3. It extends cleanly: the timer, the "finalizing…" state, and the
   clipboard-fallback badge all fit without a redesign.
4. Variant B was rejected because hiding the partial text blinds the user
   to recognition mistakes until release. Variant C was rejected because it
   shows no voice level (a dead microphone looks identical to silence) and
   its contrast depends on the wallpaper.
