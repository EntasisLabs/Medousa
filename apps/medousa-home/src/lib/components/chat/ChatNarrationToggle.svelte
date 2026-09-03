<script lang="ts">
  import { onMount } from "svelte";
  import { Volume2, VolumeX } from "@lucide/svelte";
  import { narration } from "$lib/stores/narration.svelte";

  onMount(() => narration.initialize());
</script>

<button
  type="button"
  class="chat-runtime-trigger"
  class:chat-runtime-trigger-open={narration.autoNarrate}
  disabled={!narration.available}
  aria-pressed={narration.autoNarrate}
  aria-label={narration.autoNarrate ? "Turn off automatic narration" : "Narrate replies automatically"}
  title={narration.available
    ? narration.autoNarrate
      ? "Automatic narration on"
      : "Narrate replies automatically"
    : "Narration is unavailable on this device"}
  onclick={() => narration.toggleAutoNarrate()}
>
  {#if narration.autoNarrate}
    <Volume2 size={13} strokeWidth={1.9} class="shrink-0" />
  {:else}
    <VolumeX size={13} strokeWidth={1.9} class="shrink-0 opacity-75" />
  {/if}
  <span class="chat-runtime-trigger-label">Narrate</span>
</button>
