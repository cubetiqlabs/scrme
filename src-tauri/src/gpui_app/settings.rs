//! GPUI Preferences: the same sections as the web page (shortcuts, after capture, general,
//! updates, about), drawn with GPUI elements so macOS needs no second webview.

use gpui::{
    ClickEvent, Context, Div, FocusHandle, FontWeight, IntoElement, KeyDownEvent, Render,
    SharedString, Stateful, Window, div, linear_color_stop, linear_gradient, prelude::*, px, rgb,
    rgba,
};

use crate::config::{self, Config};
use crate::paths;

use super::theme::Theme;
use super::{autostart, hotkeys, tray};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Field {
    Area,
    Screen,
    Window,
}

impl Field {
    fn label(self) -> &'static str {
        match self {
            Field::Area => "Capture Area",
            Field::Screen => "Capture Screen",
            Field::Window => "Capture Window",
        }
    }

    fn hint(self) -> &'static str {
        match self {
            Field::Area => "Drag a region; hold ⇧ when releasing to edit",
            Field::Screen => "The whole primary display",
            Field::Window => "The frontmost window",
        }
    }

    fn get(self, cfg: &Config) -> &str {
        match self {
            Field::Area => &cfg.area,
            Field::Screen => &cfg.full,
            Field::Window => &cfg.window,
        }
    }

    fn set(self, cfg: &mut Config, value: String) {
        match self {
            Field::Area => cfg.area = value,
            Field::Screen => cfg.full = value,
            Field::Window => cfg.window = value,
        }
    }
}

const FIELDS: [Field; 3] = [Field::Area, Field::Screen, Field::Window];

/// Keys that only change a modifier: pressing one while recording means "keep holding".
const MODIFIER_KEYS: [&str; 9] = [
    "shift", "control", "ctrl", "alt", "option", "platform", "super", "meta", "capslock",
];

pub(crate) struct Settings {
    cfg: Config,
    /// Shortcut being recorded, if any: the next key press replaces it.
    recording: Option<Field>,
    /// Last launch-at-login value we applied (so toggles don't rewrite the agent).
    launch_at_login: bool,
    /// Small status line under the shortcuts card.
    note: Option<(String, bool)>,
    focus: FocusHandle,
}

impl Settings {
    pub(crate) fn new(cx: &mut Context<Self>) -> Self {
        let cfg = config::current();
        Self {
            launch_at_login: cfg.launch_at_login,
            cfg,
            recording: None,
            note: None,
            focus: cx.focus_handle(),
        }
    }

    /// Called once, with the window that hosts this view.
    pub(crate) fn attach(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        window.on_window_should_close(cx, |_window, cx| {
            super::clear_prefs(cx);
            true
        });
        window.focus(&self.focus);
    }

    fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        super::clear_prefs(cx);
        window.remove_window();
    }

    /// Apply, then persist: the OS (shortcuts, login item) goes first, so a refused shortcut
    /// never reaches the config file. Any failure restores the state that was live before.
    fn persist(&mut self, cx: &mut Context<Self>, keys: bool) {
        let previous = config::current();
        if keys && let Err(e) = hotkeys::apply(&self.cfg) {
            let _ = hotkeys::apply(&previous);
            self.cfg = previous;
            self.note = Some((e, true));
            cx.notify();
            return;
        }
        if self.cfg.launch_at_login != self.launch_at_login
            && let Err(e) = autostart::set(self.cfg.launch_at_login)
        {
            if keys {
                let _ = hotkeys::apply(&previous);
            }
            self.cfg = previous;
            self.note = Some((e, true));
            cx.notify();
            return;
        }
        self.launch_at_login = self.cfg.launch_at_login;
        let mut note = ("Saved".to_string(), false);
        if let Some(path) = paths::config_path()
            && let Err(e) = config::save(&path, &self.cfg)
        {
            note = (format!("Save failed: {e}"), true);
        }
        config::set(self.cfg.clone());
        if keys {
            tray::sync_keys(&self.cfg);
        }
        self.note = Some(note);
        cx.notify();
    }

    fn on_key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let stroke = &event.keystroke;
        let key = stroke.key.to_lowercase();
        if let Some(field) = self.recording {
            if key == "escape" {
                self.recording = None;
                self.note = None;
                cx.notify();
                return;
            }
            // Waiting for the real key: a modifier on its own is not a shortcut.
            if MODIFIER_KEYS.contains(&key.as_str()) {
                return;
            }
            match hotkeys::from_keystroke(stroke) {
                Ok(shortcut) => {
                    Field::set(field, &mut self.cfg, shortcut);
                    self.recording = None;
                    self.persist(cx, true);
                }
                Err(e) => {
                    self.note = Some((e, true));
                    cx.notify();
                }
            }
            return;
        }
        if key == "escape" || (key == "w" && stroke.modifiers.platform) {
            self.close(window, cx);
        }
    }

    fn toggle(&mut self, cx: &mut Context<Self>, set: impl FnOnce(&mut Config)) {
        set(&mut self.cfg);
        self.persist(cx, false);
    }
}

impl Render for Settings {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = Theme::of(window);

        let mut keys = card(&t);
        for (i, field) in FIELDS.into_iter().enumerate() {
            let showing = self.recording == Some(field);
            let value = field.get(&self.cfg).to_string();
            let caps = if showing {
                div()
                    .flex()
                    .items_center()
                    .h(px(24.))
                    .px(px(8.))
                    .rounded(px(7.))
                    .bg(rgb(t.chip))
                    .text_size(px(12.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(t.accent))
                    .child("Press keys…")
            } else {
                keycaps(&t, &value)
            };
            let row = row(&t, i > 0)
                .id(SharedString::from(format!("key-{}", field.label())))
                .cursor_pointer()
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                    this.recording = Some(field);
                    this.note = None;
                    window.focus(&this.focus);
                    cx.notify();
                }))
                .child(label(&t, field.label(), field.hint()))
                .child(caps);
            keys = keys.child(row);
        }

        let preview_on = self.cfg.animation;
        let autosave_on = self.cfg.autosave;
        let launch_on = self.cfg.launch_at_login;
        let note = self.note.clone();

        let after = card(&t)
            .child(
                row(&t, false)
                    .id("toggle-preview")
                    .cursor_pointer()
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                        this.toggle(cx, |c| c.animation = !c.animation)
                    }))
                    .child(label(
                        &t,
                        "Preview after capture",
                        "A floating thumbnail with Edit, Copy and Save as",
                    ))
                    .child(switch(&t, preview_on)),
            )
            .child(
                row(&t, true)
                    .id("toggle-autosave")
                    .cursor_pointer()
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                        this.toggle(cx, |c| c.autosave = !c.autosave)
                    }))
                    .child(label(
                        &t,
                        "Also save a PNG",
                        "To ~/Pictures/scrme · quick captures",
                    ))
                    .child(switch(&t, autosave_on)),
            );

        let general = card(&t).child(
            row(&t, false)
                .id("toggle-launch")
                .cursor_pointer()
                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                    this.toggle(cx, |c| c.launch_at_login = !c.launch_at_login)
                }))
                .child(label(
                    &t,
                    "Launch at login",
                    "Start scrme in the menu bar when you sign in",
                ))
                .child(switch(&t, launch_on)),
        );

        let updates = card(&t).child(
            row(&t, false)
                .child(label(
                    &t,
                    "Automatic updates",
                    "Signed updates arrive with the next GPUI phase",
                ))
                .child(
                    div()
                        .flex_none()
                        .text_size(px(11.5))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgb(t.muted))
                        .child("soon"),
                ),
        );

        let about = card(&t)
            .child(
                row(&t, false)
                    .child(quiet(&t, "Version"))
                    .child(code(&t, env!("CARGO_PKG_VERSION"))),
            )
            .child(
                row(&t, true)
                    .child(quiet(&t, "Commit"))
                    .child(code(&t, env!("SCRME_COMMIT"))),
            )
            .child(
                row(&t, true)
                    .child(quiet(&t, "Built"))
                    .child(code(&t, env!("SCRME_BUILT"))),
            );

        div()
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(t.bg))
            .text_size(px(13.))
            .text_color(rgb(t.fg))
            .track_focus(&self.focus)
            .on_key_down(
                cx.listener(|this, event: &KeyDownEvent, window, cx| {
                    this.on_key(event, window, cx)
                }),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(14.))
                    .pl(px(84.)) // clear the native traffic lights
                    .pr(px(20.))
                    .py(px(16.))
                    .bg(linear_gradient(
                        135.,
                        linear_color_stop(rgb(t.pink), 0.),
                        linear_color_stop(rgb(t.accent), 1.),
                    ))
                    .text_color(rgba(0xffffffff))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(1.))
                            .child(
                                div()
                                    .text_size(px(17.))
                                    .font_weight(FontWeight::BOLD)
                                    .child("scrme"),
                            )
                            .child(
                                div()
                                    .text_size(px(11.5))
                                    .text_color(rgba(0xffffffcc))
                                    .child("Preferences"),
                            ),
                    ),
            )
            .child(
                div()
                    .id("body")
                    .flex_1()
                    .overflow_y_scroll()
                    .px(px(20.))
                    .pb(px(16.))
                    .child(heading(&t, "Shortcuts"))
                    .child(keys)
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .px(px(4.))
                            .child(match &note {
                                Some((text, true)) => div()
                                    .text_size(px(11.5))
                                    .text_color(rgb(t.bad))
                                    .child(text.clone()),
                                Some((text, false)) => div()
                                    .text_size(px(11.5))
                                    .text_color(rgb(t.good))
                                    .child(text.clone()),
                                None => div()
                                    .text_size(px(11.5))
                                    .text_color(rgb(t.muted))
                                    .child("Click a shortcut, then press new keys"),
                            })
                            .child(reset_button(&t, cx)),
                    )
                    .child(heading(&t, "After capture"))
                    .child(after)
                    .child(heading(&t, "General"))
                    .child(general)
                    .child(heading(&t, "Updates"))
                    .child(updates)
                    .child(
                        div()
                            .mt(px(12.))
                            .px(px(4.))
                            .text_size(px(11.5))
                            .text_color(rgb(t.muted))
                            .child("Hold ⇧ when releasing an area to open the editor."),
                    )
                    .child(heading(&t, "About"))
                    .child(about)
                    .child(
                        div()
                            .mt(px(10.))
                            .text_size(px(11.5))
                            .text_center()
                            .text_color(rgb(t.muted))
                            .child("MIT · Made with ♥ by Sambo Chea"),
                    ),
            )
    }
}

// ---------------------------------------------------------------------------------------
// Small element helpers, all palette-driven so light and dark stay consistent.
// ---------------------------------------------------------------------------------------

fn heading(t: &Theme, text: &str) -> Div {
    div()
        .mt(px(14.))
        .mb(px(6.))
        .px(px(4.))
        .text_size(px(11.))
        .font_weight(FontWeight::BOLD)
        .text_color(rgb(t.muted))
        .child(text.to_uppercase())
}

fn card(t: &Theme) -> Div {
    div()
        .flex()
        .flex_col()
        .border_1()
        .border_color(rgb(t.line))
        .rounded(px(14.))
        .bg(rgb(t.card))
        .px(px(14.))
}

fn row(t: &Theme, separated: bool) -> Div {
    let row = div()
        .flex()
        .items_center()
        .justify_between()
        .gap(px(12.))
        .min_h(px(42.))
        .py(px(6.));
    if separated {
        row.border_t_1().border_color(rgb(t.line))
    } else {
        row
    }
}

fn label(t: &Theme, title: &str, sub: &str) -> Div {
    div()
        .flex()
        .flex_col()
        .flex_1()
        .gap(px(1.))
        .child(
            div()
                .text_size(px(13.))
                .font_weight(FontWeight::MEDIUM)
                .child(title.to_string()),
        )
        .child(
            div()
                .text_size(px(11.5))
                .text_color(rgb(t.muted))
                .child(sub.to_string()),
        )
}

fn quiet(t: &Theme, text: &str) -> Div {
    div()
        .text_size(px(12.5))
        .text_color(rgb(t.muted))
        .child(text.to_string())
}

fn code(t: &Theme, text: &str) -> Div {
    div()
        .px(px(7.))
        .py(px(2.))
        .rounded(px(6.))
        .bg(rgb(t.chip))
        .text_size(px(12.))
        .font_weight(FontWeight::SEMIBOLD)
        .child(text.to_string())
}

fn keycaps(t: &Theme, shortcut: &str) -> Div {
    let mut row = div().flex().items_center().gap(px(4.)).flex_none();
    for part in shortcut.split('+') {
        row = row.child(
            div()
                .h(px(24.))
                .px(px(7.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(7.))
                .bg(rgb(t.chip))
                .border_1()
                .border_color(rgb(t.line))
                .text_size(px(12.))
                .font_weight(FontWeight::SEMIBOLD)
                .child(part.to_string()),
        );
    }
    row
}

fn switch(t: &Theme, on: bool) -> Div {
    div()
        .flex()
        .flex_none()
        .items_center()
        .w(px(40.))
        .h(px(24.))
        .p(px(2.))
        .rounded_full()
        .bg(if on { rgb(t.accent) } else { rgb(t.line) })
        .when(on, |this| this.justify_end())
        .child(
            div()
                .size(px(20.))
                .rounded_full()
                .bg(rgb(0xffffff))
                .shadow_sm(),
        )
}

fn reset_button(t: &Theme, cx: &mut Context<Settings>) -> Stateful<Div> {
    let (accent, chip) = (rgb(t.accent), rgb(t.chip));
    div()
        .id("reset-shortcuts")
        .flex_none()
        .px(px(8.))
        .py(px(3.))
        .rounded(px(6.))
        .text_size(px(11.5))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(accent)
        .cursor_pointer()
        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
            let defaults = Config::default();
            this.cfg.area = defaults.area;
            this.cfg.full = defaults.full;
            this.cfg.window = defaults.window;
            this.recording = None;
            this.persist(cx, true);
        }))
        .hover(move |style| style.bg(chip))
        .child("Reset")
}
