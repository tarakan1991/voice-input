<script lang="ts">
  // Поле назначения хоткея: ловит комбинацию с клавиатуры и проверяет
  // через бэкенд, что она не занята другим глобальным хоткеем.
  import { api, prettyHotkey } from "./ipc";

  let {
    value = $bindable(""),
    onvalid,
  }: { value: string; onvalid?: (combo: string) => void } = $props();

  let capturing = $state(false);
  let status = $state<"idle" | "ok" | "busy" | "invalid">("idle");
  let message = $state("");
  let btn = $state<HTMLButtonElement | null>(null);

  // Нажатие одного модификатора — это ещё не комбинация: ждём основную клавишу.
  const MODIFIER_CODES = new Set([
    "ShiftLeft",
    "ShiftRight",
    "ControlLeft",
    "ControlRight",
    "AltLeft",
    "AltRight",
    "MetaLeft",
    "MetaRight",
    "CapsLock",
  ]);

  const KEY_MAP: Record<string, string> = {
    Space: "Space",
    Enter: "Enter",
    Tab: "Tab",
    Backspace: "Backspace",
    Escape: "Escape",
    ArrowUp: "Up",
    ArrowDown: "Down",
    ArrowLeft: "Left",
    ArrowRight: "Right",
    Minus: "-",
    Equal: "=",
    Comma: ",",
    Period: ".",
    Slash: "/",
    Backquote: "`",
  };

  function comboFromEvent(e: KeyboardEvent): string | null {
    const mods: string[] = [];
    if (e.ctrlKey) mods.push("Ctrl");
    if (e.altKey) mods.push("Alt");
    if (e.shiftKey) mods.push("Shift");
    if (e.metaKey) mods.push("Cmd");

    let key: string | null = null;
    const code = e.code;
    if (/^Key[A-Z]$/.test(code)) key = code.slice(3);
    else if (/^Digit\d$/.test(code)) key = code.slice(5);
    else if (/^F\d{1,2}$/.test(code)) key = code;
    else if (KEY_MAP[code]) key = KEY_MAP[code];
    if (!key) return null;
    if (mods.length === 0) return null; // без модификаторов — не глобальный хоткей
    return [...mods, key].join("+");
  }

  function startCapture() {
    capturing = true;
    status = "idle";
    message = "";
    btn?.focus();
  }

  async function onKeydown(e: KeyboardEvent) {
    if (!capturing) return;
    e.preventDefault();
    e.stopPropagation();
    if (e.code === "Escape") {
      capturing = false;
      return;
    }
    if (MODIFIER_CODES.has(e.code)) return;
    const combo = comboFromEvent(e);
    if (!combo) {
      status = "invalid";
      message = "Нужна комбинация с модификатором (⌃/⌥/⇧/⌘) и клавишей";
      return;
    }
    capturing = false;
    try {
      await api.hotkeyCheck(combo);
      value = combo;
      status = "ok";
      message = "Комбинация свободна";
      onvalid?.(combo);
    } catch (err) {
      status = "busy";
      message = `Занята: ${err}`;
    }
  }

  // Слушаем окно, а не кнопку: в WKWebView клик по <button> не даёт ему фокус,
  // и keydown на самой кнопке не приходит. Фаза перехвата — чтобы комбинация
  // не досталась остальному интерфейсу. Клик мимо поля отменяет захват.
  $effect(() => {
    if (!capturing) return;
    const onKey = (e: KeyboardEvent) => {
      void onKeydown(e);
    };
    const onPointerDown = (e: MouseEvent) => {
      if (!btn?.contains(e.target as Node)) capturing = false;
    };
    window.addEventListener("keydown", onKey, true);
    window.addEventListener("mousedown", onPointerDown, true);
    return () => {
      window.removeEventListener("keydown", onKey, true);
      window.removeEventListener("mousedown", onPointerDown, true);
    };
  });
</script>

<div class="hotkey">
  <button
    bind:this={btn}
    class="capture"
    class:capturing
    onclick={startCapture}
  >
    {#if capturing}
      нажмите комбинацию… (Esc — отмена)
    {:else}
      {prettyHotkey(value) || "не назначен"}
    {/if}
  </button>
  {#if message}
    <span class:ok={status === "ok"} class:err={status !== "ok"}>
      {message}
    </span>
  {/if}
</div>

<style>
  .hotkey {
    display: flex;
    align-items: center;
    gap: 10px;
    flex-wrap: wrap;
  }
  .capture {
    min-width: 180px;
    font-size: 15px;
  }
  .capture.capturing {
    border-color: var(--accent);
    color: var(--muted);
  }
  .ok {
    color: var(--ok);
    font-size: 13px;
  }
  .err {
    color: var(--danger);
    font-size: 13px;
  }
</style>
