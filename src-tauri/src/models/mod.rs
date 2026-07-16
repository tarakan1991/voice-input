//! Хранилище моделей: скачивание с прогрессом и докачкой, проверка SHA256,
//! тестовый прогон на вшитом сэмпле. Модели НЕ вшиты в установщик.

pub mod manifest;

pub use manifest::{find, ModelInfo, ModelKind, MODELS};

use anyhow::{bail, Context, Result};
use parking_lot::Mutex;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Вшитый тестовый сэмпл: «Задеплой изменения на стейджинг и создай
/// пул реквест» (16 кГц, моно, s16). Используется мастером и смоук-тестами.
pub const TEST_SAMPLE_WAV: &[u8] = include_bytes!("../../resources/test-sample.wav");

#[derive(Debug, Clone, Serialize)]
pub struct ModelStatus {
    #[serde(flatten)]
    pub info: &'static ModelInfo,
    pub downloaded: bool,
    /// Есть недокачанный .part-файл.
    pub partial: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DownloadOutcome {
    Done,
    Cancelled,
}

pub struct ModelStore {
    dir: PathBuf,
    cancel_flags: Mutex<HashMap<String, Arc<AtomicBool>>>,
}

impl ModelStore {
    pub fn new(dir: PathBuf) -> Self {
        Self {
            dir,
            cancel_flags: Mutex::new(HashMap::new()),
        }
    }

    pub fn path_for(&self, info: &ModelInfo) -> PathBuf {
        self.dir.join(info.file_name)
    }

    fn part_path(&self, info: &ModelInfo) -> PathBuf {
        self.dir.join(format!("{}.part", info.file_name))
    }

    pub fn is_downloaded(&self, info: &ModelInfo) -> bool {
        self.path_for(info).exists()
    }

    pub fn statuses(&self) -> Vec<ModelStatus> {
        MODELS
            .iter()
            .map(|info| ModelStatus {
                info,
                downloaded: self.is_downloaded(info),
                partial: self.part_path(info).exists(),
            })
            .collect()
    }

    /// Путь к скачанной модели по id (ошибка с понятным текстом, если нет).
    pub fn downloaded_path(&self, id: &str) -> Result<PathBuf> {
        let info = find(id).with_context(|| format!("неизвестная модель «{id}»"))?;
        let path = self.path_for(info);
        if !path.exists() {
            bail!(
                "модель «{}» не скачана — откройте Настройки → Модели",
                info.title
            );
        }
        Ok(path)
    }

    pub fn delete(&self, id: &str) -> Result<()> {
        let info = find(id).with_context(|| format!("неизвестная модель «{id}»"))?;
        for path in [self.path_for(info), self.part_path(info)] {
            if path.exists() {
                std::fs::remove_file(&path)
                    .with_context(|| format!("удаление {}", path.display()))?;
            }
        }
        Ok(())
    }

    pub fn cancel_download(&self, id: &str) {
        if let Some(flag) = self.cancel_flags.lock().get(id) {
            flag.store(true, Ordering::Relaxed);
        }
    }

    /// Блокирующее скачивание с докачкой (HTTP Range) и проверкой SHA256.
    /// `progress(скачано, всего)` вызывается по ходу; вызывать из фонового потока.
    pub fn download(
        &self,
        id: &str,
        mut progress: impl FnMut(u64, u64),
    ) -> Result<DownloadOutcome> {
        let info = find(id).with_context(|| format!("неизвестная модель «{id}»"))?;
        std::fs::create_dir_all(&self.dir)?;
        let final_path = self.path_for(info);
        if final_path.exists() {
            return Ok(DownloadOutcome::Done);
        }

        let cancel = Arc::new(AtomicBool::new(false));
        self.cancel_flags
            .lock()
            .insert(id.to_string(), cancel.clone());
        let result = self.download_inner(info, &cancel, &mut progress);
        self.cancel_flags.lock().remove(id);
        result
    }

    fn download_inner(
        &self,
        info: &ModelInfo,
        cancel: &AtomicBool,
        progress: &mut impl FnMut(u64, u64),
    ) -> Result<DownloadOutcome> {
        let part_path = self.part_path(info);
        let mut downloaded: u64 = part_path.metadata().map(|m| m.len()).unwrap_or(0);

        // Битый или переросший .part не докачиваем — начинаем заново.
        if downloaded >= info.size_bytes {
            std::fs::remove_file(&part_path).ok();
            downloaded = 0;
        }

        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(None)
            .build()
            .into();
        let mut request = agent.get(info.url);
        if downloaded > 0 {
            request = request.header("range", &format!("bytes={downloaded}-"));
        }
        let mut response = request
            .call()
            .with_context(|| format!("не удалось начать скачивание «{}»", info.title))?;

        // Сервер мог проигнорировать Range и отдать файл целиком.
        if downloaded > 0 && response.status() != 206 {
            downloaded = 0;
        }

        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(downloaded > 0)
            .write(true)
            .truncate(downloaded == 0)
            .open(&part_path)
            .with_context(|| format!("создание файла {}", part_path.display()))?;

        let mut reader = response.body_mut().as_reader();
        let mut buf = vec![0u8; 256 * 1024];
        progress(downloaded, info.size_bytes);
        loop {
            if cancel.load(Ordering::Relaxed) {
                // .part остаётся на диске — докачаем в следующий раз.
                return Ok(DownloadOutcome::Cancelled);
            }
            let n = reader.read(&mut buf).context("обрыв скачивания")?;
            if n == 0 {
                break;
            }
            file.write_all(&buf[..n]).context("запись на диск")?;
            downloaded += n as u64;
            progress(downloaded, info.size_bytes);
        }
        file.flush()?;
        drop(file);

        // Проверка SHA256 всего файла (докачка не позволяет считать на лету).
        progress(info.size_bytes, info.size_bytes);
        let actual = sha256_of_file(&part_path)?;
        if !actual.eq_ignore_ascii_case(info.sha256) {
            std::fs::remove_file(&part_path).ok();
            bail!(
                "файл «{}» скачался битым (SHA256 не совпал) — попробуйте ещё раз",
                info.title
            );
        }
        std::fs::rename(&part_path, self.path_for(info)).context("перенос скачанного файла")?;
        Ok(DownloadOutcome::Done)
    }
}

pub fn sha256_of_file(path: &Path) -> Result<String> {
    let mut file =
        std::fs::File::open(path).with_context(|| format!("открытие {}", path.display()))?;
    let mut hasher = Sha256::new();
    std::io::copy(&mut file, &mut hasher)?;
    Ok(format!("{:x}", hasher.finalize()))
}

/// Декодирует вшитый тестовый сэмпл в моно f32 16 кГц.
pub fn test_sample_pcm() -> Result<Vec<f32>> {
    let reader = hound::WavReader::new(std::io::Cursor::new(TEST_SAMPLE_WAV))
        .context("вшитый сэмпл не читается")?;
    let spec = reader.spec();
    if spec.sample_rate != 16_000 || spec.channels != 1 {
        bail!("вшитый сэмпл должен быть 16 кГц моно");
    }
    Ok(reader
        .into_samples::<i16>()
        .filter_map(|s| s.ok())
        .map(|s| s as f32 / i16::MAX as f32)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sample_decodes() {
        let pcm = test_sample_pcm().unwrap();
        // ~3 секунды речи
        assert!(pcm.len() > 2 * 16_000, "сэмпл короче 2 секунд");
        // не тишина
        let rms = crate::audio::rms(&pcm);
        assert!(rms > 0.01, "сэмпл подозрительно тихий: rms={rms}");
    }

    #[test]
    fn sha256_of_known_content() {
        let dir = std::env::temp_dir().join("voice-input-test-sha");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("x.bin");
        std::fs::write(&path, b"abc").unwrap();
        assert_eq!(
            sha256_of_file(&path).unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn store_paths_and_statuses() {
        let store = ModelStore::new(std::env::temp_dir().join("voice-input-test-models"));
        let statuses = store.statuses();
        assert_eq!(statuses.len(), MODELS.len());
        assert!(statuses.iter().all(|s| !s.downloaded));
        assert!(store.downloaded_path("whisper-large-v3-turbo").is_err());
        assert!(store.downloaded_path("нет-такой").is_err());
    }
}
