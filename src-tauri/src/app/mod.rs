//! Сборка приложения: плагины, состояние, трей, оверлей, хоткей, жизненный цикл.

pub mod commands;
pub mod events;
pub mod overlay_ctl;
pub mod session;
pub mod state;
pub mod tray;

use crate::app::state::{AppState, ConfigStore};
use crate::asr::AsrEngine;
use crate::history::History;
use crate::models::ModelStore;
use crate::postproc::local::LocalLlm;
use anyhow::{Context, Result};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Manager, WindowEvent};

pub fn run() {
    let mut builder = tauri::Builder::default()
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_notification::init());
    builder = crate::platform::register_plugins(builder);

    builder
        .setup(|app| {
            setup(app.handle())
                .map_err(|e| -> Box<dyn std::error::Error> { format!("{e:#}").into() })
        })
        .on_window_event(|window, event| {
            // Закрытие окна настроек прячет его в трей, приложение живёт.
            if window.label() == "main" {
                if let WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::config_get,
            commands::config_set,
            commands::devices_list,
            commands::builtin_device,
            commands::permissions_list,
            commands::permission_request,
            commands::permission_open_settings,
            commands::models_status,
            commands::model_download,
            commands::model_download_cancel,
            commands::model_delete,
            commands::model_test,
            commands::cloud_key_set,
            commands::cloud_key_present,
            commands::cloud_validate,
            commands::hotkey_check,
            commands::dictation_toggle,
            commands::dictation_cancel,
            commands::dictation_test,
            commands::history_list,
            commands::history_delete,
            commands::history_clear,
            commands::autostart_status,
            commands::autostart_set,
            commands::pause_set,
            commands::wizard_complete,
            commands::main_window_hide,
        ])
        .run(tauri::generate_context!())
        .expect("ошибка запуска приложения");
}

fn setup(app: &AppHandle) -> Result<()> {
    let config_dir = app
        .path()
        .app_config_dir()
        .context("каталог конфигурации недоступен")?;
    let data_dir = app
        .path()
        .app_data_dir()
        .context("каталог данных недоступен")?;

    let config = Arc::new(ConfigStore::load(crate::config::config_path(&config_dir))?);
    let history = Arc::new(History::open(&data_dir.join("history.sqlite"))?);
    let store = Arc::new(ModelStore::new(data_dir.join("models")));
    let asr = Arc::new(AsrEngine::new());
    let llm = Arc::new(LocalLlm::new()?);
    let services = crate::platform::create(app)?;

    let session = session::spawn(session::SessionDeps {
        app: app.clone(),
        services: services.clone(),
        config: config.clone(),
        history: history.clone(),
        store: store.clone(),
        asr: asr.clone(),
        llm: llm.clone(),
    });

    let app_state = AppState {
        services: services.clone(),
        config: config.clone(),
        history,
        store,
        asr: asr.clone(),
        llm: llm.clone(),
        session,
        paused: Arc::new(std::sync::atomic::AtomicBool::new(false)),
    };
    app.manage(app_state);

    tray::build(app)?;
    overlay_ctl::create_windows(app, &services)?;

    // Хоткей: ошибка регистрации не валит запуск — мастер/настройки помогут.
    let hotkey = config.get().hotkey;
    if let Err(e) = register_main_hotkey(app, &hotkey) {
        log::error!("не удалось зарегистрировать хоткей «{hotkey}»: {e:#}");
    }

    // Выгрузка моделей по простою.
    spawn_idle_unloader(app.clone());

    // Первый запуск — мастер.
    if !config.get().wizard_completed {
        if let Some(win) = app.get_webview_window("main") {
            let _ = win.show();
            let _ = win.set_focus();
        }
    }
    Ok(())
}

/// Регистрирует главный хоткей на текущую комбинацию из конфига.
pub fn register_main_hotkey(app: &AppHandle, combo: &str) -> Result<()> {
    let state = app.state::<AppState>();
    let session = state.session.clone();
    let paused = state.paused.clone();
    state.services.hotkey.register(
        combo,
        Box::new(move || {
            if !paused.load(Ordering::Relaxed) {
                session.toggle();
            }
        }),
    )
}

/// Пауза: снимаем хоткей, меняем иконку трея.
pub fn set_paused(app: &AppHandle, paused: bool) -> Result<()> {
    let state = app.state::<AppState>();
    let combo = state.config.get().hotkey;
    if paused {
        state.paused.store(true, Ordering::Relaxed);
        let _ = state.services.hotkey.unregister(&combo);
        tray::set_state(app, tray::TrayState::Paused);
    } else {
        state.paused.store(false, Ordering::Relaxed);
        register_main_hotkey(app, &combo)?;
        tray::set_state(app, tray::TrayState::Idle);
    }
    Ok(())
}

fn spawn_idle_unloader(app: AppHandle) {
    std::thread::Builder::new()
        .name("model-idle-unload".into())
        .spawn(move || loop {
            std::thread::sleep(Duration::from_secs(60));
            let Some(state) = app.try_state::<AppState>() else {
                continue;
            };
            let mins = state.config.get().model_idle_unload_mins;
            if mins == 0 {
                continue;
            }
            let idle = Duration::from_secs(mins as u64 * 60);
            state.asr.unload_if_idle(idle);
            state.llm.unload_if_idle(idle);
        })
        .ok();
}
