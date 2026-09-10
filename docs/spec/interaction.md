# Interaction

This spec records how a dictation session moves through its states, when the
optional LLM cleanup runs, and how errors surface. It mirrors the `Session`
state machine in `stt-core`: the text below is the contract, the Rust enum
is the implementation.

## Session states

Five states, exactly the `SessionState` discriminants in `stt-core`:

- `idle`. No session is active. The bubble is hidden.
- `recording`. The hotkey engaged and audio capture runs. No hypothesis yet.
- `streaming`. Partial hypotheses stream into the target app and refine.
- `finalizing`. The key released (or the second toggle press); the engine
  completes the final transcript.
- `cancelled`. Esc ended the session. No final transcript is produced.

Transitions (method names are the `Session` API in `stt-core`):

```
idle --hold()/toggle()--> recording --on_partial()--> streaming
recording --release()--> idle               (release before any hypothesis)
recording --toggle()--> idle                (second press, nothing heard)
streaming --on_partial()--> streaming        (latest hypothesis kept)
streaming --release()/toggle()--> finalizing (partials carried over)
finalizing --finish(transcript)--> idle      (transcript goes to the injector)
any --cancel()/Esc--> cancelled              (partials dropped, inserted text removed best-effort)
```

Rules:

- Hold-to-talk is the default: the session records while the key is held
  and finalizes on release. Holding again mid-session is a no-op; the key
  is already down.
- Toggle mode is for long dictation: first press starts capture, second
  press finalizes. The bubble shows the same states; only the key gesture
  differs.
- Esc cancels from any state, including `idle` (no-op) and `finalizing`
  (partials dropped, nothing injected). Text already streamed into the
  target is removed best-effort with the backspace replay from
  `os-integration.md`; removal can fail in targets that consumed the keys
  (terminals, games), which is accepted and logged.
- The session owns the correction buffer: every `Partial` updates it, every
  `Final` or cancel clears it. A cancelled session can never leak partial
  text into a later injection.

## Cleanup timing

The cleanup pass is the optional local-LLM rewrite of the final transcript
(filler words removed, formatting and style adapted). It is configured in
settings and off by default.

Timing, fixed:

1. The pass triggers exactly once, on the `finalizing -> idle` edge, after
   the engine returns the final transcript and before the bubble hides.
2. It never runs during `recording` or `streaming`: partials must converge
   without fighting a rewriter, and the local LLM stays unloaded until it
   is needed.
3. The cleaned text replaces the raw transcript through
   `TextInjector::replace_last`, reusing live correction. If the model
   returns nothing usable, the raw transcript stands.
4. The pass has a 10 s timeout. On timeout the raw transcript stands, the
   bubble shows a "cleanup timed out" note for 3 s, and the event is
   logged. The session still ends in `idle`.
5. Esc during the pass cancels it: the session ends `cancelled`, the raw
   transcript is removed best-effort like any cancel, and the rewrite is
   discarded.

While the pass runs, the bubble stays in its `finalizing` look with the
timer replaced by "cleaning up…", so a slow local model never looks like a
hang.

## Errors

Engine and OS failures surface as a cancelled run with a bubble error
state (see `asr.md` and `os-integration.md` for the typed errors): the red
error bubble names the remedy ("microphone unavailable", "accessibility
permission required", "compositor lacks the shortcuts portal"), persists
3 s, then fades. Nothing is injected. Every error is logged with its kind
so settings can offer the matching fix.
