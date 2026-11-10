<p align="center"><img src="design/icon.png" width="96" alt="scrme"></p>

# scrme

Tiny, fast screenshot app for the menu bar. Press a shortcut, drag an area, and it's on your clipboard. Free and open source. Built with [Tauri](https://tauri.app) + Rust.

## Features

- **Instant area capture** — freeze the screen, drag, copied. No window, no Dock icon.
- **Screen and window capture** with one shortcut each.
- **Quick editor** — pen, arrows, boxes, text, highlight, blur, numbered markers, ruler, color picker, crop, undo/redo, and **Pin to screen** (floats the screenshot above everything).
- **Preferences** — rebind shortcuts live, optional auto-save to `~/Pictures/scrme`.
- **Auto-update** — checks in the background, or use **Check for Updates…** in the tray menu.

| Action | Default shortcut |
| --- | --- |
| Capture area (hold ⇧ on release to edit) | `⌘⇧2` |
| Capture screen | `⌘⇧1` |
| Capture window | `⌘⇧3` |

> Screen recording is planned. macOS only for now.

## Install

```sh
curl -fsSL https://raw.githubusercontent.com/cubetiqlabs/scrme/main/install.sh | bash
```

Or grab the `.dmg` from [Releases](https://github.com/cubetiqlabs/scrme/releases). On first capture, allow **Screen Recording** in System Settings → Privacy & Security.

> **Screen Recording after an update:** releases are signed with a stable identity so macOS keeps the permission across updates. If you update from an older (ad-hoc signed) build, macOS may ask one more time — enable it once and later updates will remember it.

> **"scrme is damaged" or "can't be opened"?** Builds are not notarized yet, so macOS quarantines downloaded copies. The install script handles this. For the `.dmg`, either right-click the app → **Open**, or run:
> ```sh
> xattr -dr com.apple.quarantine /Applications/scrme.app
> ```

## Getting started (from source)

Needs [Rust](https://rustup.rs) and the Tauri CLI.

```sh
cargo install tauri-cli --version "^2" --locked
git clone https://github.com/cubetiqlabs/scrme && cd scrme
cd src-tauri && cargo tauri dev
```

Run the tests with `cargo test --manifest-path src-tauri/Cargo.toml`.

## Two engines (GPUI migration)

The shipped app still runs on Tauri. The move to [GPUI](https://gpui.rs) (issue [#63](https://github.com/cubetiqlabs/scrme/issues/63)) is staged so `main` never regresses: the capture, clipboard, store, toast and preview code is engine-agnostic behind a small `Host` seam, and each engine is a cargo feature.

```sh
# Tauri engine (default, what releases ship)
cd src-tauri && cargo tauri dev

# GPUI engine (menu bar, hotkeys, native overlay/toast/preview/pin, GPUI Preferences)
cd src-tauri && cargo run --no-default-features --features gpui-app --bin scrme-gpui
```

The GPUI build compiles its Metal shaders, so macOS needs the Metal toolchain (`xcodebuild -downloadComponent MetalToolchain` on Xcode 26). Still Tauri-only in the GPUI build: the annotation editor (port in progress; captures are copied instead) and signed in-app updates.

## Contributing

Contributions are welcome. See [CONTRIBUTING.md](CONTRIBUTING.md). AI agents: see [AGENTS.md](AGENTS.md).

## License

[MIT](LICENSE) — free to use, modify and share.

## Author

Made by [Sambo Chea](https://github.com/sombochea).
