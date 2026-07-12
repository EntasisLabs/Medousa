<script lang="ts">
  import { vault } from "$lib/stores/vault.svelte";
  import { openAttachmentPath } from "$lib/utils/vaultAttachmentPicker";
  import { attachmentFileName } from "$lib/utils/vaultAttachments";
  import VaultAttachmentPreviewContent from "./VaultAttachmentPreviewContent.svelte";

  const attachment = $derived(vault.previewPresentation === "pane" ? vault.previewingAttachment : null);

  async function handleOpenExternal() {
    if (!attachment) return;
    await openAttachmentPath(attachment.path);
  }
</script>

<div class="external-file-library-preview flex h-full min-h-0 min-w-0 flex-1 flex-col">
  {#if !attachment}
    <div class="flex flex-1 items-center justify-center p-6 text-sm text-surface-500">
      Select a file to preview.
    </div>
  {:else}
    <header class="artifact-library-preview-header">
      <div class="min-w-0">
        <h2 class="truncate text-sm font-semibold text-surface-100">
          {attachment.label || attachmentFileName(attachment)}
        </h2>
        <p class="truncate text-xs text-surface-500">{attachment.path}</p>
      </div>
      <div class="flex shrink-0 items-center gap-2">
        <button
          type="button"
          class="artifact-library-action"
          onclick={() => void handleOpenExternal()}
        >
          Open in app
        </button>
        <button
          type="button"
          class="artifact-library-action"
          onclick={() => vault.closeAttachmentPreview()}
        >
          Close
        </button>
      </div>
    </header>

    <div class="artifact-library-preview-body">
      <VaultAttachmentPreviewContent {attachment} fill={true} />
    </div>
  {/if}
</div>

<style>
  .artifact-library-preview-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.75rem;
    border-bottom: 1px solid color-mix(in srgb, var(--color-surface-600) 40%, transparent);
    padding: 0.75rem 1rem;
  }

  .artifact-library-preview-body {
    display: flex;
    min-height: 0;
    flex: 1 1 auto;
    flex-direction: column;
    padding: 0.75rem 1rem 1rem;
  }

  .artifact-library-action {
    display: inline-flex;
    align-items: center;
    gap: 0.35rem;
    border: 1px solid color-mix(in srgb, var(--color-surface-500) 65%, transparent);
    border-radius: 999px;
    padding: 0.35rem 0.65rem;
    font-size: 0.6875rem;
    font-weight: 600;
    color: rgb(var(--color-surface-100));
    background: color-mix(in srgb, var(--color-surface-700) 72%, var(--color-surface-900));
    cursor: pointer;
  }
</style>
