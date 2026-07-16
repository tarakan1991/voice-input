//! Аудио-конвейер (платформонезависимый): сведение в моно, RMS-уровень,
//! ресемплинг. Захват — за трейтом `platform::AudioCapture`.

pub mod cues;

use anyhow::{Context, Result};
use rubato::audioadapter_buffers::direct::InterleavedSlice;
use rubato::{Fft, FixedSync, Resampler};

pub const TARGET_RATE: u32 = 16_000;

/// Сводит interleaved-каналы в моно усреднением.
pub fn mix_to_mono(samples: &[f32], channels: u16) -> Vec<f32> {
    if channels <= 1 {
        return samples.to_vec();
    }
    let ch = channels as usize;
    samples
        .chunks_exact(ch)
        .map(|frame| frame.iter().sum::<f32>() / ch as f32)
        .collect()
}

/// RMS-уровень чанка (0..1) для индикатора на плашке.
pub fn rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum_sq: f32 = samples.iter().map(|s| s * s).sum();
    (sum_sq / samples.len() as f32).sqrt()
}

/// Качественный офлайн-ресемплинг всей записи в 16 кГц для Whisper (rubato Fft).
pub fn resample_to_16k(samples: &[f32], from_rate: u32) -> Result<Vec<f32>> {
    if from_rate == TARGET_RATE {
        return Ok(samples.to_vec());
    }
    if samples.is_empty() {
        return Ok(Vec::new());
    }
    let mut resampler = Fft::<f32>::new(
        from_rate as usize,
        TARGET_RATE as usize,
        1024,
        1,
        FixedSync::Input,
    )
    .context("создание ресемплера")?;
    let input = InterleavedSlice::new(samples, 1, samples.len())
        .map_err(|e| anyhow::anyhow!("буфер ресемплера: {e:?}"))?;
    let output = resampler
        .process_all(&input, samples.len(), None)
        .map_err(|e| anyhow::anyhow!("ресемплинг: {e}"))?;
    Ok(output.take_data())
}

/// Потоковый линейный ресемплер в 16 кГц — только для VAD (реального времени).
/// Для Whisper используется качественный `resample_to_16k`.
pub struct StreamingResampler {
    ratio: f64,
    /// Позиция чтения в координатах входа (дробная), продолжается между push.
    pos: f64,
    /// Последний сэмпл предыдущего чанка для интерполяции на стыке.
    prev: Option<f32>,
}

impl StreamingResampler {
    pub fn new(from_rate: u32) -> Self {
        Self {
            ratio: from_rate as f64 / TARGET_RATE as f64,
            pos: 0.0,
            prev: None,
        }
    }

    /// Дописывает ресемплированные сэмплы в `out`.
    pub fn push(&mut self, input: &[f32], out: &mut Vec<f32>) {
        if input.is_empty() {
            return;
        }
        if (self.ratio - 1.0).abs() < f64::EPSILON {
            out.extend_from_slice(input);
            return;
        }
        // Виртуальный вход: [prev] + input; позиция -1.0 соответствует prev.
        loop {
            let idx = self.pos.floor();
            let frac = (self.pos - idx) as f32;
            let i = idx as isize;
            let (a, b) = if i < 0 {
                match self.prev {
                    Some(p) => (p, input[0]),
                    None => (input[0], input[0]),
                }
            } else if (i as usize) + 1 < input.len() {
                (input[i as usize], input[i as usize + 1])
            } else {
                break;
            };
            out.push(a + (b - a) * frac);
            self.pos += self.ratio;
        }
        self.pos -= input.len() as f64;
        self.prev = Some(*input.last().unwrap());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mono_mixdown_averages_channels() {
        let stereo = [1.0, 0.0, 0.5, 0.5];
        assert_eq!(mix_to_mono(&stereo, 2), vec![0.5, 0.5]);
    }

    #[test]
    fn rms_of_silence_is_zero() {
        assert_eq!(rms(&[0.0; 100]), 0.0);
        assert!(rms(&[]) == 0.0);
    }

    #[test]
    fn rms_of_full_scale_square_is_one() {
        let square = [1.0f32; 64];
        assert!((rms(&square) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn offline_resample_halves_sample_count() {
        // 32 кГц → 16 кГц: количество сэмплов ~вдвое меньше
        let input: Vec<f32> = (0..32_000).map(|i| (i as f32 * 0.01).sin() * 0.5).collect();
        let out = resample_to_16k(&input, 32_000).unwrap();
        let expected = input.len() / 2;
        assert!(
            (out.len() as i64 - expected as i64).unsigned_abs() < 200,
            "получили {} сэмплов, ждали ~{expected}",
            out.len()
        );
    }

    #[test]
    fn offline_resample_16k_is_passthrough() {
        let input = vec![0.1f32; 1600];
        let out = resample_to_16k(&input, 16_000).unwrap();
        assert_eq!(out, input);
    }

    #[test]
    fn streaming_resampler_rate_roughly_correct() {
        let mut r = StreamingResampler::new(48_000);
        let mut out = Vec::new();
        // 48000 сэмплов = 1 секунда → ~16000 на выходе
        for _ in 0..100 {
            let chunk = vec![0.25f32; 480];
            r.push(&chunk, &mut out);
        }
        assert!(
            (out.len() as i64 - 16_000).unsigned_abs() < 50,
            "получили {}",
            out.len()
        );
    }
}
