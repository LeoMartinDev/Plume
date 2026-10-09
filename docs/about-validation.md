# About and software update validation

Run the isolated settings preview with a separate storage directory:

```sh
cargo run -p plume-app --example settings_preview --locked -- /tmp/plume-about-preview --preview-update=current
```

The `--preview-update` fixture accepts `idle`, `checking`, `current`, `available`,
`downloading`, `preparing`, `ready`, and `error`. Fixtures are only read by the
settings preview. Download and installation are disabled there. Check for updates
uses the normal read-only release check.

1. About sits at the bottom of the sidebar. A small indicator appears for available
   and prepared updates. No update banner changes the height of other pages.
2. The page shows About, the installed version, a compact update row, and release notes displayed directly below. There is no logo or subtitle. Verify light and dark themes at the native window
   size, including the error and ready states.
3. Checking preserves the status and notes until the result arrives; the button is disabled during the check. Check for updates stays visible with its original label while checking or
   downloading. Download update keeps its label during download and preparation.
   Status and actions share a compact row without a card background. Progress
   appears below, separated from the buttons.
   Disabled controls cannot start another
   operation. Restart Plume appears as its own action when an update is prepared.
4. Error details stay below the row. Check for updates can retry; a successful check
   clears the error. Long details scroll without pushing the actions out of the page.
5. On a disposable release installation, verify download, verification, and restart
   with an actual release. Visual fixtures do not validate installation. Check
   Notes are visible directly in About, under Available update — X when available,
   downloading, or ready, and Installed version — X otherwise. The installed version
   remains visible at the top of the page. Check Markdown headings, lists, emphasis,
   code, and links; paragraphs wrap inside the page width and long notes scroll
   vertically with the page. Installed-version notes work offline.
   Release Markdown lives in `releases/vX.Y.Z.md` and is published as the GitHub
   release body. The installed version's file is also embedded in the app binary.

Visual checks do not establish keyboard or screen-reader accessibility compliance.
