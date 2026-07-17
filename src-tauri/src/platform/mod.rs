//! Платформенная граница.
//!
//! Все точки касания ОС описаны трейтами в этом файле. Общий код работает
//! только с этими трейтами и типами и не знает о существовании платформ.
//! `#[cfg(target_os)]` разрешён ТОЛЬКО здесь (фабрика) и внутри `macos/`,
//! `windows/`. Реализации, опирающиеся на кроссплатформенные библиотеки
//! (cpal, плагины Tauri), живут в `shared/` и подключаются фабриками платформ.

pub mod shared;

#[cfg(target_os = "macos")]
pub mod macos;

// С этапа 2 реализации настоящие (WinAPI) и, как и macos/, компилируются
// только на своей платформе.
#[cfg(target_os = "windows")]
pub mod windows;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

// ---------------------------------------------------------------------------
// Аудио
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct AudioDevice {
    pub id: String,
    pub name: String,
    pub is_builtin: bool,
    pub is_default: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeviceSelector {
    /// Системное устройство ввода по умолчанию.
    Default,
    /// Встроенный микрофон машины (опция «всегда встроенный»).
    Builtin,
    /// Конкретное устройство по id.
    ById(String),
}

/// Колбэк получает чанк interleaved-сэмплов f32, частоту дискретизации и
/// число каналов. Вызывается из аудио-потока ОС — внутри только пересылка.
pub type ChunkCallback = Box<dyn FnMut(&[f32], u32, u16) + Send>;

/// Открытый поток захвата. Инвариант №1 проекта: поток существует только
/// внутри состояний Arming/Recording машины состояний сессии.
pub trait CaptureStream: Send {
    /// Синхронно останавливает поток и полностью освобождает устройство.
    /// Drop реализаций обязан делать то же самое — страховка на паниках
    /// и ранних выходах.
    fn close(self: Box<Self>);
}

pub trait AudioCapture: Send + Sync {
    fn list_devices(&self) -> Result<Vec<AudioDevice>>;
    /// Встроенный микрофон, если он есть на этой машине.
    fn builtin_device(&self) -> Result<Option<AudioDevice>>;
    fn open(
        &self,
        selector: &DeviceSelector,
        on_chunk: ChunkCallback,
    ) -> Result<Box<dyn CaptureStream>>;
}

// ---------------------------------------------------------------------------
// Глобальный хоткей
// ---------------------------------------------------------------------------

/// Нажатие и отпускание глобальной комбинации — отпускание нужно режиму
/// «удержание» (push-to-talk).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotkeyEvent {
    Pressed,
    Released,
}

pub type HotkeyCallback = Box<dyn Fn(HotkeyEvent) + Send + Sync>;

pub trait GlobalHotkey: Send + Sync {
    /// Регистрирует комбинацию (формат: "Ctrl+Alt+Space").
    /// Ошибка, если комбинация занята другим глобальным хоткеем.
    fn register(&self, combo: &str, cb: HotkeyCallback) -> Result<()>;
    fn unregister(&self, combo: &str) -> Result<()>;
}

// ---------------------------------------------------------------------------
// Вставка текста
// ---------------------------------------------------------------------------

/// Платформенные примитивы вставки. Оркестрация (сохранить буфер → вставить →
/// восстановить) — общий код в `inject/`.
pub trait TextInjector: Send + Sync {
    /// Эмулирует Cmd+V (macOS) / Ctrl+V (Windows).
    fn send_paste_shortcut(&self) -> Result<()>;
    /// Посимвольный ввод юникод-строки — запасной режим вставки.
    fn type_text(&self, text: &str) -> Result<()>;
    /// Активен ли режим защищённого ввода (поле пароля) — синтетические
    /// события в нём не работают, честно сообщаем пользователю.
    fn secure_input_active(&self) -> bool;
}

// ---------------------------------------------------------------------------
// Фокус
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct FocusSnapshot {
    pub pid: i32,
    pub app_id: String,
    pub app_name: String,
}

pub trait FocusTracker: Send + Sync {
    /// Снимок приложения с фокусом (делается ДО показа оверлея).
    fn snapshot(&self) -> Result<FocusSnapshot>;
    /// То же ли приложение в фокусе сейчас: решает, вставлять ли текст.
    fn is_same_app_focused(&self, snap: &FocusSnapshot) -> Result<bool>;
}

// ---------------------------------------------------------------------------
// Разрешения
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Permission {
    Microphone,
    Accessibility,
    /// Зарезервировано под push-to-talk/Fn (этап v1 не требует).
    InputMonitoring,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionStatus {
    Granted,
    Denied,
    /// «Ещё не спрашивали» существует только там, где право выдаётся
    /// диалогом (микрофон на macOS); на Windows — системный тумблер,
    /// и реализация этот вариант не конструирует.
    #[allow(dead_code)]
    NotDetermined,
    NotApplicable,
}

pub trait PermissionChecker: Send + Sync {
    /// Права, реально необходимые на этой платформе в текущей конфигурации.
    fn required(&self) -> Vec<Permission>;
    fn status(&self, p: Permission) -> PermissionStatus;
    /// Системный запрос права, где он существует (микрофон на macOS).
    fn request(&self, p: Permission) -> Result<()>;
    /// Открыть соответствующий раздел системных настроек.
    fn open_settings(&self, p: Permission) -> Result<()>;
}

// ---------------------------------------------------------------------------
// Оверлей
// ---------------------------------------------------------------------------

/// Событие приложения: платформа поймала клик по неактивирующемуся окну
/// оверлея нативно. Нужно там, где click-роутинг вебвью не переживает
/// неактивирующийся режим (WebView2 под MA_NOACTIVATE не доводит клик до
/// DOM); общий код подписывается и трактует как нажатие кнопки ✕.
pub const OVERLAY_NATIVE_CLICK_EVENT: &str = "overlay-native-click";

/// Превращение обычных окон Tauri в неактивирующиеся панели поверх всех окон.
/// Инвариант №2 проекта: оверлей не забирает фокус у целевого приложения.
/// Показ/скрытие — тоже через трейт: обычный `window.show()` может активировать
/// приложение, платформа обязана показывать панель без активации.
pub trait OverlayWindow: Send + Sync {
    fn make_non_activating(&self, win: &tauri::WebviewWindow) -> Result<()>;
    fn set_click_through(&self, win: &tauri::WebviewWindow, on: bool) -> Result<()>;
    /// Показывает панель поверх всех окон, не забирая фокус.
    fn show(&self, win: &tauri::WebviewWindow) -> Result<()>;
    fn hide(&self, win: &tauri::WebviewWindow) -> Result<()>;
}

// ---------------------------------------------------------------------------
// Автозапуск
// ---------------------------------------------------------------------------

pub trait Autostart: Send + Sync {
    fn is_enabled(&self) -> Result<bool>;
    fn set_enabled(&self, on: bool) -> Result<()>;
}

// ---------------------------------------------------------------------------
// Фабрика
// ---------------------------------------------------------------------------

/// Полный набор платформенных сервисов; общий код получает его один раз
/// при старте и дальше платформы не различает.
#[derive(Clone)]
pub struct PlatformServices {
    pub audio: Arc<dyn AudioCapture>,
    pub hotkey: Arc<dyn GlobalHotkey>,
    pub injector: Arc<dyn TextInjector>,
    pub focus: Arc<dyn FocusTracker>,
    pub permissions: Arc<dyn PermissionChecker>,
    pub overlay: Arc<dyn OverlayWindow>,
    pub autostart: Arc<dyn Autostart>,
}

#[cfg(target_os = "macos")]
pub fn create(app: &tauri::AppHandle) -> Result<PlatformServices> {
    macos::create(app)
}

#[cfg(target_os = "windows")]
pub fn create(app: &tauri::AppHandle) -> Result<PlatformServices> {
    windows::create(app)
}

/// Платформенные плагины Tauri (например, nspanel на macOS).
/// Общий код о них не знает — регистрация только здесь.
#[cfg(target_os = "macos")]
pub fn register_plugins<R: tauri::Runtime>(builder: tauri::Builder<R>) -> tauri::Builder<R> {
    builder.plugin(tauri_nspanel::init())
}

#[cfg(target_os = "windows")]
pub fn register_plugins<R: tauri::Runtime>(builder: tauri::Builder<R>) -> tauri::Builder<R> {
    builder
}
