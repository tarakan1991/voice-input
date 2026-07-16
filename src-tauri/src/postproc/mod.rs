//! Постобработка (вычитка) распознанного текста.
//!
//! Правила SPEC.md §7: убрать филлеры, оговорки и самоповторы, расставить
//! пунктуацию, исправить грамматику — и НЕ переписывать смысл. Guardrails
//! защищают от отсебятины модели; при любой ошибке вставляется сырой текст.

pub mod cloud;
pub mod keys;
pub mod local;

/// Границы отношения длины результата к длине входа (в словах).
/// Выход за границы = модель пересказала или съела текст → берём сырой.
const LEN_RATIO_MIN: f32 = 0.4;
const LEN_RATIO_MAX: f32 = 1.5;

pub fn system_prompt(dictionary_terms: &str) -> String {
    let terms_note = if dictionary_terms.is_empty() {
        String::new()
    } else {
        format!(" Сохраняй термины и англицизмы автора как есть: {dictionary_terms}.")
    };
    format!(
        "Ты — корректор надиктованного текста. Перед тобой сырая расшифровка речи. \
         Приведи её к виду, как будто автор набрал текст руками:\n\
         — убери филлеры («ээ», «мм», «ну это самое», «как бы», «короче» и т. п.);\n\
         — убери оговорки и самоповторы, оставив финальный вариант фразы;\n\
         — расставь знаки препинания и заглавные буквы;\n\
         — исправь грамматику и согласование.\n\
         Запрещено: перефразировать, менять смысл, добавлять или выбрасывать содержание, \
         «улучшать стиль».{terms_note} \
         Верни только исправленный текст, без комментариев."
    )
}

/// Few-shot примеры: на маленьких моделях дают больше, чем формулировки.
pub fn few_shot() -> &'static [(&'static str, &'static str)] {
    &[
        (
            "ээ ну короче надо задеплоить это на стейджинг и и создать пул реквест",
            "Надо задеплоить это на стейджинг и создать пул-реквест.",
        ),
        (
            "я хотел я хотел сказать что фича готова но есть как бы один баг с авторизацией",
            "Я хотел сказать, что фича готова, но есть один баг с авторизацией.",
        ),
    ]
}

/// Проверка результата вычитки. `None` — результат забракован, берём сырой.
pub fn apply_guardrails(raw: &str, cleaned: &str) -> Option<String> {
    let cleaned = cleaned.trim();
    if cleaned.is_empty() {
        return None;
    }
    let raw_words = raw.split_whitespace().count().max(1) as f32;
    let clean_words = cleaned.split_whitespace().count() as f32;
    let ratio = clean_words / raw_words;
    if !(LEN_RATIO_MIN..=LEN_RATIO_MAX).contains(&ratio) {
        log::warn!(
            "вычитка забракована: отношение длин {ratio:.2} вне [{LEN_RATIO_MIN}, {LEN_RATIO_MAX}]"
        );
        return None;
    }
    Some(cleaned.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guardrails_accept_reasonable_cleanup() {
        let raw = "ээ ну надо задеплоить на стейджинг короче";
        let cleaned = "Надо задеплоить на стейджинг.";
        assert_eq!(apply_guardrails(raw, cleaned), Some(cleaned.to_string()));
    }

    #[test]
    fn guardrails_reject_empty() {
        assert_eq!(apply_guardrails("текст", "  "), None);
    }

    #[test]
    fn guardrails_reject_runaway_expansion() {
        let raw = "два слова";
        let cleaned = "очень много слов которые модель зачем-то добавила от себя в этот текст";
        assert_eq!(apply_guardrails(raw, cleaned), None);
    }

    #[test]
    fn guardrails_reject_truncation() {
        let raw = "здесь было довольно много слов в исходной диктовке пользователя честно";
        let cleaned = "мало слов";
        assert_eq!(apply_guardrails(raw, cleaned), None);
    }

    #[test]
    fn system_prompt_includes_terms() {
        let p = system_prompt("PR, стейджинг");
        assert!(p.contains("PR, стейджинг"));
        assert!(p.contains("корректор"));
    }
}
