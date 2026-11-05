# AGENTS.md

Guide for AI coding agents (and humans) working on **scrme**: a tiny, fast, open-source (MIT) screenshot app. Tauri 2 + Rust backend, vanilla HTML/JS frontend, macOS first.

## Layout

```
src-tauri/src/        lib.rs (wiring) · actions (hotkey/tray/CLI dispatch) · capture · output (clipboard/save) ·
                       toast · windows (overlay/settings/editor + raise) · commands (webview IPC) · config · state · tray · update
src-tauri/icons/       app icons; tray.png = menu bar template icon
src/                   frontend, no bundler: overlay (area select), editor, settings, toast
src/theme.css          shared palette + mascot styles (toast + settings)
design/                icon source SVGs (rendered with ImageMagick)
scripts/release.sh     version bump + commit + tag + push
scripts/measure.sh     memory/threads/CPU snapshot of the app + WebKit helpers
install.sh             one-line macOS installer (downloads latest release .dmg)
```

## Commands

```sh
cd src-tauri && cargo tauri dev                                  # run
cargo test --manifest-path src-tauri/Cargo.toml                  # tests
cargo fmt --manifest-path src-tauri/Cargo.toml                   # format
cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings # lint
```

CI uses the latest stable toolchain, so `rustup update stable` first; an older local clippy misses new lints (CI once failed on `chunks_exact` → `as_chunks`).

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

## CLI / automation

A second launch is forwarded to the running instance (`tauri-plugin-single-instance`): `scrme --area | --screen | --window | --prefs | --update | --quit`. All entry points (hotkeys, tray, CLI) go through `actions::dispatch`; add a new action in `actions.rs` once and it is reachable from all three.

## Native overlay + toast (macOS)

No webviews are needed at idle on macOS: `platform/overlay_macos.rs` is a borderless `NSWindow` whose layer contents are the frozen `CGImage` (dim cut-out, border and size label are `CAShapeLayer`/`CATextLayer`; Esc cancels, ⇧ on release opens the editor) and `platform/toast_macos.rs` draws the pill and mascot with Core Animation. Pure selection math lives in `capture/geometry.rs` (unit-tested). Fallbacks: the webview overlay/toast (`src/overlay.*`, `src/toast.html`) are used on other OSes or when `SCRME_WEBVIEW_OVERLAY=1` / `SCRME_WEBVIEW_TOAST=1` is set. A dismissed overlay window is parked, not released, because `dismiss` runs inside the view's own event handler.

## Capture + clipboard internals (macOS)

- `capture/macos.rs` keeps the frozen screenshot as a `CGImage` and reads its BGRA bytes once per crop; only the selected rows are converted to RGBA (xcap copied the whole retina frame 3x). xcap stays as the fallback.
- The clipboard on macOS is written natively (`pasteboard.rs` + `tiff.rs`: a hand-built uncompressed RGB TIFF moved into an `NSData`). `arboard`/the clipboard plugin retained 29-58 MB per full-screen copy forever; do not route images through them on macOS. `output::trim_memory()` returns cached malloc pages after captures.
- Regression tests that need Screen Recording permission are `#[ignore]`d: `cargo test --lib capture::macos -- --ignored --nocapture` (pixel parity with xcap, no flip, no accumulation over repeated captures).

## Performance budget

Measured on the release build (macOS arm64, idle, tray + hidden overlay/toast): app 31 MB + WebKit helpers ~47 MB physical footprint, 0.0% CPU. Keep it there: no polling loops (the only timer is the 6 h update check), animations are CSS transform/opacity only and pause when the window is unfocused/hidden (`body.still`), no animated blur, big buffers (the frozen frame) are `take()`n and dropped on commit/cancel, and the editor webview is destroyed on close. Check with `scripts/measure.sh` (or `footprint -p <pid>` and `ps -o pcpu`).

## Preferences window

Frameless (`decorations(false)` + transparent): `src/settings.html` draws its own rounded frame, a draggable header (`data-tauri-drag-region`, needs the `core:window:allow-start-dragging` permission in `capabilities/settings.json`) and a close button/Esc (`close_window` command). The About card reads `about_info`: commit and ISO-8601 UTC build time are embedded by `src-tauri/build.rs` (`SCRME_COMMIT`, `SCRME_BUILT`); links go through the allow-listed `open_url` command (github.com only).

## Tray menu

Icons are Unicode glyph prefixes in the item titles (`NSMenuItem` images do not render in status-item menus on macOS 27); shortcuts are native accelerators that `sync_menu_keys` keeps in step with the config.

## Updates

In-app updates use `tauri-plugin-updater` against the release asset `latest.json`. Update bundles are
signed: the public key is in `tauri.conf.json`, the private key is the `TAURI_SIGNING_PRIVATE_KEY` repo
secret (maintainer keeps a backup; never commit it). `createUpdaterArtifacts` is on, so a local
`cargo tauri build` needs `TAURI_SIGNING_PRIVATE_KEY` set; use `cargo tauri dev` or `cargo build` instead.
Logic lives in the "updates" section of `lib.rs` (`run_update`).

## macOS signing

Release builds are **ad-hoc signed** (`bundle.macOS.signingIdentity: "-"`), not notarized. Without any signature, a quarantined download shows "scrme is damaged". Ad-hoc signing changes the code identity on every build, so macOS may re-ask for Screen Recording after an update; real fix is an Apple Developer ID + notarization (set `APPLE_*` secrets in release.yml). DMG layout/background: `src-tauri/dmg/background.png`, rendered from `design/dmg/background.html` (headless Chrome, 2x, 144 dpi).

## Releasing

`scripts/release.sh [patch|minor|major|X.Y.Z]` bumps the version in `src-tauri/Cargo.toml` and
`src-tauri/tauri.conf.json`, commits, tags `vX.Y.Z` and pushes. The tag triggers `release.yml`, which
builds macOS dmgs + signed updater bundles and publishes a GitHub Release. The same workflow can be run from the Actions tab
(`workflow_dispatch`) to bump + tag + build in one go.

Flags: `--dry-run` (print the plan, change nothing), `--force` (re-use an existing tag: move it and force-push it, the branch with `--force-with-lease`), `--no-push`. The Actions form has the same `force` and `dry_run` options.
