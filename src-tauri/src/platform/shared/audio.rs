//! Захват аудио через cpal (CoreAudio на macOS, WASAPI на Windows).
//!
//! Ключевой инвариант проекта: устройство полностью освобождается при закрытии
//! потока. `cpal::Stream` не `Send`, поэтому им владеет выделенный поток:
//! `close()` шлёт сигнал, поток дропает Stream (release устройства в ОС)
//! и завершается; `close()`/`Drop` дожидаются завершения синхронно.

use crate::platform::{AudioCapture, AudioDevice, CaptureStream, ChunkCallback, DeviceSelector};
use anyhow::{anyhow, bail, Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use crossbeam_channel::{bounded, Sender};
use std::thread::JoinHandle;

/// Платформенный детектор встроенного микрофона по имени устройства
/// (macOS — точный, по transport type из CoreAudio; имена совпадают,
/// т.к. cpal читает то же свойство).
pub type BuiltinMatcher = Box<dyn Fn(&str) -> bool + Send + Sync>;

pub struct CpalAudioCapture {
    is_builtin: BuiltinMatcher,
}

fn device_name(device: &cpal::Device) -> Result<String> {
    Ok(device
        .description()
        .context("устройство не отдаёт описание")?
        .name()
        .to_string())
}

fn device_id(device: &cpal::Device) -> Result<String> {
    Ok(device.id().context("устройство не отдаёт id")?.to_string())
}

impl CpalAudioCapture {
    pub fn new(is_builtin: BuiltinMatcher) -> Self {
        Self { is_builtin }
    }

    fn find_device(&self, selector: &DeviceSelector) -> Result<cpal::Device> {
        let host = cpal::default_host();
        match selector {
            DeviceSelector::Default => host
                .default_input_device()
                .ok_or_else(|| anyhow!("в системе нет устройства ввода по умолчанию")),
            DeviceSelector::Builtin => {
                for d in host
                    .input_devices()
                    .context("перечисление устройств ввода")?
                {
                    if let Ok(name) = device_name(&d) {
                        if (self.is_builtin)(&name) {
                            return Ok(d);
                        }
                    }
                }
                bail!("встроенный микрофон не найден")
            }
            DeviceSelector::ById(id) => {
                for d in host
                    .input_devices()
                    .context("перечисление устройств ввода")?
                {
                    if device_id(&d).map(|n| &n == id).unwrap_or(false) {
                        return Ok(d);
                    }
                }
                bail!("выбранный микрофон не найден — возможно, устройство отключено")
            }
        }
    }
}

impl AudioCapture for CpalAudioCapture {
    fn list_devices(&self) -> Result<Vec<AudioDevice>> {
        let host = cpal::default_host();
        let default_id = host.default_input_device().and_then(|d| device_id(&d).ok());
        let mut out = Vec::new();
        for d in host
            .input_devices()
            .context("перечисление устройств ввода")?
        {
            let (Ok(id), Ok(name)) = (device_id(&d), device_name(&d)) else {
                continue;
            };
            out.push(AudioDevice {
                is_builtin: (self.is_builtin)(&name),
                is_default: Some(&id) == default_id.as_ref(),
                id,
                name,
            });
        }
        Ok(out)
    }

    fn builtin_device(&self) -> Result<Option<AudioDevice>> {
        Ok(self.list_devices()?.into_iter().find(|d| d.is_builtin))
    }

    fn open(
        &self,
        selector: &DeviceSelector,
        mut on_chunk: ChunkCallback,
    ) -> Result<Box<dyn CaptureStream>> {
        let device = self.find_device(selector)?;
        let config = device
            .default_input_config()
            .context("устройство не отдаёт конфигурацию ввода")?;
        let sample_rate = config.sample_rate();
        let channels = config.channels();
        let sample_format = config.sample_format();
        let stream_config: cpal::StreamConfig = config.into();

        let (close_tx, close_rx) = bounded::<()>(1);
        let (ready_tx, ready_rx) = bounded::<Result<()>>(1);

        // Поток-владелец Stream: cpal::Stream не Send, создаём и дропаем его
        // в одном месте. Выход из цикла (сигнал close или разрыв канала) —
        // единственный путь, и он всегда завершается drop(stream).
        let join: JoinHandle<()> = std::thread::Builder::new()
            .name("audio-capture".into())
            .spawn(move || {
                let err_fn = |e: cpal::Error| log::error!("audio stream error: {e}");
                let mut conv_buf: Vec<f32> = Vec::new();
                let built: Result<cpal::Stream> = (|| {
                    let stream = match sample_format {
                        cpal::SampleFormat::F32 => device.build_input_stream(
                            stream_config,
                            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                                on_chunk(data, sample_rate, channels)
                            },
                            err_fn,
                            None,
                        )?,
                        cpal::SampleFormat::I16 => device.build_input_stream(
                            stream_config,
                            move |data: &[i16], _: &cpal::InputCallbackInfo| {
                                conv_buf.clear();
                                conv_buf.extend(data.iter().map(|&s| s as f32 / i16::MAX as f32));
                                on_chunk(&conv_buf, sample_rate, channels);
                            },
                            err_fn,
                            None,
                        )?,
                        cpal::SampleFormat::U16 => device.build_input_stream(
                            stream_config,
                            move |data: &[u16], _: &cpal::InputCallbackInfo| {
                                conv_buf.clear();
                                conv_buf.extend(
                                    data.iter().map(|&s| s as f32 / u16::MAX as f32 * 2.0 - 1.0),
                                );
                                on_chunk(&conv_buf, sample_rate, channels);
                            },
                            err_fn,
                            None,
                        )?,
                        other => bail!("неподдерживаемый формат сэмплов: {other:?}"),
                    };
                    stream.play().context("запуск потока захвата")?;
                    Ok(stream)
                })();

                match built {
                    Ok(stream) => {
                        let _ = ready_tx.send(Ok(()));
                        // Ждём сигнал закрытия; разрыв канала = закрытие.
                        let _ = close_rx.recv();
                        drop(stream); // освобождение устройства в ОС
                        log::debug!("audio stream closed, device released");
                    }
                    Err(e) => {
                        let _ = ready_tx.send(Err(e));
                    }
                }
            })
            .context("не удалось создать поток захвата")?;

        // Дожидаемся фактического старта потока: отбивка «говорите» должна
        // звучать только когда устройство реально пишет.
        ready_rx
            .recv()
            .map_err(|_| anyhow!("поток захвата завершился, не начав работу"))??;

        Ok(Box::new(ThreadCaptureStream {
            close_tx,
            join: Some(join),
        }))
    }
}

struct ThreadCaptureStream {
    close_tx: Sender<()>,
    join: Option<JoinHandle<()>>,
}

impl ThreadCaptureStream {
    fn shutdown(&mut self) {
        let _ = self.close_tx.send(());
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

impl CaptureStream for ThreadCaptureStream {
    fn close(mut self: Box<Self>) {
        self.shutdown();
    }
}

// Страховка: любой путь уничтожения хэндла (паника, ранний выход, ошибка)
// синхронно закрывает поток и освобождает устройство.
impl Drop for ThreadCaptureStream {
    fn drop(&mut self) {
        self.shutdown();
    }
}
