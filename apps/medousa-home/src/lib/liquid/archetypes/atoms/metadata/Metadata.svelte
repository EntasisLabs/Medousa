<script lang="ts">
  /** `metadata` atom — a dot-separated meta line (e.g. cumulative tool names). */
  import type { ArchetypeProps } from "$lib/liquid/render/types";

  let { node }: ArchetypeProps = $props();
  const parts = $derived(
    Array.isArray(node.props.parts)
      ? (node.props.parts as unknown[]).filter((p): p is string => typeof p === "string")
      : [],
  );
</script>

{#if parts.length}
  <p class="liquid-metadata">
    {#each parts as part, index (index)}
      {#if index > 0}<span class="liquid-metadata-sep" aria-hidden="true">·</span>{/if}<span>{part}</span>
    {/each}
  </p>
{/if}

<style>
  .liquid-metadata {
    margin: 0;
    display: flex;
    flex-wrap: wrap;
    gap: 0.3rem;
    font-family: ui-monospace, monospace;
    font-size: 0.625rem;
    color: rgb(var(--color-surface-500));
  }

  .liquid-metadata-sep {
    opacity: 0.6;
  }
</style>
