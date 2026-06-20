<script lang="ts">
  import { Paperclip, Table2, X, ImagePlus } from "@lucide/svelte";
  import { vault } from "$lib/stores/vault.svelte";
  import { attachmentFileName, isImageAttachment } from "$lib/utils/vaultAttachments";
  import {
    bindVaultAttachmentLongPress,
    openVaultAttachmentContextMenu,
  } from "$lib/utils/vaultContextMenuEvents";

  interface Props {
    disabled?: boolean;
  }

  let { disabled = false }: Props = $props();

  const showSpreadsheetLink = $derived(vault.selectedKind === "ledger");

  async function handleLinkFiles() {
    await vault.linkAttachmentFiles();
  }

  async function handleLinkSpreadsheet() {
    await vault.linkSpreadsheetFiles();
  }

  function handleAttachmentContextMenu(
    attachmentPath: string,
    event: MouseEvent,
  ) {
    const notePath = vault.selectedPath;
    if (!notePath) return;
    event.preventDefault();
    event.stopPropagation();
    openVaultAttachmentContextMenu(attachmentPath, notePath, event.clientX, event.clientY);
  }

  function handleInsertEmbed(attachment: { path: string; label: string; mime?: string }) {
    if (!isImageAttachment(attachment)) return;
    void vault.insertImageEmbed(attachment.path);
  }
</script>

{#if vault.selectedPath}
  <div class="vault-attachment-bar flex flex-wrap items-center gap-2 border-b border-surface-500/35 bg-surface-900/40 px-3 py-2">
    <button
      type="button"
      class="btn btn-sm variant-soft-surface"
      {disabled}
      onclick={() => void handleLinkFiles()}
    >
      <Paperclip size={14} strokeWidth={2} />
      Link file
    </button>

    {#if showSpreadsheetLink}
      <button
        type="button"
        class="btn btn-sm variant-soft-primary"
        {disabled}
        onclick={() => void handleLinkSpreadsheet()}
      >
        <Table2 size={14} strokeWidth={2} />
        Link spreadsheet
      </button>
    {/if}

    {#if vault.attachments.length === 0}
      <span class="text-xs text-surface-500">
        {#if showSpreadsheetLink}
          Attach your budget sheet or other files from your desk
        {:else}
          Attach PDFs, docs, or spreadsheets from your desk
        {/if}
      </span>
    {:else}
      {#each vault.attachments as attachment (attachment.path)}
        <div
          class="inline-flex max-w-full items-center gap-1 rounded-full border border-surface-500/45 bg-surface-800/80 pl-2.5 pr-1"
          role="group"
          aria-label="Attachment"
          oncontextmenu={(event) => handleAttachmentContextMenu(attachment.path, event)}
          use:bindVaultAttachmentLongPress={() =>
            vault.selectedPath
              ? { attachmentPath: attachment.path, notePath: vault.selectedPath }
              : null}
        >
          <button
            type="button"
            class="truncate text-xs text-primary-200 hover:text-primary-100"
            title={attachment.path}
            onclick={() => vault.previewAttachment(attachment.path)}
          >
            {attachment.label || attachmentFileName(attachment)}
          </button>
          {#if isImageAttachment(attachment) && vault.editorMode === "edit"}
            <button
              type="button"
              class="inline-flex h-6 w-6 items-center justify-center rounded-full text-surface-500 hover:bg-surface-700 hover:text-primary-200"
              aria-label="Insert image embed"
              title="Insert embed"
              {disabled}
              onclick={() => handleInsertEmbed(attachment)}
            >
              <ImagePlus size={12} strokeWidth={2} />
            </button>
          {/if}
          <button
            type="button"
            class="inline-flex h-6 w-6 items-center justify-center rounded-full text-surface-500 hover:bg-surface-700 hover:text-surface-200"
            aria-label="Remove attachment"
            {disabled}
            onclick={() => vault.removeAttachment(attachment.path)}
          >
            <X size={12} strokeWidth={2} />
          </button>
        </div>
      {/each}
    {/if}
  </div>
{/if}
