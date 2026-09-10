# stt-gpui spec

Locked specification, assembled September 2026 from the merged definition
sections. stt-gpui is a desktop push-to-talk dictation app: hold a global
shortcut, speak, and the text streams into the focused app on macOS,
Windows, and Linux. Audio never leaves the machine.

This document is the whole product in one place. The per-topic sources of
truth it assembles are `spec/product.md`, `spec/principles.md`,
`spec/ux-flow.md`, `spec/platform-matrix.md`, `spec/glossary.md`,
`spec/asr.md`, `spec/os-integration.md`, `spec/overlay.md`, and
`spec/interaction.md`. In case of conflict, this locked spec wins and the
conflicting section must be amended by a follow-up change.

Foundations (from PR-1, restated to lock them): the target user is anyone
who wants local speech-to-text on the desktop; French and English are the
minimum, both streaming word by word at everyday-dictation quality; the
five principles are local-only forever, cross-platform from day one,
pluggable engine, free, and modern-and-minimal; the flows are hold-to-talk,
toggle mode for long dictation, Esc cancel, and the optional local-LLM
cleanup pass. The domain vocabulary (session, partial hypothesis, final
transcript, bubble, live correction, target app) is defined in
`spec/glossary.md`.

## Framework

Decision: build the UI on mainline GPUI (`zed-industries/zed`, crates.io
`gpui`), not on the `gpui-ce` community fork.

State of the two, checked September 2026: mainline GPUI ships macOS
(Metal), Linux (Wayland + X11, renderer moved from Blade/Vulkan to wgpu in
February 2026), and Windows (D3D11, first-class since Zed's Windows GA in
October 2025). Zed itself is the daily proof that all three backends work
in production. `gpui-ce` exists precisely because mainline rejects
non-Zed features, and it stays API-compatible with mainline, but it trails
by roughly 380 commits with single-maintainer activity and an unclear
trajectory.

Reasons, in order:

1. Mainline is the only one of the two proven on all three OS in a
   shipping app, which the cross-platform-from-day-one principle requires.
2. The choice is reversible: `gpui-ce` tracks the mainline API, so code
   written against mainline moves to the fork with a dependency swap
   (`gpui = { package = "gpui-ce" }`) if upstream ever blocks the product.
3. Betting day-one shipping on a fork ~380 commits behind, with
   single-digit merged PRs, would risk the whole program for no product
   gain today.

Known mainline risk, with mitigation: upstream stated in February 2026
that community-facing GPUI work is paused and rejects features Zed does
not need (tray-icon support was refused). The bubble is a standard window
and needs nothing exotic, and the tray menu goes through a dedicated crate
(`tray-icon`), never through GPUI. First implementation spike after this
spec: an empty GPUI window building and opening on all three OS in CI
(see Appendix A, item A1).

## Engine

Decision (from `spec/asr.md`): the default engine is chunked Whisper via
whisper.cpp, shipped as the `base` 142 MB multilingual model, with `small`
and `large-v3` selectable in settings.

Comparison summary:

| Dimension | Parakeet TDT 0.6B v3 | Qwen3-ASR 0.6B / 1.7B | Chunked Whisper (default) |
| --- | --- | --- | --- |
| Streaming | Adapted via NeMo chunked inference, not native | Native but vLLM-backend only, no timestamps | Native chunked real-time, 500 ms step, VAD |
| FR + EN | Covered, 25 EU languages, best benchmarks | Covered, 52 languages/dialects | Proven in production, 99 languages |
| Model size | 600 M params, ~680 MB INT8 ONNX | ~1.9 GB / ~4.7 GB BF16 | 39 MB – 3.1 GB across five sizes |
| License | CC-BY-4.0 | Apache 2.0 | MIT |
| Rust binding | None mature (custom `ort` pipeline needed) | None (Python + vLLM stack) | Mature (`whisper-rs`, `whisper-cpp-plus`) |

Whisper is the only candidate with a mature cross-platform Rust binding
that streams on a plain desktop CPU, with proven FR + EN quality and MIT
licensing. Parakeet v3 is the documented runner-up (best accuracy, no Rust
path today): a future `ParakeetEngine` can load the INT8 ONNX weights
behind the same trait. Qwen3-ASR is rejected for the default: its
streaming path needs a server-class vLLM GPU runtime.

Trait mapping (from `spec/asr.md`): a `WhisperEngine` struct (model path
and size, FR/EN-first language hint, beam, temperature fallback, VAD
thresholds) implements `AsrEngine::stream`, feeding resampled 16 kHz
`AudioChunk`s through overlapping VAD-gated windows. Unstable segments
surface as `Hypothesis::Partial`, stabilized ones as
`Hypothesis::Final(Transcript)`; failures map to `BoxError` and end the
run as a bubble error (see Interaction).

## OS integration

One implementation pair per OS (from `spec/os-integration.md`):

| OS | Text injection | Global hotkey (hold/release) | Permission story |
| --- | --- | --- | --- |
| macOS | `CGEvent` Unicode keyboard events | Quartz event tap (key down/up + flags-changed for `Fn`) | Accessibility grant required for both; missing grant is a named fatal error with the Settings link |
| Windows | `SendInput` with `KEYEVENTF_UNICODE` | `WH_KEYBOARD_LL` low-level hook | None for normal targets; elevated targets report "run as administrator" |
| Linux X11 | XTEST fake key events | XRecord press/release (passive, no grab) | None beyond the X session |
| Linux Wayland | `zwp_virtual_keyboard_v1`, then RemoteDesktop portal, then clipboard | GlobalShortcuts portal (`Activated`/`Deactivated`) | Per-portal consent; every link probed at startup, first missing link reported |

Rejected alternatives, locked: Carbon `RegisterEventHotKey`, Win32
`RegisterHotKey` for the hold key (no release event), `XGrabKey` (steals
keys), `XSendEvent` (ignored by toolkits), libei/EIS as a Unicode path
(keycodes only). The `Fn` default is best-effort on Windows laptops (the
embedded controller eats it); `Ctrl+Space` is the effective default there.

Live correction is `replace_last(old, new)` via backspace replay plus
re-injection: at most one application per 100 ms, bounded at 200
graphemes, invalidated by any focus change or foreign keystroke. The
clipboard fallback (stash, set, simulated paste, best-effort restore after
800 ms) applies only when native injection cannot carry the text, always
behind a visible "pasted via clipboard" badge. Shared `BoxError` kinds:
`PermissionDenied`, `CompositorUnsupported`, `TargetElevated`,
`ClipboardBusy`.

## Overlay

Decision (from `spec/overlay.md`): variant A, the pill — a dark pill with
a state dot, a 24-bar voice waveform, the current partial hypothesis, and
a session timer, bottom-center of the active screen, 64 px above the edge,
never taking focus. The throwaway prototype with all three variants
behind one switcher lives in `spec/overlay-prototype/` (open
`index.html` in a browser); one screenshot per variant sits next to it.
Variant B (ring) was rejected for hiding the partial text; variant C
(minimal caption) for showing no voice level and depending on the
wallpaper for contrast.

The bubble mirrors the session: hidden when idle, red pulsing dot while
recording, green waveform plus partial while streaming, amber frozen
"finalizing…" until the transcript (and optional cleanup) lands, 150 ms
fade on cancel, red 3 s error otherwise. The animation loop samples one
level value per frame and writes compositor-only properties (`transform`,
`opacity`); the 60 fps budget holds by construction, with a one-click
manual check (`index.html?variant=a&fps=1`) documented in `spec/overlay.md`.

## Interaction

Five session states, exactly the `SessionState` discriminants in
`stt-core` (from `spec/interaction.md`): `idle`, `recording`,
`streaming`, `finalizing`, `cancelled`. Hold starts capture, the first
partial moves to streaming, release (or second toggle press) moves to
finalizing with the partials carried over, the final transcript returns
to idle through the injector, and Esc cancels from any state with
best-effort removal of streamed text. Hold is the default gesture, toggle
serves long dictation, and a cancelled session can never leak partial
text into a later injection.

Cleanup timing, fixed: the optional local-LLM pass (settings-gated, off
by default) triggers exactly once on the `finalizing -> idle` edge,
replaces the raw transcript through `replace_last`, times out after 10 s
(raw transcript stands, note shown), and is discarded by Esc like any
cancel. It never runs during capture. Engine and OS failures end the run
as a 3 s named-error bubble; nothing is injected.

## Appendix A

Open items and follow-ups. Every item is either decided here or listed
with its next step; nothing is left unnamed.

- A1. Framework validation spike — decided, pending execution. GPUI
  mainline is the decision (see Framework). Next step: an empty GPUI
  window building and opening on macOS, Windows, and Linux in CI before
  any product UI work. If the spike fails on an OS, revisit the decision
  with evidence.
- A2. Default engine — decided. Chunked Whisper `base` 142 MB, `small`
  and `large-v3` in settings (see Engine).
- A3. Cleanup LLM choice — open, listed. The spec requires a local model
  only. Next step: pick the smallest model with acceptable FR + EN
  rewrite quality under ~2 GB, with a Rust binding or a local sidecar,
  before implementing the pass. Candidates to evaluate: the Qwen3 and
  Gemma small-instruct families.
- A4. `vicinae-hotkey-v1` — open, watched. The Wayland hotkey path stays
  the GlobalShortcuts portal until this protocol standardizes and ships
  in the compositors the app targets.
- A5. Parakeet ONNX follow-up — open, watched. Reconsider a
  `ParakeetEngine` behind `AsrEngine` when a maintained Rust ONNX path
  plus chunked decoding exists; the trait boundary already allows it.
- A6. Timing constants — decided, tunable. Live-correction coalescing
  100 ms, replacement bound 200 graphemes, clipboard restore delay
  800 ms, cleanup timeout 10 s, error persistence 3 s, cancel fade
  150 ms. Tune with user testing; changes need no spec amendment unless
  the interaction contract moves.
