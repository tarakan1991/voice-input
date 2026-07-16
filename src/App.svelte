<script lang="ts">
  import Overlay from "./overlay/Overlay.svelte";
  import CancelButton from "./overlay/CancelButton.svelte";
  import Main from "./main-window/Main.svelte";

  let { windowLabel }: { windowLabel: string } = $props();
</script>

{#if windowLabel === "overlay"}
  <Overlay />
{:else if windowLabel === "cancel"}
  <CancelButton />
{:else}
  <Main />
{/if}
