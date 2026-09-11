<script lang="ts">
  import { Check, LoaderCircle, X } from "@lucide/svelte";
  import BodyPortal from "$lib/components/ui/BodyPortal.svelte";
  import DrawSurface from "$lib/components/draw/DrawSurface.svelte";
  import { chat } from "$lib/stores/chat.svelte";
  import {
    cloneDrawDocument,
    createEmptyDrawDocument,
    type DrawDocument,
  } from "$lib/draw/drawDocument";
  import { uploadDrawingMedia } from "$lib/draw/drawMedia";
  import type { MediaRef } from "$lib/types/media";

  interface Props {
    open: boolean;
    initialDocument?: DrawDocument | null;
    label?: string;
    onclose: () => void;
    onattach: (media: MediaRef) => void;
  }

  let { open, initialDocument = null, label = "Drawing", onclose, onattach }: Props = $props();
  let drawing = $state.raw<DrawDocument>(createEmptyDrawDocument());
  let surface = $state<ReturnType<typeof DrawSurface> | null>(null);
  let attaching = $state(false);
  let error = $state<string | null>(null);
  let opened = false;

  $effect(() => {
    if (open && !opened) {
      drawing = cloneDrawDocument(initialDocument ?? createEmptyDrawDocument());
      error = null;
    }
    opened = open;
  });

  async function attachDrawing() {
    if (attaching) return;
    const current = surface?.currentDocument() ?? drawing;
    if (current.strokes.length === 0) {
      error = "Draw something before adding it to the chat.";
      return;
    }
    attaching = true;
    error = null;
    try {
      onattach(await uploadDrawingMedia(chat.sessionId, current, label));
      onclose();
    } catch (reason) {
      error = reason instanceof Error ? reason.message : String(reason);
    } finally {
      attaching = false;
    }
  }

  function handleKeydown(event: KeyboardEvent) {
    if (event.key === "Escape" && !attaching) onclose();
  }
</script>

{#if open}
  <BodyPortal>
    <div class="chat-drawing-backdrop" role="presentation">
      <div class="chat-drawing-dialog" role="dialog" aria-modal="true" aria-label="Draw for chat" tabindex="-1" onkeydown={handleKeydown}>
        <header class="chat-drawing-header">
          <button type="button" class="chat-drawing-icon" aria-label="Cancel drawing" disabled={attaching} onclick={onclose}>
            <X size={18} />
          </button>
          <div>
            <strong>Draw for chat</strong>
            <span>Editable ink + a vision preview</span>
          </div>
          <button type="button" class="chat-drawing-attach" disabled={attaching} onclick={() => void attachDrawing()}>
            {#if attaching}<LoaderCircle size={16} class="animate-spin" />{:else}<Check size={16} />{/if}
            <span>{attaching ? "Adding…" : "Add"}</span>
          </button>
        </header>
        <div class="chat-drawing-canvas">
          <DrawSurface bind:this={surface} document={drawing} editable variant="full" mobileToolbar onchange={(next) => (drawing = next)} />
        </div>
        {#if error}<p class="chat-drawing-error" role="alert">{error}</p>{/if}
      </div>
    </div>
  </BodyPortal>
{/if}

<style>
  .chat-drawing-backdrop { position: fixed; inset: 0; z-index: 280; display: grid; place-items: center; padding: 1rem; background: rgb(0 0 0 / 0.72); backdrop-filter: blur(10px); }
  .chat-drawing-dialog { display: flex; width: min(960px, 96vw); height: min(760px, 92dvh); min-height: 0; flex-direction: column; overflow: hidden; border: 1px solid rgb(var(--color-surface-500) / 0.4); border-radius: 1.25rem; background: rgb(var(--color-surface-950)); box-shadow: 0 28px 80px rgb(0 0 0 / 0.55); }
  .chat-drawing-header { display: grid; grid-template-columns: auto 1fr auto; align-items: center; gap: 0.75rem; min-height: 3.75rem; padding: 0.45rem 0.65rem; border-bottom: 1px solid rgb(var(--color-surface-500) / 0.25); }
  .chat-drawing-header > div { display: grid; min-width: 0; }
  .chat-drawing-header strong { color: rgb(var(--theme-text-primary)); font-size: 0.9rem; }
  .chat-drawing-header span { color: rgb(var(--theme-text-secondary)); font-size: 0.7rem; }
  .chat-drawing-icon, .chat-drawing-attach { display: inline-flex; height: 2.55rem; align-items: center; justify-content: center; border-radius: 0.75rem; }
  .chat-drawing-icon { width: 2.55rem; color: rgb(var(--theme-text-secondary)); }
  .chat-drawing-attach { gap: 0.35rem; padding: 0 0.85rem; background: rgb(var(--color-primary-500) / 0.2); color: rgb(var(--color-primary-200)); font-size: 0.78rem; font-weight: 650; }
  .chat-drawing-icon:hover, .chat-drawing-attach:hover { background: rgb(var(--color-surface-500) / 0.25); }
  button:disabled { opacity: 0.45; }
  .chat-drawing-canvas { min-height: 0; flex: 1; }
  .chat-drawing-error { margin: 0; padding: 0.65rem 0.85rem; border-top: 1px solid rgb(var(--color-error-500) / 0.25); color: rgb(var(--color-error-300)); font-size: 0.75rem; }
  @media (max-width: 640px) {
    .chat-drawing-backdrop { padding: 0; }
    .chat-drawing-dialog { width: 100vw; height: 100dvh; border: 0; border-radius: 0; }
    .chat-drawing-header { padding-top: max(0.45rem, env(safe-area-inset-top)); }
    .chat-drawing-header > div span { display: none; }
  }
</style>
