<script lang="ts">
  import { codeRuntimeIssue } from "$lib/code/codeRuntimeIssue";
  let { message }: { message: string } = $props();
  const issue = $derived(codeRuntimeIssue(message));
</script>
<div class="shrink-0 border-b border-amber-500/25 bg-amber-500/5 px-3 py-2 text-chrome-sm text-amber-100" role="status">
  <p class="font-medium">{issue.summary}</p>
  {#if issue.guidance}<p class="mt-1 leading-relaxed text-content-secondary">{issue.guidance}</p>{/if}
  <details class="mt-1 text-content-quiet">
    <summary class="cursor-pointer">Details</summary>
    <pre class="mt-2 max-h-40 overflow-auto whitespace-pre-wrap break-words font-mono text-chrome-xs">{issue.details}</pre>
  </details>
</div>
