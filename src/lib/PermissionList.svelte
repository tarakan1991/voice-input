<script lang="ts">
  // Список системных прав со статусами. Право выдают в системных настройках,
  // поэтому статус переспрашиваем сами, пока список на экране.
  import { onMount } from "svelte";
  import { api, type PermissionInfo } from "./ipc";

  let { onchange }: { onchange?: (perms: PermissionInfo[]) => void } = $props();

  let permissions = $state<PermissionInfo[]>([]);

  const NAME: Record<string, string> = {
    microphone: "Микрофон",
    accessibility: "Универсальный доступ (Accessibility)",
    input_monitoring: "Мониторинг ввода",
  };
  const HINT: Record<string, string> = {
    microphone: "Нужен для записи речи — только на время диктовки.",
    accessibility: "Нужен, чтобы вставлять текст в активное поле (Cmd+V).",
    input_monitoring: "В этой версии не требуется.",
  };

  async function refresh() {
    try {
      permissions = await api.permissionsList();
      onchange?.(permissions);
    } catch (e) {
      console.error(e);
    }
  }

  onMount(() => {
    refresh();
    const timer = setInterval(refresh, 1500);
    return () => clearInterval(timer);
  });
</script>

{#each permissions as p (p.permission)}
  <div class="perm">
    <span class="perm-status" data-status={p.status}>
      {p.status === "granted" ? "✓" : p.status === "not_applicable" ? "—" : "✕"}
    </span>
    <div class="grow">
      <strong>{NAME[p.permission]}</strong>
      <p class="muted">{HINT[p.permission]}</p>
    </div>
    {#if p.status !== "granted" && p.status !== "not_applicable"}
      {#if p.status === "not_determined"}
        <button class="primary" onclick={() => api.permissionRequest(p.permission)}>
          Запросить
        </button>
      {/if}
      <button onclick={() => api.permissionOpenSettings(p.permission)}>
        Открыть настройки
      </button>
    {/if}
  </div>
{/each}

<style>
  .perm {
    display: flex;
    align-items: center;
    gap: 12px;
    border: 1px solid var(--border);
    border-radius: 10px;
    padding: 10px 14px;
    margin-bottom: 10px;
  }
  .perm:last-child {
    margin-bottom: 0;
  }
  .perm-status {
    width: 24px;
    text-align: center;
    font-size: 18px;
    color: var(--muted);
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
  .grow p {
    margin: 2px 0 0;
    font-size: 13px;
  }
</style>
