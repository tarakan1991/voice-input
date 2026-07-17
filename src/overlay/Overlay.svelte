<script lang="ts">
  // Плашка-оверлей: статус, живой уровень звука, отсчёт тишины, подсказка.
  // Окно неактивирующееся и click-through — только показывает.
  import { onMount } from "svelte";
  import { api, events, prettyHotkey, type SessionState } from "../lib/ipc";

  let sessionState = $state<SessionState>("idle");
  let detail = $state<string | null>(null);
  let level = $state(0);
  let countdown = $state<number | null>(null);
  let hotkey = $state("");

  // Столбики «эквалайзера»: сглаженный уровень с лёгкой рандомизацией фаз.
  const BARS = 14;
  let bars = $state<number[]>(new Array(BARS).fill(0.08));
  let smoothed = 0;

  onMount(() => {
    const unsubs: Array<() => void> = [];
    events.onSessionState((e) => {
      sessionState = e.state;
      detail = e.detail;
      if (e.state !== "recording") level = 0;
      if (e.state !== "recording") countdown = null;
    }).then((u) => unsubs.push(u));
    events.onAudioLevel((l) => (level = l)).then((u) => unsubs.push(u));
    events
      .onSilenceCountdown((s) => (countdown = s))
      .then((u) => unsubs.push(u));
    api.configGet().then((c) => (hotkey = prettyHotkey(c.hotkey)));

    const timer = setInterval(() => {
      // Перцептивная шкала: тихая речь (RMS ~0.03–0.3) должна заметно
      // двигать столбики, иначе кажется, что звук не захватывается.
      const perceptual = Math.min(1, Math.sqrt(level * 6));
      smoothed = smoothed * 0.6 + perceptual * 0.4;
      bars = bars.map((_: number, i: number) => {
        const phase = Math.sin(Date.now() / 90 + i * 1.7) * 0.35 + 0.65;
        return Math.max(0.06, Math.min(1, smoothed * phase));
      });
    }, 66);
    return () => {
      clearInterval(timer);
      unsubs.forEach((u) => u());
    };
  });

  const statusText = $derived(
    sessionState === "error" || sessionState === "notice"
      ? (detail ?? "ошибка")
      : sessionState === "arming"
        ? "подключаю микрофон…"
        : sessionState === "recording"
          ? "говорите"
          : sessionState === "processing"
            ? "обрабатываю…"
            : "",
  );
</script>

<div class="overlay-root" data-state={sessionState}>
  <span class="dot" data-state={sessionState}></span>
  <span class="status" class:error={sessionState === "error"} class:notice={sessionState === "notice"}>
    {statusText}
  </span>

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
  .dot[data-state="error"] {
    background: #ff453a;
  }
  .dot[data-state="notice"] {
    background: #ffd60a;
  }

  @keyframes pulse {
    50% {
      opacity: 0.4;
    }
  }

  .status {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    min-width: 0;
  }
  .status.error {
    color: #ff6b61;
    white-space: normal;
    font-size: 12px;
    line-height: 1.25;
  }
  .status.notice {
    color: #ffd60a;
    white-space: normal;
    font-size: 12px;
    line-height: 1.25;
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
