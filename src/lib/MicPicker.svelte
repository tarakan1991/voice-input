<script lang="ts">
  // Выбор микрофона. Дефолт — «всегда встроенный»: Bluetooth-гарнитура
  // никогда не переключается в HFP и не портит звук.
  import { onMount } from "svelte";
  import { api, type AudioDevice, type MicSelection } from "./ipc";

  let {
    value = $bindable({ kind: "always_builtin" } as MicSelection),
    onchange,
  }: { value: MicSelection; onchange?: (v: MicSelection) => void } = $props();

  let devices = $state<AudioDevice[]>([]);
  let builtin = $state<AudioDevice | null>(null);

  onMount(async () => {
    try {
      devices = await api.devicesList();
      builtin = await api.builtinDevice();
    } catch (e) {
      console.error("устройства не перечислились", e);
    }
  });

  function set(v: MicSelection) {
    value = v;
    onchange?.(v);
  }
</script>

<div class="mic">
  <label class="option">
    <input
      type="radio"
      name="mic"
      checked={value.kind === "always_builtin"}
      disabled={!builtin}
      onchange={() => set({ kind: "always_builtin" })}
    />
    <div>
      <strong>Всегда встроенный микрофон</strong>
      {#if builtin}<span class="muted">({builtin.name})</span>{/if}
      <p class="muted">
        Рекомендуем: Bluetooth-наушники не будут переключаться в режим
        звонка и портить звук — даже во время диктовки.
      </p>
    </div>
  </label>

  <label class="option">
    <input
      type="radio"
      name="mic"
      checked={value.kind === "system_default"}
      onchange={() => set({ kind: "system_default" })}
    />
    <div>
      <strong>Системный по умолчанию</strong>
      <p class="muted">Каким пользуется система в данный момент.</p>
    </div>
  </label>

  <label class="option">
    <input
      type="radio"
      name="mic"
      checked={value.kind === "device"}
      disabled={devices.length === 0}
      onchange={() => {
        const first = devices[0];
        if (first) set({ kind: "device", id: first.id });
      }}
    />
    <div class="grow">
      <strong>Конкретное устройство</strong>
      {#if value.kind === "device"}
        <select
          value={value.id}
          onchange={(e) => set({ kind: "device", id: e.currentTarget.value })}
        >
          {#each devices as d (d.id)}
            <option value={d.id}>
              {d.name}{d.is_builtin ? " (встроенный)" : ""}
            </option>
          {/each}
        </select>
      {/if}
    </div>
  </label>
</div>

<style>
  .mic {
    display: flex;
    flex-direction: column;
    gap: 10px;
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
  .option input {
    margin-top: 3px;
  }
  .muted {
    color: var(--muted);
    font-size: 13px;
    margin: 4px 0 0;
  }
  .grow {
    flex: 1;
  }
  select {
    margin-top: 6px;
    width: 100%;
  }
</style>
