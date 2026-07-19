<script lang="ts">
  import { parseGraphemeRunResult } from "$lib/grapheme/graphemeRunOutput";
  import type { GraphemeRunResponse } from "$lib/types/grapheme";

  interface Props {
    result?: GraphemeRunResponse["result"] | null;
    error?: string | null;
    emptyMessage?: string;
  }

  let {
    result = null,
    error = null,
    emptyMessage = "Run or compile to see output here.",
  }: Props = $props();

  const parsed = $derived(parseGraphemeRunResult(result));
</script>

{#if error}
  <div class="grapheme-run-console grapheme-run-console-error">
    <div class="grapheme-run-console-status">
      <span class="grapheme-run-console-outcome is-error">Run failed</span>
    </div>
    <pre class="grapheme-run-console-body">{error}</pre>
  </div>
{:else if parsed}
  <div
    class="grapheme-run-console {parsed.succeeded
      ? 'grapheme-run-console-ok'
      : 'grapheme-run-console-error'}"
  >
    <div class="grapheme-run-console-status">
      <span
        class="grapheme-run-console-outcome {parsed.succeeded ? 'is-ok' : 'is-error'}"
      >
        {parsed.headline}
      </span>
      {#if parsed.meta}
        <span class="grapheme-run-console-meta">{parsed.meta}</span>
      {/if}
    </div>
    {#if parsed.summary}
      <pre class="grapheme-run-console-body">{parsed.summary}</pre>
    {/if}
    {#if parsed.details}
      <details class="grapheme-run-console-raw">
        <summary>Raw</summary>
        <pre class="grapheme-run-console-body">{parsed.details}</pre>
      </details>
    {/if}
  </div>
{:else}
  <p class="grapheme-run-console-empty">{emptyMessage}</p>
{/if}
