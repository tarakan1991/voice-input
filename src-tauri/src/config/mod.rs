//! Конфигурация приложения: JSON в каталоге конфигов пользователя.
//! Секреты (API-ключи) в файл не пишутся — только Keychain (см. postproc::cloud).

use crate::dictionary::Dictionary;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const CURRENT_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", tag = "kind", content = "id")]
pub enum MicSelection {
    /// «Всегда встроенный микрофон» — дефолт: BT-гарнитура никогда не
    /// переключается в HFP.
    AlwaysBuiltin,
    /// Системное устройство по умолчанию.
    SystemDefault,
    /// Конкретное устройство.
    Device(String),
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HotkeyMode {
    /// Нажал — говоришь — нажал (или тишина).
    Toggle,
    /// Говоришь, пока держишь комбинацию; отпустил — стоп (push-to-talk).
    Hold,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum InjectionMode {
    /// Через буфер обмена + Cmd+V (основной).
    Clipboard,
    /// Посимвольный ввод (запасной, для приложений, игнорирующих Cmd+V).
    Typing,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PostprocMode {
    Off,
    Local,
    Cloud,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum CloudProvider {
    Anthropic,
    Openai,
    Deepseek,
}

impl CloudProvider {
    pub fn default_model(&self) -> &'static str {
        match self {
            CloudProvider::Anthropic => "claude-haiku-4-5",
            CloudProvider::Openai => "gpt-4o-mini",
            CloudProvider::Deepseek => "deepseek-chat",
        }
    }

    pub fn keyring_user(&self) -> &'static str {
        match self {
            CloudProvider::Anthropic => "anthropic",
            CloudProvider::Openai => "openai",
            CloudProvider::Deepseek => "deepseek",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PostprocConfig {
    pub mode: PostprocMode,
    /// id модели из манифеста (для mode = Local).
    pub local_model: Option<String>,
    pub cloud_provider: CloudProvider,
    /// Модель облачного провайдера; пусто — дефолт провайдера.
    pub cloud_model: String,
    /// Таймаут вычитки; по истечении вставляется сырой текст.
    pub timeout_secs: u64,
}

impl Default for PostprocConfig {
    fn default() -> Self {
        Self {
            mode: PostprocMode::Off,
            local_model: None,
            cloud_provider: CloudProvider::Anthropic,
            cloud_model: String::new(),
            timeout_secs: 10,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub config_version: u32,
    pub wizard_completed: bool,
    /// Формат tauri-plugin-global-shortcut: "Ctrl+Alt+Space".
    pub hotkey: String,
    pub hotkey_mode: HotkeyMode,
    pub microphone: MicSelection,
    /// Язык распознавания зафиксирован («ru»), поле — задел на будущее.
    pub language: String,
    pub silence_timeout_secs: f32,
    pub max_recording_secs: u32,
    /// id ASR-модели из манифеста.
    pub asr_model: Option<String>,
    pub postproc: PostprocConfig,
    pub dictionary: Dictionary,
    pub history_enabled: bool,
    pub history_limit: u32,
    pub sounds_enabled: bool,
    pub injection_mode: InjectionMode,
    /// Пауза между Cmd+V и восстановлением буфера обмена.
    pub paste_restore_delay_ms: u64,
    /// Выгрузка моделей из памяти после простоя (0 — не выгружать).
    pub model_idle_unload_mins: u32,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            config_version: CURRENT_VERSION,
            wizard_completed: false,
            hotkey: "Ctrl+Alt+Space".into(),
            hotkey_mode: HotkeyMode::Toggle,
            microphone: MicSelection::AlwaysBuiltin,
            language: "ru".into(),
            silence_timeout_secs: 5.0,
            max_recording_secs: 300,
            asr_model: None,
            postproc: PostprocConfig::default(),
            dictionary: Dictionary::default(),
            history_enabled: true,
            history_limit: 500,
            sounds_enabled: true,
            injection_mode: InjectionMode::Clipboard,
            paste_restore_delay_ms: 300,
            model_idle_unload_mins: 15,
        }
    }
}

impl AppConfig {
    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("чтение конфига {}", path.display()))?;
        let mut config: AppConfig =
            serde_json::from_str(&text).context("конфиг повреждён — не разбирается JSON")?;
        config.migrate();
        Ok(config)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let text = serde_json::to_string_pretty(self)?;
        std::fs::write(path, text).with_context(|| format!("запись конфига {}", path.display()))?;
        Ok(())
    }

    /// Миграции старых версий конфига. Пока версия одна — заготовка.
    fn migrate(&mut self) {
        if self.config_version < CURRENT_VERSION {
            self.config_version = CURRENT_VERSION;
        }
        // Отрицательные/нулевые значения от руки в файле приводим к разумным.
        if self.silence_timeout_secs < 1.0 {
            self.silence_timeout_secs = 5.0;
        }
        if self.max_recording_secs < 10 {
            self.max_recording_secs = 300;
        }
    }
}

/// Путь к файлу конфига внутри каталога конфигурации приложения.
pub fn config_path(config_dir: &Path) -> PathBuf {
    config_dir.join("config.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_serializes_and_loads_back() {
        let dir = std::env::temp_dir().join("voice-input-test-config");
        let path = dir.join("config.json");
        let config = AppConfig::default();
        config.save(&path).unwrap();
        let loaded = AppConfig::load(&path).unwrap();
        assert_eq!(loaded.hotkey, "Ctrl+Alt+Space");
        assert_eq!(loaded.microphone, MicSelection::AlwaysBuiltin);
        assert!(!loaded.wizard_completed);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn missing_file_gives_defaults() {
        let loaded = AppConfig::load(Path::new("/nonexistent/config.json")).unwrap();
        assert_eq!(loaded.config_version, CURRENT_VERSION);
    }

    #[test]
    fn migrate_fixes_bad_values() {
        let mut config = AppConfig {
            silence_timeout_secs: 0.0,
            max_recording_secs: 1,
            ..Default::default()
        };
        config.migrate();
        assert_eq!(config.silence_timeout_secs, 5.0);
        assert_eq!(config.max_recording_secs, 300);
    }
}
