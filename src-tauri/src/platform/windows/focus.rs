//! Этап 2: отслеживание фокуса на Windows (GetForegroundWindow + GetWindowThreadProcessId).

use crate::platform::{FocusSnapshot, FocusTracker};
use anyhow::Result;

pub struct WindowsFocusTracker;

impl FocusTracker for WindowsFocusTracker {
    fn snapshot(&self) -> Result<FocusSnapshot> {
        unimplemented!("windows: снимок приложения с фокусом")
    }

    fn is_same_app_focused(&self, _snap: &FocusSnapshot) -> Result<bool> {
        unimplemented!("windows: проверка, то же ли приложение в фокусе")
    }
}
