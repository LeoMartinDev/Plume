# Principles

The product aims to stay local-only, cross-platform, pluggable, free and easy to understand. Current behavior is documented in the root README and the UX flow; the original program plan is historical.

## Local-only forever

Recognition runs on the device. There is no cloud recognition fallback, account or audio upload. Model downloads require network access; dictation uses the installed models locally.

## Cross-platform development

The CI builds and tests macOS, Windows and Linux. OS adapters are kept in separate modules. Linux currently uses X11; native Wayland support remains planned. A passing build does not replace live verification of native permissions, shortcuts, capture and insertion.

## Pluggable engine

Speech recognition sits behind the dependency-free `AsrEngine` trait in `plume-core`. Settings selects among the local model catalogue. Model loading and decoding stay separate from microphone capture, shortcuts and text insertion.

## Free

The app is free. Monetization is outside the current scope.

## Modern and minimal

Settings groups dictation, models, appearance and history. The bubble appears during capture and reacts to voice levels; release hides it while decoding can continue in the background. Controls expose product choices rather than implementation details.

## Readable implementation

Modules have clear responsibilities and descriptive names. Preference types, serialization and storage are separate. Named constants remain with their owning component; user-configurable history limits are validated in one domain type. Tests exercise the decoder and delivery functions used by production.
