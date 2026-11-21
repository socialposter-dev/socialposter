## Summary

<!-- Briefly describe the fix or feature. -->

## Checklist

- [ ] Scoped to the core desktop app (new social networks belong in standalone Wasm plugins via [`plugin-starter`](https://github.com/socialposter-dev/plugin-starter)).
- [ ] Preserves Ed25519 signature verification, Wasm sandbox domain allowlists, and OS keyring storage.
- [ ] No `.unwrap()` or `panic!()` in Tauri commands; errors propagate cleanly to the UI.
- [ ] UI copy and error messages use British English.
- [ ] Tested locally with `npm run tauri dev`.
- [ ] `npm run build`, `cargo fmt --check`, `cargo clippy -- -D warnings`, and `cargo test` pass.