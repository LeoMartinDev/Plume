# ASR engine

This spec compares the three candidate engines for local speech recognition,
names the default, and maps it onto the `AsrEngine` trait from `stt-core`.
All three candidates run on-device with no network call, per the local-only
principle. Facts below were checked against the model cards and crate
registries in September 2026.

## Candidates

| Dimension | Parakeet TDT 0.6B v3 (NVIDIA) | Qwen3-ASR 0.6B / 1.7B (Alibaba) | Chunked Whisper (whisper.cpp) |
| --- | --- | --- | --- |
| Streaming | Adapted, not native. Offline-first transducer; chunked inference via the NeMo script `speech_to_text_streaming_infer_rnnt.py` (2 s chunks, left/right context). NVIDIA's native-streaming family is Nemotron, not Parakeet. | Native unified streaming/offline (dynamic attention window, 2 s chunks, prefix rollback), but the streaming path requires the vLLM backend and returns no timestamps. | Native chunked real-time. `whisper-stream` transcribes continuously (500 ms step), with Silero VAD segmentation and raw-PCM streaming ports. |
| FR + EN quality | FR + EN covered: v3 is multilingual over 25 European languages with automatic language detection (v2 was English-only). Averages 6.34% WER on the Open ASR Leaderboard, ahead of Whisper large-v3. | FR + EN covered: 52 languages and dialects. Strong multilingual and dialect scores in the technical report, in both offline and streaming modes. | FR + EN proven: 99 languages, years of production use in both French and English dictation. Accuracy trails Parakeet v3 on benchmarks but is well within everyday-dictation quality. |
| Model size | 600 M parameters, ~680 MB as INT8 ONNX. | BF16 weights ~1.9 GB (0.6B) / ~4.7 GB (1.7B). No official small CPU-quantized desktop build. | tiny 39 MB, base 142 MB, small 466 MB, medium 1.5 GB, large-v3 3.1 GB. The app ships one size and offers the rest in settings. |
| License | CC-BY-4.0 (model weights). Permissive, attribution only. | Apache 2.0 (weights and code). Permissive. | MIT (whisper.cpp code and Whisper weights). Permissive. |
| Rust binding | None mature. Integration would mean a custom ONNX Runtime pipeline (e.g. the `ort` crate) plus hand-rolled chunking; NeMo itself is Python and GPU-oriented. | None. The stack is Python (`qwen-asr` toolkit) on vLLM, a server GPU serving engine. No desktop Rust path exists. | Mature. `whisper-rs` 0.16.0 (800k+ downloads, CUDA/Metal/Vulkan/CoreML features, Unlicense) wraps whisper.cpp; `whisper-cpp-plus` 0.1.5 (MIT) adds PCM streaming and VAD bindings. |

### Parakeet TDT

Parakeet TDT 0.6B v3 is a 600 M-parameter transducer (token-and-duration) built
for high-throughput batch transcription. It emits text in a single pass over
the audio instead of autoregressively, so it is fast (3,300x real-time
throughput class) and does not hallucinate during silence. It ships native
punctuation, capitalization, and word-level timestamps, and auto-detects the
language across 25 European languages including French and English.

The blocker for the default is integration, not accuracy. Streaming is an
adaptation of an offline model through NeMo chunked inference, NeMo is a
Python GPU-first toolkit, and no maintained Rust binding exists. A future
`ParakeetEngine` could load the INT8 ONNX weights through `ort` and implement
`AsrEngine` behind the same trait; the pluggable-engine principle keeps that
door open. Parakeet is the documented runner-up, not the default.

### Qwen3-ASR

Qwen3-ASR 0.6B and 1.7B are attention-encoder-decoder models (AuT encoder plus
a Qwen3 decoder) with genuinely unified streaming/offline inference: the same
weights stream with 2-second chunks and prefix rollback, or transcribe offline
with long context. Language coverage is the widest of the three candidates
(52 languages and dialects, FR + EN included) and robustness in noisy,
complex-audio conditions is the family's headline claim.

The blocker is the runtime. Streaming inference is only available with the
vLLM backend, a server-class GPU serving engine, and streaming returns no
timestamps. BF16 weights start at ~1.9 GB with no official small quantized
desktop build. That stack cannot ship inside a lightweight local-only desktop
app on macOS, Windows, and Linux. Qwen3-ASR is rejected for the default; it
may be reconsidered if a CPU-friendly quantized runtime with a Rust binding
appears.

### Chunked Whisper

Chunked Whisper is OpenAI's Whisper model family run through whisper.cpp
(ggml-org, MIT, 53k+ stars) in streaming mode: audio is sampled continuously,
VAD segments speech, and each chunk is transcribed as it arrives. The
`whisper-stream` example runs this loop on a plain CPU with a 500 ms step.
French and English are both first-class after years of production use across
99 languages, and the five model sizes (39 MB to 3.1 GB) let the app trade
latency against accuracy per machine.

The decisive advantage is the Rust story. `whisper-rs` is a maintained,
widely-depended binding with per-platform GPU acceleration (Metal on macOS,
CUDA/Vulkan on Windows/Linux, CoreML where useful) and a plain-CPU fallback,
so one crate covers the whole platform matrix. `whisper-cpp-plus` adds the
PCM-streaming and VAD surface the session state machine needs. No other
candidate offers a maintained Rust path today.

## Default

The default engine is chunked Whisper via whisper.cpp, shipped as the `base`
142 MB multilingual model, with `small` and `large-v3` selectable in settings.

Reasons, in order:

1. It is the only candidate with a mature, cross-platform Rust binding, which
   the `AsrEngine` trait and the cross-platform-from-day-one principle
   require on day one.
2. It streams on a plain desktop CPU with no server GPU runtime, which the
   local-only principle requires everywhere the app runs.
3. Its FR + EN quality is proven in production, which satisfies the FR plus
   EN minimum from `product.md`.
4. MIT licensing and size options (39 MB to 3.1 GB) fit the free principle
   and per-machine tuning.
5. Parakeet v3 wins on benchmark accuracy but has no Rust binding and no
   native streaming; Qwen3-ASR wins on paper streaming but needs vLLM on a
   server GPU. Both stay available behind the pluggable `AsrEngine` trait.

## Trait mapping

The default engine implements the `AsrEngine` trait from `stt-core`:

```rust
pub trait AsrEngine {
    fn stream(&self, audio: AudioStream) -> HypothesisStream;
}
```

Mapping, type by type:

- `AudioChunk { samples: Vec<f32>, sample_rate: u32 }` feeds a resampler to
  16 kHz mono float32, then a ring buffer. The engine slices the buffer into
  overlapping windows (500 ms step, VAD-gated) and runs `full` per window.
- Each window's unstable segments surface as
  `Hypothesis::Partial(PartialHypothesis)`; the overlay and the injector
  treat them as refinable, which drives live correction.
- Stabilized windows (no change across two consecutive passes, or VAD end of
  speech) surface as `Hypothesis::Final(Transcript)`; the session moves to
  `finalizing` and the injector keeps the text.
- `HypothesisStream` is a pull iterator over `Result<Hypothesis, BoxError>`.
  Whisper errors (model load failure, decode failure) map to `BoxError` and
  terminate the stream; the session surfaces them as a cancelled run with a
  bubble error state, as `interaction.md` (PR-5) will specify.
- Configuration lives in a `WhisperEngine` struct: model path and size
  (`base` default), language hint (`auto`, FR/EN-first), beam size,
  temperature fallback, and VAD thresholds. Swapping the default to a future
  `ParakeetEngine` or `QwenEngine` changes the struct, never the trait or
  the session state machine.
