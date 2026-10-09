<p align="center">
  <img src="crates/plume-app/assets/brand/plume.png" width="80" alt="Plume logo">
</p>

# Plume

**Speak. Release. Keep writing.**

Plume turns your voice into text in the app you're already using. Hold a shortcut, speak, and release to insert your words. Transcription runs on your computer, so your audio stays with you.

**[Get Plume →](https://github.com/LeoMartinDev/Plume/releases)** · [Report an issue](https://github.com/LeoMartinDev/Plume/issues)

![Plume in dark mode, showing the model picker and download options](docs/images/plume.png)

## Get Plume

Download the latest version from [Releases](https://github.com/LeoMartinDev/Plume/releases/latest).

| Your computer | Build to choose |
| --- | --- |
| macOS — Apple Silicon (M1 or newer) | macOS Apple Silicon |
| Windows — 64-bit Intel or AMD | Windows x64 |
| Linux — 64-bit Intel or AMD, X11 | Linux x64 |

On macOS, open the `.dmg`, drag **Plume** onto **Applications** in the window, then eject the disk image and open Plume from Applications. On Windows, run the `.exe` installer, then open Plume from the Start menu. On Ubuntu 24.04 or newer, install the `.deb` with `sudo apt install ./Plume-vVERSION-x86_64-unknown-linux-gnu.deb`. Speech models are downloaded separately inside the app.

Linux downloads are built on Ubuntu 24.04 and need an X11 session and a system tray; Wayland support is planned. Release installers are not publisher-signed or notarized, so macOS or Windows may show a security warning.

## Start dictating

1. Open Plume and follow **Model → Shortcuts → Permissions & test**. Choose and download a model, then keep or customize your shortcuts.
2. Allow microphone access. On macOS, also enable the Accessibility and Input Monitoring permissions requested by the app. You can try your shortcut inside the setup window or finish without a test. Plume then stays in your menu bar or system tray.
3. Click into a text field in your editor, browser, chat or document.
4. Hold **Ctrl + Space**, speak, then release. Plume inserts the finished transcription.

Press **Ctrl + Shift + Space** to start hands-free dictation, then press it again to stop. **Esc** cancels recording or transcription. You can change both shortcuts in **Settings → Dictation**.

Plume handles one take at a time. Wait while the bubble shows starting, transcription, insertion or cancellation. A shortcut pressed while busy is ignored; release it and press again. Recordings stop at ten minutes, with a warning at nine minutes. In hands-free mode, a visual reminder appears after thirty seconds without speech. Silence produces no insertion or saved audio.

Plume stays in your menu bar or system tray. Click its icon to reopen settings; right-click it for the menu. Closing settings keeps dictation available in the background.

Setup runs once, including for existing installations, and preserves your model and shortcuts. If interrupted, it resumes where you left off. All four models remain available afterward in **Settings → Model**.

## What you can do

- **Dictate across your desktop.** Write messages, notes, documents and prompts in the app you already have open.
- **Keep your audio local.** Recognition runs on your computer, without an account or audio uploads. Once a model is downloaded, dictation works offline.
- **Choose your model.** Pick a smaller download, faster response or higher accuracy, and switch models from settings.
- **Dictate in French or English.** Choose either language explicitly or use automatic language detection. Models also cover additional languages listed below.
- **Make insertion work for you.** Use automatic insertion, clipboard paste or typing, with an optional copy-to-clipboard fallback when insertion fails.
- **Find previous transcriptions.** Copy or delete entries from local history. Set how long to keep text and how many entries to retain, or choose unlimited text history.
- **Recover recordings.** The eight latest takes containing speech are saved locally, including failed transcriptions. Retranscribe them from history with the currently selected model and language. Recovered text is available for copying and is never inserted automatically. Delete audio alone or delete a history entry with its audio.
- **Match your desktop.** Choose light, dark or system appearance and keep Plume close at hand in the tray.
- **Stay up to date.** Plume checks GitHub Releases at startup. Download an update from **Settings → Updates**, then choose **Restart and install** when you're ready.

## Available models

All four models run locally and support French and English. Download and manage them directly in **Settings → Model**.

| Model | Download size | Language coverage | Profile |
| --- | --- | --- | --- |
| **Nemotron 3.5 Compact** | 793 MB | 35 languages | Prioritizes fast responses |
| **Whisper Base** | 142 MB | 99 languages | Smallest download; a lightweight starting point |
| **Whisper Small** | 466 MB | 99 languages | Balanced speed and accuracy |
| **Whisper Large-v3-Turbo** | 1.5 GB | 99 languages | Prioritizes accuracy, with a larger download |

Sizes are approximate. Speed and recognition quality depend on your computer, microphone, language and recording conditions.

The language picker currently offers **Automatic**, **French** and **English**. The language counts describe each model's broader coverage.

## Your data stays on your computer

Your audio is processed locally. An internet connection is needed to download models and check or download app updates, but not for dictation afterward. Transcription history is stored locally too; you control its retention and can delete it from settings. Eight audios are retained independently of text-history limits, without a time expiration, plus the active temporary recording. Audio is mono 16 kHz PCM16 WAV in the application's local-data `plume/recordings` folder. On macOS this is normally `~/Library/Application Support/plume/recordings`; on Windows, `%LOCALAPPDATA%/plume/recordings`; on Linux, `~/.local/share/plume/recordings`. Clearing history also deletes saved audio. Speech detection is prepared when downloading a model; dictation never downloads it.

## Feedback and development

Found a problem or have an idea? [Open an issue](https://github.com/LeoMartinDev/Plume/issues) with your operating system, selected model and what happened.

For building from source, configuration, troubleshooting, tests and release tooling, see the [development guide](docs/development.md).

## Insertion compatibility

Text is delivered to the field active when transcription finishes; Plume never returns focus to an earlier field. Known password and read-only fields are refused. When macOS Accessibility or Windows UI Automation supplies reliable adjacent characters, Plume adjusts only boundary spaces. Other fields, including X11, receive the exact transcription.

Paste temporarily replaces the clipboard and restores supported content after 300 ms if its native ownership/version still belongs to Plume. A new user copy is preserved. Empty content, text, HTML with text and images are supported; unsupported formats use typing in Automatic mode, or produce a compatibility error in Paste mode. Configured recovery copies intentionally keep the transcription in the clipboard. A dispatched keyboard shortcut confirms dispatch, not universal receipt by every application.
