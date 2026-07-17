//! Машина состояний сессии диктовки (SPEC.md §4.4).
//!
//! Idle → Arming → Recording → Processing → Injecting → Idle.
//! Инвариант №1 обеспечен конструкцией: захватный стрим живёт только внутри
//! `run_dictation` и закрывается по RAII на любом пути выхода, включая паники.

use crate::app::events;
use crate::app::state::ConfigStore;
use crate::asr::AsrEngine;
use crate::audio::{self, cues};
use crate::config::{AppConfig, MicSelection, PostprocMode};
use crate::history::{History, HistoryEntry};
use crate::inject::{self, InjectionOutcome};
use crate::models::ModelStore;
use crate::platform::{DeviceSelector, FocusSnapshot, PlatformServices};
use crate::postproc::{self, local::LocalLlm};
use crate::vad::{self, SilenceTracker, SilenceVerdict, VadEngine};
use anyhow::{Context, Result};
use crossbeam_channel::{select, unbounded, Receiver, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter};
use tauri_plugin_notification::NotificationExt;

const ESC_COMBO: &str = "Escape";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionCommand {
    /// Хоткей (режим «переключение»): старт записи или стоп с обработкой.
    Toggle,
    /// Хоткей (режим «удержание»): комбинация нажата — начать запись.
    HoldPressed,
    /// Хоткей (режим «удержание»): комбинация отпущена — стоп с обработкой.
    HoldReleased,
    /// Отмена без вставки (Esc, кнопка ✕, трей).
    Cancel,
    /// Тестовая диктовка мастера: результат событием, без вставки.
    Test,
    Shutdown,
}

#[derive(Clone)]
pub struct SessionHandle {
    tx: Sender<SessionCommand>,
}

impl SessionHandle {
    pub fn toggle(&self) {
        let _ = self.tx.send(SessionCommand::Toggle);
    }
    pub fn hold_pressed(&self) {
        let _ = self.tx.send(SessionCommand::HoldPressed);
    }
    pub fn hold_released(&self) {
        let _ = self.tx.send(SessionCommand::HoldReleased);
    }
    pub fn cancel(&self) {
        let _ = self.tx.send(SessionCommand::Cancel);
    }
    pub fn test(&self) {
        let _ = self.tx.send(SessionCommand::Test);
    }
    pub fn shutdown(&self) {
        let _ = self.tx.send(SessionCommand::Shutdown);
    }
}

pub struct SessionDeps {
    pub app: AppHandle,
    pub services: PlatformServices,
    pub config: Arc<ConfigStore>,
    pub history: Arc<History>,
    pub store: Arc<ModelStore>,
    pub asr: Arc<AsrEngine>,
    pub llm: Arc<LocalLlm>,
}

pub fn spawn(deps: SessionDeps) -> SessionHandle {
    let (tx, rx) = unbounded::<SessionCommand>();
    let handle = SessionHandle { tx };
    let self_handle = handle.clone();
    std::thread::Builder::new()
        .name("dictation-session".into())
        .spawn(move || session_loop(deps, rx, self_handle))
        .expect("не удалось создать поток сессии");
    handle
}

fn session_loop(deps: SessionDeps, rx: Receiver<SessionCommand>, self_handle: SessionHandle) {
    loop {
        // Idle: ждём команду.
        let (test_mode, hold_mode) = match rx.recv() {
            Ok(SessionCommand::Toggle) => (false, false),
            Ok(SessionCommand::HoldPressed) => (false, true),
            Ok(SessionCommand::Test) => (true, false),
            // Отпускание в Idle — хвост предыдущей записи, игнорируем.
            Ok(SessionCommand::Cancel) | Ok(SessionCommand::HoldReleased) => continue,
            Ok(SessionCommand::Shutdown) | Err(_) => return,
        };
        match run_dictation(&deps, &rx, &self_handle, test_mode, hold_mode) {
            Ok(()) => {
                crate::app::tray::set_state(&deps.app, crate::app::tray::TrayState::Idle);
            }
            Err(e) => {
                log::error!("диктовка завершилась ошибкой: {e:#}");
                // Иконка остаётся «ошибка» до следующего действия.
                crate::app::tray::set_state(&deps.app, crate::app::tray::TrayState::Error);
                let config = deps.config.get();
                if config.sounds_enabled {
                    cues::play(cues::Cue::Error);
                }
                if test_mode {
                    emit_test_error(&deps.app, &format!("{e}"));
                }
                notify(&deps.app, &format!("Диктовка не удалась: {e}"));
                // Ошибку показываем на плашке: системные уведомления могут
                // быть выключены, а молча ронять диктовку нельзя.
                show_on_overlay(&deps, "error", &format!("{e}"), 4);
            }
        }
        emit_state(&deps.app, "idle");
        crate::app::overlay_ctl::hide(&deps.app, &deps.services);
    }
}

enum StopReason {
    /// Обычная остановка: распознать и вставить.
    Process,
    /// Отмена пользователем.
    Cancelled,
    /// Тишина с самого начала — речи не было.
    NoSpeech,
    Shutdown,
}

fn run_dictation(
    deps: &SessionDeps,
    rx: &Receiver<SessionCommand>,
    self_handle: &SessionHandle,
    test_mode: bool,
    hold_mode: bool,
) -> Result<()> {
    let config = deps.config.get();
    let app = &deps.app;

    // ФОКУС запоминается ДО показа оверлея (SPEC.md §11).
    let snapshot = deps.services.focus.snapshot()?;

    emit_state(app, "arming");
    crate::app::tray::set_state(app, crate::app::tray::TrayState::Recording);

    let selector = match &config.microphone {
        MicSelection::AlwaysBuiltin => DeviceSelector::Builtin,
        MicSelection::SystemDefault => DeviceSelector::Default,
        MicSelection::Device(id) => DeviceSelector::ById(id.clone()),
    };

    // ---- Arming: открываем поток. Стрим живёт только в этой функции. ----
    let (chunk_tx, chunk_rx) = unbounded::<(Vec<f32>, u32, u16)>();
    let stream = deps
        .services
        .audio
        .open(
            &selector,
            Box::new(move |data, rate, ch| {
                let _ = chunk_tx.send((data.to_vec(), rate, ch));
            }),
        )
        .context("не удалось открыть микрофон")?;

    // Поток реально пошёл — честная отбивка «говорите» (инвариант №5).
    if config.sounds_enabled {
        cues::play(cues::Cue::Start);
    }
    emit_state(app, "recording");
    crate::app::overlay_ctl::show(app, &deps.services);

    // Esc — глобальная отмена только на время записи.
    let esc_session = self_handle.clone();
    let esc_registered = deps
        .services
        .hotkey
        .register(
            ESC_COMBO,
            Box::new(move |event| {
                if event == crate::platform::HotkeyEvent::Pressed {
                    esc_session.cancel();
                }
            }),
        )
        .is_ok();

    let mut vad_engine = vad::SileroVad::new()?;
    let mut tracker = SilenceTracker::new(config.silence_timeout_secs);
    let mut native_buf: Vec<f32> = Vec::new();
    let mut native_rate: u32 = 0;
    let mut vad_resampler: Option<audio::StreamingResampler> = None;
    let mut vad_stream: Vec<f32> = Vec::new();
    let mut vad_consumed = 0usize;
    let started = Instant::now();
    let max_duration = Duration::from_secs(config.max_recording_secs as u64);
    let mut last_level = Instant::now();
    let mut countdown_shown = false;

    let reason = 'record: loop {
        select! {
            recv(rx) -> cmd => match cmd {
                Ok(SessionCommand::Toggle) => break 'record StopReason::Process,
                // Повторные нажатия при удержании игнорируем.
                Ok(SessionCommand::HoldPressed) => {}
                Ok(SessionCommand::HoldReleased) => {
                    if hold_mode {
                        break 'record StopReason::Process;
                    }
                }
                Ok(SessionCommand::Cancel) => break 'record StopReason::Cancelled,
                Ok(SessionCommand::Test) => {} // уже записываем
                Ok(SessionCommand::Shutdown) | Err(_) => break 'record StopReason::Shutdown,
            },
            recv(chunk_rx) -> chunk => {
                let Ok((data, rate, channels)) = chunk else {
                    break 'record StopReason::Process; // поток умер — обработаем что есть
                };
                let mono = audio::mix_to_mono(&data, channels);
                if native_rate == 0 {
                    native_rate = rate;
                    vad_resampler = Some(audio::StreamingResampler::new(rate));
                }
                // Уровень для плашки, не чаще 20 раз в секунду.
                if last_level.elapsed() > Duration::from_millis(50) {
                    let _ = app.emit(events::AUDIO_LEVEL, events::AudioLevelPayload {
                        level: audio::rms(&mono).min(1.0),
                    });
                    last_level = Instant::now();
                }
                native_buf.extend_from_slice(&mono);
                if let Some(rs) = vad_resampler.as_mut() {
                    rs.push(&mono, &mut vad_stream);
                }
                while vad_stream.len() - vad_consumed >= vad::VAD_CHUNK {
                    let chunk16 = &vad_stream[vad_consumed..vad_consumed + vad::VAD_CHUNK];
                    vad_consumed += vad::VAD_CHUNK;
                    let prob = vad_engine.predict(chunk16).unwrap_or(1.0);
                    match tracker.update(prob, vad::VAD_CHUNK_SECS) {
                        SilenceVerdict::Continue => {
                            if countdown_shown {
                                countdown_shown = false;
                                let _ = app.emit(events::SILENCE_COUNTDOWN,
                                    events::SilencePayload { seconds_left: None });
                            }
                        }
                        SilenceVerdict::Countdown(left) => {
                            countdown_shown = true;
                            let _ = app.emit(events::SILENCE_COUNTDOWN,
                                events::SilencePayload { seconds_left: Some(left) });
                        }
                        SilenceVerdict::TimedOut { had_speech: true } =>
                            break 'record StopReason::Process,
                        SilenceVerdict::TimedOut { had_speech: false } =>
                            break 'record StopReason::NoSpeech,
                    }
                }
                if started.elapsed() > max_duration {
                    log::warn!("достигнут максимум записи, останавливаю");
                    break 'record StopReason::Process;
                }
            },
            default(Duration::from_millis(200)) => {
                if started.elapsed() > max_duration {
                    break 'record StopReason::Process;
                }
            },
        }
    };

    // Случайное короткое нажатие (особенно в режиме удержания) — тихая отмена:
    // за треть секунды без речи распознавать нечего.
    let reason = if matches!(reason, StopReason::Process)
        && started.elapsed() < Duration::from_millis(350)
        && !tracker.had_speech()
    {
        StopReason::Cancelled
    } else {
        reason
    };

    // SPEC §4.4: «речи не было вообще» → Idle без обработки. Раньше это
    // работало только для автостопа по тишине; стоп хоткеем без единого
    // слова гонял Whisper по чистой тишине — на CPU (Windows) это минуты
    // «обрабатываю» из-за случайного двойного нажатия.
    let reason = if matches!(reason, StopReason::Process) && !tracker.had_speech() {
        StopReason::NoSpeech
    } else {
        reason
    };

    // ---- Микрофон закрывается ЗДЕСЬ, до любой обработки (инвариант №1). ----
    stream.close();
    if esc_registered {
        let _ = deps.services.hotkey.unregister(ESC_COMBO);
    }
    let _ = app.emit(
        events::SILENCE_COUNTDOWN,
        events::SilencePayload { seconds_left: None },
    );

    match reason {
        StopReason::Shutdown => return Ok(()),
        StopReason::Cancelled => {
            if config.sounds_enabled {
                cues::play(cues::Cue::Cancel);
            }
            return Ok(());
        }
        StopReason::NoSpeech => {
            if config.sounds_enabled {
                cues::play(cues::Cue::Cancel);
            }
            let message = "Речи не слышно — диктовка отменена. Проверьте, тот ли микрофон выбран";
            if test_mode {
                emit_test_error(app, message);
            }
            notify(app, message);
            show_on_overlay(deps, "notice", message, 3);
            return Ok(());
        }
        StopReason::Process => {}
    }

    if config.sounds_enabled {
        cues::play(cues::Cue::Stop);
    }
    emit_state(app, "processing");
    crate::app::tray::set_state(app, crate::app::tray::TrayState::Processing);
    crate::app::overlay_ctl::set_processing(app);

    let duration_ms = started.elapsed().as_millis() as i64;
    process_and_inject(
        deps,
        &config,
        native_buf,
        native_rate,
        tracker.trailing_silence_secs(),
        &snapshot,
        duration_ms,
        test_mode,
    )
}

#[allow(clippy::too_many_arguments)]
fn process_and_inject(
    deps: &SessionDeps,
    config: &AppConfig,
    native_buf: Vec<f32>,
    native_rate: u32,
    trailing_silence_secs: f32,
    snapshot: &FocusSnapshot,
    duration_ms: i64,
    test_mode: bool,
) -> Result<()> {
    let app = &deps.app;
    if native_buf.is_empty() || native_rate == 0 {
        let message = "Запись пуста — микрофон не отдал ни одного сэмпла";
        if test_mode {
            emit_test_error(app, message);
        }
        notify(app, message);
        show_on_overlay(deps, "notice", message, 3);
        return Ok(());
    }

    // Ресемплинг в 16 кГц и обрезка хвостовой тишины (оставляем 0.3 с).
    let mut samples = audio::resample_to_16k(&native_buf, native_rate)?;
    let keep_tail = 0.3_f32;
    if trailing_silence_secs > keep_tail {
        let cut = ((trailing_silence_secs - keep_tail) * audio::TARGET_RATE as f32) as usize;
        if cut < samples.len() {
            samples.truncate(samples.len() - cut);
        }
    }

    // ---- ASR ----
    let model_id = config
        .asr_model
        .as_deref()
        .context("модель распознавания не выбрана — откройте Настройки")?;
    let model_path = deps.store.downloaded_path(model_id)?;
    let prompt = config.dictionary.initial_prompt();
    let asr_out =
        deps.asr
            .transcribe(&model_path, model_id, &samples, &config.language, &prompt)?;
    if asr_out.text.is_empty() {
        let message = "Не удалось разобрать речь — попробуйте ещё раз";
        if test_mode {
            emit_test_error(app, message);
        }
        notify(app, message);
        show_on_overlay(deps, "notice", message, 3);
        return Ok(());
    }

    // Словарь до вычитки.
    let raw = config.dictionary.apply(&asr_out.text);

    // ---- Вычитка (не роняет диктовку: любой сбой → сырой текст) ----
    let postproc_started = Instant::now();
    let (mut text, used) = run_postproc(deps, config, &raw);
    let postproc_ms = postproc_started.elapsed().as_millis() as u64;
    // Словарь после вычитки: LLM могла «починить» термин обратно.
    text = config.dictionary.apply(&text);

    if test_mode {
        let _ = app.emit(
            events::DICTATION_RESULT,
            events::DictationResultPayload {
                raw,
                clean: text,
                postproc: used,
                asr_ms: asr_out.elapsed_ms,
                postproc_ms,
                error: None,
            },
        );
        return Ok(());
    }

    // ---- Вставка ----
    let outcome = inject::inject(&deps.services, config, &text, snapshot)?;
    let status = match outcome {
        InjectionOutcome::Injected => "injected",
        InjectionOutcome::LeftInClipboard => {
            let message = "Фокус ушёл в другое приложение — текст в буфере обмена";
            notify(app, message);
            show_on_overlay(deps, "notice", message, 3);
            "left_in_clipboard"
        }
        InjectionOutcome::BlockedSecureInput => {
            let message = "Активно поле пароля — вставка невозможна, текст в буфере обмена";
            notify(app, message);
            show_on_overlay(deps, "notice", message, 3);
            "left_in_clipboard"
        }
    };

    if config.history_enabled {
        let entry = HistoryEntry {
            id: 0,
            ts: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_millis() as i64)
                .unwrap_or(0),
            raw_text: raw,
            clean_text: if used == "raw" { None } else { Some(text) },
            app_name: Some(snapshot.app_name.clone()),
            duration_ms: Some(duration_ms),
            model: Some(model_id.to_string()),
            status: status.to_string(),
        };
        if let Err(e) = deps.history.insert(&entry, config.history_limit) {
            log::warn!("не удалось записать историю: {e}");
        }
    }
    Ok(())
}

/// Вычитка по конфигу. Возвращает (текст, чем получен: local|cloud|raw).
fn run_postproc(deps: &SessionDeps, config: &AppConfig, raw: &str) -> (String, &'static str) {
    let timeout = Duration::from_secs(config.postproc.timeout_secs.max(3));
    let terms = config.dictionary.terms_for_llm();
    let system = postproc::system_prompt(&terms);

    let attempt: Result<(String, &'static str)> = match config.postproc.mode {
        PostprocMode::Off => return (raw.to_string(), "raw"),
        PostprocMode::Local => (|| {
            let model_id = config
                .postproc
                .local_model
                .as_deref()
                .context("модель вычитки не выбрана")?;
            let path = deps.store.downloaded_path(model_id)?;
            let cleaned =
                deps.llm
                    .cleanup(&path, model_id, &system, postproc::few_shot(), raw, timeout)?;
            Ok((cleaned, "local"))
        })(),
        PostprocMode::Cloud => (|| {
            let provider = config.postproc.cloud_provider;
            let key = postproc::keys::get_api_key(provider)?
                .context("API-ключ не задан — откройте Настройки → Вычитка")?;
            let cleaned = postproc::cloud::cleanup_via_cloud(
                provider,
                &config.postproc.cloud_model,
                &key,
                &system,
                raw,
                timeout,
            )?;
            Ok((cleaned, "cloud"))
        })(),
    };

    match attempt {
        Ok((cleaned, used)) => match postproc::apply_guardrails(raw, &cleaned) {
            Some(text) => (text, used),
            None => (raw.to_string(), "raw"),
        },
        Err(e) => {
            log::warn!("вычитка не удалась, вставляю сырой текст: {e:#}");
            notify(&deps.app, "Вычитка недоступна — вставлен сырой текст");
            (raw.to_string(), "raw")
        }
    }
}

fn emit_state(app: &AppHandle, state: &'static str) {
    let _ = app.emit(
        events::SESSION_STATE,
        events::SessionStatePayload {
            state,
            detail: None,
        },
    );
}

/// Показывает сообщение на плашке `secs` секунд (state: "error" | "notice").
/// Оверлей уже видим во время диктовки; после паузы session_loop его спрячет.
fn show_on_overlay(deps: &SessionDeps, state: &'static str, message: &str, secs: u64) {
    let _ = deps.app.emit(
        events::SESSION_STATE,
        events::SessionStatePayload {
            state,
            detail: Some(message.to_string()),
        },
    );
    std::thread::sleep(Duration::from_secs(secs));
}

/// Результат тестовой диктовки с ошибкой — мастер показывает причину
/// вместо вечного «слушаю».
fn emit_test_error(app: &AppHandle, message: &str) {
    let _ = app.emit(
        events::DICTATION_RESULT,
        events::DictationResultPayload {
            raw: String::new(),
            clean: String::new(),
            postproc: "raw",
            asr_ms: 0,
            postproc_ms: 0,
            error: Some(message.to_string()),
        },
    );
}

fn notify(app: &AppHandle, body: &str) {
    if let Err(e) = app
        .notification()
        .builder()
        .title("VoiceInput")
        .body(body)
        .show()
    {
        log::warn!("не удалось показать уведомление: {e}");
    }
}
