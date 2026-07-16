//! Неактивирующийся оверлей: превращение окон Tauri в NSPanel со стилем
//! nonactivatingPanel (tauri-nspanel). Панель видна на всех Spaces и поверх
//! полноэкранных приложений, показывается без активации приложения.

use super::on_main;
use crate::platform::OverlayWindow;
use anyhow::{anyhow, Result};
use objc2_app_kit::{NSWindowCollectionBehavior, NSWindowStyleMask};
use tauri::Manager;
use tauri_nspanel::{tauri_panel, ManagerExt, WebviewWindowExt};

tauri_panel! {
    panel!(NonActivatingPanel {
        config: {
            can_become_key_window: false,
            can_become_main_window: false
        }
    })
}

// NSStatusWindowLevel: выше обычных окон и меню-бара, ниже скринсейвера.
const OVERLAY_LEVEL: i64 = 25;

pub struct MacOverlayWindow;

fn with_panel<T: Send + 'static>(
    win: &tauri::WebviewWindow,
    f: impl FnOnce(tauri_nspanel::PanelHandle<tauri::Wry>) -> T + Send + 'static,
) -> Result<T> {
    let app = win.app_handle().clone();
    let app_for_closure = app.clone();
    let label = win.label().to_string();
    on_main(&app, move || {
        let panel = app_for_closure
            .get_webview_panel(&label)
            .map_err(|_| anyhow!("окно «{label}» ещё не превращено в панель"))?;
        Ok(f(panel))
    })?
}

impl OverlayWindow for MacOverlayWindow {
    fn make_non_activating(&self, win: &tauri::WebviewWindow) -> Result<()> {
        let window = win.clone();
        on_main(win.app_handle(), move || -> Result<()> {
            let panel = window
                .to_panel::<NonActivatingPanel>()
                .map_err(|e| anyhow!("не удалось создать NSPanel: {e}"))?;
            panel.set_style_mask(
                NSWindowStyleMask::Borderless | NSWindowStyleMask::NonactivatingPanel,
            );
            panel.set_level(OVERLAY_LEVEL);
            panel.set_collection_behavior(
                NSWindowCollectionBehavior::CanJoinAllSpaces
                    | NSWindowCollectionBehavior::FullScreenAuxiliary
                    | NSWindowCollectionBehavior::Stationary
                    | NSWindowCollectionBehavior::IgnoresCycle,
            );
            panel.set_hides_on_deactivate(false);
            panel.set_becomes_key_only_if_needed(true);
            panel.set_floating_panel(true);
            Ok(())
        })?
    }

    fn set_click_through(&self, win: &tauri::WebviewWindow, on: bool) -> Result<()> {
        with_panel(win, move |panel| panel.set_ignores_mouse_events(on))
    }

    fn show(&self, win: &tauri::WebviewWindow) -> Result<()> {
        with_panel(win, |panel| panel.order_front_regardless())
    }

    fn hide(&self, win: &tauri::WebviewWindow) -> Result<()> {
        with_panel(win, |panel| panel.hide())
    }
}
