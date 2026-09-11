#!/usr/bin/env bash
# Offline proof for stt-engine: the fixture suite transcribes inside a
# network namespace, so any fetch on the transcription path fails the run.
# The build happens outside the namespace; only the test binaries and the
# built stt-shell transcribe the en fixture inside.
# -U joins a user namespace because unprivileged unshare -n alone is denied;
# the -n net namespace is the isolation that matters here.
# ort pulls ureq/rustls through ort-sys build-dependencies (download-binaries
# fetching the prebuilt ORT dylib at compile time). cargo tree --edges normal
# excludes build edges, so the TLS denylist only constrains normal edges.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

if [ -z "${STT_MODEL_DIR:-}" ]; then
    echo "assert-offline-transcribe: STT_MODEL_DIR is not set" >&2
    exit 4
fi

cargo test --offline -q -p stt-engine --test fixture_transcribe -- --nocapture
echo "online fixture run passed; re-running inside a network namespace"

unshare -Un cargo test --offline -q -p stt-engine --test fixture_transcribe -- --nocapture
echo "offline fixture run passed (no network egress possible)"

cargo build --offline -q -p stt-shell
BIN="$ROOT/target/debug/stt-shell"
WAV="$ROOT/crates/stt-engine/fixtures/en-hello.wav"
EXP="$ROOT/crates/stt-engine/fixtures/en-hello.expected.txt"
OUT="$(mktemp)"
trap 'rm -f "$OUT"' EXIT
unshare -Un "$BIN" transcribe "$WAV" >"$OUT"
diff -u "$EXP" "$OUT"
echo "offline stt-shell transcribe passed (stdout matched en-hello.expected.txt)"
