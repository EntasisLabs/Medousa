<script lang="ts">
  import { onMount } from "svelte";
  import { Copy, Download, FilePlus2, Pencil, RotateCcw, Share2, X } from "@lucide/svelte";
  import BodyPortal from "$lib/components/ui/BodyPortal.svelte";
  import ChatDrawingDialog from "$lib/components/chat/ChatDrawingDialog.svelte";
  import { createVaultNote, readMediaBytes } from "$lib/daemon";
  import { chat } from "$lib/stores/chat.svelte";
  import type { ChatMediaAttachment } from "$lib/types/media";
  import { chatImageObjectUrl } from "$lib/utils/chatImagePreview";
  import { drawDocumentFromBytes } from "$lib/draw/drawMedia";
  import { serializeDrawFence, type DrawDocument } from "$lib/draw/drawDocument";

  interface Props {
    sessionId: string;
    attachments: ChatMediaAttachment[];
    compact?: boolean;
  }

  let { sessionId, attachments, compact = false }: Props = $props();

  let urls = $state<Record<string, string>>({});
  let preview = $state<{ url: string; attachment: ChatMediaAttachment } | null>(null);
  let editOpen = $state(false);
  let editDocument = $state.raw<DrawDocument | null>(null);
  let actionBusy = $state(false);
  let actionStatus = $state<string | null>(null);

  $effect(() => {
    const activeSessionId = sessionId;
    const images = attachments
      .filter((attachment) => attachment.mime.startsWith("image/"))
      .map((attachment) => ({ ...attachment }));
    let cancelled = false;
    let ownedUrls: string[] = [];
    urls = {};
    preview = null;
    void (async () => {
      const next: Record<string, string> = {};
      const created: string[] = [];
      for (const attachment of images) {
        try {
          const payload = await readMediaBytes(activeSessionId, attachment.mediaId);
          const url = await chatImageObjectUrl(
            payload.bytes,
            payload.mime,
            attachment.label,
          );
          created.push(url);
          next[attachment.mediaId] = url;
        } catch {
          // Thumbnail optional — chip still shows label.
        }
      }
      if (cancelled) {
        created.forEach((url) => URL.revokeObjectURL(url));
        return;
      }
      ownedUrls = created;
      urls = next;
    })();
    return () => {
      cancelled = true;
      ownedUrls.forEach((url) => URL.revokeObjectURL(url));
    };
  });

  onMount(() => {
    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key === "Escape") preview = null;
    };
    window.addEventListener("keydown", closeOnEscape);
    return () => window.removeEventListener("keydown", closeOnEscape);
  });

  function closeFromBackdrop(event: MouseEvent) {
    if (event.target === event.currentTarget) preview = null;
  }

  async function loadDrawing(attachment: ChatMediaAttachment): Promise<DrawDocument> {
    if (!attachment.editableSourceId) throw new Error("This drawing has no editable source.");
    const payload = await readMediaBytes(sessionId, attachment.editableSourceId);
    return drawDocumentFromBytes(payload.bytes);
  }

  async function editDrawing(attachment: ChatMediaAttachment) {
    if (actionBusy) return;
    actionBusy = true;
    actionStatus = null;
    try {
      editDocument = await loadDrawing(attachment);
      editOpen = true;
    } catch (reason) {
      actionStatus = reason instanceof Error ? reason.message : String(reason);
    } finally {
      actionBusy = false;
    }
  }

  function safeName(value: string): string {
    return value.trim().toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "").slice(0, 48) || "drawing";
  }

  async function saveDrawingToVault(attachment: ChatMediaAttachment) {
    if (actionBusy) return;
    actionBusy = true;
    actionStatus = null;
    try {
      const document = await loadDrawing(attachment);
      const title = attachment.label.trim() || "Drawing";
      const path = `Drawings/${safeName(title)}-${Date.now().toString(36)}.md`;
      await createVaultNote(path, `---\nkind: draw\ntitle: ${JSON.stringify(title)}\n---\n\n# ${title}\n\n${serializeDrawFence(document)}\n`, { sessionId });
      actionStatus = `Saved to ${path}`;
    } catch (reason) {
      actionStatus = reason instanceof Error ? reason.message : String(reason);
    } finally {
      actionBusy = false;
    }
  }

  function downloadPreview(url: string, label: string) {
    const anchor = document.createElement("a");
    anchor.href = url;
    anchor.download = `${safeName(label)}.png`;
    anchor.click();
  }

  function bytesToBase64(bytes: Uint8Array): string {
    let binary = "";
    const chunkSize = 0x8000;
    for (let offset = 0; offset < bytes.length; offset += chunkSize) {
      binary += String.fromCharCode(...bytes.subarray(offset, offset + chunkSize));
    }
    return btoa(binary);
  }

  async function copyImage(attachment: ChatMediaAttachment) {
    if (actionBusy) return;
    actionBusy = true;
    actionStatus = null;
    try {
      if (!navigator.clipboard?.write || typeof ClipboardItem === "undefined") {
        throw new Error("Image copying isn’t available on this device.");
      }
      const payload = await readMediaBytes(sessionId, attachment.mediaId);
      const blob = new Blob([payload.bytes], { type: payload.mime });
      await navigator.clipboard.write([new ClipboardItem({ [payload.mime]: blob })]);
      actionStatus = "Image copied.";
    } catch (reason) {
      actionStatus = reason instanceof Error ? reason.message : String(reason);
    } finally {
      actionBusy = false;
    }
  }

  async function shareImage(attachment: ChatMediaAttachment) {
    if (actionBusy) return;
    actionBusy = true;
    actionStatus = null;
    try {
      const payload = await readMediaBytes(sessionId, attachment.mediaId);
      const file = new File([payload.bytes], `${safeName(attachment.label)}.png`, { type: payload.mime });
      if (!navigator.share || (navigator.canShare && !navigator.canShare({ files: [file] }))) {
        throw new Error("Image sharing isn’t available on this device.");
      }
      await navigator.share({ title: attachment.label, files: [file] });
    } catch (reason) {
      if (reason instanceof DOMException && reason.name === "AbortError") return;
      actionStatus = reason instanceof Error ? reason.message : String(reason);
    } finally {
      actionBusy = false;
    }
  }

  async function saveGeneratedToVault(attachment: ChatMediaAttachment) {
    if (actionBusy) return;
    actionBusy = true;
    actionStatus = null;
    try {
      const payload = await readMediaBytes(sessionId, attachment.mediaId);
      const title = attachment.label.trim() || "Generated image";
      const path = `Generated images/${safeName(title)}-${Date.now().toString(36)}.md`;
      const dataUrl = `data:${payload.mime};base64,${bytesToBase64(payload.bytes)}`;
      const lineage = attachment.parentGenerationId
        ? `parent_generation_id: ${JSON.stringify(attachment.parentGenerationId)}\n`
        : "";
      await createVaultNote(path, `---\nkind: generated-image\ntitle: ${JSON.stringify(title)}\ngeneration_id: ${JSON.stringify(attachment.generationId ?? "")}\n${lineage}---\n\n# ${title}\n\n![${title.replace(/[\[\]]/g, "")}](${dataUrl})\n`, { sessionId });
      actionStatus = `Saved to ${path}`;
    } catch (reason) {
      actionStatus = reason instanceof Error ? reason.message : String(reason);
    } finally {
      actionBusy = false;
    }
  }

  function reuseGenerated(attachment: ChatMediaAttachment) {
    if (!chat.pendingMediaRefs.some((ref) => ref.media_id === attachment.mediaId)) {
      chat.pendingMediaRefs = [...chat.pendingMediaRefs, {
        media_id: attachment.mediaId,
        kind: "image",
        mime: attachment.mime,
        label: attachment.label,
        generation_id: attachment.generationId ?? null,
        parent_generation_id: attachment.parentGenerationId ?? null,
      }];
    }
    chat.prefillDraft("Refine this image: ");
    actionStatus = "Image added to the composer for refinement.";
  }

  function canShareImages(): boolean {
    return typeof navigator !== "undefined" && typeof Reflect.get(navigator, "share") === "function";
  }
</script>

<div class="chat-media-parts {compact ? 'chat-media-parts-compact' : ''}">
  {#each attachments as attachment (attachment.mediaId)}
    {#if attachment.mime.startsWith("image/") && urls[attachment.mediaId]}
      <button
        type="button"
        class="chat-media-thumbnail"
        aria-label="Open {attachment.label}"
        title={attachment.label}
        onclick={() => (preview = { url: urls[attachment.mediaId], attachment })}
      >
        <img
          src={urls[attachment.mediaId]}
          alt={attachment.label}
          class="chat-media-thumbnail-image"
          loading="lazy"
        />
        <span class="chat-media-thumbnail-shine" aria-hidden="true"></span>
      </button>
    {:else}
      <span class="chat-media-file" title={attachment.mediaId}>
        {attachment.label}
      </span>
    {/if}
  {/each}
</div>

{#if preview}
  <BodyPortal>
    <div
      class="chat-media-preview-backdrop"
      role="presentation"
      onclick={closeFromBackdrop}
    >
      <div
        class="chat-media-preview-dialog"
        role="dialog"
        tabindex="-1"
        aria-modal="true"
        aria-label={preview.attachment.label}
      >
        <button
          type="button"
          class="chat-media-preview-close"
          aria-label="Close image preview"
          onclick={() => (preview = null)}
        >
          <X size={17} strokeWidth={2} />
        </button>
        <img src={preview.url} alt={preview.attachment.label} class="chat-media-preview-image" />
        <div class="chat-media-preview-label">{preview.attachment.label}</div>
        <div class="chat-media-preview-actions">
          {#if preview.attachment.origin === "drawing" && preview.attachment.editableSourceId}
            <button type="button" disabled={actionBusy} onclick={() => preview && void editDrawing(preview.attachment)}>
              <Pencil size={15} /> Edit
            </button>
            <button type="button" disabled={actionBusy} onclick={() => preview && void saveDrawingToVault(preview.attachment)}>
              <FilePlus2 size={15} /> Save to vault
            </button>
          {/if}
          {#if preview.attachment.origin === "generated"}
            <button type="button" onclick={() => preview && reuseGenerated(preview.attachment)}>
              <RotateCcw size={15} /> Refine
            </button>
            <button type="button" disabled={actionBusy} onclick={() => preview && void saveGeneratedToVault(preview.attachment)}>
              <FilePlus2 size={15} /> Save to vault
            </button>
          {/if}
          <button type="button" disabled={actionBusy} onclick={() => preview && void copyImage(preview.attachment)}>
            <Copy size={15} /> Copy
          </button>
          {#if canShareImages()}
            <button type="button" disabled={actionBusy} onclick={() => preview && void shareImage(preview.attachment)}>
              <Share2 size={15} /> Share
            </button>
          {/if}
          <button type="button" onclick={() => preview && downloadPreview(preview.url, preview.attachment.label)}>
            <Download size={15} /> Download
          </button>
        </div>
        {#if actionStatus}<p class="chat-media-preview-status" aria-live="polite">{actionStatus}</p>{/if}
      </div>
    </div>
  </BodyPortal>
{/if}

<ChatDrawingDialog
  open={editOpen}
  initialDocument={editDocument}
  label={preview?.attachment.label ?? "Drawing"}
  onclose={() => (editOpen = false)}
  onattach={(media) => {
    chat.pendingMediaRefs = [...chat.pendingMediaRefs, media];
    chat.prefillDraft(chat.draft || "Take a look at my updated drawing.");
    actionStatus = "Updated drawing added to the composer.";
  }}
/>
