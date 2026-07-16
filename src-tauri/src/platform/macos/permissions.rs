//! Разрешения macOS: микрофон (AVCaptureDevice) и Accessibility (AX API).
//!
//! Input Monitoring в v1 не требуется: toggle-хоткей и CGEventPost работают
//! без него. Вариант остаётся в enum под будущий push-to-talk.

use crate::platform::{Permission, PermissionChecker, PermissionStatus};
use anyhow::{Context, Result};
use block2::RcBlock;
use core_foundation::base::TCFType;
use core_foundation::boolean::CFBoolean;
use core_foundation::dictionary::CFDictionary;
use core_foundation::string::CFString;
use objc2::msg_send;
use objc2::runtime::{AnyClass, Bool};
use objc2_foundation::NSString;
use std::process::Command;

// Класс AVCaptureDevice подтягивается из AVFoundation; линкуем фреймворк,
// чтобы objc-рантайм его знал.
#[link(name = "AVFoundation", kind = "framework")]
extern "C" {}

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXIsProcessTrusted() -> u8;
    fn AXIsProcessTrustedWithOptions(
        options: core_foundation::dictionary::CFDictionaryRef,
    ) -> u8;
    static kAXTrustedCheckOptionPrompt: core_foundation::string::CFStringRef;
}

// AVMediaTypeAudio == @"soun"
const MEDIA_TYPE_AUDIO: &str = "soun";

// AVAuthorizationStatus
const AV_NOT_DETERMINED: isize = 0;
const AV_RESTRICTED: isize = 1;
const AV_DENIED: isize = 2;
const AV_AUTHORIZED: isize = 3;

fn capture_device_class() -> Option<&'static AnyClass> {
    AnyClass::get(c"AVCaptureDevice")
}

fn microphone_status() -> PermissionStatus {
    let Some(cls) = capture_device_class() else {
        log::error!("класс AVCaptureDevice недоступен");
        return PermissionStatus::Denied;
    };
    let media = NSString::from_str(MEDIA_TYPE_AUDIO);
    let status: isize = unsafe { msg_send![cls, authorizationStatusForMediaType: &*media] };
    match status {
        AV_AUTHORIZED => PermissionStatus::Granted,
        AV_DENIED | AV_RESTRICTED => PermissionStatus::Denied,
        AV_NOT_DETERMINED => PermissionStatus::NotDetermined,
        other => {
            log::warn!("неизвестный статус AVCaptureDevice: {other}");
            PermissionStatus::Denied
        }
    }
}

fn request_microphone() {
    let Some(cls) = capture_device_class() else {
        return;
    };
    let media = NSString::from_str(MEDIA_TYPE_AUDIO);
    let handler = RcBlock::new(|granted: Bool| {
        log::info!("доступ к микрофону: {}", granted.as_bool());
    });
    let _: () = unsafe {
        msg_send![cls, requestAccessForMediaType: &*media, completionHandler: &*handler]
    };
}

fn accessibility_trusted() -> bool {
    unsafe { AXIsProcessTrusted() != 0 }
}

/// Показывает системный диалог «разрешите управлять компьютером» и добавляет
/// приложение в список Accessibility.
fn request_accessibility() {
    unsafe {
        let key = CFString::wrap_under_get_rule(kAXTrustedCheckOptionPrompt);
        let options =
            CFDictionary::from_CFType_pairs(&[(key.as_CFType(), CFBoolean::true_value().as_CFType())]);
        AXIsProcessTrustedWithOptions(options.as_concrete_TypeRef());
    }
}

fn settings_url(p: Permission) -> &'static str {
    match p {
        Permission::Microphone => {
            "x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone"
        }
        Permission::Accessibility => {
            "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility"
        }
        Permission::InputMonitoring => {
            "x-apple.systempreferences:com.apple.preference.security?Privacy_ListenEvent"
        }
    }
}

pub struct MacPermissionChecker;

impl PermissionChecker for MacPermissionChecker {
    fn required(&self) -> Vec<Permission> {
        vec![Permission::Microphone, Permission::Accessibility]
    }

    fn status(&self, p: Permission) -> PermissionStatus {
        match p {
            Permission::Microphone => microphone_status(),
            // У AX API нет состояния «не спрашивали» — только да/нет.
            Permission::Accessibility => {
                if accessibility_trusted() {
                    PermissionStatus::Granted
                } else {
                    PermissionStatus::Denied
                }
            }
            Permission::InputMonitoring => PermissionStatus::NotApplicable,
        }
    }

    fn request(&self, p: Permission) -> Result<()> {
        match p {
            Permission::Microphone => request_microphone(),
            Permission::Accessibility => request_accessibility(),
            Permission::InputMonitoring => {}
        }
        Ok(())
    }

    fn open_settings(&self, p: Permission) -> Result<()> {
        Command::new("open")
            .arg(settings_url(p))
            .spawn()
            .context("не удалось открыть Системные настройки")?;
        Ok(())
    }
}
