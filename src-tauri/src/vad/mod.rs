//! Определение речи/тишины: Silero VAD (нейросетевой, ONNX ~2 МБ, вшит в
//! крейт voice_activity_detector) + чистая логика таймера тишины.
//!
//! Silero выбран вместо webrtc-vad: устойчив к стационарному шуму
//! (кондиционер, вентилятор) и тихой речи — см. SPEC.md §6.

use anyhow::{Context, Result};
use voice_activity_detector::VoiceActivityDetector;

/// Размер чанка для Silero @16 кГц (32 мс).
pub const VAD_CHUNK: usize = 512;
/// Длительность чанка в секундах.
pub const VAD_CHUNK_SECS: f32 = VAD_CHUNK as f32 / 16_000.0;

/// Гистерезис: порог входа в речь выше порога выхода — защита от дребезга.
const SPEECH_ENTER: f32 = 0.55;
const SPEECH_EXIT: f32 = 0.35;

/// За сколько секунд до автостопа показывать обратный отсчёт на плашке.
pub const COUNTDOWN_WINDOW_SECS: f32 = 3.0;

pub trait VadEngine: Send {
    /// Вероятность речи в чанке из 512 сэмплов @16 кГц.
    fn predict(&mut self, chunk: &[f32]) -> Result<f32>;
}

pub struct SileroVad {
    inner: VoiceActivityDetector,
}

impl SileroVad {
    pub fn new() -> Result<Self> {
        let inner = VoiceActivityDetector::builder()
            .sample_rate(16_000)
            .chunk_size(VAD_CHUNK)
            .build()
            .context("инициализация Silero VAD")?;
        Ok(Self { inner })
    }
}

impl VadEngine for SileroVad {
    fn predict(&mut self, chunk: &[f32]) -> Result<f32> {
        Ok(self.inner.predict(chunk.iter().copied()))
    }
}

/// Что делать после очередного чанка.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SilenceVerdict {
    /// Продолжаем запись.
    Continue,
    /// Тишина затянулась — показываем отсчёт (секунд до автостопа).
    Countdown(f32),
    /// Порог тишины превышен.
    TimedOut {
        /// Была ли вообще речь: если нет — диктовка отменяется без вставки.
        had_speech: bool,
    },
}

/// Чистая, тестируемая логика таймера тишины поверх вероятностей VAD.
pub struct SilenceTracker {
    threshold_secs: f32,
    speaking: bool,
    had_speech: bool,
    silence_secs: f32,
}

impl SilenceTracker {
    pub fn new(threshold_secs: f32) -> Self {
        Self {
            threshold_secs,
            speaking: false,
            had_speech: false,
            silence_secs: 0.0,
        }
    }

    /// Длительность накопленной хвостовой тишины (для обрезки перед Whisper).
    pub fn trailing_silence_secs(&self) -> f32 {
        self.silence_secs
    }

    pub fn update(&mut self, speech_prob: f32, chunk_secs: f32) -> SilenceVerdict {
        let enter = speech_prob >= SPEECH_ENTER;
        let exit = speech_prob < SPEECH_EXIT;
        if self.speaking {
            if exit {
                self.speaking = false;
            }
        } else if enter {
            self.speaking = true;
            self.had_speech = true;
        }

        if self.speaking {
            self.silence_secs = 0.0;
            return SilenceVerdict::Continue;
        }

        self.silence_secs += chunk_secs;
        if self.silence_secs >= self.threshold_secs {
            return SilenceVerdict::TimedOut {
                had_speech: self.had_speech,
            };
        }
        let left = self.threshold_secs - self.silence_secs;
        if self.had_speech && left <= COUNTDOWN_WINDOW_SECS {
            SilenceVerdict::Countdown(left)
        } else {
            SilenceVerdict::Continue
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CHUNK: f32 = VAD_CHUNK_SECS;

    fn feed(tracker: &mut SilenceTracker, prob: f32, secs: f32) -> SilenceVerdict {
        let n = (secs / CHUNK).ceil() as usize;
        let mut last = SilenceVerdict::Continue;
        for _ in 0..n {
            last = tracker.update(prob, CHUNK);
            if matches!(last, SilenceVerdict::TimedOut { .. }) {
                break;
            }
        }
        last
    }

    #[test]
    fn no_speech_at_all_times_out_without_speech() {
        let mut t = SilenceTracker::new(5.0);
        let verdict = feed(&mut t, 0.05, 6.0);
        assert_eq!(verdict, SilenceVerdict::TimedOut { had_speech: false });
    }

    #[test]
    fn speech_then_silence_times_out_with_speech() {
        let mut t = SilenceTracker::new(5.0);
        assert_eq!(feed(&mut t, 0.9, 2.0), SilenceVerdict::Continue);
        let verdict = feed(&mut t, 0.05, 6.0);
        assert_eq!(verdict, SilenceVerdict::TimedOut { had_speech: true });
    }

    #[test]
    fn speech_resets_silence_timer() {
        let mut t = SilenceTracker::new(5.0);
        feed(&mut t, 0.9, 1.0);
        feed(&mut t, 0.05, 4.0); // тишина, но меньше порога
        feed(&mut t, 0.9, 0.5); // снова речь — таймер сброшен
        let verdict = feed(&mut t, 0.05, 4.0);
        assert!(
            !matches!(verdict, SilenceVerdict::TimedOut { .. }),
            "таймер не сбросился: {verdict:?}"
        );
    }

    #[test]
    fn countdown_appears_before_timeout() {
        let mut t = SilenceTracker::new(5.0);
        feed(&mut t, 0.9, 1.0);
        let verdict = feed(&mut t, 0.05, 3.0); // осталось ~2 c < окна отсчёта
        match verdict {
            SilenceVerdict::Countdown(left) => assert!(left > 0.0 && left <= 3.0),
            other => panic!("ждали отсчёт, получили {other:?}"),
        }
    }

    #[test]
    fn hysteresis_ignores_border_flicker() {
        let mut t = SilenceTracker::new(1.0);
        // Вероятность между порогами: речь не начинается, тишина копится
        let verdict = feed(&mut t, 0.45, 1.5);
        assert_eq!(verdict, SilenceVerdict::TimedOut { had_speech: false });
    }
}
