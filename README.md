# stt-gpui

Local-only desktop push-to-talk dictation. Hold a global shortcut, speak,
and the text streams into the focused app on macOS, Windows, and Linux.
Audio never leaves the machine.

Status: definition phase. The locked product spec is `docs/spec.md`,
assembled from the per-topic sections in `docs/spec/`. The Rust workspace
(`stt-core` + `stt-shell`) currently holds the `AsrEngine` /
`TextInjector` / `GlobalHotkey` traits and the session state machine.

## Quickstart

Prerequisites: a stable Rust toolchain (`cargo --version`) and Node 18+
for the spec checker (`node --version`).

```sh
# Build the workspace.
cargo build --workspace

# Run the tests (session state machine).
cargo test --workspace

# Run the shell (prints its version today; the full app later).
cargo run -p stt-shell

# Check the spec files carry their required headings.
node scripts/check-spec.mjs docs/spec
node scripts/check-spec.mjs docs/spec.md
```

CI (`.github/workflows/ci.yml`) runs `cargo build --workspace` and
`cargo test --workspace` on macOS, Windows, and Linux for every pull
request.

## Layout

- `crates/stt-core/` — traits (`AsrEngine`, `TextInjector`,
  `GlobalHotkey`) and the session state machine. Dependency-free.
- `crates/stt-shell/` — the binary. Today it prints its version.
- `docs/stt-gpui-plan.md` — the program plan this repo was defined from.
- `docs/spec/` — per-topic definition sections (product, principles,
  UX flow, platform matrix, glossary, ASR engine, OS integration,
  overlay, interaction) plus the throwaway overlay prototype.
- `docs/spec.md` — the locked spec. Read this first.
- `scripts/check-spec.mjs` — asserts the required headings per spec file.

## Decisions, in short

- UI framework: mainline GPUI (see `docs/spec.md`, Framework).
- Default ASR engine: chunked Whisper via whisper.cpp, `base` 142 MB
  (see `docs/spec.md`, Engine).
- Overlay: the pill variant (see `docs/spec.md`, Overlay).
- Open items live in Appendix A of `docs/spec.md`.
