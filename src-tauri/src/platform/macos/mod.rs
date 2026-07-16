//! Реализации платформенной границы для macOS.

mod audio;
mod focus;
mod inject;
mod overlay;
mod permissions;

use super::{shared, PlatformServices};
use anyhow::Result;
use std::sync::Arc;

pub fn create(app: &tauri::AppHandle) -> Result<PlatformServices> {
    Ok(PlatformServices {
        audio: Arc::new(shared::CpalAudioCapture::new(audio::builtin_matcher())),
        hotkey: Arc::new(shared::PluginGlobalHotkey::new(app.clone())),
        injector: Arc::new(inject::MacTextInjector),
        focus: Arc::new(focus::MacFocusTracker::new(app.clone())),
        permissions: Arc::new(permissions::MacPermissionChecker),
        overlay: Arc::new(overlay::MacOverlayWindow),
        autostart: Arc::new(shared::PluginAutostart::new(app.clone())),
    })
}

/// Выполняет замыкание на главном потоке AppKit и синхронно возвращает
/// результат. С главного потока выполняется инлайн — это важно: в `setup`
/// событийный цикл ещё не крутится, и ожидание `run_on_main_thread`
/// заблокировалось бы навсегда.
pub(crate) fn on_main<T: Send + 'static>(
    app: &tauri::AppHandle,
    f: impl FnOnce() -> T + Send + 'static,
) -> Result<T> {
    if objc2::MainThreadMarker::new().is_some() {
        return Ok(f());
    }
    let (tx, rx) = crossbeam_channel::bounded(1);
    app.run_on_main_thread(move || {
        let _ = tx.send(f());
    })?;
    rx.recv()
        .map_err(|_| anyhow::anyhow!("главный поток не вернул результат"))
}
