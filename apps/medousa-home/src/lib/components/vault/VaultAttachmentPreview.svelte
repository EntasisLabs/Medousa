<script lang="ts">
  import { vault } from "$lib/stores/vault.svelte";
  import {
    attachmentPreviewUrl,
    openAttachmentPath,
  } from "$lib/utils/vaultAttachmentPicker";
  import {
    attachmentFileName,
    isImageAttachment,
    isPdfAttachment,
  } from "$lib/utils/vaultAttachments";

  let previewUrl = $state<string | null>(null);
  let previewError = $state<string | null>(null);

  const attachment = $derived(vault.previewingAttachment);

  $effect(() => {
    const current = attachment;
    previewUrl = null;
    previewError = null;
    if (!current) return;
    void (async () => {
      try {
        previewUrl = await attachmentPreviewUrl(current.path);
        if (!previewUrl) {
          previewError = "Preview needs the Medousa Home desktop app and a local file path.";
        }
      } catch (err) {
        previewError = err instanceof Error ? err.message : String(err);
      }
    })();
  });

  async function handleOpenExternal() {
    if (!attachment) return;
    await openAttachmentPath(attachment.path);
  }
</script>

{#if attachment}
  <section
    class="vault-attachment-preview flex min-h-0 flex-col border-b border-surface-500/40 bg-surface-950/80"
    aria-label="Attachment preview"
  >
    <div class="flex items-center justify-between gap-3 border-b border-surface-500/35 px-4 py-2">
      <div class="min-w-0">
        <p class="truncate text-sm font-medium text-surface-100">
          {attachment.label || attachmentFileName(attachment)}
        </p>
        <p class="truncate text-[11px] text-surface-500">{attachment.path}</p>
      </div>
      <div class="flex shrink-0 gap-2">
        <button
          type="button"
          class="btn btn-sm variant-soft-surface"
          onclick={() => void handleOpenExternal()}
        >
          Open in app
        </button>
        <button
          type="button"
          class="btn btn-sm variant-ghost-surface"
          onclick={() => vault.closeAttachmentPreview()}
        >
          Close
        </button>
      </div>
    </div>

    <div class="min-h-[240px] flex-1">
      {#if previewError}
        <div class="flex h-full flex-col items-center justify-center gap-3 p-6 text-center">
          <p class="text-sm text-surface-400">{previewError}</p>
          <button
            type="button"
            class="btn btn-sm variant-soft-primary"
            onclick={() => void handleOpenExternal()}
          >
            Open file
          </button>
        </div>
      {:else if previewUrl && isPdfAttachment(attachment)}
        <iframe
          class="h-full min-h-[320px] w-full bg-surface-950"
          src={previewUrl}
          title={attachment.label}
        ></iframe>
      {:else if previewUrl && isImageAttachment(attachment)}
        <div class="flex h-full items-center justify-center overflow-auto p-4">
          <img
            class="max-h-full max-w-full rounded-md object-contain"
            src={previewUrl}
            alt={attachment.label}
          />
        </div>
      {:else}
        <div class="flex h-full items-center justify-center p-6 text-sm text-surface-400">
          Loading preview…
        </div>
      {/if}
    </div>
  </section>
{/if}
