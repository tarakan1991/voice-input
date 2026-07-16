#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod platform;

use tauri::Manager;

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    tauri::Builder::default()
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            let services = platform::create(app.handle())?;
            app.manage(services);
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("ошибка запуска приложения");
}
