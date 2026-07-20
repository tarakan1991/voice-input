use std::path::{Path, PathBuf};

/// Разделяемые библиотеки llama.cpp (dynamic-link, см. Cargo.toml): бандлер
/// кладёт их внутрь приложения. Списки должны совпадать с
/// bundle.macOS.frameworks в tauri.conf.json и bundle.resources в
/// tauri.windows.conf.json.
const LLAMA_DYLIBS: &[&str] = &[
    "libllama.0.dylib",
    "libllama-common.0.dylib",
    "libggml.0.dylib",
    "libggml-base.0.dylib",
    "libggml-cpu.0.dylib",
    "libggml-metal.0.dylib",
];

/// macOS: tauri_build валидирует существование bundle.macOS.frameworks на
/// этапе компиляции, а реальные dylib появляются только после сборки
/// llama.cpp. Кладём в frameworks/ то, что уже собрано, иначе — пустые
/// плейсхолдеры; настоящие файлы копирует beforeBundleCommand
/// (npm run sync-dylibs) перед бандлингом.
///
/// На Windows таких плейсхолдеров быть НЕ должно: build-скрипт voice-input
/// не зависит от llama-cpp-sys-2 и может выполниться раньше него, а
/// tauri_build копирует ресурсы (frameworks/*.dll) в target/<profile> —
/// пустой плейсхолдер занял бы имя, и хардлинк настоящей DLL из
/// llama-cpp-sys-2 был бы пропущен («файл уже есть»). Поэтому на Windows
/// frameworks/ наполняет только sync-dylibs, из OUT_DIR llama-cpp-sys-2.
fn sync_frameworks_dir(libs: &[&str]) {
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let frameworks = manifest_dir.join("frameworks");
    std::fs::create_dir_all(&frameworks).ok();
    let target_profile_dir = std::env::var("OUT_DIR")
        .ok()
        .map(PathBuf::from)
        .and_then(|p| p.ancestors().nth(3).map(Path::to_path_buf));
    for name in libs {
        let dst = frameworks.join(name);
        let built = target_profile_dir.as_ref().map(|d| d.join(name));
        match built {
            Some(src) if src.exists() => {
                std::fs::remove_file(&dst).ok();
                if let Err(e) = std::fs::copy(&src, &dst) {
                    println!("cargo:warning=не скопирован {name}: {e}");
                }
            }
            _ => {
                if !dst.exists() {
                    std::fs::write(&dst, []).ok();
                }
            }
        }
    }
}

fn main() {
    // Проверка платформы здесь — build-скрипт; правило «cfg только в platform/»
    // относится к исходникам приложения. Windows-ветки нет: DLL ищутся в
    // каталоге exe — ни rpath, ни плейсхолдеры не нужны (см. комментарий
    // у sync_frameworks_dir).
    match std::env::var("CARGO_CFG_TARGET_OS").as_deref() {
        Ok("macos") => {
            // rpath для dyld: внутри .app dylib лежат в Contents/Frameworks,
            // у «голого» бинаря — рядом (llama-cpp-sys-2 хардлинкает их в target/).
            println!("cargo:rustc-link-arg=-Wl,-rpath,@executable_path/../Frameworks");
            println!("cargo:rustc-link-arg=-Wl,-rpath,@executable_path");
            sync_frameworks_dir(LLAMA_DYLIBS);
        }
        Ok("windows") => {
            // vulkan-1.dll приходит с видеодрайвером; delay-load, чтобы на
            // машине без него приложение хотя бы стартовало (ggml-vulkan
            // статически слинкован в exe вместе с whisper).
            println!("cargo:rustc-link-arg=/DELAYLOAD:vulkan-1.dll");
            println!("cargo:rustc-link-arg=delayimp.lib");
        }
        _ => {}
    }
    tauri_build::build();
}
