<script lang="ts">
  import { diffPreviewLines } from "$lib/utils/vaultDiff";

  interface Props {
    before: string;
    after: string;
    maxLines?: number;
  }

  let { before, after, maxLines = 5 }: Props = $props();

  const lines = $derived(diffPreviewLines(before, after, maxLines));
</script>

{#if lines.length > 0}
  <div
    class="vault-proposal-diff max-h-32 overflow-y-auto rounded border border-surface-500/30 bg-surface-950/60 p-2 font-mono text-[11px] leading-relaxed"
    aria-label="Change preview"
  >
    {#each lines as line, index (index)}
      <p
        class="truncate {line.kind === 'add'
          ? 'text-success-300'
          : line.kind === 'remove'
            ? 'text-error-300 line-through'
            : 'text-surface-300'}"
      >
        {line.kind === "add" ? "+ " : line.kind === "remove" ? "- " : "~ "}{line.text}
      </p>
    {/each}
  </div>
{/if}
