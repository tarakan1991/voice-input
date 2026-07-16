//! Этап 2: разрешения на Windows.
//!
//! Содержательных прав почти нет: проверка системного тумблера доступа к
//! микрофону; открытие ms-settings:privacy-microphone. Остальное — NotApplicable.

use crate::platform::{Permission, PermissionChecker, PermissionStatus};
use anyhow::Result;

pub struct WindowsPermissionChecker;

impl PermissionChecker for WindowsPermissionChecker {
    fn required(&self) -> Vec<Permission> {
        unimplemented!("windows: список требуемых прав (только микрофон)")
    }

    fn status(&self, _p: Permission) -> PermissionStatus {
        unimplemented!("windows: проверка тумблера доступа к микрофону")
    }

    fn request(&self, _p: Permission) -> Result<()> {
        unimplemented!("windows: запрос права")
    }

    fn open_settings(&self, _p: Permission) -> Result<()> {
        unimplemented!("windows: открытие ms-settings:privacy-microphone")
    }
}
