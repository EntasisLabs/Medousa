<script lang="ts">
  import { FileSpreadsheet, Link2, Paperclip } from "@lucide/svelte";
  import { vault } from "$lib/stores/vault.svelte";
  import { attachmentFileName, guessMimeFromPath } from "$lib/utils/vaultAttachments";

  interface Props {
    disabled?: boolean;
  }

  let { disabled = false }: Props = $props();

  const attachments = $derived(vault.attachments);
  const spreadsheet = $derived(
    attachments.find((row) => {
      const mime = row.mime ?? guessMimeFromPath(row.path);
      return (
        mime.includes("spreadsheet") ||
        mime.includes("excel") ||
        row.path.match(/\.(csv|xlsx?|numbers)$/i)
      );
    }),
  );
  const otherFiles = $derived(attachments.filter((row) => row !== spreadsheet));
</script>

<div class="flex flex-wrap items-center gap-x-3 gap-y-1 border-b border-surface-500/30 px-4 py-2 text-xs">
  {#if spreadsheet}
    <button
      type="button"
      class="inline-flex max-w-full items-center gap-1.5 text-primary-200 hover:text-primary-100"
      title={spreadsheet.path}
      {disabled}
      onclick={() => vault.previewAttachment(spreadsheet.path)}
    >
      <FileSpreadsheet size={13} strokeWidth={2} />
      <span class="truncate">{spreadsheet.label || attachmentFileName(spreadsheet)}</span>
    </button>
  {:else}
    <button
      type="button"
      class="vault-attachment-link"
      {disabled}
      onclick={() => void vault.linkSpreadsheetFiles()}
    >
      <FileSpreadsheet size={12} strokeWidth={2} />
      Attach source spreadsheet
    </button>
  {/if}

  {#each otherFiles as file (file.path)}
    <button
      type="button"
      class="inline-flex max-w-[12rem] items-center gap-1 truncate text-surface-400 hover:text-surface-200"
      title={file.path}
      {disabled}
      onclick={() => vault.previewAttachment(file.path)}
    >
      <Paperclip size={11} strokeWidth={2} />
      {file.label || attachmentFileName(file)}
    </button>
  {/each}

  {#if !spreadsheet || otherFiles.length === 0}
    <button
      type="button"
      class="vault-attachment-link"
      {disabled}
      onclick={() => void vault.linkAttachmentFiles()}
    >
      <Link2 size={12} strokeWidth={2} />
      Attach file
    </button>
  {/if}
</div>
