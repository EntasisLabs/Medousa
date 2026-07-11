<script lang="ts">
  import {
    FileSpreadsheet,
    ImagePlus,
    Link2,
    Paperclip,
    Table2,
    X,
  } from "@lucide/svelte";
  import { vault } from "$lib/stores/vault.svelte";
  import {
    attachmentFileName,
    isImageAttachment,
    isSpreadsheetAttachment,
    partitionAttachments,
    type VaultAttachment,
  } from "$lib/utils/vaultAttachments";
  import { openVaultAttachmentContextMenu } from "$lib/utils/vaultContextMenuEvents";

  interface Props {
    disabled?: boolean;
  }

  let { disabled = false }: Props = $props();

  let open = $state(false);

  const attachments = $derived(vault.attachments);
  const count = $derived(attachments.length);
  const isLedger = $derived(vault.selectedKind === "ledger");
  const { spreadsheet, others } = $derived(partitionAttachments(attachments));

  function closeMenu() {
    open = false;
  }

  function handlePreview(attachment: VaultAttachment) {
    vault.previewAttachment(attachment.path, "panel");
    closeMenu();
  }

  function handleRemove(path: string, event: MouseEvent) {
    event.stopPropagation();
    vault.removeAttachment(path);
  }

  function handleInsertEmbed(attachment: VaultAttachment, event: MouseEvent) {
    event.stopPropagation();
    if (!isImageAttachment(attachment)) return;
    void vault.insertImageEmbed(attachment.path);
    closeMenu();
  }

  function handleContextMenu(attachment: VaultAttachment, event: MouseEvent) {
    const notePath = vault.selectedPath;
    if (!notePath) return;
    event.preventDefault();
    event.stopPropagation();
    openVaultAttachmentContextMenu(attachment.path, notePath, event.clientX, event.clientY);
  }

  function attachmentIcon(attachment: VaultAttachment) {
    return isSpreadsheetAttachment(attachment) ? FileSpreadsheet : Paperclip;
  }
</script>

<svelte:window onclick={closeMenu} />

<div class="relative">
  <button
    type="button"
    class="btn btn-sm {open || count > 0 ? 'variant-soft-surface' : 'variant-ghost-surface'}"
    aria-haspopup="menu"
    aria-expanded={open}
    title={count > 0 ? `${count} linked file${count === 1 ? "" : "s"}` : "Linked files"}
    {disabled}
    onclick={(event) => {
      event.stopPropagation();
      open = !open;
    }}
  >
    <Paperclip size={14} strokeWidth={2} />
    {#if count > 0}
      <span class="tabular-nums text-surface-400">{count}</span>
    {/if}
  </button>

  {#if open}
    <div
      class="vault-linked-files-menu absolute right-0 top-full z-30 mt-1 rounded-lg border border-surface-500/50 bg-surface-900 py-1 shadow-xl"
      role="menu"
      onclick={(event) => event.stopPropagation()}
    >
      {#if count === 0}
        <p class="px-3 py-2 text-xs leading-relaxed text-surface-500">
          Link files from your desk without leaving this note.
        </p>
      {/if}

      {#if spreadsheet}
        <p class="px-3 pb-1 pt-2 text-[10px] font-semibold uppercase tracking-wide text-surface-500">
          {isLedger ? "Source" : "Spreadsheet"}
        </p>
        <div
          class="vault-linked-file-row"
          role="presentation"
          oncontextmenu={(event) => handleContextMenu(spreadsheet, event)}
        >
          <button
            type="button"
            class="vault-linked-file-main"
            onclick={() => handlePreview(spreadsheet)}
          >
            <FileSpreadsheet size={14} strokeWidth={2} class="shrink-0 opacity-80" />
            <span class="truncate">{spreadsheet.label || attachmentFileName(spreadsheet)}</span>
          </button>
          <button
            type="button"
            class="vault-linked-file-action"
            aria-label="Remove linked file"
            {disabled}
            onclick={(event) => handleRemove(spreadsheet.path, event)}
          >
            <X size={12} strokeWidth={2} />
          </button>
        </div>
      {/if}

      {#if others.length > 0}
        {#if spreadsheet}
          <p class="px-3 pb-1 pt-2 text-[10px] font-semibold uppercase tracking-wide text-surface-500">
            Files
          </p>
        {/if}
        {#each others as attachment (attachment.path)}
          {@const Icon = attachmentIcon(attachment)}
          <div
            class="vault-linked-file-row"
            role="presentation"
            oncontextmenu={(event) => handleContextMenu(attachment, event)}
          >
            <button
              type="button"
              class="vault-linked-file-main"
              onclick={() => handlePreview(attachment)}
            >
              <Icon size={14} strokeWidth={2} class="shrink-0 opacity-80" />
              <span class="truncate">{attachment.label || attachmentFileName(attachment)}</span>
            </button>
            {#if isImageAttachment(attachment) && vault.editorMode === "edit"}
              <button
                type="button"
                class="vault-linked-file-action"
                aria-label="Insert image in note"
                title="Insert in note"
                {disabled}
                onclick={(event) => handleInsertEmbed(attachment, event)}
              >
                <ImagePlus size={12} strokeWidth={2} />
              </button>
            {/if}
            <button
              type="button"
              class="vault-linked-file-action"
              aria-label="Remove linked file"
              {disabled}
              onclick={(event) => handleRemove(attachment.path, event)}
            >
              <X size={12} strokeWidth={2} />
            </button>
          </div>
        {/each}
      {/if}

      <div class="my-1 border-t border-surface-500/35"></div>

      {#if isLedger && !spreadsheet}
        <button
          type="button"
          role="menuitem"
          class="vault-menu-item"
          {disabled}
          onclick={() => {
            closeMenu();
            void vault.linkSpreadsheetFiles();
          }}
        >
          <Table2 size={14} strokeWidth={2} />
          Link source spreadsheet
        </button>
      {/if}
      <button
        type="button"
        role="menuitem"
        class="vault-menu-item"
        {disabled}
        onclick={() => {
          closeMenu();
          void vault.linkAttachmentFiles();
        }}
      >
        <Link2 size={14} strokeWidth={2} />
        Link file
      </button>
    </div>
  {/if}
</div>
