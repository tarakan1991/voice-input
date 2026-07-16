//! Реализации трейтов платформенной границы поверх кроссплатформенных
//! библиотек (cpal, плагины Tauri). Здесь нет `#[cfg(target_os)]` — эти
//! реализации подключаются фабриками платформ из `macos/` и `windows/`.

pub mod audio;
pub mod autostart;
pub mod hotkey;

pub use audio::CpalAudioCapture;
pub use autostart::PluginAutostart;
pub use hotkey::PluginGlobalHotkey;
