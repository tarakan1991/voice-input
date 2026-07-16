//! Вставка текста: эмуляция Cmd+V и посимвольный ввод через CGEvent.
//! Требует право Accessibility (проверяется PermissionChecker'ом).

use crate::platform::TextInjector;
use anyhow::{anyhow, Result};
use core_graphics::event::{CGEvent, CGEventFlags, CGEventTapLocation};
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
use std::thread::sleep;
use std::time::Duration;

// kVK_ANSI_V — физическая клавиша V, не зависит от раскладки.
const KEY_V: u16 = 9;
// Максимум UTF-16-юнитов на одно событие CGEventKeyboardSetUnicodeString.
const TYPE_CHUNK: usize = 20;

#[link(name = "Carbon", kind = "framework")]
extern "C" {
    fn IsSecureEventInputEnabled() -> u8;
}

pub struct MacTextInjector;

fn event_source() -> Result<CGEventSource> {
    CGEventSource::new(CGEventSourceStateID::CombinedSessionState)
        .map_err(|_| anyhow!("не удалось создать CGEventSource"))
}

impl TextInjector for MacTextInjector {
    fn send_paste_shortcut(&self) -> Result<()> {
        let source = event_source()?;
        let down = CGEvent::new_keyboard_event(source.clone(), KEY_V, true)
            .map_err(|_| anyhow!("не удалось создать событие клавиатуры"))?;
        down.set_flags(CGEventFlags::CGEventFlagCommand);
        down.post(CGEventTapLocation::HID);
        sleep(Duration::from_millis(10));
        let up = CGEvent::new_keyboard_event(source, KEY_V, false)
            .map_err(|_| anyhow!("не удалось создать событие клавиатуры"))?;
        up.set_flags(CGEventFlags::CGEventFlagCommand);
        up.post(CGEventTapLocation::HID);
        Ok(())
    }

    fn type_text(&self, text: &str) -> Result<()> {
        let source = event_source()?;
        let utf16: Vec<u16> = text.encode_utf16().collect();
        for chunk in utf16.chunks(TYPE_CHUNK) {
            let down = CGEvent::new_keyboard_event(source.clone(), 0, true)
                .map_err(|_| anyhow!("не удалось создать событие клавиатуры"))?;
            down.set_string_from_utf16_unchecked(chunk);
            down.post(CGEventTapLocation::HID);
            let up = CGEvent::new_keyboard_event(source.clone(), 0, false)
                .map_err(|_| anyhow!("не удалось создать событие клавиатуры"))?;
            up.post(CGEventTapLocation::HID);
            // Небольшая пауза, чтобы приложения успевали обрабатывать поток событий.
            sleep(Duration::from_millis(3));
        }
        Ok(())
    }

    fn secure_input_active(&self) -> bool {
        unsafe { IsSecureEventInputEnabled() != 0 }
    }
}
