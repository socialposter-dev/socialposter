# SocialPoster

A desktop application for posting text and media to multiple social networks at once using your own credentials.

## Supported Platforms

| Platform | Setup Modes | Media Support |
| --- | --- | --- |
| **Bluesky** | App Password | Up to 4 images (with alt text) |
| **Mastodon** | Browser OAuth / Access Token | Up to 4 images or video (with alt text) |
| **Discord** | Webhook / Bot Token | Up to 10 images or video (with alt text) |
| **Telegram** | Bot Token | Up to 10 images or video |
| **Misskey / Sharkey** | API Token | Up to 16 images or video (with alt text) |
| **Slack** | Incoming Webhook / Bot Token | Up to 10 images or video (Bot mode) |
| **LinkedIn** | Browser OAuth / Access Token | Up to 9 images (with alt text) |
| **Tumblr** | OAuth Bearer Token | Up to 10 images (with alt text) |
| **X / Twitter** | Browser OAuth 2.0 / Access Token | Up to 4 images, 1 GIF, or 1 video (with alt text) |

## Key Features

- **Direct & Private:** Credentials are stored in your operating system's native keyring (Windows Credential Manager, macOS Keychain, Linux Secret Service). No central servers, accounts, or telemetry.
- **Multi-Profile Workspaces & Encrypted Backups:** Isolate accounts, drafts, snippets, queues, and post history per profile with optional password locking and passphrase-encrypted `.spbackup` export/import.
- **Concurrent Posting & Auto-Threading:** Dispatches posts in parallel across enabled networks, automatically splits long posts into chained reply threads on microblogging platforms, and compresses oversized images per network limit.
- **Scheduled Queue & System Tray:** Queue posts with media for future publishing, minimise to the system tray, and open Quick Compose from anywhere via a configurable global hotkey.
- **Media, Alt Text & Post Recall:** Native drag-and-drop attachments with alt text, reusable text snippets, automatic draft recovery, local post history, and reverse-order post recall ("Delete Everywhere").
- **Single-File Platform Ports:** Each social network lives in a single self-contained Rust file under `src-tauri/src/platforms/` with automatic UI discovery, SemVer compatibility checks, and built-in author attribution.

## Getting Started

### Pre-Built Installers

Download the latest installer for Windows (`.exe` / `.msi`), macOS (`.dmg`), or Linux (`.AppImage` / `.deb`) from the [Releases](https://github.com/socialposter-dev/socialposter/releases) page.

### Build from Source

Prerequisites: [Rust](https://www.rust-lang.org/tools/install) and [Node.js](https://nodejs.org/).

```bash
git clone https://github.com/socialposter-dev/socialposter.git
cd socialposter
npm install
npm run tauri dev
```

To create a production build:

```bash
npm run tauri build
```

## Contributing

Platform integrations are self-contained single files inside `src-tauri/src/platforms/`. See [CONTRIBUTING.md](CONTRIBUTING.md) for the platform template, SemVer rules, and pull request guidelines.

## Licence

Released under the [MIT Licence](LICENSE).
