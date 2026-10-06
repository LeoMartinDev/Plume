#!/usr/bin/env node
// Spec checker for the Plume program. Asserts that a spec file or every
// spec file in a directory carries the headings and markers the plan
// requires. Exits 0 when all checks pass, 1 naming each gap otherwise.

import { readdirSync, readFileSync, statSync } from "node:fs";
import { basename, join } from "node:path";

const usage = `Usage: node scripts/check-spec.mjs <spec-file-or-directory>

Checks one spec file, or every known spec file in a directory, for the
headings and markers docs/Plume-plan.md requires. Unknown files get the
generic check: non-empty, at least one markdown heading.

Examples:
  node scripts/check-spec.mjs docs/spec
  node scripts/check-spec.mjs docs/spec/asr.md
  node scripts/check-spec.mjs docs/spec.md

Options:
  --help     Print this usage and exit 0.`;

// Required markers per known spec file. A plain string matches anywhere in
// the file. A /pattern/ matches by regular expression.
const REQUIREMENTS = {
  "product.md": ["# Product", "## Target user", "FR", "EN"],
  "principles.md": ["# Principles", "local-only", "cross-platform"],
  "ux-flow.md": ["# UX flow", "hold", "toggle", "Esc", "cleanup"],
  "platform-matrix.md": [
    "# Platform matrix",
    "macOS",
    "Windows",
    "X11",
    "Wayland",
  ],
  "glossary.md": ["# Glossary", "session state machine", "partial hypothesis"],
  "asr.md": [
    "# ASR engine",
    "## Candidates",
    "Parakeet",
    "Qwen3-ASR",
    "Whisper",
    "## Default",
  ],
  "os-integration.md": [
    "# OS integration",
    "## macOS",
    "## Windows",
    "## X11",
    "## Wayland",
    "## Live correction",
    "## Clipboard fallback",
    "## Trait mapping",
  ],
  "overlay.md": ["# Overlay", "## Variants", "## Chosen variant"],
  "interaction.md": ["# Interaction", "## Session states", "## Cleanup timing"],
  "spec.md": [
    "# Plume spec",
    "## Framework",
    "## Engine",
    "## OS integration",
    "## Overlay",
    "## Interaction",
    "## Appendix A",
  ],
};

// The known files a spec directory must contain when it is docs/spec.
const SPEC_DIR_FILES = [
  "product.md",
  "principles.md",
  "ux-flow.md",
  "platform-matrix.md",
  "glossary.md",
];

function checkFile(path, failures) {
  const name = basename(path);
  const text = readFileSync(path, "utf8");
  if (text.trim().length === 0) {
    failures.push(`${path}: file is empty`);
    return;
  }
  const required = REQUIREMENTS[name];
  if (!required) {
    if (!/^#\s/m.test(text)) {
      failures.push(`${path}: no markdown heading found`);
    }
    return;
  }
  for (const marker of required) {
    if (!text.includes(marker)) {
      failures.push(`${path}: missing required marker "${marker}"`);
    }
  }
  if (name === "spec.md" && /TODO|TBD|\bunresolved\b/i.test(text)) {
    failures.push(`${path}: contains an unresolved marker`);
  }
}

function main(argv) {
  const target = argv[2];
  if (target === "--help" || target === "-h") {
    console.log(usage);
    return 0;
  }
  if (!target) {
    console.error("error: missing spec file or directory path\n");
    console.error(usage);
    return 1;
  }

  let stat;
  try {
    stat = statSync(target);
  } catch {
    console.error(`error: ${target}: no such file or directory`);
    return 1;
  }

  const failures = [];
  if (stat.isDirectory()) {
    const entries = readdirSync(target);
    const expected =
      basename(target) === "spec" ? SPEC_DIR_FILES : [];
    for (const file of expected) {
      if (!entries.includes(file)) {
        failures.push(`${target}: missing required file ${file}`);
      }
    }
    const specs = entries.filter((entry) => entry.endsWith(".md")).sort();
    if (specs.length === 0) {
      failures.push(`${target}: no spec files found`);
    }
    for (const entry of specs) {
      checkFile(join(target, entry), failures);
    }
  } else {
    checkFile(target, failures);
  }

  if (failures.length > 0) {
    for (const failure of failures) {
      console.error(`FAIL ${failure}`);
    }
    return 1;
  }
  console.log(`ok: ${target}`);
  return 0;
}

process.exit(main(process.argv));
