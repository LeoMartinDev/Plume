# Shell identity

Shell identity is the live user-facing program. Running `stt-shell` prints the crate version and exits. That line is how a user knows which build they launched.

## Sub-features

- `shell-run` prints `stt-shell <version>` and exits `0`.
- `shell-flags` prints the same version line for `--help`. Unknown commands exit `2` with usage.
- `shell-match-toml` uses the version from `crates/stt-shell/Cargo.toml`, not a hardcoded string.

## How to get to it (user POV)

- Run the `stt-shell` binary from a terminal after a workspace build.
- Run `cargo run -p stt-shell` from the repository root.
- Pass flags such as `--help`. The program still identifies itself the same way.

## Driving it with control-stt-gpui

Preconditions:

- `control-stt-gpui doctor` reports `surface=cli` and `ok version` matching `crates/stt-shell/Cargo.toml`.
- The binary under test is the path printed by launch, inside `/tmp/stt-gpui-verify-$RUN_ID/`.

- **Run shell.** Launch the program with no arguments. Run `control-stt-gpui cli`. Exit code `0`. The first line of stdout is `stt-shell 0.1.0` when the crate version is `0.1.0`. Stderr is empty.
- **Ignore flags.** Ask the program for help. Run `control-stt-gpui cli -- --help`. Exit code `0`. Stdout is the same version line. No usage text appears.
- **Proof.** Keep the transcripts. They land at `/tmp/stt-gpui-verify-artifacts/$RUN_ID/cli-*.transcript.txt`. Both runs must show the version line from doctor, not a workspace `cargo run` log.

## Gotchas

- `cargo run -p stt-shell` is a user entry point. Verification still goes through `control-stt-gpui cli` so the binary is the isolated launch build.
- `control-stt-gpui --help` is harness usage. It is not `stt-shell --help`.
- `stt-shell --help` does not print usage. Assert the version line.
- The version string must match `crates/stt-shell/Cargo.toml`. Do not hardcode `0.1.0` after a crate bump.
- A passing `cargo test -p stt-core` is not shell identity.
