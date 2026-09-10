# stt-gpui plan

Build the definition of a desktop push-to-talk dictation app for anyone who wants local speech-to-text. Hold a global shortcut, speak, and text streams into the focused app on macOS, Windows, and Linux. The rule is local-only. Audio never leaves the machine. The stack is Rust plus GPUI or gpui-ce, an `AsrEngine` trait, and per-OS injection and hotkey. This plan defines the product and lands the build scaffold. It stops at merge-ready. The PR ids are PR-1 through PR-6 in order.

## How to read this

One box is one unit of work. Every box names the evidence that checks it. A nested box is a sub-step of the box above it. Check a box only when its evidence exists, a file, a log line, a screenshot, a test run, or a SHA. The body is a how-to. The appendices explain and record.

The program runs `pstack/skills/poteto-mode/playbooks/autopilot-stack.md`. The operator reviews and lands the stack herself. PR-1 through PR-6 stop at merge-ready.

Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

## Program checklist

### Arm the program

- [ ] State the protocol and this plan to the operator, then stop. Start execution only on her explicit go.
- [ ] On her go, arm a `/goal` with this exact text. "docs/stt-gpui-plan.md, PR-1 through PR-6 in order, a PR is verified only when its unit, live, and perf boxes are all checked, the operator lands the stack, done when PR-6 merges."
- [ ] Read these from trunk at program start. Re-read them at every tick.
  - [ ] `git show origin/main:pstack/skills/poteto-mode/playbooks/autopilot-stack.md`
  - [ ] `git show origin/main:pstack/skills/swarm/SKILL.md`
  - [ ] `git show origin/main:pstack/skills/poteto-mode/playbooks/opening-a-pr.md`
  - [ ] `git show origin/main:pstack/skills/poteto-mode/playbooks/prototype.md`
  - [ ] `git show origin/main:pstack/skills/poteto-mode/playbooks/feature.md`
- [ ] Arm the 30-minute audit tick. In a local session, a real terminal `/loop`. Never leave the cadence to memory.
- [ ] Use this tick prompt, verbatim. "Re-read the execution playbook from trunk and the armed /goal. Audit the operation against both and fix drift in this tick. Probe every active lane and judge progress by side effects only. Stand down a stuck lane and dispatch its replacement now. Then send the operator a status message, whether or not anything changed, with the queue table of PR, owner, state, and head SHA, the verdicts since the last tick, what merged, open operator gates, and blockers."
- [ ] On the operator's hold or stand-down, send every owner a zero-writes order at once.

### Spawn owners

- [ ] Spawn one owner per PR with the full lifecycle the execution playbook names.
- [ ] Follow this dependency graph. Start dependent work only after its parent merges, or base it on the parent branch when the execution playbook stacks.
  - [ ] PR-1 and PR-2 are independent and first. Both branch from `main`.
  - [ ] PR-3 after PR-2.
  - [ ] PR-4 after PR-2.
  - [ ] PR-5 after PR-2.
  - [ ] PR-6 after PR-1, PR-3, PR-4, and PR-5.
- [ ] Hold the file boundaries. PR-1 touches only `docs/**`. PR-2 touches only `Cargo.toml`, `crates/**`, `src/**`, and `.github/**`. PR-3 touches only `docs/spec/asr.md`. PR-4 touches only `docs/spec/os-integration.md`. PR-5 touches only `docs/spec/overlay.md` and `docs/spec/interaction.md`. PR-6 touches only `docs/spec.md` and `README.md`.
- [ ] Hold the review gate. No PR changes an interaction. Every PR writes `**Review gate.** None. <PR id> is not review-gated.`

### PR mechanics, for every PR

- [ ] Resolve the forge once. Default to `gh`; if `command -v origin` succeeds and Origin can resolve the repository, use `origin pr` for every PR operation. Record any fallback to `gh`. Never require `gt`.
- [ ] Open the PR ready, never draft, with `origin pr create --status open --base <base-branch>` or `gh pr create --base <base-branch>` according to the resolved forge. A stack child targets its parent branch.
- [ ] Run the repo's lint and typecheck once before the PR-facing push. Push with hooks on.
- [ ] Run `/deslop` before each commit and `/no-comments` before review.
- [ ] Triage every Bugbot and security-reviewer comment per `../references/bugbot-triage.md`.
- [ ] Rebase onto current trunk before babysit and again before the merge-ready report.

### Verdict and merge, for every PR

- [ ] At the merge-ready head SHA, run the swarm per `pstack/skills/swarm/SKILL.md`. One gates lane. The ten live lanes from the PR's **Verify, live** block. The perf lane from its **Verify, perf** block. One audit lane that reads the diff and the receipts and distrusts the PR body.
- [ ] Clean only when every lane is `PASS`. Findings go back to the owner. A new head gets a fresh swarm and a fresh verdict.
- [ ] The root appends the PR to the base-branch stack on a clean verdict. The operator lands the contiguous verified run bottom-up. Compare the stable `git patch-id` per `playbooks/shipping.md`.

### Boot recipe, for every live lane

Each live lane runs on its own cloud VM at the PR head. Drive through `control-cli` from `cursor-team-kit`. No control skill exists for a native GPUI overlay yet. That gap is a risk in Appendix C.

- [ ] `git fetch origin <head-branch> && git checkout <head SHA>`.
- [ ] Install the Rust toolchain and run the lane's command. Wait for the binary or the doc tree.
- [ ] Deliver input only through `control-cli` commands. Read-only diagnostics are `cargo --version`, `git log -1`, and `ls`.
- [ ] Save every screenshot to `/tmp/swarm-<pr-id>/worker-<n>/<slug>.png` and return the paths with the report.

## Write the product definition (PR-1)

**Depends on.** None.

**Files.**

- [ ] Create `docs/spec/product.md`.
- [ ] Create `docs/spec/principles.md`.
- [ ] Create `docs/spec/ux-flow.md`.
- [ ] Create `docs/spec/platform-matrix.md`.
- [ ] Create `docs/spec/glossary.md`.

**Build.**

- [ ] Write the one-sentence product, the target user, and the FR plus EN minimum in `docs/spec/product.md`.
- [ ] Write the founding principles in `docs/spec/principles.md`. Local-only forever, cross-platform from day one, pluggable engine, free, minimal.
- [ ] Write the hold-to-talk flow, the toggle mode, the Esc cancel, and the optional LLM cleanup in `docs/spec/ux-flow.md`.
- [ ] Write the per-OS rows for injection, hotkey, and overlay in `docs/spec/platform-matrix.md`. Mark Wayland a first-class row.
- [ ] Write the domain terms in `docs/spec/glossary.md`. Name the session state machine, the partial hypothesis, and the final transcript.

**You see.**

- [ ] `ls docs/spec/` lists the five files and each is non-empty.

**Verify, unit.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] `scripts/check-spec.mjs docs/spec` exits 0 and reports every required heading present. Run `node scripts/check-spec.mjs docs/spec`.

**Verify, live.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked. Ten lanes on `muse-spark-1.3-max` at the PR head, per the boot recipe.

- [ ] Lane 1. Regression lane against trunk. Run `ls docs/spec` at trunk and head. Trunk has no `docs/spec`. Gate that the head adds the five files and that `scripts/check-spec.mjs` exits 0. Save `p1-lane1.png`. Pass when the head lists five files and the checker exits 0.
- [ ] Lane 2. Read `product.md`. Save `p1-lane2.png`. Pass when it names the one-sentence product and FR plus EN.
- [ ] Lane 3. Read `principles.md`. Save `p1-lane3.png`. Pass when it states local-only forever and cross-platform from day one.
- [ ] Lane 4. Read `ux-flow.md`. Save `p1-lane4.png`. Pass when it names hold, toggle, Esc, and the cleanup pass.
- [ ] Lane 5. Read `platform-matrix.md`. Save `p1-lane5.png`. Pass when it has a row each for macOS, Windows, Linux X11, and Linux Wayland.
- [ ] Lane 6. Read `glossary.md`. Save `p1-lane6.png`. Pass when it defines the session state machine and the partial hypothesis.
- [ ] Lane 7. Run `wc -w docs/spec/*.md`. Save `p1-lane7.png`. Pass when no file is empty.
- [ ] Lane 8. Run `git log --oneline -3`. Save `p1-lane8.png`. Pass when the head commit names the spec.
- [ ] Lane 9. Run the checker with a missing file. Save `p1-lane9.png`. Pass when it exits non-zero and names the gap.
- [ ] Lane 10. Run the checker on the full tree. Save `p1-lane10.png`. Pass when it exits 0.

**Verify, perf.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] Metric. Wall time of `node scripts/check-spec.mjs docs/spec`.
- [ ] Probe. Run the checker at trunk and at the head, interleaved, three times each.
- [ ] Baseline. Record the trunk time first. Trunk has no checker, so record that fact.
- [ ] Rule. Head completes under 5 seconds. No ratio against trunk applies.

**Review gate.** None. PR-1 is not review-gated.

**Merge.**

- [ ] Root's clean verdict at the exact head SHA.
- [ ] Bugbot triage done.
- [ ] Rebased onto current trunk after the verdict, patch-id unchanged.
- [ ] The root appends PR-1 to the base-branch stack and the operator lands it bottom-up.

## Boot the Rust workspace (PR-2)

**Depends on.** None.

**Files.**

- [ ] Create `Cargo.toml`.
- [ ] Create `crates/stt-core/Cargo.toml` and `crates/stt-core/src/lib.rs`.
- [ ] Create `crates/stt-shell/Cargo.toml` and `crates/stt-shell/src/main.rs`.
- [ ] Create `.github/workflows/ci.yml`.
- [ ] Create `scripts/check-spec.mjs`.

**Build.**

- [ ] Declare a workspace in `Cargo.toml` with members `stt-core` and `stt-shell`.
- [ ] Define the `AsrEngine` trait and the session state machine in `crates/stt-core/src/lib.rs`.
- [ ] Define the `TextInjector` and `GlobalHotkey` traits in `crates/stt-core/src/lib.rs`.
- [ ] Write a `main` in `crates/stt-shell/src/main.rs` that prints the crate version and exits 0.
- [ ] Build the workspace on macOS, Windows, and Linux in `.github/workflows/ci.yml`.
- [ ] Write `scripts/check-spec.mjs` to assert required headings in a spec directory.

**You see.**

- [ ] `cargo build` exits 0 and `cargo run -p stt-shell` prints a version line.

**Verify, unit.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] `crates/stt-core/src/lib.rs` gains a test that drives the state machine through hold, release, and cancel. Run `cargo test -p stt-core`.

**Verify, live.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked. Ten lanes on `muse-spark-1.3-max` at the PR head, per the boot recipe.

- [ ] Lane 1. Regression lane against trunk. Run `cargo build` at trunk and head. Trunk has no workspace. Gate that the head builds and `stt-shell` runs. Save `p2-lane1.png`. Pass when the head build exits 0 and the binary prints a version.
- [ ] Lane 2. Run `cargo test -p stt-core`. Save `p2-lane2.png`. Pass when the state machine test passes.
- [ ] Lane 3. Run `cargo build --workspace`. Save `p2-lane3.png`. Pass when both crates build.
- [ ] Lane 4. Run `cargo run -p stt-shell`. Save `p2-lane4.png`. Pass when stdout has a version line and exit 0.
- [ ] Lane 5. Read `Cargo.toml`. Save `p2-lane5.png`. Pass when it lists both members.
- [ ] Lane 6. Read the `AsrEngine` trait. Save `p2-lane6.png`. Pass when it exposes a streaming method.
- [ ] Lane 7. Read `.github/workflows/ci.yml`. Save `p2-lane7.png`. Pass when it names macOS, Windows, and Linux.
- [ ] Lane 8. Run `cargo clippy --workspace -- -D warnings`. Save `p2-lane8.png`. Pass when it exits 0.
- [ ] Lane 9. Run `cargo fmt --check`. Save `p2-lane9.png`. Pass when it exits 0.
- [ ] Lane 10. Run `node scripts/check-spec.mjs --help`. Save `p2-lane10.png`. Pass when it prints usage and exits 0.

**Verify, perf.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] Metric. Wall time of `cargo build --workspace` from a clean target.
- [ ] Probe. Run `cargo clean` then `cargo build --workspace` at trunk and at the head, interleaved.
- [ ] Baseline. Record the trunk time first. Trunk has no workspace, so record that fact.
- [ ] Rule. Head builds under 120 seconds on the lane VM. No ratio against trunk applies.

**Review gate.** None. PR-2 is not review-gated.

**Merge.**

- [ ] Root's clean verdict at the exact head SHA.
- [ ] Bugbot triage done.
- [ ] Rebased onto current trunk after the verdict, patch-id unchanged.
- [ ] The root appends PR-2 to the base-branch stack and the operator lands it bottom-up.

## Choose the ASR engine (PR-3)

**Depends on.** PR-2.

**Files.**

- [ ] Create `docs/spec/asr.md`.

**Build.**

- [ ] Compare Parakeet TDT, Qwen3-ASR, and chunked Whisper in `docs/spec/asr.md`.
- [ ] Record streaming support, FR and EN quality, model size, license, and the Rust binding for each.
- [ ] Name the default engine and the reason in `docs/spec/asr.md`.
- [ ] Map the default engine onto the `AsrEngine` trait from `stt-core`.

**You see.**

- [ ] `docs/spec/asr.md` names one default engine and a comparison table.

**Verify, unit.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] `scripts/check-spec.mjs docs/spec/asr.md` exits 0 and the file names a default. Run `node scripts/check-spec.mjs docs/spec/asr.md`.

**Verify, live.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked. Ten lanes on `muse-spark-1.3-max` at the PR head, per the boot recipe.

- [ ] Lane 1. Regression lane against trunk. Run `ls docs/spec` at trunk and head. Trunk has no `asr.md`. Gate that the head adds it and names a default. Save `p3-lane1.png`. Pass when the head file exists and names a default engine.
- [ ] Lane 2. Read the comparison table. Save `p3-lane2.png`. Pass when it covers all three candidates.
- [ ] Lane 3. Check the streaming row. Save `p3-lane3.png`. Pass when each candidate has a streaming verdict.
- [ ] Lane 4. Check the FR and EN row. Save `p3-lane4.png`. Pass when each candidate has a language verdict.
- [ ] Lane 5. Check the license row. Save `p3-lane5.png`. Pass when each candidate has a license.
- [ ] Lane 6. Check the default. Save `p3-lane6.png`. Pass when one engine is named default with a reason.
- [ ] Lane 7. Check the trait mapping. Save `p3-lane7.png`. Pass when the default maps onto `AsrEngine`.
- [ ] Lane 8. Run the checker. Save `p3-lane8.png`. Pass when it exits 0.
- [ ] Lane 9. Read the model size row. Save `p3-lane9.png`. Pass when each candidate has a size.
- [ ] Lane 10. Read the Rust binding row. Save `p3-lane10.png`. Pass when each candidate names a binding.

**Verify, perf.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] Metric. Wall time of `node scripts/check-spec.mjs docs/spec/asr.md`.
- [ ] Probe. Run the checker at trunk and at the head, interleaved.
- [ ] Baseline. Record the trunk time first. Trunk has no file, so record that fact.
- [ ] Rule. Head completes under 5 seconds. No ratio against trunk applies.

**Review gate.** None. PR-3 is not review-gated.

**Merge.**

- [ ] Root's clean verdict at the exact head SHA.
- [ ] Bugbot triage done.
- [ ] Rebased onto current trunk after the verdict, patch-id unchanged.
- [ ] The root appends PR-3 to the base-branch stack and the operator lands it bottom-up.

## Specify OS integration (PR-4)

**Depends on.** PR-2.

**Files.**

- [ ] Create `docs/spec/os-integration.md`.

**Build.**

- [ ] Specify text injection per OS in `docs/spec/os-integration.md`. Name the macOS, Windows, X11, and Wayland mechanisms.
- [ ] Specify the global hotkey per OS in `docs/spec/os-integration.md`. Name the hold and release detection.
- [ ] Specify how live correction replaces inserted text in `docs/spec/os-integration.md`.
- [ ] Specify the clipboard fallback in `docs/spec/os-integration.md`.
- [ ] Map each mechanism onto the `TextInjector` and `GlobalHotkey` traits from `stt-core`.

**You see.**

- [ ] `docs/spec/os-integration.md` has a section per OS and a trait mapping.

**Verify, unit.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] `scripts/check-spec.mjs docs/spec/os-integration.md` exits 0. Run `node scripts/check-spec.mjs docs/spec/os-integration.md`.

**Verify, live.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked. Ten lanes on `muse-spark-1.3-max` at the PR head, per the boot recipe.

- [ ] Lane 1. Regression lane against trunk. Run `ls docs/spec` at trunk and head. Trunk has no `os-integration.md`. Gate that the head adds it. Save `p4-lane1.png`. Pass when the head file exists.
- [ ] Lane 2. Read the macOS section. Save `p4-lane2.png`. Pass when it names an injection and a hotkey mechanism.
- [ ] Lane 3. Read the Windows section. Save `p4-lane3.png`. Pass when it names an injection and a hotkey mechanism.
- [ ] Lane 4. Read the X11 section. Save `p4-lane4.png`. Pass when it names an injection and a hotkey mechanism.
- [ ] Lane 5. Read the Wayland section. Save `p4-lane5.png`. Pass when it names an injection and a hotkey mechanism.
- [ ] Lane 6. Read the live-correction section. Save `p4-lane6.png`. Pass when it describes replacing inserted text.
- [ ] Lane 7. Read the fallback section. Save `p4-lane7.png`. Pass when it names the clipboard fallback.
- [ ] Lane 8. Check the trait mapping. Save `p4-lane8.png`. Pass when each OS maps onto `TextInjector` and `GlobalHotkey`.
- [ ] Lane 9. Run the checker. Save `p4-lane9.png`. Pass when it exits 0.
- [ ] Lane 10. Read the permissions notes. Save `p4-lane10.png`. Pass when macOS accessibility and Wayland portal are named.

**Verify, perf.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] Metric. Wall time of `node scripts/check-spec.mjs docs/spec/os-integration.md`.
- [ ] Probe. Run the checker at trunk and at the head, interleaved.
- [ ] Baseline. Record the trunk time first. Trunk has no file, so record that fact.
- [ ] Rule. Head completes under 5 seconds. No ratio against trunk applies.

**Review gate.** None. PR-4 is not review-gated.

**Merge.**

- [ ] Root's clean verdict at the exact head SHA.
- [ ] Bugbot triage done.
- [ ] Rebased onto current trunk after the verdict, patch-id unchanged.
- [ ] The root appends PR-4 to the base-branch stack and the operator lands it bottom-up.

## Prototype the overlay bubble (PR-5)

**Depends on.** PR-2.

**Files.**

- [ ] Create `docs/spec/overlay.md`.
- [ ] Create `docs/spec/interaction.md`.
- [ ] Create `docs/spec/overlay-prototype/` with the throwaway variants.

**Build.**

- [ ] Build two or three throwaway bubble variants in `docs/spec/overlay-prototype/` behind one switcher.
- [ ] Record the voice-reactive animation, the position, and the states in `docs/spec/overlay.md`.
- [ ] Record the session state machine and the cleanup timing in `docs/spec/interaction.md`.
- [ ] Screenshot each variant and store the paths in `docs/spec/overlay.md`.
- [ ] Record the chosen variant and the reason in `docs/spec/overlay.md`.

**You see.**

- [ ] `docs/spec/overlay.md` names a chosen variant and links a screenshot per variant.

**Verify, unit.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] `scripts/check-spec.mjs docs/spec/overlay.md` exits 0 and the file names a chosen variant. Run `node scripts/check-spec.mjs docs/spec/overlay.md`.

**Verify, live.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked. Ten lanes on `muse-spark-1.3-max` at the PR head, per the boot recipe.

- [ ] Lane 1. Regression lane against trunk. Run `ls docs/spec` at trunk and head. Trunk has no `overlay.md`. Gate that the head adds it and a chosen variant. Save `p5-lane1.png`. Pass when the head file exists and names a chosen variant.
- [ ] Lane 2. Count the variants. Save `p5-lane2.png`. Pass when at least two variants exist.
- [ ] Lane 3. Open variant one. Save `p5-lane3.png`. Pass when the bubble renders bottom-center.
- [ ] Lane 4. Open variant two. Save `p5-lane4.png`. Pass when the bubble renders bottom-center.
- [ ] Lane 5. Drive the voice animation. Save `p5-lane5.png`. Pass when the animation reacts to input level.
- [ ] Lane 6. Read `interaction.md`. Save `p5-lane6.png`. Pass when it names the session states.
- [ ] Lane 7. Read the cleanup timing. Save `p5-lane7.png`. Pass when it states when the LLM pass runs.
- [ ] Lane 8. Check the screenshots. Save `p5-lane8.png`. Pass when each variant links a real screenshot path.
- [ ] Lane 9. Run the checker. Save `p5-lane9.png`. Pass when it exits 0.
- [ ] Lane 10. Read the chosen reason. Save `p5-lane10.png`. Pass when the choice has a stated reason.

**Verify, perf.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] Metric. Frame time of the voice-reactive animation in the chosen variant.
- [ ] Probe. Measure the animation frame time at the head. Trunk has no prototype, so record that fact.
- [ ] Baseline. Record the trunk absence first.
- [ ] Rule. The animation holds 60 frames per second on the lane VM. No ratio against trunk applies.

**Review gate.** None. PR-5 is not review-gated.

**Merge.**

- [ ] Root's clean verdict at the exact head SHA.
- [ ] Bugbot triage done.
- [ ] Rebased onto current trunk after the verdict, patch-id unchanged.
- [ ] The root appends PR-5 to the base-branch stack and the operator lands it bottom-up.

## Assemble the locked spec (PR-6)

**Depends on.** PR-1, PR-3, PR-4, and PR-5.

**Files.**

- [ ] Create `docs/spec.md`.
- [ ] Edit `README.md`.

**Build.**

- [ ] Assemble the locked spec in `docs/spec.md` from the merged sections.
- [ ] Record the GPUI versus gpui-ce decision in `docs/spec.md`.
- [ ] Record the default engine, the OS integration, the overlay, and the interaction in `docs/spec.md`.
- [ ] Write the build quickstart in `README.md`.
- [ ] Resolve every open decision or list it in Appendix A of `docs/spec.md`.

**You see.**

- [ ] `docs/spec.md` reads as one spec and `README.md` builds the workspace.

**Verify, unit.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] `scripts/check-spec.mjs docs/spec.md` exits 0 and the spec has no unresolved marker. Run `node scripts/check-spec.mjs docs/spec.md`.

**Verify, live.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked. Ten lanes on `muse-spark-1.3-max` at the PR head, per the boot recipe.

- [ ] Lane 1. Regression lane against trunk. Run `ls docs` at trunk and head. Trunk has no `spec.md`. Gate that the head adds it and that the README builds. Save `p6-lane1.png`. Pass when the head file exists and `cargo build` exits 0.
- [ ] Lane 2. Read the framework decision. Save `p6-lane2.png`. Pass when GPUI or gpui-ce is named with a reason.
- [ ] Lane 3. Read the engine section. Save `p6-lane3.png`. Pass when the default engine matches `asr.md`.
- [ ] Lane 4. Read the OS section. Save `p6-lane4.png`. Pass when it matches `os-integration.md`.
- [ ] Lane 5. Read the overlay section. Save `p6-lane5.png`. Pass when it matches `overlay.md`.
- [ ] Lane 6. Read the interaction section. Save `p6-lane6.png`. Pass when it matches `interaction.md`.
- [ ] Lane 7. Run the checker. Save `p6-lane7.png`. Pass when it exits 0 and finds no unresolved marker.
- [ ] Lane 8. Follow the README quickstart. Save `p6-lane8.png`. Pass when `cargo run -p stt-shell` prints a version.
- [ ] Lane 9. Read Appendix A of the spec. Save `p6-lane9.png`. Pass when every open decision is resolved or listed.
- [ ] Lane 10. Run `cargo test --workspace`. Save `p6-lane10.png`. Pass when all tests pass.

**Verify, perf.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] Metric. Wall time of `cargo build --workspace` from a clean target.
- [ ] Probe. Run `cargo clean` then `cargo build --workspace` at trunk and at the head, interleaved.
- [ ] Baseline. Record the trunk time first. Trunk has no workspace, so record that fact.
- [ ] Rule. Head builds under 120 seconds on the lane VM. No ratio against trunk applies.

**Review gate.** None. PR-6 is not review-gated.

**Merge.**

- [ ] Root's clean verdict at the exact head SHA.
- [ ] Bugbot triage done.
- [ ] Rebased onto current trunk after the verdict, patch-id unchanged.
- [ ] The root appends PR-6 to the base-branch stack and the operator lands it bottom-up.

## Close the program

- [ ] Every box above is checked with its evidence.
- [ ] Reply to the operator with the report the execution playbook names.

## Appendix A. Prototype evidence

The overlay bubble is the one open question a prototype settles. PR-5 builds two or three throwaway variants behind one switcher and screenshots each. The branch, the SHA, and the screenshot paths land in `docs/spec/overlay.md` when PR-5 runs. The GPUI versus gpui-ce decision stays unproven until PR-2 confirms which crate builds on all three OS. The default ASR engine stays unproven until PR-3 compares the candidates. No prototype ran before this plan. The repo has no code yet.

## Appendix B. Alternatives rejected

- Tauri plus a web view. The prior `speech-to-text` app used it. Rejected because the overlay and the injection need native control GPUI gives, and the user named GPUI.
- Cloud ASR fallback. Rejected by the local-only principle.
- One OS at the MVP. Rejected by the cross-platform-from-day-one principle.
- Autopilot-full as the execution playbook. Rejected because merge authority stays with the operator.

## Appendix C. Risks

- No control skill drives a native GPUI overlay. Lands in PR-5. The owner watches whether `control-cli` plus screenshots suffice, and flags it if a real overlay driver is needed.
- Wayland injection and hotkey may need compositor portals. Lands in PR-4. The owner watches the portal requirement.
- Live correction of inserted text may not be reversible in every target app. Lands in PR-4. The owner watches the clipboard fallback.
- GPUI or gpui-ce may not build on all three OS. Lands in PR-2. The owner watches the CI matrix.
- Streaming ASR quality in FR may lag EN. Lands in PR-3. The owner watches the FR verdict per candidate.

## Appendix D. Links and reading list

Read `pstack/skills/poteto-mode/playbooks/autopilot-stack.md` and `pstack/skills/poteto-mode/playbooks/prototype.md` before editing. PR-2 gets `pstack/skills/how/SKILL.md`. PR-3 and PR-4 get `pstack/skills/interrogate/SKILL.md` because the engine and the OS integration are contested. Each owner keeps a `decisions.tsv` trail per `pstack/skills/show-me-your-work/SKILL.md`.
