//! Этап 2: вставка текста на Windows (SendInput: Ctrl+V и KEYEVENTF_UNICODE).

use crate::platform::TextInjector;
use anyhow::Result;

pub struct WindowsTextInjector;

impl TextInjector for WindowsTextInjector {
    fn send_paste_shortcut(&self) -> Result<()> {
        unimplemented!("windows: эмуляция Ctrl+V через SendInput")
    }

    fn type_text(&self, _text: &str) -> Result<()> {
        unimplemented!("windows: посимвольный ввод через SendInput KEYEVENTF_UNICODE")
    }

    fn secure_input_active(&self) -> bool {
        unimplemented!("windows: детект защищённого ввода")
    }
}
