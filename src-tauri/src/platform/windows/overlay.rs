//! Неактивирующийся оверлей на Windows (риск R-1 SPEC.md).
//!
//! Оборона фокуса:
//! 1. `set_focusable(false)` — tao держит WS_EX_NOACTIVATE в собственной
//!    модели флагов, поэтому бит переживает пересчёты стиля изнутри tao.
//! 2. Ручные биты WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW на HWND хоста
//!    (TOOLWINDOW прячет окно из Alt+Tab; tao этот бит не знает и может
//!    затереть при пересчёте — переприменяем после каждого вмешательства).
//! 3. Сабклассинг HWND хоста: WM_MOUSEACTIVATE → MA_NOACTIVATE.
//!
//! Клики по кнопке ✕: дочерние HWND WebView2 принадлежат процессам
//! msedgewebview2.exe — сабклассинг из нашего процесса на них не работает,
//! а сам WebView2 в неактивирующемся окне не доводит клик до DOM (проверено
//! живыми прогонами). Поэтому на время видимости оверлея ставится
//! низкоуровневый мышиный хук (WH_MOUSE_LL): отпускание левой кнопки над
//! некликнасквозь-окном оверлея трактуется как нажатие ✕ и уходит общему
//! коду событием OVERLAY_NATIVE_CLICK_EVENT. Плашка помечена
//! WS_EX_TRANSPARENT, WindowFromPoint её пропускает — клики «сквозь» неё
//! хук не трогает.
//!
//! Показ — только SetWindowPos(SWP_NOACTIVATE): обычный show() может
//! активировать окно.

use crate::platform::{OverlayWindow, OVERLAY_NATIVE_CLICK_EVENT};
use anyhow::{Context, Result};
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::OnceLock;
use tauri::{Emitter, Manager};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GetAncestor, GetWindowLongPtrW, GetWindowThreadProcessId, SetWindowLongPtrW,
    SetWindowPos, SetWindowsHookExW, ShowWindow, UnhookWindowsHookEx, WindowFromPoint, GA_ROOT,
    GWL_EXSTYLE, HHOOK, HWND_TOPMOST, MA_NOACTIVATE, MSLLHOOKSTRUCT, SWP_NOACTIVATE, SWP_NOMOVE,
    SWP_NOSIZE, SWP_SHOWWINDOW, SW_HIDE, WH_MOUSE_LL, WM_LBUTTONUP, WM_MOUSEACTIVATE,
    WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
};

/// Идентификатор сабкласса; повторный SetWindowSubclass с тем же id и той же
/// процедурой лишь обновляет данные — двойной установки не происходит.
const SUBCLASS_ID: usize = 0x564F_4943; // "VOIC"

pub struct WindowsOverlayWindow;

/// Окна оверлея по корневому HWND — для мышиного хука. Плашка click-through
/// (WS_EX_TRANSPARENT) и в WindowFromPoint не попадает, так что фактически
/// событие клика рождает только окно кнопки ✕.
fn click_targets() -> &'static Mutex<HashMap<isize, tauri::WebviewWindow>> {
    static TARGETS: OnceLock<Mutex<HashMap<isize, tauri::WebviewWindow>>> = OnceLock::new();
    TARGETS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Хэндл активного WH_MOUSE_LL (0 — хука нет). Хук живёт только пока
/// оверлей на экране: постоянный глобальный хук добавлял бы задержку каждому
/// клику в системе.
fn hook_handle() -> &'static Mutex<isize> {
    static HOOK: OnceLock<Mutex<isize>> = OnceLock::new();
    HOOK.get_or_init(|| Mutex::new(0))
}

fn hwnd_of(win: &tauri::WebviewWindow) -> Result<isize> {
    Ok(win.hwnd().context("окно не отдаёт HWND")?.0 as isize)
}

/// WM_MOUSEACTIVATE → MA_NOACTIVATE: хост принимает клик, но не активируется.
unsafe extern "system" fn no_activate_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _subclass_id: usize,
    _ref_data: usize,
) -> LRESULT {
    if msg == WM_MOUSEACTIVATE {
        return LRESULT(MA_NOACTIVATE as isize);
    }
    unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
}

/// Мышиный хук: отпускание левой кнопки над окном оверлея → событие клика.
/// Дочернее окно под курсором принадлежит процессу WebView2, но GA_ROOT
/// приводит к нашему top-level — по нему и ищем.
unsafe extern "system" fn mouse_hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 && wparam.0 as u32 == WM_LBUTTONUP {
        let info = unsafe { &*(lparam.0 as *const MSLLHOOKSTRUCT) };
        let under = unsafe { WindowFromPoint(info.pt) };
        if !under.is_invalid() {
            let root = unsafe { GetAncestor(under, GA_ROOT) };
            let target = click_targets().lock().get(&(root.0 as isize)).cloned();
            if let Some(win) = target {
                log::debug!("overlay: клик по ✕ пойман мышиным хуком");
                // Эмит вне хука: обработчик обязан отработать мгновенно.
                std::thread::spawn(move || {
                    if let Err(e) = win.emit(OVERLAY_NATIVE_CLICK_EVENT, ()) {
                        log::warn!("не удалось послать {OVERLAY_NATIVE_CLICK_EVENT}: {e}");
                    }
                });
            }
        }
    }
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

fn install_mouse_hook() {
    let mut guard = hook_handle().lock();
    if *guard != 0 {
        return;
    }
    match unsafe { SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_hook_proc), None, 0) } {
        Ok(hook) => *guard = hook.0 as isize,
        Err(e) => log::warn!("не удалось поставить мышиный хук оверлея: {e}"),
    }
}

fn remove_mouse_hook() {
    let mut guard = hook_handle().lock();
    if *guard != 0 {
        unsafe {
            let _ = UnhookWindowsHookEx(HHOOK(*guard as _));
        }
        *guard = 0;
    }
}

/// Есть ли ещё видимые окна оверлея (кроме, возможно, только что скрытого).
fn any_target_visible() -> bool {
    click_targets()
        .lock()
        .values()
        .any(|w| w.is_visible().unwrap_or(false))
}

/// Сабклассинг работает только из потока-владельца окна: во время `setup`
/// мы уже на нём (выполняется инлайн), из потока сессии — переносим на
/// главный поток без ожидания.
fn on_owner_thread(win: &tauri::WebviewWindow, f: impl FnOnce() + Send + 'static) -> Result<()> {
    let hwnd_raw = hwnd_of(win)?;
    let owner = unsafe { GetWindowThreadProcessId(HWND(hwnd_raw as _), None) };
    if owner == unsafe { GetCurrentThreadId() } {
        f();
    } else {
        win.app_handle()
            .run_on_main_thread(f)
            .context("не удалось выполнить на главном потоке")?;
    }
    Ok(())
}

/// Ручные ex-стили поверх выставленных tao. tao при пересчёте перезаписывает
/// GWL_EXSTYLE целиком, поэтому биты добавляются заново после каждого вызова
/// tauri-API, меняющего стили.
fn apply_no_activate_styles(win: &tauri::WebviewWindow) -> Result<()> {
    let hwnd = HWND(hwnd_of(win)? as _);
    unsafe {
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        let wanted = ex | WS_EX_NOACTIVATE.0 as isize | WS_EX_TOOLWINDOW.0 as isize;
        if wanted != ex {
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, wanted);
        }
    }
    Ok(())
}

impl OverlayWindow for WindowsOverlayWindow {
    fn make_non_activating(&self, win: &tauri::WebviewWindow) -> Result<()> {
        // Штатный путь: tao хранит «нефокусируемость» у себя и сам держит
        // WS_EX_NOACTIVATE при любых пересчётах стилей.
        win.set_focusable(false)
            .context("не удалось сделать окно нефокусируемым")?;
        apply_no_activate_styles(win)?;
        click_targets().lock().insert(hwnd_of(win)?, win.clone());
        let hwnd_raw = hwnd_of(win)?;
        on_owner_thread(win, move || unsafe {
            let _ = SetWindowSubclass(HWND(hwnd_raw as _), Some(no_activate_proc), SUBCLASS_ID, 0);
        })
    }

    fn set_click_through(&self, win: &tauri::WebviewWindow, on: bool) -> Result<()> {
        // WS_EX_TRANSPARENT | WS_EX_LAYERED — через tao, чтобы флаг жил в его
        // модели; после пересчёта возвращаем свои биты. Для мышиного хука
        // click-through-окно исключается из целей: WindowFromPoint его
        // пропускает сам.
        win.set_ignore_cursor_events(on)
            .context("не удалось переключить click-through")?;
        apply_no_activate_styles(win)
    }

    fn show(&self, win: &tauri::WebviewWindow) -> Result<()> {
        apply_no_activate_styles(win)?;
        let hwnd = HWND(hwnd_of(win)? as _);
        unsafe {
            SetWindowPos(
                hwnd,
                Some(HWND_TOPMOST),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
            )
            .context("не удалось показать оверлей без активации")?;
        }
        // Хук ловит клики по ✕, пока оверлей на экране. Ставится с главного
        // потока: колбэк LL-хука приходит в поток установки, а он обязан
        // качать сообщения.
        on_owner_thread(win, install_mouse_hook)
    }

    fn hide(&self, win: &tauri::WebviewWindow) -> Result<()> {
        let hwnd = HWND(hwnd_of(win)? as _);
        unsafe {
            let _ = ShowWindow(hwnd, SW_HIDE);
        }
        if !any_target_visible() {
            on_owner_thread(win, remove_mouse_hook)?;
        }
        Ok(())
    }
}
