//! Облачная вычитка: Anthropic / OpenAI / DeepSeek.
//! В облако уходит только распознанный ТЕКСТ (аудио — никогда), осознанный
//! опт-ин в настройках. Ключи — в системном хранилище (см. keys.rs).

use crate::config::CloudProvider;
use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::time::Duration;

pub fn cleanup_via_cloud(
    provider: CloudProvider,
    model: &str,
    api_key: &str,
    system: &str,
    user_text: &str,
    timeout: Duration,
) -> Result<String> {
    let model = if model.trim().is_empty() {
        provider.default_model()
    } else {
        model.trim()
    };
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(timeout))
        .build()
        .into();

    match provider {
        CloudProvider::Anthropic => {
            let body = json!({
                "model": model,
                "max_tokens": 2048,
                "system": system,
                "messages": [{"role": "user", "content": user_text}],
            });
            let mut resp = agent
                .post("https://api.anthropic.com/v1/messages")
                .header("x-api-key", api_key)
                .header("anthropic-version", "2023-06-01")
                .header("content-type", "application/json")
                .send(body.to_string())
                .context("запрос к Anthropic не прошёл")?;
            let value: Value = serde_json::from_str(&resp.body_mut().read_to_string()?)
                .context("ответ Anthropic не разбирается")?;
            let text = value["content"][0]["text"]
                .as_str()
                .context("в ответе Anthropic нет текста")?;
            Ok(text.trim().to_string())
        }
        CloudProvider::Openai | CloudProvider::Deepseek => {
            let url = match provider {
                CloudProvider::Openai => "https://api.openai.com/v1/chat/completions",
                CloudProvider::Deepseek => "https://api.deepseek.com/chat/completions",
                CloudProvider::Anthropic => unreachable!(),
            };
            let body = json!({
                "model": model,
                "messages": [
                    {"role": "system", "content": system},
                    {"role": "user", "content": user_text},
                ],
                "temperature": 0.0,
            });
            let mut resp = agent
                .post(url)
                .header("authorization", &format!("Bearer {api_key}"))
                .header("content-type", "application/json")
                .send(body.to_string())
                .context("запрос к провайдеру не прошёл")?;
            let value: Value = serde_json::from_str(&resp.body_mut().read_to_string()?)
                .context("ответ провайдера не разбирается")?;
            let text = value["choices"][0]["message"]["content"]
                .as_str()
                .context("в ответе провайдера нет текста")?;
            Ok(text.trim().to_string())
        }
    }
}

/// Быстрая проверка ключа: короткий запрос; ошибки сети/авторизации — наружу
/// человеческим языком.
pub fn validate_key(provider: CloudProvider, model: &str, api_key: &str) -> Result<()> {
    if api_key.trim().is_empty() {
        bail!("ключ пустой");
    }
    cleanup_via_cloud(
        provider,
        model,
        api_key,
        "Ответь одним словом: ок",
        "проверка",
        Duration::from_secs(15),
    )
    .map(|_| ())
}
