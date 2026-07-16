//! Словарь: термины (мягкое смещение через initial_prompt Whisper и промпт LLM)
//! и правила замены (жёсткая детерминированная постобработка текста).
//!
//! Правила применяются дважды: после ASR (до вычитки) и после вычитки —
//! LLM может «починить» термин обратно.

use regex::Regex;
use serde::{Deserialize, Serialize};

/// Встроенный базовый список IT-англицизмов для initial_prompt.
/// Пользовательские термины имеют приоритет при нехватке бюджета подсказки.
const BUILTIN_TERMS: &[&str] = &[
    "задеплоить",
    "стейджинг",
    "продакшен",
    "пул-реквест",
    "мердж",
    "коммит",
    "ребейз",
    "фича",
    "баг",
    "фикс",
    "хотфикс",
    "релиз",
    "пайплайн",
    "бэкенд",
    "фронтенд",
    "девопс",
    "докер",
    "кубернетес",
    "эндпоинт",
    "миддлвэр",
    "токен",
    "апрув",
    "ревью",
    "таска",
    "спринт",
    "легаси",
    "рефакторинг",
    "роллбэк",
    "конфиг",
    "билд",
];

/// Бюджет initial_prompt Whisper — ~224 токена; держим запас по символам.
const PROMPT_CHAR_BUDGET: usize = 600;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReplacementRule {
    pub from: String,
    pub to: String,
    /// `from` — регулярное выражение (иначе литерал).
    #[serde(default)]
    pub regex: bool,
    #[serde(default = "default_true")]
    pub ignore_case: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Dictionary {
    /// Термины, имена, названия — попадают в подсказки ASR и LLM.
    pub terms: Vec<String>,
    /// Жёсткие правила замены, например «пи ар» → «PR».
    pub replacements: Vec<ReplacementRule>,
}

impl Dictionary {
    /// Собирает initial_prompt для Whisper: осмысленная русская фраза,
    /// смещающая декодер к терминам. Пользовательские термины — в приоритете.
    pub fn initial_prompt(&self) -> String {
        let mut prompt = String::from("Заметки разработчика о работе: ");
        let mut first = true;
        for term in self.terms.iter().map(String::as_str).chain(
            BUILTIN_TERMS
                .iter()
                .copied()
                .filter(|b| !self.terms.iter().any(|t| t.eq_ignore_ascii_case(b))),
        ) {
            let sep = if first { "" } else { ", " };
            if prompt.len() + sep.len() + term.len() > PROMPT_CHAR_BUDGET {
                break;
            }
            prompt.push_str(sep);
            prompt.push_str(term);
            first = false;
        }
        prompt.push('.');
        prompt
    }

    /// Термины для промпта вычитки (LLM должна сохранять их написание).
    pub fn terms_for_llm(&self) -> String {
        self.terms.join(", ")
    }

    /// Применяет правила замены. Некорректные регулярные выражения
    /// пропускаются с предупреждением — одно кривое правило не должно
    /// ронять диктовку.
    pub fn apply(&self, text: &str) -> String {
        let mut result = text.to_string();
        for rule in &self.replacements {
            let pattern = if rule.regex {
                rule.from.clone()
            } else {
                regex::escape(&rule.from)
            };
            let pattern = if rule.ignore_case {
                format!("(?i){pattern}")
            } else {
                pattern
            };
            match Regex::new(&pattern) {
                Ok(re) => {
                    result = re.replace_all(&result, rule.to.as_str()).into_owned();
                }
                Err(e) => {
                    log::warn!("правило словаря «{}» пропущено: {e}", rule.from);
                }
            }
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(from: &str, to: &str) -> ReplacementRule {
        ReplacementRule {
            from: from.into(),
            to: to.into(),
            regex: false,
            ignore_case: true,
        }
    }

    #[test]
    fn literal_replacement_ignores_case() {
        let dict = Dictionary {
            terms: vec![],
            replacements: vec![rule("пи ар", "PR")],
        };
        assert_eq!(dict.apply("Открой Пи Ар с фиксом"), "Открой PR с фиксом");
    }

    #[test]
    fn regex_replacement_works() {
        let dict = Dictionary {
            terms: vec![],
            replacements: vec![ReplacementRule {
                from: r"гит\s*хаб".into(),
                to: "GitHub".into(),
                regex: true,
                ignore_case: true,
            }],
        };
        assert_eq!(dict.apply("залей на гит хаб"), "залей на GitHub");
    }

    #[test]
    fn invalid_regex_is_skipped() {
        let dict = Dictionary {
            terms: vec![],
            replacements: vec![ReplacementRule {
                from: "((".into(),
                to: "x".into(),
                regex: true,
                ignore_case: false,
            }],
        };
        assert_eq!(dict.apply("текст не тронут"), "текст не тронут");
    }

    #[test]
    fn initial_prompt_prefers_user_terms_and_fits_budget() {
        let dict = Dictionary {
            terms: vec!["Tauri".into(), "Свелте".into()],
            replacements: vec![],
        };
        let prompt = dict.initial_prompt();
        assert!(prompt.contains("Tauri"));
        assert!(prompt.contains("Свелте"));
        assert!(prompt.contains("задеплоить"));
        assert!(prompt.len() <= super::PROMPT_CHAR_BUDGET + 10);
    }

    #[test]
    fn user_term_overrides_builtin_duplicate() {
        let dict = Dictionary {
            terms: vec!["коммит".into()],
            replacements: vec![],
        };
        let prompt = dict.initial_prompt();
        assert_eq!(prompt.matches("коммит").count(), 1);
    }
}
