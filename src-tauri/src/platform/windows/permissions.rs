//! Разрешения на Windows.
//!
//! Содержательное право одно — доступ к микрофону. Для классических (win32)
//! приложений это не диалог, а тумблеры в Параметрах: глобальный и «для
//! классических приложений». Их состояние лежит в реестре
//! (CapabilityAccessManager\ConsentStore). Отдельного системного запроса нет —
//! request() просто открывает нужный раздел Параметров, как и open_settings().

use crate::platform::{Permission, PermissionChecker, PermissionStatus};
use anyhow::{Context, Result};
use windows::core::{w, PCWSTR};
use windows::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_SZ};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

const CONSENT_MICROPHONE: PCWSTR = w!(
    r"Software\Microsoft\Windows\CurrentVersion\CapabilityAccessManager\ConsentStore\microphone"
);
const CONSENT_MICROPHONE_NONPACKAGED: PCWSTR = w!(
    r"Software\Microsoft\Windows\CurrentVersion\CapabilityAccessManager\ConsentStore\microphone\NonPackaged"
);

/// Читает REG_SZ `Value` из HKCU. None — ключа/значения нет (система трактует
/// отсутствие как «разрешено»).
fn consent_value(subkey: PCWSTR) -> Option<String> {
    unsafe {
        let mut buf = [0u16; 16];
        let mut size = (buf.len() * 2) as u32;
        let status = RegGetValueW(
            HKEY_CURRENT_USER,
            subkey,
            w!("Value"),
            RRF_RT_REG_SZ,
            None,
            Some(buf.as_mut_ptr() as *mut _),
            Some(&mut size),
        );
        if status.is_err() {
            return None;
        }
        let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        Some(String::from_utf16_lossy(&buf[..len]))
    }
}

fn microphone_status() -> PermissionStatus {
    // Запрещает любой из двух тумблеров: глобальный или «для классических
    // приложений». Отсутствие ключей = разрешено.
    for subkey in [CONSENT_MICROPHONE, CONSENT_MICROPHONE_NONPACKAGED] {
        if let Some(value) = consent_value(subkey) {
            if value.eq_ignore_ascii_case("deny") {
                return PermissionStatus::Denied;
            }
        }
    }
    PermissionStatus::Granted
}

fn open_privacy_microphone() -> Result<()> {
    let result = unsafe {
        ShellExecuteW(
            None,
            w!("open"),
            w!("ms-settings:privacy-microphone"),
            None,
            None,
            SW_SHOWNORMAL,
        )
    };
    // ShellExecuteW возвращает «HINSTANCE» > 32 при успехе.
    if result.0 as isize <= 32 {
        anyhow::bail!("не удалось открыть Параметры → Конфиденциальность → Микрофон");
    }
    Ok(())
}

pub struct WindowsPermissionChecker;

impl PermissionChecker for WindowsPermissionChecker {
    fn required(&self) -> Vec<Permission> {
        // Accessibility/Input Monitoring — понятия macOS: SendInput и
        // глобальные хоткеи на Windows отдельных прав не требуют.
        vec![Permission::Microphone]
    }

    fn status(&self, p: Permission) -> PermissionStatus {
        match p {
            Permission::Microphone => microphone_status(),
            Permission::Accessibility | Permission::InputMonitoring => {
                PermissionStatus::NotApplicable
            }
        }
    }

    fn request(&self, p: Permission) -> Result<()> {
        // Системного диалога для win32-приложений нет — ведём пользователя
        // к тумблеру.
        self.open_settings(p)
    }

    fn open_settings(&self, p: Permission) -> Result<()> {
        match p {
            Permission::Microphone => open_privacy_microphone(),
            Permission::Accessibility | Permission::InputMonitoring => {
                open_privacy_microphone().context("раздел применим только к микрофону")
            }
        }
    }
}
