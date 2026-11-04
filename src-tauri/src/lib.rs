use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::{Duration, Instant};

use image::RgbaImage;
use serde::{Deserialize, Serialize};
use tauri::image::Image;
use tauri::ipc::Response;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{
    AppHandle, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder,
};
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

// ───────────────────────── state ─────────────────────────

/// Frozen frame of the monitor, grabbed at hotkey time. Never leaves Rust:
/// the overlay only draws the selection, the crop happens here.
#[derive(Default)]
struct Frozen(Mutex<Option<RgbaImage>>);

/// Cropped image waiting to be loaded by the editor window.
#[derive(Default)]
struct Pending(Mutex<Option<RgbaImage>>);

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
struct Config {
    area: String,
    full: String,
    window: String,
    /// Also write a PNG to ~/Pictures/scrme on every quick capture.
    autosave: bool,
    /// Show the success toast.
    animation: bool,
    /// Check for updates on launch and every few hours; download in the background.
    auto_update: bool,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            area: "Cmd+Shift+2".into(),
            full: "Cmd+Shift+1".into(),
            window: "Cmd+Shift+3".into(),
            autosave: false,
            animation: true,
            auto_update: true,
        }
    }
}

#[derive(Clone, Copy)]
struct Keys {
    area: Shortcut,
    full: Shortcut,
    window: Shortcut,
}
impl Keys {
    fn parse(c: &Config) -> Result<Self, String> {
        let p = |name: &str, v: &str| v.parse::<Shortcut>().map_err(|e| format!("{name}: {e}"));
        let k = Self {
            area: p("Area", &c.area)?,
            full: p("Screen", &c.full)?,
            window: p("Window", &c.window)?,
        };
        if k.area == k.full || k.area == k.window || k.full == k.window {
            return Err("Each action needs a different shortcut".into());
        }
        Ok(k)
    }
}

struct Settings {
    cfg: Mutex<Config>,
    keys: Mutex<Keys>,
}

fn config_path(app: &AppHandle) -> Option<std::path::PathBuf> {
    app.path()
        .app_config_dir()
        .ok()
        .map(|d| d.join("config.json"))
}

fn load_config(app: &AppHandle) -> Config {
    config_path(app)
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

fn write_config(app: &AppHandle, cfg: &Config) -> std::io::Result<()> {
    let p = config_path(app).ok_or(std::io::ErrorKind::NotFound)?;
    std::fs::create_dir_all(p.parent().unwrap())?;
    std::fs::write(p, serde_json::to_string_pretty(cfg).unwrap())
}

// ───────────────────────── helpers ─────────────────────────

/// Crop rect in physical pixels, clamped to the image. None if empty.
fn clamp_rect(x: i64, y: i64, w: i64, h: i64, iw: u32, ih: u32) -> Option<(u32, u32, u32, u32)> {
    let x0 = x.clamp(0, iw as i64);
    let y0 = y.clamp(0, ih as i64);
    let x1 = (x + w).clamp(0, iw as i64);
    let y1 = (y + h).clamp(0, ih as i64);
    (x1 > x0 && y1 > y0).then(|| (x0 as u32, y0 as u32, (x1 - x0) as u32, (y1 - y0) as u32))
}

fn copy_image(app: &AppHandle, img: RgbaImage) {
    let (w, h) = img.dimensions();
    let _ = app
        .clipboard()
        .write_image(&Image::new_owned(img.into_raw(), w, h));
}

fn save_png(img: &RgbaImage) -> Result<String, String> {
    let home = std::env::var("HOME").map_err(|e| e.to_string())?;
    let dir = std::path::Path::new(&home).join("Pictures/scrme");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis());
    let path = dir.join(format!("scrme-{ms}.png"));
    img.save(&path).map_err(|e| e.to_string())?;
    Ok(path.display().to_string())
}

fn primary_monitor() -> Option<xcap::Monitor> {
    xcap::Monitor::all()
        .ok()?
        .into_iter()
        .find(|m| m.is_primary().unwrap_or(false))
}

/// Quick-capture finish: clipboard first (that's the "instant" part),
/// then the toast, then the optional file write.
fn finish(app: &AppHandle, img: RgbaImage) {
    let cfg = app.state::<Settings>().cfg.lock().unwrap().clone();
    let saved = cfg.autosave.then(|| img.clone());
    copy_image(app, img);
    if cfg.animation {
        show_toast(
            app,
            if cfg.autosave {
                "Copied & saved"
            } else {
                "Copied"
            },
        );
    }
    if let Some(img) = saved
        && let Err(e) = save_png(&img)
    {
        eprintln!("autosave failed: {e}");
    }
}

// ───────────────────────── windows ─────────────────────────

/// Transparent, click-through, never-focused pill with the success animation.
fn build_toast(app: &AppHandle) -> tauri::Result<WebviewWindow> {
    let w = WebviewWindowBuilder::new(app, "toast", WebviewUrl::App("toast.html".into()))
        .inner_size(TOAST_W, TOAST_H)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .focused(false)
        .focusable(false)
        .visible(false)
        .build()?;
    w.set_ignore_cursor_events(true)?;
    Ok(w)
}

const TOAST_W: f64 = 170.0;
const TOAST_H: f64 = 44.0;
static TOAST_GEN: AtomicU32 = AtomicU32::new(0);

fn show_toast(app: &AppHandle, text: &str) {
    show_toast_kind(app, "ok", text);
}

/// Sad, red variant for failures.
fn show_toast_err(app: &AppHandle, text: &str) {
    show_toast_kind(app, "err", text);
}

fn show_toast_kind(app: &AppHandle, kind: &str, text: &str) {
    let Some(w) = app
        .get_webview_window("toast")
        .or_else(|| build_toast(app).ok())
    else {
        return;
    };
    if let Ok(Some(m)) = w.primary_monitor() {
        let (mp, ms, s) = (m.position(), m.size(), m.scale_factor());
        let x = mp.x as f64 + (ms.width as f64 - TOAST_W * s) / 2.0;
        let y = mp.y as f64 + ms.height as f64 - (TOAST_H + 100.0) * s;
        let _ = w.set_position(PhysicalPosition::new(x, y));
    }
    let id = TOAST_GEN.fetch_add(1, Ordering::SeqCst) + 1;
    let _ = w.show();
    let _ = w.eval(format!("window.__play&&window.__play({text:?},{kind:?})"));
    let ms = if kind == "err" { 2100 } else { 1500 };
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(ms));
        if TOAST_GEN.load(Ordering::SeqCst) == id {
            let _ = w.hide();
        }
    });
}

fn build_overlay(app: &AppHandle) -> tauri::Result<WebviewWindow> {
    WebviewWindowBuilder::new(app, "overlay", WebviewUrl::App("overlay.html".into()))
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .always_on_top(true)
        .resizable(false)
        .skip_taskbar(true)
        .visible(false)
        .build()
}

fn open_settings(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("settings") {
        let _ = w.show();
        let _ = w.set_focus();
        return;
    }
    let _ = WebviewWindowBuilder::new(app, "settings", WebviewUrl::App("settings.html".into()))
        .title("Preferences")
        .inner_size(420.0, 640.0)
        .resizable(false)
        .center()
        .build();
}

fn open_editor(app: &AppHandle, w: u32, h: u32) {
    let win = app.get_webview_window("editor").unwrap_or_else(|| {
        WebviewWindowBuilder::new(app, "editor", WebviewUrl::App("editor.html".into()))
            .title("scrme")
            .visible(false)
            .build()
            .expect("editor window")
    });
    // Fit to image (physical px ≈ 2x logical on retina), capped; toolbar adds ~48px.
    let s = win.scale_factor().unwrap_or(1.0);
    let lw = (w as f64 / s).clamp(480.0, 1400.0);
    let lh = (h as f64 / s).clamp(300.0, 900.0) + 48.0;
    let _ = win.set_size(tauri::LogicalSize::new(lw, lh));
    let _ = win.center();
    let _ = win.show();
    let _ = win.set_focus();
    let _ = win.eval("window.__load && window.__load()");
}

// ───────────────────────── capture ─────────────────────────

/// Freeze the primary monitor, then show the (pre-built) transparent overlay.
fn start_area_capture(app: &AppHandle) {
    let t = Instant::now();
    // ponytail: primary monitor only; one overlay per monitor for multi-display.
    let Some(mon) = primary_monitor() else { return };
    let Ok(img) = mon.capture_image() else {
        eprintln!("capture failed (Screen Recording permission?)");
        show_toast_err(app, "Capture failed");
        return;
    };
    let scale = mon.scale_factor().unwrap_or(1.0) as f64;
    let pos = PhysicalPosition::new(
        mon.x().unwrap_or(0) as f64 * scale,
        mon.y().unwrap_or(0) as f64 * scale,
    );
    let size = PhysicalSize::new(img.width(), img.height());
    *app.state::<Frozen>().0.lock().unwrap() = Some(img);

    let Some(win) = app
        .get_webview_window("overlay")
        .or_else(|| build_overlay(app).ok())
    else {
        return;
    };
    let _ = win.set_position(pos);
    let _ = win.set_size(size);
    let _ = win.show();
    let _ = win.set_focus();
    let _ = win.eval("window.__arm&&window.__arm()");
    eprintln!("hotkey→overlay {:?}", t.elapsed());
}

fn capture_full(app: &AppHandle) {
    match primary_monitor().and_then(|m| m.capture_image().ok()) {
        Some(img) => finish(app, img),
        None => show_toast_err(app, "Capture failed"),
    }
}

/// Focused window of the frontmost app (we are an Accessory app, so never ourselves).
fn capture_window(app: &AppHandle) {
    let win = xcap::Window::all().ok().and_then(|w| {
        w.into_iter()
            .find(|w| w.is_focused().unwrap_or(false) && !w.is_minimized().unwrap_or(true))
    });
    match win.and_then(|w| w.capture_image().ok()) {
        Some(img) => finish(app, img),
        None => show_toast_err(app, "No window found"),
    }
}

// ───────────────────────── commands ─────────────────────────

#[tauri::command]
fn commit(app: AppHandle, state: tauri::State<Frozen>, x: i64, y: i64, w: i64, h: i64, edit: bool) {
    let t = Instant::now();
    if let Some(win) = app.get_webview_window("overlay") {
        let _ = win.hide();
    }
    let Some(img) = state.0.lock().unwrap().take() else {
        return;
    };
    let Some((cx, cy, cw, ch)) = clamp_rect(x, y, w, h, img.width(), img.height()) else {
        return;
    };
    let crop = image::imageops::crop_imm(&img, cx, cy, cw, ch).to_image();
    drop(img);
    if edit {
        *app.state::<Pending>().0.lock().unwrap() = Some(crop);
        open_editor(&app, cw, ch);
    } else {
        finish(&app, crop);
    }
    eprintln!("commit {:?}", t.elapsed());
}

#[tauri::command]
fn cancel(app: AppHandle, state: tauri::State<Frozen>) {
    state.0.lock().unwrap().take();
    if let Some(win) = app.get_webview_window("overlay") {
        let _ = win.hide();
    }
}

#[tauri::command]
fn edit_size(state: tauri::State<Pending>) -> Option<(u32, u32)> {
    state
        .0
        .lock()
        .unwrap()
        .as_ref()
        .map(|i| (i.width(), i.height()))
}

#[tauri::command]
fn edit_bytes(state: tauri::State<Pending>) -> Response {
    Response::new(
        state
            .0
            .lock()
            .unwrap()
            .as_ref()
            .map(|i| i.as_raw().clone())
            .unwrap_or_default(),
    )
}

/// Body = raw RGBA, headers `w`/`h` = dimensions.
fn rgba_from(req: &tauri::ipc::Request) -> Option<RgbaImage> {
    let tauri::ipc::InvokeBody::Raw(raw) = req.body() else {
        return None;
    };
    let num = |k| req.headers().get(k)?.to_str().ok()?.parse::<u32>().ok();
    RgbaImage::from_raw(num("w")?, num("h")?, raw.clone())
}

#[tauri::command]
fn copy_rgba(app: AppHandle, req: tauri::ipc::Request) -> Result<(), String> {
    let img = rgba_from(&req).ok_or("bad image")?;
    let (w, h) = img.dimensions();
    app.clipboard()
        .write_image(&Image::new_owned(img.into_raw(), w, h))
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn save_rgba(req: tauri::ipc::Request) -> Result<String, String> {
    save_png(&rgba_from(&req).ok_or("bad image")?)
}

#[tauri::command]
fn close_editor(app: AppHandle, state: tauri::State<Pending>) {
    state.0.lock().unwrap().take();
    if let Some(w) = app.get_webview_window("editor") {
        let _ = w.hide();
    }
}

#[tauri::command]
fn default_config() -> Config {
    Config::default()
}

#[tauri::command]
fn get_config(state: tauri::State<Settings>) -> Config {
    state.cfg.lock().unwrap().clone()
}

/// Validate, re-register hotkeys live (rolling back on failure), persist.
#[tauri::command]
fn set_config(app: AppHandle, state: tauri::State<Settings>, cfg: Config) -> Result<(), String> {
    let new = Keys::parse(&cfg)?;
    let old = *state.keys.lock().unwrap();
    let gs = app.global_shortcut();
    let register = |k: Keys| -> Result<(), String> {
        gs.unregister_all().map_err(|e| e.to_string())?;
        for s in [k.area, k.full, k.window] {
            gs.register(s).map_err(|e| format!("{s}: {e}"))?;
        }
        Ok(())
    };
    if let Err(e) = register(new) {
        let _ = register(old);
        return Err(e);
    }
    *state.keys.lock().unwrap() = new;
    let _ = write_config(&app, &cfg);
    sync_menu_keys(&app, &cfg);
    *state.cfg.lock().unwrap() = cfg;
    Ok(())
}

// ───────────────────────── tray menu ─────────────────────────

/// Capture items whose native (gray) shortcut hint follows the config.
struct TrayKeys {
    area: MenuItem<tauri::Wry>,
    full: MenuItem<tauri::Wry>,
    window: MenuItem<tauri::Wry>,
}

fn sync_menu_keys(app: &AppHandle, cfg: &Config) {
    let Some(t) = app.try_state::<TrayKeys>() else {
        return;
    };
    for (item, key) in [
        (&t.area, &cfg.area),
        (&t.full, &cfg.full),
        (&t.window, &cfg.window),
    ] {
        let _ = item.set_accelerator(Some(key.as_str()));
    }
}

// ───────────────────────── updates ─────────────────────────

/// Signed updates come from the GitHub release `latest.json` (see tauri.conf.json).
#[derive(Default)]
struct UpdateState {
    /// Version already downloaded + installed on disk, waiting for a restart.
    ready: Mutex<Option<String>>,
    busy: AtomicBool,
    item: Mutex<Option<MenuItem<tauri::Wry>>>,
}

async fn do_update(app: &AppHandle, manual: bool) -> Result<Option<String>, String> {
    use tauri_plugin_updater::UpdaterExt;
    let updater = app.updater().map_err(|e| e.to_string())?;
    let Some(update) = updater.check().await.map_err(|e| e.to_string())? else {
        if manual {
            show_toast(app, "Up to date");
        }
        return Ok(None);
    };
    let v = update.version.clone();
    if manual {
        show_toast(app, "Updating…");
    }
    update
        .download_and_install(|_, _| {}, || {})
        .await
        .map_err(|e| e.to_string())?;
    let st = app.state::<UpdateState>();
    *st.ready.lock().unwrap() = Some(v.clone());
    if let Some(item) = st.item.lock().unwrap().as_ref() {
        let _ = item.set_text(format!("↻  Restart to update to v{v}"));
    }
    show_toast(app, "Restart to update");
    Ok(Some(v))
}

/// Check, download and install an update. `Ok(Some(v))` = v is ready, restart to apply.
async fn run_update(app: &AppHandle, manual: bool) -> Result<Option<String>, String> {
    let st = app.state::<UpdateState>();
    if let Some(v) = st.ready.lock().unwrap().clone() {
        return Ok(Some(v));
    }
    if st.busy.swap(true, Ordering::SeqCst) {
        return Ok(None);
    }
    let res = do_update(app, manual).await;
    st.busy.store(false, Ordering::SeqCst);
    if let Err(e) = &res {
        eprintln!("update check failed: {e}");
        if manual {
            show_toast_err(app, "Update failed");
        }
    }
    res
}

#[tauri::command]
async fn check_update(app: AppHandle) -> Result<Option<String>, String> {
    run_update(&app, true).await
}

#[tauri::command]
fn update_ready(state: tauri::State<UpdateState>) -> Option<String> {
    state.ready.lock().unwrap().clone()
}

#[tauri::command]
fn restart_app(app: AppHandle) {
    app.restart()
}

// ───────────────────────── app ─────────────────────────

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, sc, ev| {
                    if ev.state != ShortcutState::Pressed {
                        return;
                    }
                    let k = *app.state::<Settings>().keys.lock().unwrap();
                    let (app, sc) = (app.clone(), *sc);
                    // Off the main thread so the event loop never stalls on capture/encode.
                    std::thread::spawn(move || {
                        if sc == k.area {
                            start_area_capture(&app);
                        } else if sc == k.full {
                            capture_full(&app);
                        } else if sc == k.window {
                            capture_window(&app);
                        }
                    });
                })
                .build(),
        )
        .manage(Frozen::default())
        .manage(Pending::default())
        .manage(UpdateState::default())
        .invoke_handler(tauri::generate_handler![
            commit,
            cancel,
            edit_size,
            edit_bytes,
            copy_rgba,
            save_rgba,
            close_editor,
            get_config,
            set_config,
            default_config,
            check_update,
            update_ready,
            restart_app
        ])
        .setup(|app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            let handle = app.handle();
            let cfg = load_config(handle);
            let keys =
                Keys::parse(&cfg).unwrap_or_else(|_| Keys::parse(&Config::default()).unwrap());
            if config_path(handle).is_some_and(|p| !p.exists()) {
                let _ = write_config(handle, &cfg);
            }
            let gs = app.global_shortcut();
            for s in [keys.area, keys.full, keys.window] {
                if let Err(e) = gs.register(s) {
                    eprintln!("cannot register {s}: {e}");
                }
            }
            app.manage(Settings {
                cfg: Mutex::new(cfg),
                keys: Mutex::new(keys),
            });

            // NSMenuItem images don't render in status-item menus on recent macOS,
            // so icons are Unicode glyph prefixes; shortcuts are native accelerators (gray).
            let cfg = app.state::<Settings>().cfg.lock().unwrap().clone();
            let item = |id: &str, label: &str, key: Option<&str>| {
                MenuItem::with_id(app, id, label, true, key)
            };
            let area = item("area", "⛶  Capture Area", Some(&cfg.area))?;
            let full = item("full", "▭  Capture Screen", Some(&cfg.full))?;
            let window = item("window", "❏  Capture Window", Some(&cfg.window))?;
            let upd = item("update", "↻  Check for Updates…", None)?;
            let prefs = item("prefs", "⚙︎  Preferences…", Some("Cmd+,"))?;
            let quit = item("quit", "⏻  Quit scrme", Some("Cmd+Q"))?;
            *app.state::<UpdateState>().item.lock().unwrap() = Some(upd.clone());
            app.manage(TrayKeys {
                area: area.clone(),
                full: full.clone(),
                window: window.clone(),
            });
            let menu = Menu::with_items(
                app,
                &[
                    &area,
                    &full,
                    &window,
                    &PredefinedMenuItem::separator(app)?,
                    &upd,
                    &prefs,
                    &PredefinedMenuItem::separator(app)?,
                    &quit,
                ],
            )?;
            TrayIconBuilder::new()
                .icon(Image::from_bytes(include_bytes!("../icons/tray.png"))?)
                .icon_as_template(true)
                .menu(&menu)
                .on_menu_event(|app, e| {
                    let app = app.clone();
                    match e.id.as_ref() {
                        "prefs" => open_settings(&app),
                        "quit" => app.exit(0),
                        "update" => {
                            if app.state::<UpdateState>().ready.lock().unwrap().is_some() {
                                app.restart();
                            }
                            tauri::async_runtime::spawn(async move {
                                let _ = run_update(&app, true).await;
                            });
                        }
                        id => {
                            let id = id.to_string();
                            std::thread::spawn(move || match id.as_str() {
                                "area" => start_area_capture(&app),
                                "full" => capture_full(&app),
                                "window" => capture_window(&app),
                                _ => {}
                            });
                        }
                    }
                })
                .build(app)?;

            // Tray + hotkeys are live; warm the hidden windows in the background so
            // startup returns immediately. (A hotkey before this finishes builds the overlay itself.)
            let h = handle.clone();
            std::thread::spawn(move || {
                let _ = build_overlay(&h);
                std::thread::sleep(Duration::from_millis(800));
                let _ = build_toast(&h);
            });

            // Auto-update: first check shortly after launch, then every 6 hours.
            let h = handle.clone();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_secs(8));
                loop {
                    if h.state::<Settings>().cfg.lock().unwrap().auto_update {
                        let _ = tauri::async_runtime::block_on(run_update(&h, false));
                    }
                    std::thread::sleep(Duration::from_secs(6 * 3600));
                }
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::{Config, Keys, clamp_rect};
    #[test]
    fn clamp() {
        assert_eq!(clamp_rect(10, 10, 50, 50, 100, 100), Some((10, 10, 50, 50)));
        assert_eq!(clamp_rect(-5, -5, 20, 20, 100, 100), Some((0, 0, 15, 15)));
        assert_eq!(clamp_rect(90, 90, 50, 50, 100, 100), Some((90, 90, 10, 10)));
        assert_eq!(clamp_rect(200, 0, 5, 5, 100, 100), None);
    }
    #[test]
    fn keys_parse_and_dupes_rejected() {
        assert!(Keys::parse(&Config::default()).is_ok());
        let c = Config {
            full: "Cmd+Shift+2".into(),
            ..Config::default()
        };
        assert!(Keys::parse(&c).is_err());
        let c = Config {
            area: "nonsense++".into(),
            ..Config::default()
        };
        assert!(Keys::parse(&c).is_err());
    }
}
