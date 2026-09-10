# Principles

Five principles govern every decision in this program. When a design choice conflicts with one of them, the principle wins.

## Local-only forever

Audio never leaves the machine. All recognition runs on the device. There is no cloud fallback, no account, and no audio upload of any kind.

This is the differentiator against Wispr Flow, which sends audio to the cloud. A user who dictates passwords, patient notes, or private messages can check the claim by watching the network. The app makes no network calls for recognition.

## Cross-platform from day one

macOS, Windows, and Linux ship in parallel. There is no port-later phase and no porting debt. Every change builds and runs on all three systems from the first commit. A one-OS MVP was considered and rejected.

## Pluggable engine

Speech recognition sits behind a Rust `AsrEngine` trait in `stt-core`. The engine is interchangeable in settings. No engine choice is permanent. PR-3 picks the default by comparing candidates against the trait.

## Free

The app is free. Monetization is out of scope for this program.

## Modern and minimal

The bubble is the only permanent UI. It appears at the bottom center of the screen while a session runs and disappears when the session ends. There is no main window to manage. Settings cover the shortcut, the engine, and the cleanup pass, and stay out of the way.
