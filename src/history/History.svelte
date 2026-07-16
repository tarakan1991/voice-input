<script lang="ts">
  // История распознаваний: локальный SQLite, копирование, очистка.
  import { onMount } from "svelte";
  import { api, type HistoryEntry } from "../lib/ipc";

  let entries = $state<HistoryEntry[]>([]);
  let copied = $state<number | null>(null);

  async function refresh() {
    entries = await api.historyList(200);
  }

  onMount(refresh);

  function fmtDate(ts: number): string {
    return new Date(ts).toLocaleString("ru-RU", {
      day: "2-digit",
      month: "2-digit",
      hour: "2-digit",
      minute: "2-digit",
    });
  }

  const statusText: Record<string, string> = {
    injected: "вставлено",
    left_in_clipboard: "в буфере обмена",
    cancelled: "отменено",
    error: "ошибка",
  };

  async function copy(entry: HistoryEntry) {
    await navigator.clipboard.writeText(entry.clean_text ?? entry.raw_text);
    copied = entry.id;
    setTimeout(() => (copied = null), 1200);
  }

  async function remove(id: number) {
    await api.historyDelete(id);
    await refresh();
  }

  async function clearAll() {
    await api.historyClear();
    await refresh();
  }
</script>

<div class="history">
  <div class="toolbar">
    <button onclick={refresh}>Обновить</button>
    <button class="danger" onclick={clearAll} disabled={entries.length === 0}>
      Очистить всё
    </button>
  </div>

  {#if entries.length === 0}
    <p class="muted">История пуста. Продиктуйте что-нибудь!</p>
  {:else}
    {#each entries as e (e.id)}
      <article>
        <header>
          <span class="muted">{fmtDate(e.ts)}</span>
          {#if e.app_name}<span class="app">{e.app_name}</span>{/if}
          <span class="status">{statusText[e.status] ?? e.status}</span>
          <div class="spacer"></div>
          <button onclick={() => copy(e)}>
            {copied === e.id ? "✓" : "Копировать"}
          </button>
          <button class="danger" onclick={() => remove(e.id)}>✕</button>
        </header>
        <p class="text">{e.clean_text ?? e.raw_text}</p>
        {#if e.clean_text && e.clean_text !== e.raw_text}
          <details>
            <summary class="muted">сырой текст распознавания</summary>
            <p class="raw">{e.raw_text}</p>
          </details>
        {/if}
      </article>
    {/each}
  {/if}
</div>

<style>
  .history {
    padding: 16px 24px 40px;
    max-width: 760px;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .toolbar {
    display: flex;
    gap: 10px;
  }
  article {
    border: 1px solid var(--border);
    border-radius: 10px;
    padding: 10px 14px;
    background: var(--panel);
  }
  article header {
    display: flex;
    align-items: center;
    gap: 10px;
    font-size: 12px;
  }
  .app {
    background: var(--bg);
    border-radius: 5px;
    padding: 1px 7px;
  }
  .status {
    color: var(--muted);
  }
  .spacer {
    flex: 1;
  }
  .text {
    margin: 8px 0 4px;
    white-space: pre-wrap;
  }
  .raw {
    color: var(--muted);
    white-space: pre-wrap;
  }
  .muted {
    color: var(--muted);
  }
  button.danger {
    color: var(--danger);
  }
</style>
