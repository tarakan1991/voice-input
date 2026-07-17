//! Управление окнами оверлея: плашка (click-through) + отдельное окошко
//! кнопки отмены (кликабельное). Оба — неактивирующиеся панели через трейт
//! `OverlayWindow`; показ без активации, фокус остаётся у целевого приложения.

use crate::app::events;
use crate::platform::PlatformServices;
use tauri::{
    AppHandle, Emitter, Listener, LogicalSize, Manager, PhysicalPosition, WebviewUrl,
    WebviewWindowBuilder,
};

pub const OVERLAY_LABEL: &str = "overlay";
pub const CANCEL_LABEL: &str = "cancel";

const PLATE_W: f64 = 400.0;
const PLATE_H: f64 = 56.0;
const CANCEL_SIZE: f64 = 32.0;
const GAP: f64 = 8.0;
const MARGIN_BOTTOM: f64 = 72.0;

/// Создаёт оба окна (скрытыми) и превращает их в неактивирующиеся панели.
/// Плюс подписка на нативные клики платформы: там, где вебвью в
/// неактивирующемся окне не доводит клик до DOM (WebView2), платформа шлёт
/// OVERLAY_NATIVE_CLICK_EVENT — трактуем как кнопку ✕ (отмена диктовки).
pub fn create_windows(app: &AppHandle, services: &PlatformServices) -> anyhow::Result<()> {
    let session = app.state::<crate::app::state::AppState>().session.clone();
    app.listen(crate::platform::OVERLAY_NATIVE_CLICK_EVENT, move |_| {
        session.cancel();
    });
    let overlay =
        WebviewWindowBuilder::new(app, OVERLAY_LABEL, WebviewUrl::App("index.html".into()))
            .title("VoiceInput Overlay")
            .inner_size(PLATE_W, PLATE_H)
            .decorations(false)
            .transparent(true)
            .shadow(false)
            .always_on_top(true)
            .skip_taskbar(true)
            .resizable(false)
            .visible(false)
            .focused(false)
            .accept_first_mouse(true)
            .build()?;
    let cancel = WebviewWindowBuilder::new(app, CANCEL_LABEL, WebviewUrl::App("index.html".into()))
        .title("VoiceInput Cancel")
        .inner_size(CANCEL_SIZE, CANCEL_SIZE)
        // Без явного минимума Windows раздувает окно до системной
        // минимальной ширины (~136px), и кнопка занимает лишь угол.
        .min_inner_size(CANCEL_SIZE, CANCEL_SIZE)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .visible(false)
        .focused(false)
        .accept_first_mouse(true)
        .build()?;

    services.overlay.make_non_activating(&overlay)?;
    services.overlay.make_non_activating(&cancel)?;
    // Плашка прозрачна для кликов; кнопка отмены — нет.
    services.overlay.set_click_through(&overlay, true)?;
    services.overlay.set_click_through(&cancel, false)?;
    Ok(())
}

/// Позиционирует плашку внизу по центру монитора с курсором и показывает
/// оба окна без активации.
pub fn show(app: &AppHandle, services: &PlatformServices) {
    let result: anyhow::Result<()> = (|| {
        let overlay = app
            .get_webview_window(OVERLAY_LABEL)
            .ok_or_else(|| anyhow::anyhow!("окно оверлея не создано"))?;
        let cancel = app
            .get_webview_window(CANCEL_LABEL)
            .ok_or_else(|| anyhow::anyhow!("окно отмены не создано"))?;

        // Монитор, на котором курсор; фолбэк — основной.
        let cursor = app.cursor_position().ok();
        let monitor = match cursor {
            Some(pos) => app
                .monitor_from_point(pos.x, pos.y)
                .ok()
                .flatten()
                .or_else(|| app.primary_monitor().ok().flatten()),
            None => app.primary_monitor().ok().flatten(),
        };
        let Some(monitor) = monitor else {
            anyhow::bail!("не удалось определить монитор");
        };
        let scale = monitor.scale_factor();
        let mpos = monitor.position();
        let msize = monitor.size();

        let total_w = (PLATE_W + GAP + CANCEL_SIZE) * scale;
        let x = mpos.x as f64 + (msize.width as f64 - total_w) / 2.0;
        let y = mpos.y as f64 + msize.height as f64 - (PLATE_H + MARGIN_BOTTOM) * scale;
        overlay.set_size(LogicalSize::new(PLATE_W, PLATE_H))?;
        overlay.set_position(PhysicalPosition::new(x, y))?;
        // Размер кнопки задаётся и здесь: при создании Windows раздувает
        // окно до системной минимальной ширины, и справа от кнопки остаётся
        // невидимая кликабельная полоса.
        cancel.set_size(LogicalSize::new(CANCEL_SIZE, CANCEL_SIZE))?;
        let cancel_y = y + ((PLATE_H - CANCEL_SIZE) / 2.0) * scale;
        cancel.set_position(PhysicalPosition::new(x + (PLATE_W + GAP) * scale, cancel_y))?;

        services.overlay.show(&overlay)?;
        services.overlay.show(&cancel)?;
        Ok(())
    })();
    if let Err(e) = result {
        // Оверлей — вспомогательный UI: его сбой не должен ломать диктовку.
        log::warn!("не удалось показать оверлей: {e:#}");
    }
}

pub fn set_processing(app: &AppHandle) {
    // Плашка сама переключит вид по событию session-state; здесь прячем
    // кнопку отмены записи не прячем — отмена работает и в обработке.
    let _ = app.emit(
        events::SILENCE_COUNTDOWN,
        events::SilencePayload { seconds_left: None },
    );
}

pub fn hide(app: &AppHandle, services: &PlatformServices) {
    for label in [OVERLAY_LABEL, CANCEL_LABEL] {
        if let Some(win) = app.get_webview_window(label) {
            if let Err(e) = services.overlay.hide(&win) {
                log::warn!("не удалось скрыть {label}: {e:#}");
            }
        }
    }
}
