<script lang="ts">
  import FilePreviewHeader from "$lib/files/FilePreviewHeader.svelte";
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
    <div class="flex flex-1 items-center justify-center p-6 text-sm text-content-quiet">
      Select a file to preview.
    </div>
  {:else}
    <FilePreviewHeader path={attachment.path} title={attachment.label || attachmentFileName(attachment)} onOpenExternal={handleOpenExternal} onClose={() => vault.closeAttachmentPreview()} />

    <div class="min-h-0 flex-1 overflow-hidden">
      <VaultAttachmentPreviewContent {attachment} fill={true} />
    </div>
  {/if}
</div>
