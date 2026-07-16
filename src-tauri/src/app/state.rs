//! Общее состояние приложения (tauri State) и потокобезопасный конфиг-стор.

use crate::asr::AsrEngine;
use crate::config::AppConfig;
use crate::history::History;
use crate::models::ModelStore;
use crate::platform::PlatformServices;
use crate::postproc::local::LocalLlm;
use anyhow::Result;
use parking_lot::RwLock;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

pub struct ConfigStore {
    path: PathBuf,
    inner: RwLock<AppConfig>,
}

impl ConfigStore {
    pub fn load(path: PathBuf) -> Result<Self> {
        let config = AppConfig::load(&path)?;
        Ok(Self {
            path,
            inner: RwLock::new(config),
        })
    }

    pub fn get(&self) -> AppConfig {
        self.inner.read().clone()
    }

    /// Заменяет конфиг и сохраняет на диск.
    pub fn set(&self, config: AppConfig) -> Result<()> {
        config.save(&self.path)?;
        *self.inner.write() = config;
        Ok(())
    }

    /// Точечное изменение с сохранением.
    pub fn update(&self, f: impl FnOnce(&mut AppConfig)) -> Result<()> {
        let mut config = self.get();
        f(&mut config);
        self.set(config)
    }
}

pub struct AppState {
    pub services: PlatformServices,
    pub config: Arc<ConfigStore>,
    pub history: Arc<History>,
    pub store: Arc<ModelStore>,
    pub asr: Arc<AsrEngine>,
    pub llm: Arc<LocalLlm>,
    pub session: crate::app::session::SessionHandle,
    /// Пауза из трея: хоткей снят, диктовка не запускается.
    pub paused: Arc<AtomicBool>,
}
