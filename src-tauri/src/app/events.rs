//! События backend → frontend (имена и payload'ы).

use serde::Serialize;

/// Смена состояния сессии: idle | arming | recording | processing | error.
pub const SESSION_STATE: &str = "session-state";
/// Живой уровень звука для индикатора на плашке.
pub const AUDIO_LEVEL: &str = "audio-level";
/// Обратный отсчёт тишины до автостопа.
pub const SILENCE_COUNTDOWN: &str = "silence-countdown";
/// Результат тестовой диктовки (мастер, шаг «финальный тест»).
pub const DICTATION_RESULT: &str = "dictation-result";
/// Прогресс скачивания модели.
pub const DOWNLOAD_PROGRESS: &str = "download-progress";
/// Навигация главного окна (из меню трея).
pub const NAVIGATE: &str = "navigate";
/// Прогресс скачивания обновления приложения.
pub const UPDATE_PROGRESS: &str = "update-progress";

#[derive(Debug, Clone, Serialize)]
pub struct SessionStatePayload {
    /// idle | arming | recording | processing | error | notice
    pub state: &'static str,
    /// Текст для плашки (ошибка или уведомление) — системные уведомления
    /// могут быть выключены, оверлей видно всегда.
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AudioLevelPayload {
    /// RMS 0..1.
    pub level: f32,
}

#[derive(Debug, Clone, Serialize)]
pub struct SilencePayload {
    /// Секунд до автостопа; None — отсчёт скрыть.
    pub seconds_left: Option<f32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DictationResultPayload {
    pub raw: String,
    pub clean: String,
    /// Какой вычиткой получен результат: local | cloud | raw.
    pub postproc: &'static str,
    pub asr_ms: u64,
    pub postproc_ms: u64,
    /// Тестовая диктовка не удалась (мастер показывает причину и не виснет).
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DownloadProgressPayload {
    pub id: String,
    pub downloaded: u64,
    pub total: u64,
    pub done: bool,
    pub cancelled: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct NavigatePayload {
    /// settings | history | wizard
    pub route: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub struct UpdateProgressPayload {
    pub downloaded: u64,
    /// Полный размер, если сервер его сообщил.
    pub total: Option<u64>,
}
