<script lang="ts">
  import VaultMarkdownPreview from "$lib/components/vault/VaultMarkdownPreview.svelte";
  import { getVaultNote } from "$lib/daemon";
  import { vaultDisplayTitle } from "$lib/utils/formatVault";

  interface Props {
    notePath: string;
  }

  let { notePath }: Props = $props();

  let content = $state("");
  let error = $state<string | null>(null);
  let loading = $state(true);

  const labelByPath = $derived(new Map([[notePath, vaultDisplayTitle(notePath)]]));

  $effect(() => {
    const path = notePath;
    content = "";
    error = null;
    loading = true;
    void (async () => {
      try {
        const note = await getVaultNote(path);
        content = note.content;
      } catch (err) {
        error = err instanceof Error ? err.message : String(err);
      } finally {
        loading = false;
      }
    })();
  });
</script>

{#if loading}
  <p class="environment-medousa-view-status">Loading note…</p>
{:else if error}
  <p class="environment-medousa-view-error">{error}</p>
{:else}
  <VaultMarkdownPreview {content} {labelByPath} compact={true} />
{/if}

<style>
  .environment-medousa-view-status,
  .environment-medousa-view-error {
    margin: 0;
    padding: 1rem;
    font-size: 0.75rem;
    color: rgb(var(--color-surface-400));
  }

  .environment-medousa-view-error {
    color: rgb(var(--color-warning-300));
  }
</style>
