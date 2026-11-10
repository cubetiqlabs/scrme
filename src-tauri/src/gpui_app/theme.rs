//! The palette from `src/shared/theme.css`, so the GPUI Preferences window and the web UI
//! look like the same app. Follows the system light/dark appearance.

use gpui::{Window, WindowAppearance};

#[derive(Clone, Copy)]
pub(crate) struct Theme {
    pub(crate) bg: u32,
    pub(crate) card: u32,
    pub(crate) line: u32,
    pub(crate) fg: u32,
    pub(crate) muted: u32,
    pub(crate) chip: u32,
    pub(crate) accent: u32,
    pub(crate) pink: u32,
    pub(crate) good: u32,
    pub(crate) bad: u32,
}

const LIGHT: Theme = Theme {
    bg: 0xfaf8ff,
    card: 0xffffff,
    line: 0xece7ff,
    fg: 0x2a2240,
    muted: 0x8b84a3,
    chip: 0xf3efff,
    accent: 0x7c5cff,
    pink: 0xff8ad4,
    good: 0x2a9d57,
    bad: 0xe5484d,
};

const DARK: Theme = Theme {
    bg: 0x171420,
    card: 0x221e30,
    line: 0x332e47,
    fg: 0xf1eefc,
    muted: 0x9c95b5,
    chip: 0x2d2840,
    accent: 0x9b83ff,
    pink: 0xff8ad4,
    good: 0x46cf86,
    bad: 0xff6b6e,
};

impl Theme {
    pub(crate) fn of(window: &Window) -> Self {
        match window.appearance() {
            WindowAppearance::Dark | WindowAppearance::VibrantDark => DARK,
            _ => LIGHT,
        }
    }
}
