//! The GPUI build of scrme (issue #63). The Tauri build stays the default for now:
//!
//! ```sh
//! cargo run --no-default-features --features gpui-app --bin scrme-gpui
//! ```

fn main() {
    scrme_lib::gpui_app::run();
}
