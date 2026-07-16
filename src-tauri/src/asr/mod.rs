//! Распознавание речи: whisper.cpp через whisper-rs (Metal на Apple Silicon).
//!
//! Модель грузится лениво при первой диктовке и остаётся в памяти для тёплого
//! старта; выгружается по простою (см. `unload_if_idle`).

use anyhow::{bail, Context, Result};
use parking_lot::Mutex;
use std::path::Path;
use std::time::{Duration, Instant};
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

/// Известные артефакты Whisper на тишине/шуме в русских моделях.
const HALLUCINATION_BLACKLIST: &[&str] = &[
    "субтитры",
    "продолжение следует",
    "редактор субтитров",
    "dimatorzok",
    "dima torzok",
    "корректор а.егорова",
];

/// Сегменты с no_speech_prob выше порога отбрасываются.
const NO_SPEECH_PROB_CUTOFF: f32 = 0.75;

pub struct AsrOutcome {
    pub text: String,
    pub elapsed_ms: u64,
}

struct Loaded {
    model_id: String,
    ctx: WhisperContext,
}

pub struct AsrEngine {
    loaded: Mutex<Option<Loaded>>,
    last_used: Mutex<Instant>,
}

impl AsrEngine {
    pub fn new() -> Self {
        Self {
            loaded: Mutex::new(None),
            last_used: Mutex::new(Instant::now()),
        }
    }

    fn ensure_loaded(&self, model_path: &Path, model_id: &str) -> Result<()> {
        let mut guard = self.loaded.lock();
        if guard.as_ref().map(|l| l.model_id.as_str()) == Some(model_id) {
            return Ok(());
        }
        *guard = None; // выгрузить старую до загрузки новой (память)
        log::info!("загрузка ASR-модели {model_id}");
        let path_str = model_path
            .to_str()
            .context("путь к модели содержит невалидный UTF-8")?;
        let ctx = WhisperContext::new_with_params(path_str, WhisperContextParameters::default())
            .with_context(|| format!("не удалось загрузить модель {model_id}"))?;
        *guard = Some(Loaded {
            model_id: model_id.to_string(),
            ctx,
        });
        Ok(())
    }

    /// Распознаёт моно-аудио 16 кГц. `language` — код языка («ru»),
    /// `initial_prompt` — подсказка со словарём англицизмов.
    pub fn transcribe(
        &self,
        model_path: &Path,
        model_id: &str,
        samples_16k: &[f32],
        language: &str,
        initial_prompt: &str,
    ) -> Result<AsrOutcome> {
        if samples_16k.is_empty() {
            bail!("пустая запись");
        }
        self.ensure_loaded(model_path, model_id)?;
        *self.last_used.lock() = Instant::now();

        let started = Instant::now();
        let guard = self.loaded.lock();
        let loaded = guard.as_ref().expect("модель загружена выше");
        let mut state = loaded
            .ctx
            .create_state()
            .context("создание состояния Whisper")?;

        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_language(Some(language));
        params.set_translate(false);
        params.set_temperature(0.0);
        params.set_suppress_blank(true);
        params.set_no_speech_thold(0.6);
        params.set_initial_prompt(initial_prompt);
        params.set_print_special(false);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);
        let threads = std::thread::available_parallelism()
            .map(|n| n.get().min(8))
            .unwrap_or(4);
        params.set_n_threads(threads as i32);

        state.full(params, samples_16k).context("распознавание")?;

        let mut pieces: Vec<String> = Vec::new();
        for i in 0..state.full_n_segments() {
            let Some(segment) = state.get_segment(i) else {
                continue;
            };
            if segment.no_speech_probability() > NO_SPEECH_PROB_CUTOFF {
                continue;
            }
            let Ok(text) = segment.to_str_lossy() else {
                continue;
            };
            let trimmed = text.trim();
            if trimmed.is_empty() || is_hallucination(trimmed) {
                continue;
            }
            pieces.push(trimmed.to_string());
        }

        *self.last_used.lock() = Instant::now();
        Ok(AsrOutcome {
            text: pieces.join(" ").trim().to_string(),
            elapsed_ms: started.elapsed().as_millis() as u64,
        })
    }

    pub fn unload(&self) {
        let mut guard = self.loaded.lock();
        if guard.take().is_some() {
            log::info!("ASR-модель выгружена");
        }
    }

    pub fn unload_if_idle(&self, idle: Duration) {
        if self.last_used.lock().elapsed() >= idle {
            self.unload();
        }
    }
}

/// Отсев известных галлюцинаций Whisper (артефакты обучающих субтитров).
pub fn is_hallucination(text: &str) -> bool {
    let lower = text.to_lowercase();
    HALLUCINATION_BLACKLIST.iter().any(|h| lower.contains(h))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hallucination_filter_catches_known_artifacts() {
        assert!(is_hallucination("Субтитры сделал DimaTorzok"));
        assert!(is_hallucination("ПРОДОЛЖЕНИЕ СЛЕДУЕТ..."));
        assert!(!is_hallucination("Задеплой изменения на стейджинг"));
    }
}
