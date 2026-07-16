//! Этап 2: неактивирующийся оверлей на Windows.
//!
//! План: WS_EX_NOACTIVATE | WS_EX_TOPMOST | WS_EX_TOOLWINDOW на HWND хоста;
//! отдельно проверить перехват фокуса дочерним HWND WebView2 (риск R-1 SPEC.md).

use crate::platform::OverlayWindow;
use anyhow::Result;

pub struct WindowsOverlayWindow;

impl OverlayWindow for WindowsOverlayWindow {
    fn make_non_activating(&self, _win: &tauri::WebviewWindow) -> Result<()> {
        unimplemented!("windows: WS_EX_NOACTIVATE для окна оверлея")
    }

    fn set_click_through(&self, _win: &tauri::WebviewWindow, _on: bool) -> Result<()> {
        unimplemented!("windows: WS_EX_TRANSPARENT (click-through) для окна оверлея")
    }

    fn show(&self, _win: &tauri::WebviewWindow) -> Result<()> {
        unimplemented!("windows: показ оверлея без активации (SW_SHOWNOACTIVATE)")
    }

    fn hide(&self, _win: &tauri::WebviewWindow) -> Result<()> {
        unimplemented!("windows: скрытие оверлея")
    }
}
