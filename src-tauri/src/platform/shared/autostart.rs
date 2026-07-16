//! Автозапуск через tauri-plugin-autostart (обе платформы).

use crate::platform::Autostart;
use anyhow::{Context, Result};
use tauri_plugin_autostart::ManagerExt;

pub struct PluginAutostart {
    app: tauri::AppHandle,
}

impl PluginAutostart {
    pub fn new(app: tauri::AppHandle) -> Self {
        Self { app }
    }
}

impl Autostart for PluginAutostart {
    fn is_enabled(&self) -> Result<bool> {
        self.app
            .autolaunch()
            .is_enabled()
            .context("проверка автозапуска")
    }

    fn set_enabled(&self, on: bool) -> Result<()> {
        let launcher = self.app.autolaunch();
        if on {
            launcher.enable().context("включение автозапуска")
        } else {
            launcher.disable().context("выключение автозапуска")
        }
    }
}
