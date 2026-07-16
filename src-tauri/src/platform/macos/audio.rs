//! Точное определение встроенного микрофона через CoreAudio.
//!
//! cpal не отдаёт тип транспорта устройства, поэтому спрашиваем CoreAudio
//! напрямую: перебираем устройства, берём те, у которых транспорт BuiltIn и
//! есть входные стримы, и запоминаем их имена. Имена в cpal приходят из того
//! же CoreAudio-свойства, так что сопоставление по точному равенству надёжно.

use crate::platform::shared::audio::BuiltinMatcher;
use core_foundation::base::TCFType;
use core_foundation::string::{CFString, CFStringRef};
use coreaudio_sys::{
    kAudioDevicePropertyStreams, kAudioDevicePropertyTransportType,
    kAudioDeviceTransportTypeBuiltIn, kAudioHardwarePropertyDevices,
    kAudioObjectPropertyElementMaster, kAudioObjectPropertyName, kAudioObjectPropertyScopeGlobal,
    kAudioObjectPropertyScopeInput, kAudioObjectSystemObject, AudioObjectGetPropertyData,
    AudioObjectGetPropertyDataSize, AudioObjectID, AudioObjectPropertyAddress,
};
use std::sync::OnceLock;

fn builtin_input_names() -> Vec<String> {
    let mut names = Vec::new();
    unsafe {
        let devices_addr = AudioObjectPropertyAddress {
            mSelector: kAudioHardwarePropertyDevices,
            mScope: kAudioObjectPropertyScopeGlobal,
            mElement: kAudioObjectPropertyElementMaster,
        };
        let mut size: u32 = 0;
        if AudioObjectGetPropertyDataSize(
            kAudioObjectSystemObject,
            &devices_addr,
            0,
            std::ptr::null(),
            &mut size,
        ) != 0
        {
            return names;
        }
        let count = size as usize / std::mem::size_of::<AudioObjectID>();
        let mut ids = vec![0 as AudioObjectID; count];
        if AudioObjectGetPropertyData(
            kAudioObjectSystemObject,
            &devices_addr,
            0,
            std::ptr::null(),
            &mut size,
            ids.as_mut_ptr() as *mut _,
        ) != 0
        {
            return names;
        }

        for id in ids {
            // Есть ли у устройства входные стримы
            let input_addr = AudioObjectPropertyAddress {
                mSelector: kAudioDevicePropertyStreams,
                mScope: kAudioObjectPropertyScopeInput,
                mElement: kAudioObjectPropertyElementMaster,
            };
            let mut input_size: u32 = 0;
            if AudioObjectGetPropertyDataSize(id, &input_addr, 0, std::ptr::null(), &mut input_size)
                != 0
                || input_size == 0
            {
                continue;
            }

            // Транспорт — встроенный?
            let transport_addr = AudioObjectPropertyAddress {
                mSelector: kAudioDevicePropertyTransportType,
                mScope: kAudioObjectPropertyScopeGlobal,
                mElement: kAudioObjectPropertyElementMaster,
            };
            let mut transport: u32 = 0;
            let mut tsize = std::mem::size_of::<u32>() as u32;
            if AudioObjectGetPropertyData(
                id,
                &transport_addr,
                0,
                std::ptr::null(),
                &mut tsize,
                &mut transport as *mut u32 as *mut _,
            ) != 0
                || transport != kAudioDeviceTransportTypeBuiltIn
            {
                continue;
            }

            // Имя устройства (то же свойство читает cpal)
            let name_addr = AudioObjectPropertyAddress {
                mSelector: kAudioObjectPropertyName,
                mScope: kAudioObjectPropertyScopeGlobal,
                mElement: kAudioObjectPropertyElementMaster,
            };
            let mut cf_name: CFStringRef = std::ptr::null();
            let mut nsize = std::mem::size_of::<CFStringRef>() as u32;
            if AudioObjectGetPropertyData(
                id,
                &name_addr,
                0,
                std::ptr::null(),
                &mut nsize,
                &mut cf_name as *mut CFStringRef as *mut _,
            ) != 0
                || cf_name.is_null()
            {
                continue;
            }
            let name = CFString::wrap_under_create_rule(cf_name).to_string();
            log::debug!("builtin input device: {name}");
            names.push(name);
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
