//! Every user-triggerable action: global hotkeys, the tray menu and CLI flags
//! (`scrme --screen`) all name the same things. The table lives here, outside either
//! engine, so the Tauri build and the GPUI build cannot drift apart.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Action {
    Area,
    Screen,
    Window,
    Last,
    Prefs,
    Update,
    Quit,
}

/// One table = tray item id = CLI flag (`--<name>`).
const NAMES: [(Action, &str); 7] = [
    (Action::Area, "area"),
    (Action::Screen, "screen"),
    (Action::Window, "window"),
    (Action::Last, "last"),
    (Action::Prefs, "prefs"),
    (Action::Update, "update"),
    (Action::Quit, "quit"),
];

impl Action {
    pub(crate) fn name(self) -> &'static str {
        NAMES
            .iter()
            .find(|(a, _)| *a == self)
            .map_or("", |(_, n)| n)
    }

    pub(crate) fn from_name(name: &str) -> Option<Self> {
        NAMES.iter().find(|(_, n)| *n == name).map(|(a, _)| *a)
    }

    pub(crate) fn from_flag(flag: &str) -> Option<Self> {
        Self::from_name(flag.strip_prefix("--")?)
    }

    /// Every recognised `--flag` in `args`, in order. Unknown arguments are ignored:
    /// macOS passes things like `-psn_0_12345` to a freshly launched app.
    pub(crate) fn from_args<I, S>(args: I) -> Vec<Self>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        args.into_iter()
            .filter_map(|a| Self::from_flag(a.as_ref()))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::{Action, NAMES};

    #[test]
    fn names_round_trip() {
        for (a, n) in NAMES {
            assert_eq!(a.name(), n);
            assert_eq!(Action::from_name(n), Some(a));
            assert_eq!(Action::from_flag(&format!("--{n}")), Some(a));
        }
    }

    #[test]
    fn unknown_flags_are_ignored() {
        assert_eq!(Action::from_flag("--nope"), None);
        assert_eq!(Action::from_flag("area"), None); // flags need the dashes
        assert_eq!(Action::from_name(""), None);
        assert_eq!(
            Action::from_args(["-psn_0_1", "--area", "--nope", "--quit"]),
            vec![Action::Area, Action::Quit]
        );
    }
}
