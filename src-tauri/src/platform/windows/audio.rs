//! Этап 2: захват аудио на Windows.
//!
//! План: общая реализация на cpal (WASAPI) из `platform/shared/audio.rs`
//! + определение встроенного микрофона по форм-фактору эндпоинта WASAPI.

use crate::platform::{AudioCapture, AudioDevice, CaptureStream, ChunkCallback, DeviceSelector};
use anyhow::Result;

pub struct WindowsAudioCapture;

impl AudioCapture for WindowsAudioCapture {
    fn list_devices(&self) -> Result<Vec<AudioDevice>> {
        unimplemented!("windows: перечисление устройств ввода (WASAPI через cpal)")
    }

    fn builtin_device(&self) -> Result<Option<AudioDevice>> {
        unimplemented!("windows: определение встроенного микрофона по форм-фактору эндпоинта")
    }

    fn open(
        &self,
        _selector: &DeviceSelector,
        _on_chunk: ChunkCallback,
    ) -> Result<Box<dyn CaptureStream>> {
        unimplemented!(
            "windows: открытие потока захвата с гарантией полного освобождения устройства"
        )
    }
}
