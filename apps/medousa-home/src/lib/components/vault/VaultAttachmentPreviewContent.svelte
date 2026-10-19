<script lang="ts">
  import "$lib/styles/files-surface.postcss";
  import { AlertCircle, ExternalLink, LoaderCircle } from "@lucide/svelte";
  import { fileViewKind } from "$lib/files/fileTypes";
  import {
    attachmentPreviewUrl,
    openAttachmentPath,
  } from "$lib/utils/vaultAttachmentPicker";
  import {
    isImageAttachment,
    isSpreadsheetAttachment,
    type VaultAttachment,
  } from "$lib/utils/vaultAttachments";
  import { loadSpreadsheetPreview } from "$lib/utils/spreadsheetPreviewLoader";
  import type { SpreadsheetPreviewData } from "$lib/utils/spreadsheetPreview";
  import VaultSpreadsheetPreview from "./VaultSpreadsheetPreview.svelte";

  interface Props {
    attachment: VaultAttachment;
    /** Fill parent height (library pane / popup) vs compact inline min-height. */
    fill?: boolean;
    editable?: boolean;
  }

  let { attachment, fill = true, editable = false }: Props = $props();

  let previewUrl = $state<string | null>(null);
  let previewError = $state<string | null>(null);
  let spreadsheetData = $state<SpreadsheetPreviewData | null>(null);

  const kind = $derived(fileViewKind(attachment.path));
  $effect(() => {
    const current = attachment;
    let cancelled = false;
    let ownedUrl: string | null = null;
    previewUrl = null; previewError = null; spreadsheetData = null;
    if (["markdown", "text", "pdf"].includes(kind)) return;
    void (async () => {
      try {
        if (isSpreadsheetAttachment(current)) {
          const data = await loadSpreadsheetPreview(current.path);
          if (!cancelled) spreadsheetData = data;
        } else {
          const url = await attachmentPreviewUrl(current.path);
          if (cancelled) { if (url) URL.revokeObjectURL(url); return; }
          ownedUrl = url; previewUrl = url;
          if (!url) previewError = "This file type cannot be previewed here. Use Open in app.";
        }
      } catch (err) {
        if (!cancelled) previewError = err instanceof Error ? err.message : String(err);
      }
    })();
    return () => { cancelled = true; if (ownedUrl) URL.revokeObjectURL(ownedUrl); };
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
  {#if kind === "markdown" || kind === "text"}
    {#key attachment.path}
      {#await import("$lib/files/FileTextDocument.svelte") then { default: FileTextDocument }}
        <FileTextDocument path={attachment.path} {editable} />
      {/await}
    {/key}
  {:else if kind === "pdf"}
    {#key attachment.path}
      {#await import("$lib/files/FilePdfDocument.svelte") then { default: FilePdfDocument }}
        <FilePdfDocument path={attachment.path} />
      {/await}
    {/key}
  {:else if previewError}
    <div class="files-empty">
      <span class="files-empty-icon"><AlertCircle size={22} strokeWidth={1.5} /></span>
      <h3>Couldn't preview this file</h3>
      <p role="alert">{previewError}</p>
      <button
        type="button"
        class="files-button"
        onclick={() => void handleOpenExternal()}
      >
        <ExternalLink size={13} />Open in app
      </button>
    </div>
  {:else if spreadsheetData}
    <VaultSpreadsheetPreview data={spreadsheetData} />
  {:else if previewUrl && isImageAttachment(attachment)}
    <div class="files-image-stage">
      <img
        src={previewUrl}
        alt={attachment.label}
        draggable={false}
        onerror={() => { previewError = "This image could not be decoded. Use Open in app to view it."; }}
      />
    </div>
  {:else}
    <div class="files-empty" role="status">
      <LoaderCircle size={20} class="animate-spin text-content-tertiary" /><p>Opening preview…</p>
    </div>
  {/if}
</div>
