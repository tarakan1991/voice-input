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
  let holdMode = $state(false);

  function refreshHint() {
    api.configGet().then((c) => {
      hotkey = prettyHotkey(c.hotkey);
      holdMode = c.hotkey_mode === "hold";
    });
  }

  // Столбики «эквалайзера»: автоусиление под голос говорящего (нормируем по
  // скользящему пику, а не по абсолютной шкале — RMS речи с ноутбучного
  // микрофона всего ~0.02–0.15) + быстрая атака и плавное затухание.
  const BARS = 14;
  let bars = $state<number[]>(new Array(BARS).fill(0.08));
  let smoothed = 0;
  let peak = 0.02;

  onMount(() => {
    const unsubs: Array<() => void> = [];
    events.onSessionState((e) => {
      sessionState = e.state;
      detail = e.detail;
      if (e.state !== "recording") level = 0;
      if (e.state !== "recording") countdown = null;
      // Хоткей и режим могли поменяться в настройках — обновляем подсказку.
      if (e.state === "arming") refreshHint();
    }).then((u) => unsubs.push(u));
    events.onAudioLevel((l) => (level = l)).then((u) => unsubs.push(u));
    events
      .onSilenceCountdown((s) => (countdown = s))
      .then((u) => unsubs.push(u));
    refreshHint();

    const timer = setInterval(() => {
      // Скользящий пик: медленно затухает, мгновенно подтягивается к громким
      // моментам. Уровень нормируем относительно него — столбики пляшут на
      // полную при любой громкости голоса и любом усилении микрофона.
      peak = Math.max(peak * 0.985, level, 0.015);
      const norm = Math.min(1, level / peak);
      const target = Math.pow(norm, 0.6);
      // Атака быстрая, затухание плавное — как у VU-метра.
      smoothed =
        target > smoothed
          ? smoothed * 0.3 + target * 0.7
          : smoothed * 0.72 + target * 0.28;
      bars = bars.map((_: number, i: number) => {
        const phase = 0.55 + 0.45 * Math.sin(Date.now() / 80 + i * 1.9);
        return Math.max(0.08, Math.min(1, smoothed * phase));
      });
    }, 50);
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
      <span class="hint">{holdMode ? "отпустите — стоп" : `${hotkey} — стоп`}</span>
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
