//! Отслеживание приложения с фокусом через NSWorkspace.

use super::on_main;
use crate::platform::{FocusSnapshot, FocusTracker};
use anyhow::{anyhow, Result};
use objc2_app_kit::NSWorkspace;

pub struct MacFocusTracker {
    app: tauri::AppHandle,
}

impl MacFocusTracker {
    pub fn new(app: tauri::AppHandle) -> Self {
        Self { app }
    }

    fn frontmost(&self) -> Result<Option<FocusSnapshot>> {
        on_main(&self.app, || {
            let workspace = NSWorkspace::sharedWorkspace();
            let front = workspace.frontmostApplication();
            front.map(|app| FocusSnapshot {
                pid: app.processIdentifier(),
                app_id: app
                    .bundleIdentifier()
                    .map(|s| s.to_string())
                    .unwrap_or_default(),
                app_name: app
                    .localizedName()
                    .map(|s| s.to_string())
                    .unwrap_or_default(),
            })
        })
    }
}

impl FocusTracker for MacFocusTracker {
    fn snapshot(&self) -> Result<FocusSnapshot> {
        self.frontmost()?
            .ok_or_else(|| anyhow!("не удалось определить приложение с фокусом"))
    }

    fn is_same_app_focused(&self, snap: &FocusSnapshot) -> Result<bool> {
        Ok(self
            .frontmost()?
            .map(|cur| cur.pid == snap.pid)
            .unwrap_or(false))
    }
}
