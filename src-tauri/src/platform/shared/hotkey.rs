//! Глобальный хоткей через tauri-plugin-global-shortcut (обе платформы).

use crate::platform::{GlobalHotkey, HotkeyCallback};
use anyhow::{Context, Result};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

pub struct PluginGlobalHotkey {
    app: tauri::AppHandle,
}

impl PluginGlobalHotkey {
    pub fn new(app: tauri::AppHandle) -> Self {
        Self { app }
    }
}

impl GlobalHotkey for PluginGlobalHotkey {
    fn register(&self, combo: &str, cb: HotkeyCallback) -> Result<()> {
        let shortcut: Shortcut = combo
            .parse()
            .map_err(|e| anyhow::anyhow!("не удалось разобрать комбинацию «{combo}»: {e}"))?;
        self.app
            .global_shortcut()
            .on_shortcut(shortcut, move |_app, _sc, event| {
                if event.state() == ShortcutState::Pressed {
                    cb();
                }
            })
            .with_context(|| format!("комбинация «{combo}» занята или не зарегистрировалась"))?;
        Ok(())
    }

    fn unregister(&self, combo: &str) -> Result<()> {
        let shortcut: Shortcut = combo
            .parse()
            .map_err(|e| anyhow::anyhow!("не удалось разобрать комбинацию «{combo}»: {e}"))?;
        self.app
            .global_shortcut()
            .unregister(shortcut)
            .with_context(|| format!("не удалось снять комбинацию «{combo}»"))?;
        Ok(())
    }
}
