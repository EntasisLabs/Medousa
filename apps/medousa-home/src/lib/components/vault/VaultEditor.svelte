<script lang="ts">
  import { vault } from "$lib/stores/vault.svelte";
  import { vaultBreadcrumb, vaultDisplayTitle } from "$lib/utils/formatVault";
  import { renderMarkdownPreview } from "$lib/utils/markdownPreview";

  interface Props {
    visible: boolean;
  }

  let { visible }: Props = $props();

  const displayTitle = $derived(
    vault.selectedPath
      ? (vault.labelByPath().get(vault.selectedPath) ??
        vaultDisplayTitle(vault.title, vault.selectedPath))
      : "Library",
  );

  const breadcrumb = $derived(
    vault.selectedPath ? vaultBreadcrumb(vault.selectedPath) : null,
  );

  const previewHtml = $derived(
    vault.content
      ? renderMarkdownPreview(vault.content, vault.labelByPath())
      : "",
  );

  async function handleSave(event: Event) {
    event.preventDefault();
    await vault.save();
  }
</script>

<section class="flex h-full min-w-0 flex-1 flex-col {visible ? '' : 'hidden'}">
  <header class="workshop-header flex items-center justify-between gap-3 py-3">
    <div class="min-w-0" title={vault.selectedPath ?? undefined}>
      {#if breadcrumb}
        <p class="workshop-faint truncate">{breadcrumb}</p>
      {/if}
      <h1 class="truncate text-base font-semibold">{displayTitle}</h1>
    </div>
    <div class="flex shrink-0 items-center gap-2">
      {#if vault.diffChip()}
        <span class="badge variant-soft-warning text-xs font-mono">
          {vault.diffChip()}
        </span>
      {/if}
      <button
        type="button"
        class="btn btn-sm variant-ghost-surface"
        onclick={() => vault.toggleEditorMode()}
      >
        {vault.editorMode === "edit" ? "Preview" : "Edit"}
      </button>
      <button
        type="button"
        class="btn btn-sm variant-filled-primary"
        disabled={!vault.selectedPath || !vault.dirty || vault.saving}
        onclick={handleSave}
      >
        {vault.saving ? "Saving…" : "Save"}
      </button>
    </div>
  </header>

  {#if vault.error}
    <p class="border-b border-error-500/30 bg-error-500/10 px-4 py-2 text-xs text-error-300">
      {vault.error}
    </p>
  {/if}

  {#if !vault.selectedPath}
    <div class="flex flex-1 items-center justify-center p-8 text-sm text-surface-400">
      Select a note from the tree or search results.
    </div>
  {:else if vault.loading}
    <div class="flex flex-1 items-center justify-center text-sm text-surface-400">
      Loading note…
    </div>
  {:else if vault.editorMode === "edit"}
    <textarea
      class="textarea flex-1 resize-none rounded-none border-0 bg-surface-950 font-mono text-sm leading-relaxed"
      value={vault.content}
      oninput={(event) =>
        vault.markDirty((event.currentTarget as HTMLTextAreaElement).value)}
    ></textarea>
  {:else}
    <article
      class="prose-invert flex-1 overflow-y-auto px-6 py-4 text-sm text-surface-100"
    >
      {@html previewHtml}
    </article>
  {/if}
</section>
