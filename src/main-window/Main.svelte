<script lang="ts">
  // Главное окно: мастер при первом запуске, дальше вкладки
  // «Настройки»/«История». Закрытие окна прячет его в трей (бэкенд).
  import { onMount } from "svelte";
  import { api, events } from "../lib/ipc";
  import Wizard from "../wizard/Wizard.svelte";
  import Settings from "../settings/Settings.svelte";
  import History from "../history/History.svelte";

  let route = $state<"loading" | "wizard" | "settings" | "history">("loading");

  onMount(() => {
    api.configGet().then((c) => {
      route = c.wizard_completed ? "settings" : "wizard";
    });
    let unsub: (() => void) | undefined;
    events
      .onNavigate((r) => {
        if (r === "settings" || r === "history") route = r;
      })
      .then((u) => (unsub = u));
    return () => unsub?.();
  });
</script>

{#if route === "wizard"}
  <Wizard ondone={() => (route = "settings")} />
{:else if route !== "loading"}
  <div class="main">
    <nav>
      <button
        class:active={route === "settings"}
        onclick={() => (route = "settings")}
      >
        Настройки
      </button>
      <button
        class:active={route === "history"}
        onclick={() => (route = "history")}
      >
        История
      </button>
    </nav>
    {#if route === "settings"}
      <Settings />
    {:else}
      <History />
    {/if}
  </div>
{/if}

<style>
  .main {
    height: 100vh;
    display: flex;
    flex-direction: column;
  }
  nav {
    display: flex;
    gap: 6px;
    padding: 10px 24px;
    border-bottom: 1px solid var(--border);
    position: sticky;
    top: 0;
    background: var(--bg);
    z-index: 1;
  }
  nav button {
    border: none;
    background: transparent;
    color: var(--muted);
    padding: 6px 12px;
  }
  nav button.active {
    background: var(--panel);
    color: var(--fg);
    border: 1px solid var(--border);
  }
  .main > :global(*:last-child) {
    overflow-y: auto;
  }
</style>
