//! IPC-команды для фронтенда. Ошибки — человеческим языком (String).

use crate::app::events;
use crate::app::state::AppState;
use crate::config::{AppConfig, CloudProvider};
use crate::history::HistoryEntry;
use crate::models::{self, DownloadOutcome, ModelKind, ModelStatus};
use crate::platform::{AudioDevice, Permission, PermissionStatus};
use crate::postproc;
use serde::Serialize;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, State};

type CmdResult<T> = Result<T, String>;

fn err_str(e: impl std::fmt::Display) -> String {
    format!("{e}")
}

// ---------------------------------------------------------------------------
// Конфиг
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn config_get(state: State<AppState>) -> AppConfig {
    state.config.get()
}

// ---------------------------------------------------------------------------
// Автообновление
// ---------------------------------------------------------------------------

/// Найденное, но ещё не установленное обновление — между check и install.
#[derive(Default)]
pub struct PendingUpdate(pub parking_lot::Mutex<Option<tauri_plugin_updater::Update>>);

#[derive(Debug, Clone, Serialize)]
pub struct UpdateInfo {
    pub version: String,
    pub notes: Option<String>,
}

/// Собирает updater с учётом тестового рычага VOICE_INPUT_UPDATE_URL
/// (подменяет endpoint — так обновление проверяется без публикации релиза).
pub(crate) fn build_updater(app: &AppHandle) -> Result<tauri_plugin_updater::Updater, String> {
    use tauri_plugin_updater::UpdaterExt;
    let mut builder = app.updater_builder();
    if let Ok(url) = std::env::var("VOICE_INPUT_UPDATE_URL") {
        log::warn!("VOICE_INPUT_UPDATE_URL задан — проверяю обновления по {url}");
        let parsed = url.parse().map_err(err_str)?;
        builder = builder.endpoints(vec![parsed]).map_err(err_str)?;
    }
    builder.build().map_err(err_str)
}

#[tauri::command]
pub async fn update_check(app: AppHandle) -> CmdResult<Option<UpdateInfo>> {
    let updater = build_updater(&app)?;
    let update = updater.check().await.map_err(err_str)?;
    let info = update.as_ref().map(|u| UpdateInfo {
        version: u.version.clone(),
        notes: u.body.clone(),
    });
    *app.state::<PendingUpdate>().0.lock() = update;
    Ok(info)
}

/// Скачивает и устанавливает найденное обновление. Прогресс — событием
/// `update-progress`. На Windows инсталлер перезапускает установку сам
/// (приложение выходит), на macOS перезапускаемся явно.
#[tauri::command]
pub async fn update_install(app: AppHandle) -> CmdResult<()> {
    let update = app
        .state::<PendingUpdate>()
        .0
        .lock()
        .take()
        .ok_or_else(|| "сначала проверьте наличие обновления".to_string())?;
    let progress_app = app.clone();
    let mut downloaded: u64 = 0;
    update
        .download_and_install(
            move |chunk, total| {
                downloaded += chunk as u64;
                let _ = progress_app.emit(
                    events::UPDATE_PROGRESS,
                    events::UpdateProgressPayload { downloaded, total },
                );
            },
            || {},
        )
        .await
        .map_err(err_str)?;
    app.restart();
}

#[tauri::command]
pub fn config_set(app: AppHandle, state: State<AppState>, config: AppConfig) -> CmdResult<()> {
    let old = state.config.get();
    state.config.set(config.clone()).map_err(err_str)?;
    // Смена комбинации или режима — перерегистрация на лету. Снять старую
    // нужно до регистрации: при смене только режима комбинация та же.
    if old.hotkey != config.hotkey || old.hotkey_mode != config.hotkey_mode {
        let _ = state.services.hotkey.unregister(&old.hotkey);
        crate::app::register_main_hotkey(&app, &config.hotkey).map_err(err_str)?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Устройства и разрешения
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn devices_list(state: State<AppState>) -> CmdResult<Vec<AudioDevice>> {
    state.services.audio.list_devices().map_err(err_str)
}

/// Встроенный микрофон машины (для шага мастера «микрофон»: опция
/// «всегда встроенный» доступна, только если он есть).
#[tauri::command]
pub fn builtin_device(state: State<AppState>) -> CmdResult<Option<AudioDevice>> {
    state.services.audio.builtin_device().map_err(err_str)
}

#[derive(Serialize)]
pub struct PermissionInfo {
    pub permission: Permission,
    pub status: PermissionStatus,
}

#[tauri::command]
pub fn permissions_list(state: State<AppState>) -> Vec<PermissionInfo> {
    state
        .services
        .permissions
        .required()
        .into_iter()
        .map(|p| PermissionInfo {
            permission: p,
            status: state.services.permissions.status(p),
        })
        .collect()
}

#[tauri::command]
pub fn permission_request(state: State<AppState>, permission: Permission) -> CmdResult<()> {
    state
        .services
        .permissions
        .request(permission)
        .map_err(err_str)
}

#[tauri::command]
pub fn permission_open_settings(state: State<AppState>, permission: Permission) -> CmdResult<()> {
    state
        .services
        .permissions
        .open_settings(permission)
        .map_err(err_str)
}

// ---------------------------------------------------------------------------
// Модели
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn models_status(state: State<AppState>) -> Vec<ModelStatus> {
    state.store.statuses()
}

#[tauri::command]
pub fn model_download(app: AppHandle, state: State<AppState>, id: String) -> CmdResult<()> {
    let store = state.store.clone();
    std::thread::Builder::new()
        .name(format!("download-{id}"))
        .spawn(move || {
            let emit = |downloaded, total, done, cancelled, error: Option<String>| {
                let _ = app.emit(
                    events::DOWNLOAD_PROGRESS,
                    events::DownloadProgressPayload {
                        id: id.clone(),
                        downloaded,
                        total,
                        done,
                        cancelled,
                        error,
                    },
                );
            };
            let mut last = Instant::now();
            let result = store.download(&id, |downloaded, total| {
                // Прогресс не чаще 10 раз в секунду.
                if last.elapsed() > Duration::from_millis(100) {
                    emit(downloaded, total, false, false, None);
                    last = Instant::now();
                }
            });
            match result {
                Ok(DownloadOutcome::Done) => emit(0, 0, true, false, None),
                Ok(DownloadOutcome::Cancelled) => emit(0, 0, false, true, None),
                Err(e) => emit(0, 0, false, false, Some(format!("{e}"))),
            }
        })
        .map_err(err_str)?;
    Ok(())
}

#[tauri::command]
pub fn model_download_cancel(state: State<AppState>, id: String) {
    state.store.cancel_download(&id);
}

#[tauri::command]
pub fn model_delete(state: State<AppState>, id: String) -> CmdResult<()> {
    state.store.delete(&id).map_err(err_str)
}

#[derive(Serialize)]
pub struct ModelTestResult {
    pub ok: bool,
    /// Распознанный/вычитанный текст тестового прогона.
    pub text: String,
    pub elapsed_ms: u64,
    pub error: Option<String>,
}

/// Тестовый прогон модели на вшитом сэмпле: убеждаемся, что файл не битый,
/// и замеряем скорость на этой машине. Блокирующая команда — Tauri выполняет
/// её в пуле, UI не виснет.
#[tauri::command]
pub fn model_test(state: State<AppState>, id: String) -> ModelTestResult {
    let run = || -> anyhow::Result<(String, u64)> {
        let info = models::find(&id).ok_or_else(|| anyhow::anyhow!("неизвестная модель"))?;
        let path = state.store.downloaded_path(&id)?;
        match info.kind {
            ModelKind::Asr => {
                let pcm = models::test_sample_pcm()?;
                let config = state.config.get();
                let out = state.asr.transcribe(
                    &path,
                    &id,
                    &pcm,
                    &config.language,
                    &config.dictionary.initial_prompt(),
                )?;
                if out.text.is_empty() {
                    anyhow::bail!("модель не распознала тестовую фразу");
                }
                Ok((out.text, out.elapsed_ms))
            }
            ModelKind::Llm => {
                let started = Instant::now();
                let raw = "ээ ну короче надо задеплоить это на стейджинг и создать пул реквест";
                let cleaned = state.llm.cleanup(
                    &path,
                    &id,
                    &postproc::system_prompt(""),
                    postproc::few_shot(),
                    raw,
                    Duration::from_secs(60),
                )?;
                if cleaned.is_empty() {
                    anyhow::bail!("модель вернула пустой результат");
                }
                Ok((cleaned, started.elapsed().as_millis() as u64))
            }
        }
    };
    match run() {
        Ok((text, elapsed_ms)) => ModelTestResult {
            ok: true,
            text,
            elapsed_ms,
            error: None,
        },
        Err(e) => ModelTestResult {
            ok: false,
            text: String::new(),
            elapsed_ms: 0,
            error: Some(format!("{e}")),
        },
    }
}

// ---------------------------------------------------------------------------
// Облачные ключи
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn cloud_key_set(provider: CloudProvider, key: String) -> CmdResult<()> {
    postproc::keys::set_api_key(provider, &key).map_err(err_str)
}

#[tauri::command]
pub fn cloud_key_present(provider: CloudProvider) -> CmdResult<bool> {
    postproc::keys::get_api_key(provider)
        .map(|k| k.is_some())
        .map_err(err_str)
}

/// Проверка ключа коротким запросом (блокирующая).
#[tauri::command]
pub fn cloud_validate(provider: CloudProvider, model: String) -> CmdResult<()> {
    let key = postproc::keys::get_api_key(provider)
        .map_err(err_str)?
        .ok_or_else(|| "ключ не сохранён".to_string())?;
    postproc::cloud::validate_key(provider, &model, &key).map_err(err_str)
}

// ---------------------------------------------------------------------------
// Хоткей
// ---------------------------------------------------------------------------

/// Проверяет, свободна ли комбинация: пробная регистрация + снятие.
#[tauri::command]
pub fn hotkey_check(state: State<AppState>, combo: String) -> CmdResult<()> {
    if combo == state.config.get().hotkey {
        return Ok(()); // текущая комбинация приложения — валидна
    }
    state
        .services
        .hotkey
        .register(&combo, Box::new(|_| {}))
        .map_err(err_str)?;
    state.services.hotkey.unregister(&combo).map_err(err_str)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Диктовка
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn dictation_toggle(state: State<AppState>) {
    if !state.paused.load(std::sync::atomic::Ordering::Relaxed) {
        state.session.toggle();
    }
}

#[tauri::command]
pub fn dictation_cancel(state: State<AppState>) {
    state.session.cancel();
}

/// Тестовая диктовка для мастера: результат придёт событием
/// `dictation-result`, вставки не будет.
#[tauri::command]
pub fn dictation_test(state: State<AppState>) {
    state.session.test();
}

// ---------------------------------------------------------------------------
// История
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn history_list(state: State<AppState>, limit: u32) -> CmdResult<Vec<HistoryEntry>> {
    state.history.list(limit).map_err(err_str)
}

#[tauri::command]
pub fn history_delete(state: State<AppState>, id: i64) -> CmdResult<()> {
    state.history.delete(id).map_err(err_str)
}

#[tauri::command]
pub fn history_clear(state: State<AppState>) -> CmdResult<()> {
    state.history.clear().map_err(err_str)
}

// ---------------------------------------------------------------------------
// Прочее
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn autostart_status(state: State<AppState>) -> CmdResult<bool> {
    state.services.autostart.is_enabled().map_err(err_str)
}

#[tauri::command]
pub fn autostart_set(state: State<AppState>, enabled: bool) -> CmdResult<()> {
    state
        .services
        .autostart
        .set_enabled(enabled)
        .map_err(err_str)
}

#[tauri::command]
pub fn pause_set(app: AppHandle, paused: bool) -> CmdResult<()> {
    crate::app::set_paused(&app, paused).map_err(err_str)
}

#[tauri::command]
pub fn wizard_complete(state: State<AppState>) -> CmdResult<()> {
    state
        .config
        .update(|c| c.wizard_completed = true)
        .map_err(err_str)
}

#[tauri::command]
pub fn main_window_hide(app: AppHandle) {
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.hide();
    }
}
