//! Этап 2: глобальный хоткей на Windows (tauri-plugin-global-shortcut, RegisterHotKey).

use crate::platform::{GlobalHotkey, HotkeyCallback};
use anyhow::Result;

pub struct WindowsGlobalHotkey;

impl GlobalHotkey for WindowsGlobalHotkey {
    fn register(&self, _combo: &str, _cb: HotkeyCallback) -> Result<()> {
        unimplemented!("windows: регистрация глобального хоткея")
    }

    fn unregister(&self, _combo: &str) -> Result<()> {
        unimplemented!("windows: снятие глобального хоткея")
    }
}
