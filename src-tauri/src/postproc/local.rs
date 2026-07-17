//! Локальная вычитка: llama.cpp через llama-cpp-2 (Metal на Apple Silicon).
//!
//! Выбор llama.cpp вместо Ollama обоснован в SPEC.md §7.3: никаких внешних
//! установок и фоновых демонов, полный контроль жизненного цикла модели.

use anyhow::{bail, Context, Result};
use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::{AddBos, LlamaModel};
use llama_cpp_2::sampling::LlamaSampler;
use parking_lot::Mutex;
use std::num::NonZeroU32;
use std::path::Path;
use std::time::{Duration, Instant};

const N_CTX: u32 = 4096;

struct Loaded {
    model_id: String,
    model: LlamaModel,
}

pub struct LocalLlm {
    backend: LlamaBackend,
    loaded: Mutex<Option<Loaded>>,
    last_used: Mutex<Instant>,
}

impl LocalLlm {
    pub fn new() -> Result<Self> {
        let backend = LlamaBackend::init().context("инициализация llama.cpp")?;
        Ok(Self {
            backend,
            loaded: Mutex::new(None),
            last_used: Mutex::new(Instant::now()),
        })
    }

    fn ensure_loaded(&self, model_path: &Path, model_id: &str) -> Result<()> {
        let mut guard = self.loaded.lock();
        if guard.as_ref().map(|l| l.model_id.as_str()) == Some(model_id) {
            return Ok(());
        }
        *guard = None;
        log::info!("загрузка LLM-модели {model_id}");
        // Все слои на GPU (Metal); на машинах без Metal llama.cpp сам
        // откатится на CPU.
        let params = LlamaModelParams::default().with_n_gpu_layers(1_000_000);
        let model = LlamaModel::load_from_file(&self.backend, model_path, &params)
            .with_context(|| format!("не удалось загрузить модель {model_id}"))?;
        *guard = Some(Loaded {
            model_id: model_id.to_string(),
            model,
        });
        Ok(())
    }

    /// Вычитка текста. Собирает чат в формате Qwen (ChatML), генерирует
    /// жадно (temperature=0), останавливается по EOG, лимиту токенов
    /// или дедлайну.
    pub fn cleanup(
        &self,
        model_path: &Path,
        model_id: &str,
        system: &str,
        few_shot: &[(&str, &str)],
        user_text: &str,
        timeout: Duration,
    ) -> Result<String> {
        self.ensure_loaded(model_path, model_id)?;
        *self.last_used.lock() = Instant::now();
        let deadline = Instant::now() + timeout;

        let guard = self.loaded.lock();
        let model = &guard.as_ref().expect("модель загружена выше").model;

        let mut prompt = format!("<|im_start|>system\n{system}<|im_end|>\n");
        for (user, assistant) in few_shot {
            prompt.push_str(&format!(
                "<|im_start|>user\n{user}<|im_end|>\n<|im_start|>assistant\n{assistant}<|im_end|>\n"
            ));
        }
        prompt.push_str(&format!(
            "<|im_start|>user\n{user_text}<|im_end|>\n<|im_start|>assistant\n"
        ));

        // Спец-токены ChatML размечаются токенизатором (parse_special=true).
        let tokens = model
            .str_to_token(&prompt, AddBos::Never)
            .context("токенизация промпта")?;
        if tokens.len() as u32 > N_CTX - 256 {
            bail!("текст слишком длинный для вычитки локальной моделью");
        }

        let mut ctx = model
            .new_context(
                &self.backend,
                LlamaContextParams::default().with_n_ctx(NonZeroU32::new(N_CTX)),
            )
            .context("создание контекста llama")?;

        let mut batch = LlamaBatch::new(tokens.len().max(64), 1);
        let last_idx = tokens.len() as i32 - 1;
        for (i, token) in tokens.iter().enumerate() {
            batch
                .add(*token, i as i32, &[0], i as i32 == last_idx)
                .context("наполнение батча")?;
        }
        ctx.decode(&mut batch).context("прогон промпта")?;

        // Ограничение на вывод: вычитка не длиннее входа с запасом.
        let max_out = (user_text.split_whitespace().count() * 4).clamp(64, 1024);
        let mut sampler = LlamaSampler::greedy();
        let mut out = String::new();
        let mut decoder = encoding_rs::UTF_8.new_decoder();

        for pos in (tokens.len() as i32..).take(max_out) {
            if Instant::now() > deadline {
                bail!("вычитка не уложилась в таймаут");
            }
            let token = sampler.sample(&ctx, batch.n_tokens() - 1);
            if model.is_eog_token(token) {
                break;
            }
            if let Ok(piece) = model.token_to_piece(token, &mut decoder, false, None) {
                out.push_str(&piece);
            }
            batch.clear();
            batch
                .add(token, pos, &[0], true)
                .context("батч генерации")?;
            ctx.decode(&mut batch).context("шаг генерации")?;
        }

        *self.last_used.lock() = Instant::now();
        Ok(out.trim().to_string())
    }

    pub fn unload(&self) {
        let mut guard = self.loaded.lock();
        if guard.take().is_some() {
            log::info!("LLM-модель выгружена");
        }
    }

    pub fn unload_if_idle(&self, idle: Duration) {
        if self.last_used.lock().elapsed() >= idle {
            self.unload();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Смоук на реальной модели (проверяет и изоляцию ggml от whisper):
    /// VOICE_INPUT_LLM=~/.../qwen2.5-1.5b-instruct-q4_k_m.gguf \
    ///   cargo test llm_smoke -- --ignored --nocapture
    #[test]
    #[ignore = "требует скачанную модель (VOICE_INPUT_LLM)"]
    fn llm_smoke_cleanup() {
        let model = std::env::var("VOICE_INPUT_LLM").expect("нужен VOICE_INPUT_LLM");
        let llm = LocalLlm::new().unwrap();
        let raw = "ээ ну короче надо задеплоить это на стейджинг и и создать пул реквест";
        let out = llm
            .cleanup(
                std::path::Path::new(&model),
                "smoke",
                &crate::postproc::system_prompt(""),
                crate::postproc::few_shot(),
                raw,
                Duration::from_secs(120),
            )
            .unwrap();
        eprintln!("ВЫЧИТКА: «{out}»");
        assert!(!out.is_empty());
        assert!(
            crate::postproc::apply_guardrails(raw, &out).is_some(),
            "guardrails забраковали: «{out}»"
        );
    }
}
