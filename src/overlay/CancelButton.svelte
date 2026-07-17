<script lang="ts">
  // Отдельное окошко кнопки отмены: единственная кликабельная часть оверлея
  // (сама плашка — click-through). На Windows клик дополнительно ловится
  // нативно (WebView2 в неактивирующемся окне не доводит его до DOM) —
  // это делает Rust-слой, см. overlay_ctl и platform::OVERLAY_NATIVE_CLICK_EVENT.
  import { api } from "../lib/ipc";

  function cancel() {
    api.dictationCancel();
  }
</script>

<button class="cancel-root" onclick={cancel} title="Отменить диктовку (Esc)">
  ✕
</button>

<style>
  .cancel-root {
    width: 100vw;
    height: 100vh;
    display: flex;
    align-items: center;
    justify-content: center;
    background: rgba(22, 23, 30, 0.92);
    border: 1px solid rgba(255, 255, 255, 0.12);
    border-radius: 50%;
    color: #f2f2f7;
    font-size: 13px;
    cursor: pointer;
    padding: 0;
  }
  .cancel-root:hover {
    background: rgba(70, 30, 30, 0.95);
  }
</style>
