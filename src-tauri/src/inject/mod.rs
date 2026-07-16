//! Оркестрация вставки текста (общий код).
//!
//! Основной путь — буфер обмена: сохранить содержимое (текст или картинку),
//! положить свой текст, эмулировать Cmd+V/Ctrl+V, восстановить буфер.
//! Запасной — посимвольный ввод. Инвариант №3: буфер пользователя
//! восстанавливается; №4: текст не теряется молча.

use crate::config::{AppConfig, InjectionMode};
use crate::platform::{FocusSnapshot, PlatformServices};
use anyhow::{Context, Result};
use arboard::Clipboard;
use serde::Serialize;
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InjectionOutcome {
    /// Текст вставлен в целевое приложение.
    Injected,
    /// Фокус ушёл в другое приложение — текст оставлен в буфере обмена.
    LeftInClipboard,
    /// Активно поле защищённого ввода (пароль) — вставка невозможна,
    /// текст оставлен в буфере обмена.
    BlockedSecureInput,
}

/// Снимок буфера обмена перед вставкой.
enum ClipboardBackup {
    Text(String),
    Image(arboard::ImageData<'static>),
    Empty,
}

fn backup_clipboard(clipboard: &mut Clipboard) -> ClipboardBackup {
    if let Ok(text) = clipboard.get_text() {
        return ClipboardBackup::Text(text);
    }
    if let Ok(image) = clipboard.get_image() {
        return ClipboardBackup::Image(arboard::ImageData {
            width: image.width,
            height: image.height,
            bytes: std::borrow::Cow::Owned(image.bytes.into_owned()),
        });
    }
    ClipboardBackup::Empty
}

fn restore_clipboard(clipboard: &mut Clipboard, backup: ClipboardBackup) {
    let result = match backup {
        ClipboardBackup::Text(text) => clipboard.set_text(text),
        ClipboardBackup::Image(image) => clipboard.set_image(image),
        ClipboardBackup::Empty => clipboard.clear(),
    };
    if let Err(e) = result {
        log::warn!("не удалось восстановить буфер обмена: {e}");
    }
}

/// Вставляет текст в приложение из снимка фокуса.
pub fn inject(
    services: &PlatformServices,
    config: &AppConfig,
    text: &str,
    snapshot: &FocusSnapshot,
) -> Result<InjectionOutcome> {
    // Решение SPEC.md §3: фокус ушёл в другое приложение — не вставляем
    // вслепую, оставляем текст в буфере и сообщаем.
    let same_app = services
        .focus
        .is_same_app_focused(snapshot)
        .unwrap_or(false);
    if !same_app {
        let mut clipboard = Clipboard::new().context("буфер обмена недоступен")?;
        clipboard
            .set_text(text.to_string())
            .context("не удалось положить текст в буфер обмена")?;
        return Ok(InjectionOutcome::LeftInClipboard);
    }

    if services.injector.secure_input_active() {
        let mut clipboard = Clipboard::new().context("буфер обмена недоступен")?;
        clipboard.set_text(text.to_string()).ok();
        return Ok(InjectionOutcome::BlockedSecureInput);
    }

    match config.injection_mode {
        InjectionMode::Clipboard => {
            let mut clipboard = Clipboard::new().context("буфер обмена недоступен")?;
            let backup = backup_clipboard(&mut clipboard);
            clipboard
                .set_text(text.to_string())
                .context("не удалось положить текст в буфер обмена")?;
            // Небольшая пауза: буфер должен устаканиться до Cmd+V.
            std::thread::sleep(Duration::from_millis(50));
            let paste_result = services.injector.send_paste_shortcut();
            // Приложению нужно время обработать Cmd+V до восстановления буфера.
            std::thread::sleep(Duration::from_millis(config.paste_restore_delay_ms));
            restore_clipboard(&mut clipboard, backup);
            paste_result.context("не удалось послать Cmd+V")?;
        }
        InjectionMode::Typing => {
            services
                .injector
                .type_text(text)
                .context("посимвольный ввод не удался")?;
        }
    }
    Ok(InjectionOutcome::Injected)
}
