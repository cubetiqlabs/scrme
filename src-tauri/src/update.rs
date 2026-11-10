use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::config;
use crate::toast;
use tauri::menu::MenuItem;
use tauri::{AppHandle, Manager};

/// Signed updates come from the GitHub release `latest.json` (see tauri.conf.json).
#[derive(Default)]
pub(crate) struct UpdateState {
    /// Version already downloaded + installed on disk, waiting for a restart.
    pub(crate) ready: Mutex<Option<String>>,
    pub(crate) busy: AtomicBool,
    pub(crate) item: Mutex<Option<MenuItem<tauri::Wry>>>,
}

pub(crate) async fn do_update(app: &AppHandle, manual: bool) -> Result<Option<String>, String> {
    use tauri_plugin_updater::UpdaterExt;
    let updater = app.updater().map_err(|e| e.to_string())?;
    let Some(update) = updater.check().await.map_err(|e| e.to_string())? else {
        if manual {
            toast::ok_now("Up to date");
        }
        return Ok(None);
    };
    let v = update.version.clone();
    if manual {
        toast::ok_now("Updating…");
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
    toast::ok_now("Restart to update");
    Ok(Some(v))
}

/// Check, download and install an update. `Ok(Some(v))` = v is ready, restart to apply.
pub(crate) async fn run_update(app: &AppHandle, manual: bool) -> Result<Option<String>, String> {
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
            toast::err_now("Update failed");
        }
    }
    res
}

#[tauri::command]
pub(crate) async fn check_update(app: AppHandle) -> Result<Option<String>, String> {
    run_update(&app, true).await
}

#[tauri::command]
pub(crate) fn update_ready(state: tauri::State<UpdateState>) -> Option<String> {
    state.ready.lock().unwrap().clone()
}

#[tauri::command]
pub(crate) fn restart_app(app: AppHandle) {
    app.restart()
}

/// Auto-update: first check shortly after launch, then every 6 hours.
pub(crate) fn spawn_auto_check(app: &tauri::AppHandle) {
    let h = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(8));
        loop {
            if config::current().auto_update {
                let _ = tauri::async_runtime::block_on(run_update(&h, false));
            }
            std::thread::sleep(std::time::Duration::from_secs(6 * 3600));
        }
    });
}
