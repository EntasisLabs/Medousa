<script lang="ts">
  /** `whisper` atom — a muted stage-direction line above the main voice. */
  import type { ArchetypeProps } from "$lib/liquid/render/types";

  let { node }: ArchetypeProps = $props();
  const text = $derived(typeof node.props.text === "string" ? node.props.text : "");
</script>

{#if text}
  <p class="liquid-whisper">{text}</p>
{/if}

<style>
  .liquid-whisper {
    margin: 0;
    font-size: 0.75rem;
    font-style: italic;
    color: color-mix(in srgb, rgb(var(--color-surface-300)) 80%, transparent);
  }
</style>
