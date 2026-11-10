//! The GPUI engine (issue #63), phase 2: a menu-bar shell that reuses the engine-agnostic
//! core. Tray icon, global hotkeys, native overlay/toast/preview/pin, a GPUI Preferences
//! window, single-instance and CLI flags. The annotation editor is still web-only (phase 3)
//! and the updater is the Tauri build's job until phase 4.

mod autostart;
mod host;
mod hotkeys;
mod settings;
mod single;
mod theme;
mod tray;

use std::sync::Arc;
use std::time::Duration;

use gpui::{
    App, AppContext, Application, AsyncApp, Bounds, Global, SharedString, Size, Timer,
    TitlebarOptions, WindowBounds, WindowHandle, WindowOptions, px,
};

use crate::action::Action;
use crate::capture::{capture_full, capture_window, start_area_capture};
use crate::config;
use crate::host as seam;
use crate::paths;
use crate::platform;
use crate::store;
use crate::toast;

/// Start the GPUI engine. Called from the `scrme-gpui` binary.
pub fn run() {
    // Core first: config, the newest capture on disk, then the engine seam.
    let cfg = config::init_from_disk();
    if let Some(dir) = paths::captures_dir() {
        store::init(dir);
    }

    // Menu clicks, hotkeys and a forwarded second launch all arrive here.
    let (tx, rx) = async_channel::unbounded::<Action>();
    if !single::claim(&tx) {
        return; // another instance is running and has the flags now
    }
    seam::install(Arc::new(host::GpuiHost::new()));

    // Startup is cheap and the agent must match the setting even after a manual edit.
    if let Err(e) = autostart::set(cfg.launch_at_login) {
        eprintln!("launch at login: {e}");
    }

    Application::new().run(move |cx| {
        accessory_activation_policy();
        if let Err(e) = tray::build(&cfg, tx.clone()) {
            eprintln!("tray: {e}");
        }
        if let Err(e) = hotkeys::install(&cfg, tx.clone()) {
            eprintln!("hotkeys: {e}");
        }
        pump(cx, rx);
        // Flags given on a cold start (`scrme-gpui --prefs`) behave like a forwarded launch.
        for action in Action::from_args(std::env::args().skip(1)) {
            dispatch(cx, action);
        }
    });
}

/// Menu-bar apps have no Dock icon and take key focus only when they ask for it.
#[cfg(target_os = "macos")]
fn accessory_activation_policy() {
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy};

    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let app = NSApplication::sharedApplication(mtm);
    // GPUI starts as a Regular app; switch before anything is shown.
    if !app.setActivationPolicy(NSApplicationActivationPolicy::Accessory) {
        eprintln!("could not become an accessory app");
    }
}

#[cfg(not(target_os = "macos"))]
fn accessory_activation_policy() {}

/// Background events → main thread. `recv` parks, so an idle app burns no CPU.
fn pump(cx: &mut App, rx: async_channel::Receiver<Action>) {
    cx.spawn(async move |cx: &mut AsyncApp| {
        while let Ok(action) = rx.recv().await {
            if cx.update(|cx| dispatch(cx, action)).is_err() {
                break; // the app is gone
            }
        }
    })
    .detach();
}

/// Run an action. Captures and disk work go to a worker thread; UI stays on the main thread.
fn dispatch(cx: &mut App, action: Action) {
    match action {
        Action::Area | Action::Screen | Action::Window => {
            std::thread::spawn(move || match action {
                Action::Area => start_area_capture(),
                Action::Screen => capture_full(),
                _ => capture_window(),
            });
        }
        // Let the closing tray menu give activation back before the window appears.
        Action::Prefs => {
            cx.spawn(async move |cx: &mut AsyncApp| {
                Timer::after(Duration::from_millis(80)).await;
                let _ = cx.update(open_prefs);
            })
            .detach();
        }
        // Reopen the newest capture: no GPUI editor yet, so copy it.
        Action::Last => {
            std::thread::spawn(
                || match store::latest_path().and_then(|p| store::load(&p)) {
                    Some(img) => {
                        crate::host::host().copy_image(&img);
                        toast::ok_now("Copied");
                    }
                    None => toast::err_now("No capture yet"),
                },
            );
        }
        Action::Update => toast::ok_now("Signed updates arrive with the next GPUI phase"),
        Action::Quit => cx.quit(),
    }
}

/// The Preferences window, if it is open.
#[derive(Default)]
struct PrefsWindow(Option<WindowHandle<settings::Settings>>);

impl Global for PrefsWindow {}

/// Hide the window (called by the view when it closes, so a re-open builds a fresh one).
pub(crate) fn clear_prefs(cx: &mut App) {
    cx.set_global(PrefsWindow(None));
}

fn open_prefs(cx: &mut App) {
    let existing = cx.try_global::<PrefsWindow>().and_then(|g| g.0);
    if let Some(handle) = existing
        && handle
            .update(cx, |_, window, _| window.activate_window())
            .is_ok()
    {
        platform::activate_app();
        cx.activate(true);
        return;
    }

    let bounds = Bounds::centered(None, Size::new(px(460.), px(820.)), cx);
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        titlebar: Some(TitlebarOptions {
            title: Some(SharedString::from("scrme Preferences")),
            // The gradient header runs to the window edge, under the traffic lights
            // (the header pads 84 px on the left so they never overlap the title).
            appears_transparent: true,
            traffic_light_position: None,
        }),
        is_resizable: false,
        ..Default::default()
    };
    let opened = cx.open_window(options, |window, cx| {
        let view = cx.new(settings::Settings::new);
        view.update(cx, |this, cx| this.attach(window, cx));
        view
    });
    match opened {
        Ok(handle) => cx.set_global(PrefsWindow(Some(handle))),
        Err(e) => eprintln!("preferences: {e}"),
    }
    platform::activate_app();
    cx.activate(true);
}
