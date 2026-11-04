# AGENTS.md

Guide for AI coding agents (and humans) working on **scrme**: a tiny, fast, open-source (MIT) screenshot app. Tauri 2 + Rust backend, vanilla HTML/JS frontend, macOS first.

## Layout

```
src-tauri/src/lib.rs   all Rust: state, capture (xcap), clipboard, tray, shortcuts, commands
src-tauri/icons/       app icons; menu/ = tray menu glyphs; tray.png = menu bar template icon
src/                   frontend, no bundler: overlay (area select), editor, settings, toast
src/theme.css          shared palette + mascot styles (toast + settings)
design/                icon source SVGs (rendered with ImageMagick)
scripts/release.sh     version bump + commit + tag + push
install.sh             one-line macOS installer (downloads latest release .dmg)
```

## Commands

```sh
cd src-tauri && cargo tauri dev                                  # run
cargo test --manifest-path src-tauri/Cargo.toml                  # tests
cargo fmt --manifest-path src-tauri/Cargo.toml                   # format
cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings # lint
```

## Principles

- **Fast + light first.** No frontend framework or npm deps. Don't add a crate for what a few lines do.
- **Pixels never cross IPC as JSON/base64.** The frozen screenshot stays in Rust; the overlay only draws the selection and Rust crops. Editor images move as raw bytes.
- Windows (overlay, toast) are pre-built hidden in the background; keep startup and the hotkey path minimal.
- Keep UI consistent: reuse `src/theme.css` tokens and the mascot.
- Non-trivial logic gets a small `#[test]` (see `clamp_rect`, `Keys::parse`).

## Workflow rules (MUST follow)

1. For every change, **create a GitHub issue first** with `gh issue create`.
2. Work on a branch, then **open a PR with `gh pr create`** whose body says `Closes #<issue>`.
3. **Never put a Claude Code session link (or any agent session URL) in a PR, issue, or commit message.**
4. Don't push directly to `main` except via the release script/workflow.
5. CI (fmt, clippy, test, build) must pass before merge.

## GitHub Actions

Always use the **latest major version** of every action (e.g. `actions/checkout@v7`). Check with
`gh api repos/<owner>/<repo>/releases/latest --jq .tag_name`. Dependabot also keeps them current.

## Releasing

`scripts/release.sh [patch|minor|major|X.Y.Z]` bumps the version in `src-tauri/Cargo.toml` and
`src-tauri/tauri.conf.json`, commits, tags `vX.Y.Z` and pushes. The tag triggers `release.yml`, which
builds macOS dmgs and publishes a GitHub Release. The same workflow can be run from the Actions tab
(`workflow_dispatch`) to bump + tag + build in one go.
