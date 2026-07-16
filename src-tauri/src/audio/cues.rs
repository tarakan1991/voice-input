//! Звуковые отбивки: синтезируются на лету и играются через устройство
//! ВЫВОДА (cpal). Микрофон не затрагивается — HFP у Bluetooth-гарнитуры
//! отбивки не провоцируют.

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cue {
    /// «Говорите» — поток реально пошёл (восходящий двутон, как в браузере).
    Start,
    /// Запись остановлена, началась обработка (нисходящий двутон).
    Stop,
    /// Отмена (короткий низкий тон).
    Cancel,
    /// Ошибка (двойной низкий тон).
    Error,
}

/// Синтез сигнала: последовательность (частота, длительность мс), амплитуда
/// с атакой/затуханием, чтобы не щёлкало.
fn synth(cue: Cue, rate: f32) -> Vec<f32> {
    let tones: &[(f32, f32)] = match cue {
        Cue::Start => &[(660.0, 70.0), (990.0, 90.0)],
        Cue::Stop => &[(990.0, 70.0), (660.0, 90.0)],
        Cue::Cancel => &[(440.0, 110.0)],
        Cue::Error => &[(220.0, 90.0), (0.0, 40.0), (220.0, 90.0)],
    };
    let mut out = Vec::new();
    for &(freq, ms) in tones {
        let n = (rate * ms / 1000.0) as usize;
        for i in 0..n {
            let t = i as f32 / rate;
            // Огибающая: 8 мс атака, плавное затухание к концу тона.
            let attack = (i as f32 / (rate * 0.008)).min(1.0);
            let release = ((n - i) as f32 / (rate * 0.03)).min(1.0);
            let amp = 0.18 * attack * release;
            let sample = if freq == 0.0 {
                0.0
            } else {
                (2.0 * std::f32::consts::PI * freq * t).sin()
            };
            out.push(sample * amp);
        }
    }
    out
}

/// Проигрывает отбивку. Не блокирует: открывает устройство вывода в отдельном
/// потоке, играет и закрывает. Ошибки только логируются — отбивка не должна
/// ломать диктовку.
pub fn play(cue: Cue) {
    std::thread::Builder::new()
        .name("audio-cue".into())
        .spawn(move || {
            if let Err(e) = play_blocking(cue) {
                log::warn!("не удалось проиграть отбивку: {e}");
            }
        })
        .ok();
}

fn play_blocking(cue: Cue) -> anyhow::Result<()> {
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .ok_or_else(|| anyhow::anyhow!("нет устройства вывода"))?;
    let config = device.default_output_config()?;
    let rate = config.sample_rate() as f32;
    let channels = config.channels() as usize;
    let samples = Arc::new(synth(cue, rate));
    let pos = Arc::new(AtomicUsize::new(0));
    let total = samples.len();

    let (done_tx, done_rx) = crossbeam_channel::bounded::<()>(1);
    let samples_cb = samples.clone();
    let pos_cb = pos.clone();

    let stream = device.build_output_stream(
        config.into(),
        move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
            let mut p = pos_cb.load(Ordering::Relaxed);
            for frame in data.chunks_mut(channels) {
                let value = samples_cb.get(p).copied().unwrap_or(0.0);
                for out in frame.iter_mut() {
                    *out = value;
                }
                p += 1;
            }
            pos_cb.store(p, Ordering::Relaxed);
            if p >= total {
                let _ = done_tx.try_send(());
            }
        },
        |e| log::warn!("ошибка вывода отбивки: {e}"),
        None,
    )?;
    stream.play()?;
    // Ждём конца плюс небольшой хвост на буферизацию.
    let _ = done_rx.recv_timeout(std::time::Duration::from_secs(2));
    std::thread::sleep(std::time::Duration::from_millis(60));
    drop(stream);
    Ok(())
}
