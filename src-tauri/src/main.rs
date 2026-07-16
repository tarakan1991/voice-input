#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod asr;
mod audio;
mod config;
mod dictionary;
mod history;
mod inject;
mod models;
mod platform;
mod postproc;
mod vad;

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    app::run();
}
