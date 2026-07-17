# CLAUDE.md — рабочие правила проекта VoiceInput

Десктопное приложение голосового ввода (аналог Wispr Flow): глобальный хоткей →
диктовка по-русски с англицизмами → локальное распознавание (whisper.cpp) →
вычитка (локальная LLM / облако) → вставка в активное поле.
**Продуктовая спецификация — [SPEC.md](SPEC.md), читать перед любой содержательной
работой.** Приоритет №1 проекта: микрофон открыт ТОЛЬКО во время записи.

## Стек (зафиксирован, не обсуждается)

- **Бэкенд**: Rust (stable), Tauri v2.
- **Фронтенд**: TypeScript, Svelte 5, Vite.
- **Ключевые крейты**: `whisper-rs` (Metal), `llama-cpp-2` (Metal), `cpal`,
  `voice_activity_detector` + `ort` (Silero VAD), `rubato`, `arboard`, `rusqlite`,
  `keyring`, `serde`, `thiserror`/`anyhow`, `objc2` (macOS-слой).
- **Плагины Tauri**: global-shortcut, autostart, notification.
- **Платформы**: macOS 12+ (только Apple Silicon) и Windows 10/11 x64.
  Обе платформы собираются и проверяются в CI; `platform/macos/` и
  `platform/windows/` компилируются каждый на своей платформе.

## Структура проекта

```
voice-input/
├── SPEC.md                  # спецификация продукта
├── CLAUDE.md                # этот файл
├── PROGRESS.md              # статус этапов + чеклист ручной приёмки
├── src/                     # фронтенд (TS + Svelte)
│   ├── main.ts              # выбор компонента по label окна
│   ├── App.svelte
│   ├── lib/                 # ipc.ts (типизированные команды/события), общие виджеты
│   ├── overlay/             # плашка + окно кнопки отмены
│   ├── wizard/              # мастер первого запуска
│   ├── settings/            # настройки (вкл. редактор словаря)
│   ├── history/             # история распознаваний
│   └── main-window/         # роутер главного окна (мастер/настройки/история)
├── src-tauri/
│   ├── tauri.conf.json
│   ├── Info.plist           # NSMicrophoneUsageDescription, LSUIElement
│   ├── capabilities/        # права окон (main, overlay, cancel)
│   ├── icons/               # иконка приложения + icons/tray/ (статусы трея)
│   ├── resources/           # вшитый тестовый WAV-сэмпл
│   └── src/
│       ├── main.rs
│       ├── app/             # сессия (машина состояний), трей, оверлей, команды, события
│       ├── audio/           # ресемплинг 16кГц, RMS, звуковые отбивки (синтез)
│       ├── vad/             # VadEngine + Silero, таймер тишины
│       ├── asr/             # whisper-rs, параметры ru, фильтр галлюцинаций
│       ├── postproc/        # вычитка: llama.cpp, облако, guardrails, ключи (keyring)
│       ├── dictionary/      # термины, правила замены, сборка initial_prompt
│       ├── models/          # манифест (SHA256), загрузчик с докачкой, тест-прогон
│       ├── inject/          # оркестрация вставки: буфер, восстановление, фолбэк
│       ├── config/          # конфиг + миграции
│       ├── history/         # SQLite
│       └── platform/
│           ├── mod.rs       # трейты, общие типы, фабрика (ЕДИНСТВЕННОЕ место с cfg)
│           ├── shared/      # реализации на кроссплатформенных библиотеках (cpal, плагины)
│           ├── macos/       # NSPanel, CGEvent, NSWorkspace, TCC-права
│           └── windows/     # WS_EX_NOACTIVATE, SendInput, WASAPI, реестр
└── .github/workflows/       # ci.yml (push), release.yml (теги v*)
```

## Команды

```bash
npm install                # один раз после клонирования
cargo tauri dev            # запуск в dev-режиме (сам поднимает Vite)
cargo tauri build          # сборка .app/.dmg

# Проверки — обязательны перед каждым коммитом:
cargo test    --manifest-path src-tauri/Cargo.toml
cargo clippy  --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo fmt     --manifest-path src-tauri/Cargo.toml -- --check
npm run check              # svelte-check + tsc
```

Нюанс `.dmg`: шаг «украшения» образа управляет Finder через AppleScript и
требует права Automation. В headless-окружении (CI, агенты) сборка падает с
«Время AppleEvent истекло» — запускайте `CI=true cargo tauri build`: bundler
передаст `--skip-jenkins`, dmg соберётся без косметики Finder. На живой машине
достаточно один раз разрешить терминалу управлять Finder.

## Соглашения

### Платформенная граница (главное правило кодовой базы)

- `#[cfg(target_os = ...)]` разрешён **только** в `platform/mod.rs` (фабрика) и
  внутри `platform/macos/`, `platform/windows/`. Появление `cfg(target_os)` в любом
  другом модуле — ошибка ревью. Быстрая проверка:
  `grep -rn "target_os" src-tauri/src --include="*.rs" | grep -v "/platform/"`
  → должно быть пусто.
- Общий код работает с `Arc<dyn Trait>` и типами из `platform/mod.rs`. Типы
  `objc2`/`windows`-крейтов не покидают каталог своей платформы.
- Новая точка касания ОС = новый метод существующего трейта или новый трейт в
  `platform/mod.rs` + реализации в `macos/` И `windows/` — в том же коммите
  (обе платформы живые, полумер в виде no-op не оставлять).

### Микрофонный инвариант

- Захватный стрим живёт только внутри состояний Arming/Recording машины состояний
  и закрывается по RAII (Drop). Не добавлять путей создания стрима в обход
  перехода Idle → Arming. Никаких «прогреть устройство заранее».
- Любое изменение в `audio/` или `platform/*/audio*` требует ручной проверки:
  20 циклов старт/стоп/отмена — системный индикатор микрофона гаснет после каждого.

### Код и тексты

- Код, идентификаторы, сообщения логов — английский. Комментарии — русский.
- UI-тексты — русский, человеческим языком (см. описания моделей в SPEC.md §7 —
  это эталон тона).
- Ошибки: `thiserror` для типизированных ошибок модулей, `anyhow` — на уровне
  оркестрации. Ошибка, видимая пользователю, всегда формулируется по-русски и
  содержит действие («Модель не скачана — откройте Настройки → Распознавание»).
- Диктовка пользователя не логируется (ни сырой текст, ни вычитанный) — только
  в историю (SQLite), которую пользователь может отключить. В логах — длины,
  тайминги, коды ошибок.
- Секреты (API-ключи) — только через `keyring`, никогда в конфиге/логах.

### Git

- Коммиты — **на русском**: заголовок ≤ 72 символов, совершенный вид
  («Добавлен загрузчик моделей», «Исправлено восстановление буфера обмена»),
  тело — по необходимости, что и почему.
- Один коммит — одно логическое изменение. Проверки из раздела «Команды» зелёные.
- Не коммитить: модели, бинарные артефакты, `target/`, `node_modules/`,
  локальные конфиги. Вшитые ресурсы (звуки отбивки, тестовый WAV-сэмпл) — можно.

### Тесты

- Чистые модули (VAD-гейтинг, словарь замен, сборка initial_prompt, guardrails
  вычитки, миграции конфига, манифест моделей) покрываются юнит-тестами;
  аудио-фикстуры — маленькие WAV в `src-tauri/tests/fixtures/`.
- ASR/LLM-смоук: вшитый сэмпл → непустой результат (этот же прогон использует
  мастер). Запускается вручную/в CI по флагу — модели в CI не качаем на каждый push.
- Платформенные реализации тестируются приёмочным чеклистом вручную (ниже).

## Dev-заметки (macOS)

- **llama-cpp-2 обязан собираться с фичей `dynamic-link`** — иначе его копия
  ggml конфликтует с копией из whisper-rs (одинаковые C-символы), whisper
  получает чужой Metal-бэкенд и возвращает пустой текст (NaN-логиты).
  Dylib-библиотеки кладутся в `src-tauri/frameworks/` (beforeBundleCommand
  `npm run sync-dylibs`) и попадают в Contents/Frameworks; rpath задаёт build.rs.
- Смоук-тесты на реальных моделях (обязательны после изменений в asr/postproc
  или обновления whisper-rs/llama-cpp-2):
  `VOICE_INPUT_MODEL=~/Library/Application\ Support/com.vixarev.voiceinput/models/ggml-large-v3-turbo.bin cargo test asr_smoke -- --ignored --nocapture`
  и аналогично `VOICE_INPUT_LLM=<путь к gguf> cargo test llm_smoke -- --ignored --nocapture`.
- Диагностические рычаги: `VOICE_INPUT_NO_GPU=1` (Whisper на CPU),
  `VOICE_INPUT_FLASH_ATTN=1` (flash attention в Whisper).

- **TCC-права (микрофон, Accessibility) привязаны к подписи бинаря** и слетают
  при каждой пересборке с ad-hoc подписью (designated requirement = cdhash
  конкретного бинаря; запись в Настройках остаётся, но указывает на старый
  бинарь — выглядит как «право выдано, а приложение его не видит»).
  Решение уже настроено: в login-keychain лежит самоподписанный сертификат
  **«VoiceInput Dev Signing»**; его designated requirement стабилен
  (identifier + certificate leaf). Локальная сборка с подписью:
  `APPLE_SIGNING_IDENTITY="VoiceInput Dev Signing" CI=true cargo tauri build`
  (или переподписать готовый бандл:
  `codesign --force --deep -s "VoiceInput Dev Signing" /Applications/VoiceInput.app`).
  В tauri.conf.json identity сознательно НЕ прописан — на CI-раннере этого
  сертификата нет, релизные артефакты CI остаются ad-hoc.
  Если права всё же зависли (например, после установки ad-hoc сборки):
  `tccutil reset Accessibility com.vixarev.voiceinput && tccutil reset
  Microphone com.vixarev.voiceinput` и выдать заново.
- `Info.plist` обязан содержать `NSMicrophoneUsageDescription` — иначе краш при
  первом обращении к микрофону.
- Быстрая проверка микрофонного инварианта: оранжевая точка в строке меню +
  Панель управления звуком.

## Dev-заметки (Windows)

- Тулчейн: VS Build Tools (C++ + Windows SDK), CMake, LLVM. bindgen обоих
  -sys-крейтов требует `LIBCLANG_PATH` (обычно `C:\Program Files\LLVM\bin`).
- **llama-cpp-2 собирается с `dynamic-link` и здесь** (риск R-13): llama.cpp
  уходит в DLL (llama, llama-common, ggml, ggml-base, ggml-cpu), у DLL своё
  пространство имён символов, и статический ggml из whisper-rs в exe с ним
  не конфликтует. llama-cpp-sys-2 сам хардлинкает DLL в `target/<profile>/`
  и `deps/`; в инсталлер они попадают как ресурсы из `src-tauri/frameworks/`
  (обновляет `npm run sync-dylibs` перед бандлингом).
- Не собирать проект во временных каталогах (`%TEMP%`): MSBuild отказывает
  (MSB8029/MSB3491); очень длинные пути тоже ломают CMake-шаги.
- **Debug-сборка падает при загрузке модели** (0x80000003 в
  whisper_model_load): llama-cpp-sys-2 в debug линкует отладочный CRT
  (msvcrtd/ucrtbased), а C++-код whisper/llama собран с /MD — две кучи в
  одном процессе. Поэтому на Windows: диктовку проверять `cargo tauri dev
  --release`, смоук-тесты гонять с `--release`.
- Смоук-тесты на реальных моделях (--release обязателен, см. выше):
  `$env:VOICE_INPUT_MODEL="$env:APPDATA\com.vixarev.voiceinput\models\ggml-large-v3-turbo.bin"; cargo test --release asr_smoke -- --ignored --nocapture`
  и аналогично `VOICE_INPUT_LLM` → `cargo test --release llm_smoke -- --ignored --nocapture`.
- Инференс на CPU (Metal-фичи в target-секции macOS; Vulkan — кандидат на
  будущее, потребует Vulkan SDK локально и на CI).
- Быстрая проверка микрофонного инварианта: значок микрофона в трее;
  программно — `LastUsedTimeStop` в
  `HKCU\...\CapabilityAccessManager\ConsentStore\microphone\NonPackaged`
  (0 = занят, время = освобождён).

## Критерии готовности (Definition of Done)

Для любой задачи:

1. Компилируется под macOS **вместе с виндовыми заглушками**; clippy без warnings;
   fmt чистый; тесты зелёные; `npm run check` зелёный.
2. Правило платформенной границы соблюдено (grep-проверка выше).
3. Затронуты инварианты из SPEC.md §2 → соответствующий пункт приёмочного
   чеклиста прогнан вручную и упомянут в описании коммита.

Приёмочный чеклист релиза (SPEC.md §2, прогоняется целиком перед тегом):

- [ ] Микрофон: 20 циклов (стоп по хоткею, по тишине, отмена, ошибка) —
      индикатор гаснет после каждого; вне записи нет открытых input-стримов.
- [ ] Фокус: диктовка в Telegram, браузере (форма), VS Code — оверлей не забирает
      фокус, текст попадает в исходное поле.
- [ ] Буфер: до диктовки скопирован текст и картинка — после вставки содержимое
      восстановлено.
- [ ] Отказоустойчивость: вычитка отключена/сломана → вставлен сырой текст +
      уведомление; смена приложения во время обработки → текст в буфере + история.
- [ ] Латентность: фраза 10–15 с → вставка ≤ 4 с (large-v3-turbo + Qwen2.5-3B).
- [ ] Отбивка: сигнал «говорите» звучит только после реального старта потока.
