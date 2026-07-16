<script lang="ts">
  // Плашка-оверлей: статус, живой уровень звука, отсчёт тишины, подсказка.
  // Окно неактивирующееся и click-through — только показывает.
  import { onMount } from "svelte";
  import { api, events, prettyHotkey, type SessionState } from "../lib/ipc";

  let sessionState = $state<SessionState>("idle");
  let level = $state(0);
  let countdown = $state<number | null>(null);
  let hotkey = $state("");

  // Столбики «эквалайзера»: сглаженный уровень с лёгкой рандомизацией фаз.
  const BARS = 14;
  let bars = $state<number[]>(new Array(BARS).fill(0.08));
  let smoothed = 0;

  onMount(() => {
    const unsubs: Array<() => void> = [];
    events.onSessionState((s) => {
      sessionState = s;
      if (s !== "recording") level = 0;
      if (s !== "recording") countdown = null;
    }).then((u) => unsubs.push(u));
    events.onAudioLevel((l) => (level = l)).then((u) => unsubs.push(u));
    events
      .onSilenceCountdown((s) => (countdown = s))
      .then((u) => unsubs.push(u));
    api.configGet().then((c) => (hotkey = prettyHotkey(c.hotkey)));

    const timer = setInterval(() => {
      // Плавное затухание + распределение по столбикам
      smoothed = smoothed * 0.6 + level * 0.4;
      bars = bars.map((_: number, i: number) => {
        const phase = Math.sin(Date.now() / 90 + i * 1.7) * 0.35 + 0.65;
        return Math.max(0.06, Math.min(1, smoothed * 3.2 * phase));
      });
    }, 66);
    return () => {
      clearInterval(timer);
      unsubs.forEach((u) => u());
    };
  });

  const statusText = $derived(
    sessionState === "arming"
      ? "подключаю микрофон…"
      : sessionState === "recording"
        ? "говорите"
        : sessionState === "processing"
          ? "обрабатываю…"
          : sessionState === "error"
            ? "ошибка"
            : "",
  );
</script>

<div class="overlay-root" data-state={sessionState}>
  <span class="dot" data-state={sessionState}></span>
  <span class="status">{statusText}</span>

  {#if sessionState === "recording"}
    <div class="bars">
      {#each bars as b, i (i)}
        <span class="bar" style="height: {6 + b * 26}px"></span>
      {/each}
    </div>
    {#if countdown !== null}
      <span class="countdown">стоп через {Math.ceil(countdown)}</span>
    {:else}
      <span class="hint">{hotkey} — стоп</span>
    {/if}
  {:else if sessionState === "processing"}
    <div class="spinner"></div>
  {/if}
</div>

<style>
  .overlay-root {
    height: 100vh;
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 0 18px;
    background: rgba(22, 23, 30, 0.92);
    border: 1px solid rgba(255, 255, 255, 0.12);
    border-radius: 14px;
    color: #f2f2f7;
    font-size: 14px;
    overflow: hidden;
    user-select: none;
  }

  .dot {
    width: 10px;
    height: 10px;
    border-radius: 50%;
    flex-shrink: 0;
    background: #8e8e93;
  }
  .dot[data-state="recording"] {
    background: #ff453a;
    animation: pulse 1.2s ease-in-out infinite;
  }
  .dot[data-state="arming"] {
    background: #ffd60a;
  }
  .dot[data-state="processing"] {
    background: #ff9f0a;
  }

  @keyframes pulse {
    50% {
      opacity: 0.4;
    }
  }

  .status {
    white-space: nowrap;
  }

  .bars {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 3px;
    height: 36px;
  }
  .bar {
    width: 4px;
    border-radius: 2px;
    background: #6d87f0;
    transition: height 60ms linear;
  }

  .countdown {
    color: #ffd60a;
    white-space: nowrap;
    font-variant-numeric: tabular-nums;
  }

  .hint {
    color: #98989f;
    font-size: 12px;
    white-space: nowrap;
  }

  .spinner {
    width: 16px;
    height: 16px;
    margin-left: auto;
    border: 2px solid rgba(255, 255, 255, 0.25);
    border-top-color: #f2f2f7;
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
</style>
