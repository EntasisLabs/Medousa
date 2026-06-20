<script lang="ts">
  import {
    parseGraphemeRunResult,
  } from "$lib/grapheme/graphemeRunOutput";
  import type { GraphemeRunResponse } from "$lib/types/grapheme";
  import { settings } from "$lib/stores/settings.svelte";

  interface Props {
    result?: GraphemeRunResponse["result"] | null;
    error?: string | null;
    emptyMessage?: string;
  }

  let {
    result = null,
    error = null,
    emptyMessage = "Run your script to see output here.",
  }: Props = $props();

  const parsed = $derived(parseGraphemeRunResult(result));
  const showFriendlyHeadline = $derived(settings.showWorkshopGuidance);
</script>

{#if error}
  <div class="grapheme-run-card grapheme-run-card-error mt-4">
    <p class="text-sm font-medium text-error-300">Run failed</p>
    <p class="mt-1 font-mono text-xs text-error-400/90 whitespace-pre-wrap">{error}</p>
  </div>
{:else if parsed}
  <div class="grapheme-run-card mt-4 {parsed.succeeded ? 'grapheme-run-card-success' : 'grapheme-run-card-error'}">
    {#if showFriendlyHeadline && parsed.headline}
      <p class="text-xs text-surface-300">{parsed.headline}</p>
    {/if}
    {#if parsed.summary}
      <pre class="grapheme-run-output mt-2 max-h-48 overflow-auto rounded-md border border-surface-500/35 p-2 font-mono text-[10px] leading-relaxed text-surface-200 whitespace-pre-wrap">{parsed.summary}</pre>
    {/if}
    {#if parsed.details}
      <details class="mt-2" open={!showFriendlyHeadline}>
        <summary class="workshop-text-action cursor-pointer text-[11px]">
          Raw output
        </summary>
        <pre class="grapheme-run-output mt-2 max-h-48 overflow-auto rounded-md border border-surface-500/35 p-2 font-mono text-[10px] leading-relaxed text-surface-300 whitespace-pre-wrap">{parsed.details}</pre>
      </details>
    {/if}
  </div>
{:else}
  <p class="workshop-muted mt-4 text-sm">{emptyMessage}</p>
{/if}
