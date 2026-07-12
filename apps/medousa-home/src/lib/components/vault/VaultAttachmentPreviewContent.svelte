<script lang="ts">
  import {
    attachmentPreviewUrl,
    openAttachmentPath,
  } from "$lib/utils/vaultAttachmentPicker";
  import {
    isImageAttachment,
    isPdfAttachment,
    isSpreadsheetAttachment,
    type VaultAttachment,
  } from "$lib/utils/vaultAttachments";
  import { loadSpreadsheetPreview } from "$lib/utils/spreadsheetPreviewLoader";
  import type { SpreadsheetPreviewData } from "$lib/utils/spreadsheetPreview";
  import { isCoLocatedWorkshop } from "$lib/utils/workshopLocality";
  import VaultSpreadsheetPreview from "./VaultSpreadsheetPreview.svelte";

  interface Props {
    attachment: VaultAttachment;
    /** Fill parent height (library pane / popup) vs compact inline min-height. */
    fill?: boolean;
  }

  let { attachment, fill = true }: Props = $props();

  let previewUrl = $state<string | null>(null);
  let previewError = $state<string | null>(null);
  let spreadsheetData = $state<SpreadsheetPreviewData | null>(null);

  $effect(() => {
    const current = attachment;
    previewUrl = null;
    previewError = null;
    spreadsheetData = null;
    if (!current) return;

    if (isSpreadsheetAttachment(current)) {
      void (async () => {
        try {
          spreadsheetData = await loadSpreadsheetPreview(current.path);
        } catch (err) {
          previewError = err instanceof Error ? err.message : String(err);
        }
      })();
      return;
    }

    void (async () => {
      try {
        previewUrl = await attachmentPreviewUrl(current.path);
        if (!previewUrl) {
          previewError = isCoLocatedWorkshop()
            ? "Preview needs a readable file path on this Mac."
            : "Preview for linked files is available on the workshop Mac.";
        }
      } catch (err) {
        previewError = err instanceof Error ? err.message : String(err);
      }
    })();
  });

  async function handleOpenExternal() {
    await openAttachmentPath(attachment.path);
  }
</script>

<div
  class="vault-attachment-preview-content flex min-h-0 flex-1 flex-col {fill
    ? 'h-full'
    : 'min-h-[240px]'}"
>
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
  {:else if spreadsheetData}
    <VaultSpreadsheetPreview data={spreadsheetData} />
  {:else if previewUrl && isPdfAttachment(attachment)}
    <iframe
      class="h-full min-h-0 w-full flex-1 bg-surface-950 {fill ? '' : 'min-h-[320px]'}"
      src={previewUrl}
      title={attachment.label}
    ></iframe>
  {:else if previewUrl && isImageAttachment(attachment)}
    <div class="flex h-full min-h-0 flex-1 items-center justify-center overflow-auto p-4">
      <img
        class="max-h-full max-w-full rounded-md object-contain"
        src={previewUrl}
        alt={attachment.label}
      />
    </div>
  {:else}
    <div class="flex h-full flex-1 items-center justify-center p-6 text-sm text-surface-400">
      Loading preview…
    </div>
  {/if}
</div>
