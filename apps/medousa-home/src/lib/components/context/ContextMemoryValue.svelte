<script lang="ts">
  import MarkdownContent from "$lib/components/ui/MarkdownContent.svelte";
  import { memoryLabel } from "$lib/utils/contextMemory";
  let { value, depth = 0 }: { value: unknown; depth?: number } = $props();
</script>

{#snippet renderValue(item: unknown, level: number)}
  {#if item === null}<span class="text-content-quiet">Not recorded</span>
  {:else if Array.isArray(item)}
    <ul class="memory-values">{#each item as entry}<li>{@render renderValue(entry, level + 1)}</li>{/each}</ul>
  {:else if typeof item === "object" && level < 16}
    <dl class="memory-fields">{#each Object.entries(item) as [key, entry]}<div><dt>{memoryLabel(key)}</dt><dd>{@render renderValue(entry, level + 1)}</dd></div>{/each}</dl>
  {:else if typeof item === "string"}<MarkdownContent content={item} openLinksInWeb={true} />
  {:else}<span>{typeof item === "boolean" ? item ? "Yes" : "No" : String(item)}</span>{/if}
{/snippet}

<div class="memory-value">{@render renderValue(value, depth)}</div>

<style>
  .memory-value { overflow-wrap: anywhere; line-height: 1.75; }
  .memory-values { list-style: disc; padding-left: 1.2rem; }
  .memory-values li + li { margin-top: .4rem; }
  .memory-fields { display: grid; gap: .85rem; }
  .memory-fields dt { font-size: 12px; color: rgb(var(--theme-text-secondary)); margin-bottom: .2rem; }
  .memory-fields dd { border-left: 1px solid rgb(var(--color-surface-700) / .4); padding-left: .85rem; }
</style>
