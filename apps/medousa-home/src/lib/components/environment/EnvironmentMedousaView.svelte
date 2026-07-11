<script lang="ts">
  import VaultMarkdownPreview from "$lib/components/vault/VaultMarkdownPreview.svelte";
  import { getVaultNote } from "$lib/daemon";
  import { layout } from "$lib/stores/layout.svelte";
  import { vault } from "$lib/stores/vault.svelte";
  import { vaultDisplayTitle } from "$lib/utils/formatVault";
  import { resolveWikilinkTarget } from "$lib/utils/resolveWikilink";
  import { ExternalLink } from "@lucide/svelte";

  interface Props {
    notePath: string;
    fill?: boolean;
  }

  let { notePath, fill = false }: Props = $props();

  let content = $state("");
  let error = $state<string | null>(null);
  let loading = $state(true);

  const labelByPath = $derived(new Map([[notePath, vaultDisplayTitle(notePath)]]));
  const displayTitle = $derived(
    vaultDisplayTitle(vault.labelByPathMap.get(notePath) ?? notePath, notePath),
  );

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

  async function openInVault() {
    layout.navigateDesktop("library");
    await vault.openNote(notePath);
  }

  async function followWikilink(raw: string) {
    const resolved = resolveWikilinkTarget(raw, notePath, vault.notes);
    if (!resolved) return;
    layout.navigateDesktop("library");
    await vault.openNote(resolved);
  }
</script>

<div class="environment-medousa-view" class:environment-medousa-view-fill={fill}>
  {#if fill}
    <header class="environment-medousa-view-header">
      <p class="environment-medousa-view-title">{displayTitle}</p>
      <button type="button" class="environment-medousa-view-open" onclick={() => void openInVault()}>
        <ExternalLink size={12} strokeWidth={2} aria-hidden="true" />
        Open in Vault
      </button>
    </header>
  {/if}

  {#if loading}
    <p class="environment-medousa-view-status">Loading note…</p>
  {:else if error}
    <p class="environment-medousa-view-error">{error}</p>
  {:else}
    <div class="environment-medousa-view-body" class:environment-medousa-view-body-fill={fill}>
      <VaultMarkdownPreview
        {content}
        {labelByPath}
        compact={!fill}
        configureViews={false}
        onWikilink={(target) => void followWikilink(target)}
      />
    </div>
  {/if}
</div>

<style>
  .environment-medousa-view {
    display: flex;
    flex-direction: column;
    min-height: 0;
    min-width: 0;
  }

  .environment-medousa-view-fill {
    flex: 1 1 auto;
    height: 100%;
  }

  .environment-medousa-view-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.5rem;
    flex-shrink: 0;
    padding: 0.45rem 0.65rem;
    border-bottom: 1px solid color-mix(in srgb, var(--color-surface-700) 45%, transparent);
    background: color-mix(in srgb, var(--color-surface-900) 55%, transparent);
  }

  .environment-medousa-view-title {
    margin: 0;
    font-size: 0.6875rem;
    font-weight: 600;
    color: rgb(var(--color-surface-200));
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .environment-medousa-view-open {
    display: inline-flex;
    align-items: center;
    gap: 0.25rem;
    flex-shrink: 0;
    border-radius: 999px;
    padding: 0.2rem 0.45rem;
    font-size: 0.625rem;
    color: rgb(var(--color-surface-300));
    background: transparent;
    cursor: pointer;
  }

  .environment-medousa-view-open:hover {
    color: rgb(var(--color-surface-100));
    background: color-mix(in srgb, var(--color-surface-700) 45%, transparent);
  }

  .environment-medousa-view-body {
    min-height: 0;
    min-width: 0;
  }

  .environment-medousa-view-body-fill {
    display: flex;
    flex: 1 1 auto;
    flex-direction: column;
    overflow: auto;
  }

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
