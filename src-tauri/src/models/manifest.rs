//! Манифест моделей: что можно скачать, откуда, с какими SHA256.
//! Хэши — фактические SHA256 LFS-файлов Hugging Face (зафиксированы при
//! добавлении модели в манифест). Обновление манифеста = обновление приложения.

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelKind {
    Asr,
    Llm,
}

#[derive(Debug, Clone, Serialize)]
pub struct ModelInfo {
    pub id: &'static str,
    pub kind: ModelKind,
    pub file_name: &'static str,
    pub url: &'static str,
    pub sha256: &'static str,
    pub size_bytes: u64,
    /// Название для UI.
    pub title: &'static str,
    /// Описание человеческим языком — показывается в мастере как есть.
    pub description: &'static str,
    pub recommended: bool,
}

pub const MODELS: &[ModelInfo] = &[
    // ------------------------------- ASR --------------------------------
    ModelInfo {
        id: "whisper-small",
        kind: ModelKind::Asr,
        file_name: "ggml-small.bin",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.bin",
        sha256: "1be3a9b2063867b937e64e2ec7483364a79917e157fa98c5d94b5c1fffea987b",
        size_bytes: 487_601_967,
        title: "Whisper Small",
        description: "Быстро и компактно, но русский посредственный. Годится проверить, \
                      что всё работает, или для слабой машины.",
        recommended: false,
    },
    ModelInfo {
        id: "whisper-medium",
        kind: ModelKind::Asr,
        file_name: "ggml-medium.bin",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-medium.bin",
        sha256: "6c14d5adee5f86394037b4e4e8b59f1673b6cee10e3cf0b11bbdbee79c156208",
        size_bytes: 1_533_763_059,
        title: "Whisper Medium",
        description: "Приемлемый русский, средняя скорость. Компромисс, если не хочется \
                      качать turbo.",
        recommended: false,
    },
    ModelInfo {
        id: "whisper-large-v3-turbo",
        kind: ModelKind::Asr,
        file_name: "ggml-large-v3-turbo.bin",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo.bin",
        sha256: "1fc70f774d38eb169993ac391eea357ef47c88757ef72ee5943879b7e8e2bc69",
        size_bytes: 1_624_555_275,
        title: "Whisper Large v3 Turbo",
        description: "Рекомендуем. Качество русского почти как у large-v3, но в 5–6 раз \
                      быстрее: ~1–2 с на фразу. Нужно ~4 ГБ свободной RAM.",
        recommended: true,
    },
    ModelInfo {
        id: "whisper-large-v3",
        kind: ModelKind::Asr,
        file_name: "ggml-large-v3.bin",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3.bin",
        sha256: "64d182b440b98d5203c4f9bd541544d84c605196c4f7b845dfa11fb23594d1e2",
        size_bytes: 3_095_033_483,
        title: "Whisper Large v3",
        description: "Максимальная точность русского и англицизмов, но в разы медленнее \
                      turbo: ~4–8 с на фразу. Берите, только если turbo ошибается на вашей \
                      лексике.",
        recommended: false,
    },
    // ------------------------------- LLM --------------------------------
    ModelInfo {
        id: "qwen2.5-1.5b",
        kind: ModelKind::Llm,
        file_name: "qwen2.5-1.5b-instruct-q4_k_m.gguf",
        url: "https://huggingface.co/Qwen/Qwen2.5-1.5B-Instruct-GGUF/resolve/main/qwen2.5-1.5b-instruct-q4_k_m.gguf",
        sha256: "6a1a2eb6d15622bf3c96857206351ba97e1af16c30d7a74ee38970e434e9407e",
        size_bytes: 1_117_320_736,
        title: "Qwen2.5 1.5B",
        description: "Самая быстрая, базовая вычитка. Если хочется мгновенно.",
        recommended: false,
    },
    ModelInfo {
        id: "qwen2.5-3b",
        kind: ModelKind::Llm,
        file_name: "qwen2.5-3b-instruct-q4_k_m.gguf",
        url: "https://huggingface.co/Qwen/Qwen2.5-3B-Instruct-GGUF/resolve/main/qwen2.5-3b-instruct-q4_k_m.gguf",
        sha256: "626b4a6678b86442240e33df819e00132d3ba7dddfe1cdc4fbb18e0a9615c62d",
        size_bytes: 2_104_932_768,
        title: "Qwen2.5 3B",
        description: "Рекомендуем. Хороший русский, ~1–2 с на фразу.",
        recommended: true,
    },
    ModelInfo {
        id: "qwen2.5-7b",
        kind: ModelKind::Llm,
        file_name: "Qwen2.5-7B-Instruct-Q4_K_M.gguf",
        url: "https://huggingface.co/bartowski/Qwen2.5-7B-Instruct-GGUF/resolve/main/Qwen2.5-7B-Instruct-Q4_K_M.gguf",
        sha256: "65b8fcd92af6b4fefa935c625d1ac27ea29dcb6ee14589c55a8f115ceaaa1423",
        size_bytes: 4_683_074_240,
        title: "Qwen2.5 7B",
        description: "Лучшее качество, ощутимо медленнее. Для Mac с 16+ ГБ RAM.",
        recommended: false,
    },
];

pub fn find(id: &str) -> Option<&'static ModelInfo> {
    MODELS.iter().find(|m| m.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_is_consistent() {
        for m in MODELS {
            assert_eq!(m.sha256.len(), 64, "SHA256 модели {} неполон", m.id);
            assert!(m.url.starts_with("https://"), "url модели {}", m.id);
            assert!(m.size_bytes > 0);
            assert!(!m.description.is_empty());
        }
        // Ровно одна рекомендация на каждый тип
        let rec_asr = MODELS
            .iter()
            .filter(|m| m.kind == ModelKind::Asr && m.recommended)
            .count();
        let rec_llm = MODELS
            .iter()
            .filter(|m| m.kind == ModelKind::Llm && m.recommended)
            .count();
        assert_eq!(rec_asr, 1);
        assert_eq!(rec_llm, 1);
    }

    #[test]
    fn ids_are_unique() {
        let mut ids: Vec<_> = MODELS.iter().map(|m| m.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), MODELS.len());
    }
}
