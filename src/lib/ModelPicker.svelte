<script lang="ts">
  // Выбор и скачивание модели (ASR или LLM): карточки с человеческим
  // описанием, прогресс, докачка, SHA256 на бэкенде, тестовый прогон.
  import { onMount } from "svelte";
  import {
    api,
    events,
    formatBytes,
    type ModelStatus,
    type ModelTestResult,
    type DownloadProgress,
  } from "./ipc";

  let {
    kind,
    selected = null,
    onselect,
  }: {
    kind: "asr" | "llm";
    selected?: string | null;
    onselect: (id: string | null) => void;
  } = $props();

  let models = $state<ModelStatus[]>([]);
  let progress = $state<Record<string, DownloadProgress>>({});
  let testing = $state<Record<string, boolean>>({});
  let testResults = $state<Record<string, ModelTestResult>>({});
  let errors = $state<Record<string, string>>({});

  async function refresh() {
    models = (await api.modelsStatus()).filter((m) => m.kind === kind);
  }

  onMount(() => {
    refresh();
    let unsub: (() => void) | undefined;
    events
      .onDownloadProgress(async (p) => {
        if (!models.some((m) => m.id === p.id)) return;
        if (p.error) {
          errors = { ...errors, [p.id]: p.error };
          const { [p.id]: _, ...rest } = progress;
          progress = rest;
          return;
        }
        if (p.done || p.cancelled) {
          const { [p.id]: _, ...rest } = progress;
          progress = rest;
          await refresh();
          if (p.done) runTest(p.id);
          return;
        }
        progress = { ...progress, [p.id]: p };
      })
      .then((u) => (unsub = u));
    return () => unsub?.();
  });

  function download(id: string) {
    const { [id]: _, ...rest } = errors;
    errors = rest;
    progress = {
      ...progress,
      [id]: {
        id,
        downloaded: 0,
        total: 1,
        done: false,
        cancelled: false,
        error: null,
      },
    };
    api.modelDownload(id);
  }

  async function runTest(id: string) {
    testing = { ...testing, [id]: true };
    try {
      const result = await api.modelTest(id);
      testResults = { ...testResults, [id]: result };
      if (result.ok && !selected) {
        selected = id;
        onselect(id);
      }
    } finally {
      testing = { ...testing, [id]: false };
    }
  }

  async function remove(id: string) {
    await api.modelDelete(id);
    if (selected === id) {
      selected = null;
      onselect(null);
    }
    await refresh();
  }

  function pick(id: string) {
    selected = id;
    onselect(id);
  }
</script>

<div class="models">
  {#each models as m (m.id)}
    <label class="card" class:selected={selected === m.id}>
      <div class="head">
        <input
          type="radio"
          name="model-{kind}"
          checked={selected === m.id}
          disabled={!m.downloaded}
          onchange={() => pick(m.id)}
        />
        <strong>{m.title}</strong>
        {#if m.recommended}<span class="badge">рекомендуем</span>{/if}
        <span class="size">{formatBytes(m.size_bytes)}</span>
      </div>
      <p class="desc">{m.description}</p>

      {#if progress[m.id]}
        {@const p = progress[m.id]}
        <div class="progress-row">
          <progress value={p.downloaded} max={p.total || 1}></progress>
          <span>
            {formatBytes(p.downloaded)} / {formatBytes(p.total)}
          </span>
          <button onclick={() => api.modelDownloadCancel(m.id)}>Пауза</button>
        </div>
      {:else if m.downloaded}
        <div class="actions">
          <span class="ok">✓ скачана</span>
          {#if testing[m.id]}
            <span class="muted">тестовый прогон…</span>
          {:else if testResults[m.id]}
            {@const t = testResults[m.id]}
            {#if t.ok}
              <span class="ok">
                проверена: «{t.text}» за {(t.elapsed_ms / 1000).toFixed(1)} с
              </span>
            {:else}
              <span class="err">тест не прошёл: {t.error}</span>
            {/if}
          {:else}
            <button onclick={() => runTest(m.id)}>Проверить</button>
          {/if}
          <button class="danger" onclick={() => remove(m.id)}>Удалить</button>
        </div>
      {:else}
        <div class="actions">
          <button class="primary" onclick={() => download(m.id)}>
            {m.partial ? "Докачать" : "Скачать"}
          </button>
          {#if errors[m.id]}<span class="err">{errors[m.id]}</span>{/if}
        </div>
      {/if}
    </label>
  {/each}
</div>

<style>
  .models {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .card {
    display: block;
    border: 1px solid var(--border);
    border-radius: 10px;
    padding: 12px 14px;
    background: var(--panel);
    cursor: pointer;
  }
  .card.selected {
    border-color: var(--accent);
    box-shadow: 0 0 0 1px var(--accent);
  }
  .head {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .badge {
    background: var(--accent);
    color: var(--accent-fg);
    border-radius: 5px;
    padding: 1px 7px;
    font-size: 11px;
  }
  .size {
    margin-left: auto;
    color: var(--muted);
    font-size: 12px;
  }
  .desc {
    margin: 6px 0;
    color: var(--muted);
    font-size: 13px;
  }
  .progress-row,
  .actions {
    display: flex;
    align-items: center;
    gap: 10px;
    font-size: 13px;
    flex-wrap: wrap;
  }
  progress {
    flex: 1;
    height: 8px;
  }
  .ok {
    color: var(--ok);
  }
  .err {
    color: var(--danger);
  }
  .muted {
    color: var(--muted);
  }
  button.danger {
    color: var(--danger);
  }
</style>
