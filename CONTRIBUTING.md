# Contributing

scrme is MIT-licensed and open to everyone. Thanks for helping!

## Quick start

```sh
cargo install tauri-cli --version "^2" --locked
cd src-tauri && cargo tauri dev
```

Before opening a PR:

```sh
cargo fmt --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml
```

## Workflow

1. Open (or pick) an issue describing the change.
2. Branch from `main` (`feat/…`, `fix/…`, `docs/…`).
3. Keep the change small and focused; add a test when you add logic.
4. Open a PR that says `Closes #<issue>`. CI must pass.

## Ground rules

- Stay light: no frontend framework, no new dependency for what a few lines can do.
- Speed matters: never send pixels through JSON/base64; keep the hotkey path short.
- Be kind. Everyone is welcome.

By contributing you agree your work is released under the [MIT License](LICENSE).
