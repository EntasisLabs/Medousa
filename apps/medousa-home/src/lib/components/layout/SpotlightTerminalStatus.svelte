<script lang="ts">
  import type { SpotlightSshTargets } from "$lib/spotlight/sshTargets.svelte";
  let { sshMode, data, query, commandError, empty, onManage }: {
    sshMode: boolean;
    data: SpotlightSshTargets | null;
    query: string;
    commandError: string | null;
    empty: boolean;
    onManage: () => void;
  } = $props();
</script>

{#if commandError}<p class="command-spotlight-empty text-content-warning" role="alert">{commandError}</p>{/if}
{#if sshMode && data?.loading}
  <p class="command-spotlight-empty" role="status">Loading saved servers…</p>
{:else if data?.error && (sshMode || /ssh|terminal/i.test(query))}
  <p class="command-spotlight-empty text-content-warning" role="alert">Couldn’t load SSH servers: {data.error}
    <button type="button" class="btn btn-sm variant-ghost-surface" onclick={() => void data?.refresh()}>Retry</button>
  </p>
{:else if sshMode && empty}
  <p class="command-spotlight-empty">{query.trim() ? "No matching servers." : "No saved SSH servers yet."}
    <button type="button" class="btn btn-sm variant-ghost-surface" onclick={onManage}>Manage SSH servers</button>
  </p>
{/if}
