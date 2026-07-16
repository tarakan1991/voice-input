//! Хранение API-ключей облачных провайдеров: только системное хранилище
//! (Keychain на macOS, Credential Manager на Windows). В конфиг-файл и логи
//! ключи не попадают никогда.

use crate::config::CloudProvider;
use anyhow::{Context, Result};
use keyring::Entry;

const SERVICE: &str = "VoiceInput";

fn entry(provider: CloudProvider) -> Result<Entry> {
    Entry::new(SERVICE, provider.keyring_user()).context("доступ к системному хранилищу ключей")
}

pub fn set_api_key(provider: CloudProvider, key: &str) -> Result<()> {
    if key.trim().is_empty() {
        // Пустой ввод = удаление ключа
        return delete_api_key(provider);
    }
    entry(provider)?
        .set_password(key.trim())
        .context("сохранение ключа в системное хранилище")
}

pub fn get_api_key(provider: CloudProvider) -> Result<Option<String>> {
    match entry(provider)?.get_password() {
        Ok(key) => Ok(Some(key)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(e).context("чтение ключа из системного хранилища"),
    }
}

pub fn delete_api_key(provider: CloudProvider) -> Result<()> {
    match entry(provider)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(e).context("удаление ключа из системного хранилища"),
    }
}
