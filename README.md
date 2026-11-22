# SocialPoster

A desktop app for cross-posting text and media to multiple social networks at once using your own credentials and WebAssembly plugins.

- **Plugins & Platform Hub:** [socialposter.dev](https://socialposter.dev)
- **Download App:** [GitHub Releases](https://github.com/socialposter-dev/socialposter/releases)

## Features

- **Local & Private:** Credentials stay in your OS keyring (Windows Credential Manager, macOS Keychain, Linux Secret Service). No central post servers or telemetry.
- **Sandboxed Plugins:** Integrations run as isolated [Extism](https://extism.org/) WebAssembly (`.wasm`) plugins with per-plugin domain allowlists, no filesystem access, and Ed25519 signature verification. Browse and install plugins directly from [socialposter.dev](https://socialposter.dev) or sideload local `.wasm` builds.
- **Multi-Profile & Encrypted Backups:** Separate workspaces per profile with optional password locking and passphrase-encrypted `.spbackup` export/import.
- **Parallel Posting & Auto-Threading:** Posts concurrently across active networks, splits long posts into reply threads on supported platforms, and compresses oversized images automatically.
- **Queue, Tray & Hotkey:** Schedule posts with media, minimise to the system tray, and trigger Quick Compose with a customisable global hotkey.
- **Media & Post Recall:** Drag-and-drop attachments with alt text, reusable snippets, draft recovery, local post history, and bulk post deletion.

## Getting Started

### Download

Grab the latest installer for Windows (`.exe` / `.msi`), macOS (`.dmg`), or Linux (`.AppImage` / `.deb`) from [GitHub Releases](https://github.com/socialposter-dev/socialposter/releases) or [socialposter.dev](https://socialposter.dev).

### Build from Source

Requires [Rust](https://www.rust-lang.org/tools/install) and [Node.js](https://nodejs.org/).

```bash
git clone [https://github.com/socialposter-dev/socialposter.git](https://github.com/socialposter-dev/socialposter.git)
cd socialposter
npm install
npm run tauri dev

```

Build a production release:

```bash
npm run tauri build

```

## Building Plugins & Contributing

* **Create a Plugin:** Use the [`socialposter-dev/plugin-starter`](https://www.google.com/search?q=https://github.com/socialposter-dev/plugin-starter) template to build `.wasm` integrations and publish them on [socialposter.dev](https://socialposter.dev).
* **App Contributions:** See [CONTRIBUTING.md](CONTRIBUTING.md).

## Licence

SocialPoster is licensed under [PolyForm Noncommercial 1.0.0](LICENSE). Anyone is free to fork, inspect, and modify the source code for personal and non-commercial use, while all commercial rights remain reserved by the author.