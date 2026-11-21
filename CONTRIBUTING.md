# Contributing

SocialPoster splits platform integrations from the core desktop app:

- **New social networks** are built as standalone WebAssembly (`.wasm`) plugins using [`socialposter-dev/plugin-starter`](https://github.com/socialposter-dev/plugin-starter) and published on [socialposter.dev](https://socialposter.dev). PRs adding platform code directly to `src-tauri/` are not accepted.
- **Core app fixes and features** belong in this repository.

## Building a Plugin

1. Create a repository from [`socialposter-dev/plugin-starter`](https://github.com/socialposter-dev/plugin-starter) targeting `compatible_app_version: ">=1.0.0"`.
2. Implement the Extism exports (`describe`, `validate_auth`, `publish`, `delete_post`).
3. Declare every external host in `allowed_domains`, configure `auth_modes`, and write clear Markdown `setup_instructions` so users can find their own tokens/webhooks.
4. Build with `cargo build --target wasm32-unknown-unknown --release` and test locally in the app via **Plugins → Sideload .wasm**.
5. Upload the `.wasm` artefact to [socialposter.dev](https://socialposter.dev) for Ed25519 signing and distribution.

## Local App Setup

Requires [Rust](https://www.rust-lang.org/tools/install) and [Node.js](https://nodejs.org/).

```bash
git checkout -b feat/your-change
npm install
npm run tauri dev

```

## Pull Request Rules

* Keep PRs focused on a single fix or feature.
* Do not weaken Ed25519 signature verification, Wasm sandbox domain allowlists, or OS keyring storage.
* Avoid `.unwrap()` and `panic!()` in Tauri commands; propagate `Result<T, String>` errors to the UI.
* Use British English for UI copy and error messages.
* Run these checks before submitting:

```bash
npm run build
cd src-tauri
cargo fmt --check
cargo clippy -- -D warnings
cargo test

```