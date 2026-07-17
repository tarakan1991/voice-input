<script lang="ts">
  // Мастер первого запуска: 8 шагов (SPEC.md §10.2), язык зафиксирован «ru».
  import { onMount } from "svelte";
  import {
    api,
    events,
    prettyHotkey,
    type AppConfig,
    type PermissionInfo,
    type DictationResult,
    type CloudProvider,
  } from "../lib/ipc";
  import ModelPicker from "../lib/ModelPicker.svelte";
  import HotkeyInput from "../lib/HotkeyInput.svelte";
  import MicPicker from "../lib/MicPicker.svelte";

  let { ondone }: { ondone: () => void } = $props();

  const STEPS = [
    "Разрешения",
    "Распознавание",
    "Вычитка",
    "Микрофон",
    "Хоткей",
    "Тишина",
    "Автозапуск",
    "Проверка",
  ] as const;

  let step = $state(0);
  let config = $state<AppConfig | null>(null);

  // Шаг 1: разрешения
  let permissions = $state<PermissionInfo[]>([]);
  let permTimer: ReturnType<typeof setInterval> | undefined;

  // Шаг 3: вычитка
  let cloudKey = $state("");
  let cloudStatus = $state("");

  // Шаг 7: автозапуск
  let autostart = $state(false);

  // Шаг 8: финальный тест
  let testResult = $state<DictationResult | null>(null);
  let testRunning = $state(false);

  onMount(() => {
    api.configGet().then((c) => (config = c));
    api.autostartStatus().then((v) => (autostart = v)).catch(() => {});
    refreshPermissions();
    permTimer = setInterval(() => {
      if (step === 0) refreshPermissions();
    }, 1500);
    let unsub: (() => void) | undefined;
    events
      .onDictationResult((r) => {
        testResult = r;
        testRunning = false;
      })
      .then((u) => (unsub = u));
    return () => {
      clearInterval(permTimer);
      unsub?.();
    };
  });

  async function refreshPermissions() {
    try {
      permissions = await api.permissionsList();
    } catch (e) {
      console.error(e);
    }
  }

  const permissionsOk = $derived(
    permissions.length > 0 &&
      permissions.every(
        (p) => p.status === "granted" || p.status === "not_applicable",
      ),
  );

  const permName: Record<string, string> = {
    microphone: "Микрофон",
    accessibility: "Универсальный доступ (Accessibility)",
    input_monitoring: "Мониторинг ввода",
  };
  const permHint: Record<string, string> = {
    microphone: "Нужен для записи речи — только на время диктовки.",
    accessibility: "Нужен, чтобы вставлять текст в активное поле (Cmd+V).",
    input_monitoring: "В этой версии не требуется.",
  };

  async function save() {
    if (config) await api.configSet($state.snapshot(config));
  }

  async function next() {
    await save();
    if (step === STEPS.length - 1) {
      await api.wizardComplete();
      ondone();
      api.mainWindowHide();
      return;
    }
    step += 1;
  }

  function runFinalTest() {
    testResult = null;
    testRunning = true;
    api.dictationTest();
  }

  async function saveCloudKey() {
    if (!config) return;
    cloudStatus = "проверяю…";
    try {
      await api.cloudKeySet(config.postproc.cloud_provider, cloudKey);
      await api.cloudValidate(
        config.postproc.cloud_provider,
        config.postproc.cloud_model,
      );
      cloudStatus = "✓ ключ работает";
    } catch (e) {
      cloudStatus = `Ошибка: ${e}`;
    }
  }

  const canProceed = $derived.by(() => {
    if (!config) return false;
    switch (step) {
      case 0:
        return permissionsOk;
      case 1:
        return config.asr_model !== null;
      case 2:
        return (
          config.postproc.mode === "off" ||
          (config.postproc.mode === "local" &&
            config.postproc.local_model !== null) ||
          config.postproc.mode === "cloud"
        );
      case 4:
        return config.hotkey.length > 0;
      case 7:
        return testResult !== null && testResult.error === null;
      default:
        return true;
    }
  });
</script>

{#if config}
  <div class="wizard">
    <header>
      <h1>Настройка VoiceInput</h1>
      <div class="steps">
        {#each STEPS as name, i (name)}
          <span class:active={i === step} class:done={i < step}>{name}</span>
        {/each}
      </div>
    </header>

    <main>
      {#if step === 0}
        <h2>Разрешения macOS</h2>
        <p class="muted">
          Статус проверяется автоматически — выдайте право и вернитесь сюда.
        </p>
        {#each permissions as p (p.permission)}
          <div class="perm">
            <span class="perm-status" data-status={p.status}>
              {p.status === "granted"
                ? "✓"
                : p.status === "not_applicable"
                  ? "—"
                  : "✕"}
            </span>
            <div class="grow">
              <strong>{permName[p.permission]}</strong>
              <p class="muted">{permHint[p.permission]}</p>
            </div>
            {#if p.status !== "granted" && p.status !== "not_applicable"}
              {#if p.status === "not_determined"}
                <button
                  class="primary"
                  onclick={() => api.permissionRequest(p.permission)}
                >
                  Запросить
                </button>
              {/if}
              <button
                onclick={() => api.permissionOpenSettings(p.permission)}
              >
                Открыть настройки
              </button>
            {/if}
          </div>
        {/each}
      {:else if step === 1}
        <h2>Модель распознавания речи</h2>
        <p class="muted">
          Распознавание работает локально и офлайн — аудио не покидает
          машину. После скачивания модель проверяется на тестовой фразе.
        </p>
        <ModelPicker
          kind="asr"
          selected={config.asr_model}
          onselect={(id) => {
            if (config) config.asr_model = id;
          }}
        />
      {:else if step === 2}
        <h2>Вычитка текста</h2>
        <p class="muted">
          Вычитка убирает «ээ», оговорки и расставляет запятые, не меняя
          смысла. Можно локально, через облако или отключить.
        </p>
        <label class="option">
          <input
            type="radio"
            name="pp"
            checked={config.postproc.mode === "local"}
            onchange={() => (config!.postproc.mode = "local")}
          />
          <strong>Локальная модель</strong>
        </label>
        {#if config.postproc.mode === "local"}
          <ModelPicker
            kind="llm"
            selected={config.postproc.local_model}
            onselect={(id) => {
              if (config) config.postproc.local_model = id;
            }}
          />
        {/if}
        <label class="option">
          <input
            type="radio"
            name="pp"
            checked={config.postproc.mode === "cloud"}
            onchange={() => (config!.postproc.mode = "cloud")}
          />
          <div>
            <strong>Облачный API</strong>
            <p class="muted">
              Качество выше, но распознанный текст уходит провайдеру
              (аудио — никогда).
            </p>
          </div>
        </label>
        {#if config.postproc.mode === "cloud"}
          <div class="cloud">
            <select
              value={config.postproc.cloud_provider}
              onchange={(e) =>
                (config!.postproc.cloud_provider = e.currentTarget
                  .value as CloudProvider)}
            >
              <option value="anthropic">Anthropic</option>
              <option value="openai">OpenAI</option>
              <option value="deepseek">DeepSeek</option>
            </select>
            <input
              type="password"
              placeholder="API-ключ"
              bind:value={cloudKey}
            />
            <button onclick={saveCloudKey}>Сохранить и проверить</button>
            {#if cloudStatus}<span class="muted">{cloudStatus}</span>{/if}
          </div>
        {/if}
        <label class="option">
          <input
            type="radio"
            name="pp"
            checked={config.postproc.mode === "off"}
            onchange={() => (config!.postproc.mode = "off")}
          />
          <div>
            <strong>Пока без вычитки</strong>
            <p class="muted">Будет вставляться сырой текст распознавания.</p>
          </div>
        </label>
      {:else if step === 3}
        <h2>Микрофон</h2>
        <MicPicker bind:value={config.microphone} />
      {:else if step === 4}
        <h2>Глобальный хоткей</h2>
        <HotkeyInput bind:value={config.hotkey} />
        <label class="option">
          <input
            type="radio"
            name="hkmode"
            checked={config.hotkey_mode === "toggle"}
            onchange={() => (config!.hotkey_mode = "toggle")}
          />
          <div>
            <strong>Переключение</strong>
            <p class="muted">Нажал — говоришь — нажал ещё раз (или тишина).</p>
          </div>
        </label>
        <label class="option">
          <input
            type="radio"
            name="hkmode"
            checked={config.hotkey_mode === "hold"}
            onchange={() => (config!.hotkey_mode = "hold")}
          />
          <div>
            <strong>Удержание</strong>
            <p class="muted">Говоришь, пока держишь комбинацию; отпустил — стоп.</p>
          </div>
        </label>
      {:else if step === 5}
        <h2>Остановка по тишине</h2>
        <p class="muted">
          Запись остановится сама, если вы молчите дольше порога.
        </p>
        <div class="slider-row">
          <input
            type="range"
            min="2"
            max="15"
            step="0.5"
            bind:value={config.silence_timeout_secs}
          />
          <strong>{config.silence_timeout_secs} с</strong>
        </div>
      {:else if step === 6}
        <h2>Автозапуск</h2>
        <label class="option">
          <input
            type="checkbox"
            checked={autostart}
            onchange={async (e) => {
              autostart = e.currentTarget.checked;
              try {
                await api.autostartSet(autostart);
              } catch (err) {
                console.error(err);
              }
            }}
          />
          <strong>Запускать VoiceInput при входе в систему</strong>
        </label>
      {:else if step === 7}
        <h2>Финальная проверка</h2>
        <p class="muted">
          Нажмите кнопку (или хоткей {prettyHotkey(config.hotkey)}) и скажите
          фразу — например: «Задеплой изменения на стейджинг и создай
          пул-реквест».
        </p>
        <button class="primary" onclick={runFinalTest} disabled={testRunning}>
          {testRunning ? "Слушаю… говорите" : "Начать проверку"}
        </button>
        {#if testResult}
          {#if testResult.error}
            <div class="test-error">
              Проверка не удалась: {testResult.error}
            </div>
          {:else}
            <div class="result">
              <div>
                <h3>Распознано</h3>
                <p>{testResult.raw}</p>
                <span class="muted">{(testResult.asr_ms / 1000).toFixed(1)} с</span>
              </div>
              <div>
                <h3>
                  После вычитки
                  {#if testResult.postproc === "raw"}
                    <span class="muted">(вычитка не применялась)</span>
                  {/if}
                </h3>
                <p>{testResult.clean}</p>
                {#if testResult.postproc !== "raw"}
                  <span class="muted">
                    {(testResult.postproc_ms / 1000).toFixed(1)} с
                  </span>
                {/if}
              </div>
            </div>
          {/if}
        {/if}
      {/if}
    </main>

    <footer>
      {#if step > 0}
        <button onclick={() => (step -= 1)}>Назад</button>
      {/if}
      <div class="spacer"></div>
      <button class="primary" disabled={!canProceed} onclick={next}>
        {step === STEPS.length - 1 ? "Готово" : "Далее"}
      </button>
    </footer>
  </div>
{/if}

<style>
  .wizard {
    display: flex;
    flex-direction: column;
    height: 100vh;
  }
  header {
    padding: 18px 24px 10px;
    border-bottom: 1px solid var(--border);
  }
  h1 {
    margin: 0 0 10px;
  }
  .steps {
    display: flex;
    gap: 6px;
    flex-wrap: wrap;
    font-size: 12px;
  }
  .steps span {
    padding: 3px 9px;
    border-radius: 20px;
    background: var(--panel);
    border: 1px solid var(--border);
    color: var(--muted);
  }
  .steps span.active {
    background: var(--accent);
    border-color: var(--accent);
    color: var(--accent-fg);
  }
  .steps span.done {
    color: var(--ok);
    border-color: var(--ok);
  }
  main {
    flex: 1;
    overflow-y: auto;
    padding: 18px 24px;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  footer {
    display: flex;
    padding: 14px 24px;
    border-top: 1px solid var(--border);
  }
  .spacer {
    flex: 1;
  }
  .muted {
    color: var(--muted);
    font-size: 13px;
    margin: 2px 0;
  }
  .perm {
    display: flex;
    align-items: center;
    gap: 12px;
    border: 1px solid var(--border);
    border-radius: 10px;
    padding: 10px 14px;
  }
  .perm-status {
    font-size: 18px;
    width: 24px;
    text-align: center;
  }
  .perm-status[data-status="granted"] {
    color: var(--ok);
  }
  .perm-status[data-status="denied"],
  .perm-status[data-status="not_determined"] {
    color: var(--danger);
  }
  .grow {
    flex: 1;
  }
  .option {
    display: flex;
    gap: 10px;
    align-items: flex-start;
    border: 1px solid var(--border);
    border-radius: 10px;
    padding: 10px 12px;
    cursor: pointer;
  }
  .cloud {
    display: flex;
    gap: 8px;
    align-items: center;
    flex-wrap: wrap;
    padding-left: 26px;
  }
  .cloud input {
    flex: 1;
    min-width: 200px;
  }
  .slider-row {
    display: flex;
    align-items: center;
    gap: 14px;
  }
  .slider-row input {
    flex: 1;
  }
  .result {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 14px;
  }
  .result > div {
    border: 1px solid var(--border);
    border-radius: 10px;
    padding: 12px;
  }
  .result h3 {
    margin: 0 0 8px;
    font-size: 13px;
    color: var(--muted);
  }
  .result p {
    margin: 0 0 6px;
  }
  .test-error {
    border: 1px solid var(--danger);
    border-radius: 10px;
    padding: 12px;
    color: var(--danger);
  }
</style>
