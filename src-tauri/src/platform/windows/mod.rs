//! Реализации платформенной границы для Windows (этап 2).
//!
//! Кроссплатформенные части (cpal-аудио, плагины хоткеев и автозапуска)
//! берутся из `shared/` — так же, как в фабрике macOS. Платформенные здесь:
//! оверлей (WS_EX_NOACTIVATE), вставка (SendInput), фокус
//! (GetForegroundWindow), разрешения (тумблер микрофона в реестре).

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
        injector: Arc::new(inject::WindowsTextInjector),
        focus: Arc::new(focus::WindowsFocusTracker),
        permissions: Arc::new(permissions::WindowsPermissionChecker),
        overlay: Arc::new(overlay::WindowsOverlayWindow),
        autostart: Arc::new(shared::PluginAutostart::new(app.clone())),
    })
}
