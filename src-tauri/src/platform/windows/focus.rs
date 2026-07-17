//! Отслеживание приложения с фокусом: GetForegroundWindow +
//! GetWindowThreadProcessId. Оверлей неактивирующийся, поэтому окно
//! переднего плана всё время диктовки остаётся у целевого приложения.

use crate::platform::{FocusSnapshot, FocusTracker};
use anyhow::{bail, Context, Result};
use windows::Win32::Foundation::CloseHandle;
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

pub struct WindowsFocusTracker;

/// Полный путь exe процесса; пустая строка, если процесс недоступен
/// (например, системный с более высокими правами).
fn process_image_path(pid: u32) -> String {
    unsafe {
        let Ok(handle) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else {
            return String::new();
        };
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let result = QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_WIN32,
            windows::core::PWSTR(buf.as_mut_ptr()),
            &mut len,
        );
        let _ = CloseHandle(handle);
        if result.is_err() {
            return String::new();
        }
        String::from_utf16_lossy(&buf[..len as usize])
    }
}

fn foreground() -> Result<FocusSnapshot> {
    let hwnd = unsafe { GetForegroundWindow() };
    if hwnd.is_invalid() {
        bail!("не удалось определить окно с фокусом");
    }
    let mut pid: u32 = 0;
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    if pid == 0 {
        bail!("окно с фокусом не отдаёт процесс");
    }
    let path = process_image_path(pid);
    let app_name = std::path::Path::new(&path)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    Ok(FocusSnapshot {
        pid: i32::try_from(pid).context("pid вне диапазона i32")?,
        app_id: path,
        app_name,
    })
}

impl FocusTracker for WindowsFocusTracker {
    fn snapshot(&self) -> Result<FocusSnapshot> {
        foreground()
    }

    fn is_same_app_focused(&self, snap: &FocusSnapshot) -> Result<bool> {
        // Как и на macOS, сравниваем pid: тот же процесс = то же приложение.
        Ok(foreground().map(|cur| cur.pid == snap.pid).unwrap_or(false))
    }
}
