<script lang="ts">
  // Настройки: все параметры SPEC.md §10.3. Изменения сохраняются сразу.
  import { onMount } from "svelte";
  import {
    api,
    type AppConfig,
    type CloudProvider,
    type ReplacementRule,
  } from "../lib/ipc";
  import ModelPicker from "../lib/ModelPicker.svelte";
  import HotkeyInput from "../lib/HotkeyInput.svelte";
  import MicPicker from "../lib/MicPicker.svelte";

  let config = $state<AppConfig | null>(null);
  let autostart = $state(false);
  let cloudKey = $state("");
  let cloudStatus = $state("");
  let termsText = $state("");
  let saveStatus = $state("");

  onMount(async () => {
    config = await api.configGet();
    termsText = config.dictionary.terms.join("\n");
    api.autostartStatus().then((v) => (autostart = v)).catch(() => {});
  });

  async function save() {
    if (!config) return;
    try {
      await api.configSet($state.snapshot(config));
      saveStatus = "✓ сохранено";
      setTimeout(() => (saveStatus = ""), 1500);
    } catch (e) {
      saveStatus = `Ошибка: ${e}`;
    }
  }

  function saveTerms() {
    if (!config) return;
    config.dictionary.terms = termsText
      .split("\n")
      .map((t) => t.trim())
      .filter(Boolean);
    save();
  }

  function addRule() {
    if (!config) return;
    config.dictionary.replacements.push({
      from: "",
      to: "",
      regex: false,
      ignore_case: true,
    });
  }

  function removeRule(rule: ReplacementRule) {
    if (!config) return;
    config.dictionary.replacements = config.dictionary.replacements.filter(
      (r) => r !== rule,
    );
    save();
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
      cloudKey = "";
    } catch (e) {
      cloudStatus = `Ошибка: ${e}`;
    }
  }
</script>

{#if config}
  <div class="settings">
    <div class="savebar">{saveStatus}</div>

    <section>
      <h2>Хоткей</h2>
      <HotkeyInput bind:value={config.hotkey} onvalid={() => save()} />
      <div class="radio-row" style="margin-top: 10px">
        <label>
          <input
            type="radio"
            name="hkmode"
            checked={config.hotkey_mode === "toggle"}
            onchange={() => {
              config!.hotkey_mode = "toggle";
              save();
            }}
          />
          Переключение — нажал, говоришь, нажал ещё раз (или тишина)
        </label>
        <label>
          <input
            type="radio"
            name="hkmode"
            checked={config.hotkey_mode === "hold"}
            onchange={() => {
              config!.hotkey_mode = "hold";
              save();
            }}
          />
          Удержание — говоришь, пока держишь комбинацию
        </label>
      </div>
    </section>

    <section>
      <h2>Микрофон</h2>
      <MicPicker bind:value={config.microphone} onchange={() => save()} />
    </section>

    <section>
      <h2>Запись</h2>
      <label class="row">
        <span>Тайм-аут тишины: {config.silence_timeout_secs} с</span>
        <input
          type="range"
          min="2"
          max="15"
          step="0.5"
          bind:value={config.silence_timeout_secs}
          onchange={save}
        />
      </label>
      <label class="row">
        <span>Максимум записи, секунд</span>
        <input
          type="number"
          min="30"
          max="1800"
          bind:value={config.max_recording_secs}
          onchange={save}
        />
      </label>
      <label class="row">
        <input
          type="checkbox"
          bind:checked={config.sounds_enabled}
          onchange={save}
        />
        <span>Звуковые отбивки (старт/стоп/отмена)</span>
      </label>
    </section>

    <section>
      <h2>Распознавание (Whisper)</h2>
      <ModelPicker
        kind="asr"
        selected={config.asr_model}
        onselect={(id) => {
          if (config) {
            config.asr_model = id;
            save();
          }
        }}
      />
    </section>

    <section>
      <h2>Вычитка</h2>
      <div class="radio-row">
        {#each [["off", "Выключена"], ["local", "Локальная модель"], ["cloud", "Облачный API"]] as [mode, title] (mode)}
          <label>
            <input
              type="radio"
              name="ppmode"
              checked={config.postproc.mode === mode}
              onchange={() => {
                config!.postproc.mode = mode as "off" | "local" | "cloud";
                save();
              }}
            />
            {title}
          </label>
        {/each}
      </div>
      {#if config.postproc.mode === "local"}
        <ModelPicker
          kind="llm"
          selected={config.postproc.local_model}
          onselect={(id) => {
            if (config) {
              config.postproc.local_model = id;
              save();
            }
          }}
        />
      {:else if config.postproc.mode === "cloud"}
        <div class="cloud">
          <select
            value={config.postproc.cloud_provider}
            onchange={(e) => {
              config!.postproc.cloud_provider = e.currentTarget
                .value as CloudProvider;
              save();
            }}
          >
            <option value="anthropic">Anthropic</option>
            <option value="openai">OpenAI</option>
            <option value="deepseek">DeepSeek</option>
          </select>
          <input
            placeholder="модель (пусто — по умолчанию)"
            bind:value={config.postproc.cloud_model}
            onchange={save}
          />
          <input
            type="password"
            placeholder="новый API-ключ"
            bind:value={cloudKey}
          />
          <button onclick={saveCloudKey}>Сохранить ключ</button>
          {#if cloudStatus}<span class="muted">{cloudStatus}</span>{/if}
        </div>
      {/if}
      <label class="row">
        <span>Таймаут вычитки, секунд (потом вставляется сырой текст)</span>
        <input
          type="number"
          min="3"
          max="60"
          bind:value={config.postproc.timeout_secs}
          onchange={save}
        />
      </label>
    </section>

    <section>
      <h2>Словарь</h2>
      <p class="muted">
        Термины (по одному на строку) подсказываются распознаванию и вычитке:
        имена, названия, англицизмы.
      </p>
      <textarea rows="5" bind:value={termsText} onchange={saveTerms}
      ></textarea>

      <h3>Правила замены</h3>
      <p class="muted">
        Жёсткие замены после распознавания: например «пи ар» → «PR».
      </p>
      {#each config.dictionary.replacements as rule, i (i)}
        <div class="rule">
          <input
            placeholder="что"
            bind:value={rule.from}
            onchange={save}
          />
          <span>→</span>
          <input placeholder="чем" bind:value={rule.to} onchange={save} />
          <label title="Регулярное выражение">
            <input type="checkbox" bind:checked={rule.regex} onchange={save} />
            regex
          </label>
          <label title="Не различать регистр">
            <input
              type="checkbox"
              bind:checked={rule.ignore_case}
              onchange={save}
            />
            Аа
          </label>
          <button class="danger" onclick={() => removeRule(rule)}>✕</button>
        </div>
      {/each}
      <button onclick={addRule}>+ правило</button>
    </section>

    <section>
      <h2>Вставка</h2>
      <div class="radio-row">
        <label>
          <input
            type="radio"
            name="inj"
            checked={config.injection_mode === "clipboard"}
            onchange={() => {
              config!.injection_mode = "clipboard";
              save();
            }}
          />
          Через буфер обмена (рекомендуется)
        </label>
        <label>
          <input
            type="radio"
            name="inj"
            checked={config.injection_mode === "typing"}
            onchange={() => {
              config!.injection_mode = "typing";
              save();
            }}
          />
          Посимвольный ввод
        </label>
      </div>
      <label class="row">
        <span>Пауза до восстановления буфера, мс</span>
        <input
          type="number"
          min="100"
          max="2000"
          step="50"
          bind:value={config.paste_restore_delay_ms}
          onchange={save}
        />
      </label>
    </section>

    <section>
      <h2>История</h2>
      <label class="row">
        <input
          type="checkbox"
          bind:checked={config.history_enabled}
          onchange={save}
        />
        <span>Вести историю распознаваний</span>
      </label>
      <label class="row">
        <span>Лимит записей</span>
        <input
          type="number"
          min="10"
          max="10000"
          bind:value={config.history_limit}
          onchange={save}
        />
      </label>
    </section>

    <section>
      <h2>Система</h2>
      <label class="row">
        <input
          type="checkbox"
          checked={autostart}
          onchange={async (e) => {
            autostart = e.currentTarget.checked;
            try {
              await api.autostartSet(autostart);
            } catch (err) {
              saveStatus = `Ошибка автозапуска: ${err}`;
            }
          }}
        />
        <span>Автозапуск при входе в систему</span>
      </label>
      <label class="row">
        <span>Выгружать модели из памяти после простоя, минут (0 — никогда)</span>
        <input
          type="number"
          min="0"
          max="240"
          bind:value={config.model_idle_unload_mins}
          onchange={save}
        />
      </label>
    </section>
  </div>
{/if}

<style>
  .settings {
    padding: 16px 24px 40px;
    display: flex;
    flex-direction: column;
    gap: 20px;
    max-width: 760px;
  }
  .savebar {
    position: sticky;
    top: 0;
    text-align: right;
    color: var(--ok);
    font-size: 13px;
    min-height: 18px;
  }
  section {
    border: 1px solid var(--border);
    border-radius: 12px;
    padding: 14px 18px;
    background: var(--panel);
  }
  h2 {
    margin: 0 0 10px;
  }
  h3 {
    margin: 14px 0 4px;
    font-size: 14px;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 10px;
    margin: 8px 0;
  }
  .row input[type="range"] {
    flex: 1;
  }
  .row input[type="number"] {
    width: 90px;
  }
  .radio-row {
    display: flex;
    gap: 16px;
    flex-wrap: wrap;
    margin-bottom: 10px;
  }
  .cloud {
    display: flex;
    gap: 8px;
    align-items: center;
    flex-wrap: wrap;
    margin-bottom: 8px;
  }
  .muted {
    color: var(--muted);
    font-size: 13px;
  }
  textarea {
    width: 100%;
    resize: vertical;
  }
  .rule {
    display: flex;
    gap: 8px;
    align-items: center;
    margin: 6px 0;
    flex-wrap: wrap;
  }
  .rule input:not([type="checkbox"]) {
    flex: 1;
    min-width: 120px;
  }
  button.danger {
    color: var(--danger);
  }
</style>
