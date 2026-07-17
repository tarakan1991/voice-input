use std::path::PathBuf;

/// Dylib-библиотеки llama.cpp (dynamic-link, см. Cargo.toml): бандлер кладёт
/// их в Contents/Frameworks. Список должен совпадать с bundle.macOS.frameworks
/// в tauri.conf.json.
const LLAMA_DYLIBS: &[&str] = &[
    "libllama.0.dylib",
    "libllama-common.0.dylib",
    "libggml.0.dylib",
    "libggml-base.0.dylib",
    "libggml-cpu.0.dylib",
    "libggml-metal.0.dylib",
];

fn main() {
    // Проверка платформы здесь — build-скрипт; правило «cfg только в platform/»
    // относится к исходникам приложения.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        // rpath для dyld: внутри .app dylib лежат в Contents/Frameworks,
        // у «голого» бинаря — рядом (llama-cpp-sys-2 хардлинкает их в target/).
        println!("cargo:rustc-link-arg=-Wl,-rpath,@executable_path/../Frameworks");
        println!("cargo:rustc-link-arg=-Wl,-rpath,@executable_path");

        // tauri_build валидирует существование bundle.macOS.frameworks на этапе
        // компиляции, а реальные dylib появляются только после сборки llama.cpp.
        // Кладём в frameworks/ то, что уже собрано, иначе — пустые плейсхолдеры;
        // настоящие файлы копирует beforeBundleCommand перед бандлингом.
        let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
        let frameworks = manifest_dir.join("frameworks");
        std::fs::create_dir_all(&frameworks).ok();
        let target_profile_dir = std::env::var("OUT_DIR")
            .ok()
            .map(PathBuf::from)
            .and_then(|p| p.ancestors().nth(3).map(PathBuf::from));
        for name in LLAMA_DYLIBS {
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
    tauri_build::build();
}
