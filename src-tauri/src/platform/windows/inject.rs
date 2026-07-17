//! Вставка текста на Windows: SendInput.
//!
//! Основной путь — эмуляция Ctrl+V. Перед ней отпускаются физически зажатые
//! модификаторы (пользователь мог ещё держать хоткей): SendInput шлёт события
//! в общий входной поток, и зажатый Alt превратил бы Ctrl+V в Ctrl+Alt+V.
//! Запасной путь — посимвольный ввод KEYEVENTF_UNICODE.

use crate::platform::TextInjector;
use anyhow::{bail, Result};
use std::thread::sleep;
use std::time::Duration;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS,
    KEYEVENTF_KEYUP, KEYEVENTF_UNICODE, VIRTUAL_KEY, VK_CONTROL, VK_LWIN, VK_MENU, VK_RWIN,
    VK_SHIFT,
};

/// Пауза между порциями посимвольного ввода — чтобы приложения успевали
/// обрабатывать поток событий (аналогично macOS-реализации).
const TYPE_CHUNK: usize = 20;

pub struct WindowsTextInjector;

fn key_input(vk: VIRTUAL_KEY, flags: KEYBD_EVENT_FLAGS) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                wScan: 0,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

fn unicode_input(unit: u16, flags: KEYBD_EVENT_FLAGS) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(0),
                wScan: unit,
                dwFlags: KEYEVENTF_UNICODE | flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

fn send(inputs: &[INPUT]) -> Result<()> {
    let sent = unsafe { SendInput(inputs, std::mem::size_of::<INPUT>() as i32) };
    if sent != inputs.len() as u32 {
        // Типичная причина — окно с более высокими правами (UIPI).
        bail!("система приняла {sent} из {} событий ввода", inputs.len());
    }
    Ok(())
}

/// Отпускает зажатые модификаторы, чтобы они не примешались к Ctrl+V.
/// Физическое отпускание клавиши позже пришлёт свой keyup — это безвредно.
fn release_held_modifiers() -> Result<()> {
    let mut ups: Vec<INPUT> = Vec::new();
    for vk in [VK_MENU, VK_SHIFT, VK_LWIN, VK_RWIN, VK_CONTROL] {
        let held = unsafe { GetAsyncKeyState(vk.0 as i32) } as u16 & 0x8000 != 0;
        if held {
            ups.push(key_input(vk, KEYEVENTF_KEYUP));
        }
    }
    if !ups.is_empty() {
        send(&ups)?;
        sleep(Duration::from_millis(10));
    }
    Ok(())
}

impl TextInjector for WindowsTextInjector {
    fn send_paste_shortcut(&self) -> Result<()> {
        release_held_modifiers()?;
        const VK_V: VIRTUAL_KEY = VIRTUAL_KEY(b'V' as u16);
        send(&[
            key_input(VK_CONTROL, KEYBD_EVENT_FLAGS(0)),
            key_input(VK_V, KEYBD_EVENT_FLAGS(0)),
        ])?;
        sleep(Duration::from_millis(10));
        send(&[
            key_input(VK_V, KEYEVENTF_KEYUP),
            key_input(VK_CONTROL, KEYEVENTF_KEYUP),
        ])
    }

    fn type_text(&self, text: &str) -> Result<()> {
        release_held_modifiers()?;
        let utf16: Vec<u16> = text.encode_utf16().collect();
        // Суррогатные пары шлются как два отдельных UNICODE-события —
        // штатный способ ввода символов вне BMP.
        for chunk in utf16.chunks(TYPE_CHUNK) {
            let mut inputs = Vec::with_capacity(chunk.len() * 2);
            for &unit in chunk {
                inputs.push(unicode_input(unit, KEYBD_EVENT_FLAGS(0)));
                inputs.push(unicode_input(unit, KEYEVENTF_KEYUP));
            }
            send(&inputs)?;
            sleep(Duration::from_millis(3));
        }
        Ok(())
    }

    fn secure_input_active(&self) -> bool {
        // Аналога macOS secure input на Windows нет: поля паролей не блокируют
        // синтетический ввод. Окна с повышенными правами отсекает UIPI —
        // это ловится как ошибка SendInput, а не отдельным состоянием.
        false
    }
}
