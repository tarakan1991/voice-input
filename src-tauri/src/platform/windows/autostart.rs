//! Этап 2: автозапуск на Windows (tauri-plugin-autostart, ключ реестра Run).

use crate::platform::Autostart;
use anyhow::Result;

pub struct WindowsAutostart;

impl Autostart for WindowsAutostart {
    fn is_enabled(&self) -> Result<bool> {
        unimplemented!("windows: проверка автозапуска")
    }

    fn set_enabled(&self, _on: bool) -> Result<()> {
        unimplemented!("windows: включение/выключение автозапуска")
    }
}
