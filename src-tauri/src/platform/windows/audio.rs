//! Определение встроенного микрофона на Windows.
//!
//! cpal (WASAPI) не сообщает, каким транспортом подключено устройство,
//! поэтому спрашиваем Core Audio напрямую: перебираем capture-эндпоинты,
//! у каждого берём имя (то же PKEY_Device_FriendlyName, что читает cpal —
//! сопоставление по точному равенству надёжно), форм-фактор и шину
//! адаптера из топологии. Встроенным считаем микрофон (форм-фактор
//! Microphone) на внутренней шине (HDAUDIO/INTELAUDIO/ACPI): USB-вебкамеры
//! и Bluetooth-гарнитуры это отсекает — ровно ради них опция
//! «всегда встроенный» и существует.

use crate::platform::shared::audio::BuiltinMatcher;
use std::sync::OnceLock;
use windows::core::Interface;
use windows::Win32::Devices::FunctionDiscovery::PKEY_Device_FriendlyName;
use windows::Win32::Media::Audio::{
    eCapture, IConnector, IDeviceTopology, IMMDeviceEnumerator, IPart, MMDeviceEnumerator,
    Microphone, PKEY_AudioEndpoint_FormFactor, DEVICE_STATE_ACTIVE,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_MULTITHREADED, STGM_READ,
};

/// Шины, на которых живут встроенные аудиокодеки. Строка берётся из id
/// топологии адаптера вида `{2}.\\?\hdaudio#func_01&ven_10ec...`.
const INTERNAL_BUSES: &[&str] = &["hdaudio#", "intelaudio#", "acpi#"];

fn form_factor(store: &windows::Win32::UI::Shell::PropertiesSystem::IPropertyStore) -> Option<u32> {
    unsafe {
        let value = store.GetValue(&PKEY_AudioEndpoint_FormFactor).ok()?;
        Some(value.Anonymous.Anonymous.Anonymous.ulVal)
    }
}

fn friendly_name(
    store: &windows::Win32::UI::Shell::PropertiesSystem::IPropertyStore,
) -> Option<String> {
    unsafe {
        let value = store.GetValue(&PKEY_Device_FriendlyName).ok()?;
        let pwsz = value.Anonymous.Anonymous.Anonymous.pwszVal;
        if pwsz.is_null() {
            return None;
        }
        pwsz.to_string().ok()
    }
}

/// Id устройства-адаптера, к которому подключён эндпоинт: эндпоинт →
/// коннектор → коннектор на стороне адаптера → его топология → id.
fn adapter_device_id(endpoint: &windows::Win32::Media::Audio::IMMDevice) -> Option<String> {
    unsafe {
        let topology: IDeviceTopology = endpoint.Activate(CLSCTX_ALL, None).ok()?;
        let connector: IConnector = topology.GetConnector(0).ok()?;
        let other_side: IConnector = connector.GetConnectedTo().ok()?;
        let part: IPart = other_side.cast().ok()?;
        let adapter_topology: IDeviceTopology = part.GetTopologyObject().ok()?;
        adapter_topology.GetDeviceId().ok()?.to_string().ok()
    }
}

/// Имена всех встроенных capture-эндпоинтов. Ошибки COM приводят к пустому
/// списку — «встроенный не найден» честнее падения.
fn builtin_input_names() -> Vec<String> {
    let mut names = Vec::new();
    unsafe {
        // На потоках cpal COM уже инициализирован — повторный вызов с другой
        // моделью вернёт RPC_E_CHANGED_MODE, это не ошибка для нас.
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let enumerator: IMMDeviceEnumerator =
            match CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) {
                Ok(e) => e,
                Err(e) => {
                    log::warn!("MMDeviceEnumerator недоступен: {e}");
                    return names;
                }
            };
        let Ok(endpoints) = enumerator.EnumAudioEndpoints(eCapture, DEVICE_STATE_ACTIVE) else {
            return names;
        };
        let count = endpoints.GetCount().unwrap_or(0);
        for i in 0..count {
            let Ok(endpoint) = endpoints.Item(i) else {
                continue;
            };
            let Ok(store) = endpoint.OpenPropertyStore(STGM_READ) else {
                continue;
            };
            let Some(name) = friendly_name(&store) else {
                continue;
            };
            let is_microphone = form_factor(&store) == Some(Microphone.0 as u32);
            let bus = adapter_device_id(&endpoint)
                .unwrap_or_default()
                .to_lowercase();
            let internal = INTERNAL_BUSES.iter().any(|b| bus.contains(b));
            log::debug!(
                "capture endpoint «{name}»: microphone={is_microphone} internal={internal} bus={bus}"
            );
            if is_microphone && internal {
                names.push(name);
            }
        }
    }
    names
}

/// Детектор «встроенности» для общей cpal-реализации. Набор встроенных
/// устройств не меняется за время работы — кэшируем.
pub fn builtin_matcher() -> BuiltinMatcher {
    static NAMES: OnceLock<Vec<String>> = OnceLock::new();
    Box::new(|device_name: &str| {
        NAMES
            .get_or_init(builtin_input_names)
            .iter()
            .any(|n| n == device_name)
    })
}
