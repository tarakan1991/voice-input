//! Заглушки Windows — этап 2.
//!
//! Каждая заглушка — честный `unimplemented!()`, а не тихий no-op: no-op
//! маскировал бы пропущенную работу. Файлы компилируются на всех платформах,
//! чтобы структура этапа 2 была видна и синтаксически корректна с первого дня.

mod audio;
mod autostart;
mod focus;
mod hotkey;
mod inject;
mod overlay;
mod permissions;

// До этапа 2 реэкспорты не используются: фабрика Windows соберёт их,
// когда появится реальная реализация.
#[allow(unused_imports)]
pub use audio::WindowsAudioCapture;
#[allow(unused_imports)]
pub use autostart::WindowsAutostart;
#[allow(unused_imports)]
pub use focus::WindowsFocusTracker;
#[allow(unused_imports)]
pub use hotkey::WindowsGlobalHotkey;
#[allow(unused_imports)]
pub use inject::WindowsTextInjector;
#[allow(unused_imports)]
pub use overlay::WindowsOverlayWindow;
#[allow(unused_imports)]
pub use permissions::WindowsPermissionChecker;

#[cfg(target_os = "windows")]
pub fn create(_app: &tauri::AppHandle) -> anyhow::Result<super::PlatformServices> {
    unimplemented!("windows: сборка фабрики платформенных сервисов — этап 2")
}
