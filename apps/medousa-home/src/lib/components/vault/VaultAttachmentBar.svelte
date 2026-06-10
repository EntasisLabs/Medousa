<script lang="ts">
  import { Paperclip, X } from "@lucide/svelte";
  import { vault } from "$lib/stores/vault.svelte";
  import { attachmentFileName } from "$lib/utils/vaultAttachments";

  interface Props {
    disabled?: boolean;
  }

  let { disabled = false }: Props = $props();

  async function handleLinkFiles() {
    await vault.linkAttachmentFiles();
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

    {#if vault.attachments.length === 0}
      <span class="text-xs text-surface-500">Attach PDFs, docs, or spreadsheets from your desk</span>
    {:else}
      {#each vault.attachments as attachment (attachment.path)}
        <div class="inline-flex max-w-full items-center gap-1 rounded-full border border-surface-500/45 bg-surface-800/80 pl-2.5 pr-1">
          <button
            type="button"
            class="truncate text-xs text-primary-200 hover:text-primary-100"
            title={attachment.path}
            onclick={() => vault.previewAttachment(attachment.path)}
          >
            {attachment.label || attachmentFileName(attachment)}
          </button>
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
