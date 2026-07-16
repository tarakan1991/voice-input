//! Иконка в трее: статус приложения + меню управления.

use crate::app::events;
use crate::app::state::AppState;
use anyhow::{Context, Result};
use tauri::image::Image;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager};

const TRAY_ID: &str = "voice-input-tray";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayState {
    Idle,
    Recording,
    Processing,
    Error,
    Paused,
}

fn icon_for(state: TrayState) -> Result<Image<'static>> {
    let bytes: &[u8] = match state {
        TrayState::Idle => include_bytes!("../../icons/tray/tray-idle.png"),
        TrayState::Recording => include_bytes!("../../icons/tray/tray-recording.png"),
        TrayState::Processing => include_bytes!("../../icons/tray/tray-processing.png"),
        TrayState::Error => include_bytes!("../../icons/tray/tray-error.png"),
        TrayState::Paused => include_bytes!("../../icons/tray/tray-paused.png"),
    };
    Image::from_bytes(bytes).context("иконка трея не декодируется")
}

fn tooltip_for(state: TrayState) -> &'static str {
    match state {
        TrayState::Idle => "VoiceInput — готов к диктовке",
        TrayState::Recording => "VoiceInput — идёт запись",
        TrayState::Processing => "VoiceInput — обрабатываю",
        TrayState::Error => "VoiceInput — ошибка",
        TrayState::Paused => "VoiceInput — пауза",
    }
}

pub fn build(app: &AppHandle) -> Result<()> {
    let toggle = MenuItem::with_id(
        app,
        "toggle",
        "Начать/остановить диктовку",
        true,
        None::<&str>,
    )?;
    let pause = CheckMenuItem::with_id(
        app,
        "pause",
        "Пауза (снять хоткей)",
        true,
        false,
        None::<&str>,
    )?;
    let settings = MenuItem::with_id(app, "settings", "Настройки…", true, None::<&str>)?;
    let history = MenuItem::with_id(app, "history", "История распознаваний…", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, "quit", "Выйти", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[
            &toggle, &pause, &separator, &settings, &history, &separator, &quit,
        ],
    )?;

    TrayIconBuilder::with_id(TRAY_ID)
        .icon(icon_for(TrayState::Idle)?)
        .tooltip(tooltip_for(TrayState::Idle))
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(move |app, event| {
            let state = app.state::<AppState>();
            match event.id().as_ref() {
                "toggle" => {
                    if !state.paused.load(std::sync::atomic::Ordering::Relaxed) {
                        state.session.toggle();
                    }
                }
                "pause" => {
                    let now_paused = pause.is_checked().unwrap_or(false);
                    if let Err(e) = crate::app::set_paused(app, now_paused) {
                        log::error!("не удалось переключить паузу: {e:#}");
                    }
                }
                "settings" => open_main(app, "settings"),
                "history" => open_main(app, "history"),
                "quit" => {
                    state.session.shutdown();
                    app.exit(0);
                }
                _ => {}
            }
        })
        .build(app)
        .context("создание иконки трея")?;
    Ok(())
}

fn open_main(app: &AppHandle, route: &'static str) {
    let _ = app.emit(events::NAVIGATE, events::NavigatePayload { route });
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.show();
        let _ = win.set_focus();
    }
}

/// Обновляет иконку и тултип по состоянию. Ошибки только логируются.
pub fn set_state(app: &AppHandle, state: TrayState) {
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return;
    };
    if let Ok(icon) = icon_for(state) {
        let _ = tray.set_icon(Some(icon));
    }
    let _ = tray.set_tooltip(Some(tooltip_for(state)));
}
