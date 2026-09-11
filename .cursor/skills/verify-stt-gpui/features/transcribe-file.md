# Transcribe a file

`stt-shell transcribe` reads a local WAV, runs local recognition, and prints the final transcript. Stdout is only that text plus a trailing newline. The model pack is already on disk.

## Sub-features

- `transcribe-run` prints the transcript for a checked-in fixture WAV and exits `0`.
- `transcribe-model-dir` accepts `--model-dir <dir>` or `STT_MODEL_DIR`.
- `transcribe-usage` exits `2` when the WAV path is missing or an unknown flag is passed.
- `transcribe-wav-error` exits `3` when the WAV cannot be parsed.
- `transcribe-missing-pack` exits `4` when the model directory is missing or incomplete.

## How to get to it (user POV)

- Export `STT_MODEL_DIR` to a complete ONNX pack, then run `stt-shell transcribe <wav>`.
- Pass `--model-dir <dir>` instead of the environment variable.
- Run the same command through `control-stt-gpui cli -- transcribe ...` during verification.

## Driving it with control-stt-gpui

Preconditions:

- `control-stt-gpui doctor` reports `surface=cli` and `ok version` matching `crates/stt-shell/Cargo.toml`.
- `STT_MODEL_DIR` is exported to a complete pack. Launch and doctor do not set it.
- The binary under test is the path printed by launch, inside `/tmp/stt-gpui-verify-$RUN_ID/`.

- **English fixture.** Transcribe the checked-in English clip. Run `control-stt-gpui cli -- transcribe crates/stt-engine/fixtures/en-hello.wav`. Exit code `0`. Stdout is exactly the bytes of `crates/stt-engine/fixtures/en-hello.expected.txt`. Stderr contains `latency_ms=<integer>` and does not contain the transcript.
- **French fixture.** Transcribe the checked-in French clip. Run `control-stt-gpui cli -- transcribe crates/stt-engine/fixtures/fr-bonjour.wav`. Exit code `0`. Stdout is exactly the bytes of `crates/stt-engine/fixtures/fr-bonjour.expected.txt`.
- **Usage.** Omit the WAV path. Run `control-stt-gpui cli -- transcribe`. Exit code `2`. Stdout is empty. Stderr is one usage line.
- **Proof.** Keep the transcripts. They land at `/tmp/stt-gpui-verify-artifacts/$RUN_ID/cli-*.transcript.txt`. Compare stdout to the expected files with `diff`, not a substring check.

## Gotchas

- Doctor still runs the binary with no arguments. That path prints `stt-shell <version>` and is not this feature.
- `control-stt-gpui --help` is harness usage. It is not `stt-shell transcribe`.
- The harness launch probe does not export `STT_MODEL_DIR`. Export it in the shell that runs `cli`, or pass `--model-dir`.
- A passing `cargo test -p stt-engine --test fixture_transcribe` is not a shell transcribe proof. Drive `stt-shell`.
- If `cli -- transcribe` exits non-zero while the same arguments work on the launch binary invoked directly, record both runs. Do not treat the engine suite as a substitute.
