<script lang="ts">
  import { onMount, type Snippet } from "svelte";
  import { popBrowserPopoverOverlay, pushBrowserPopoverOverlay } from "$lib/utils/browserPopoverOverlay";

  let { label, compact = false, mobile = false, initialFocus, onClose, children }: {
    label: string; compact?: boolean; mobile?: boolean; initialFocus?: string; onClose: () => void; children: Snippet;
  } = $props();
  let dialog: HTMLDialogElement;

  onMount(() => {
    const previous = document.activeElement;
    let alive = true;
    void pushBrowserPopoverOverlay().then(() => { if (alive) { dialog.showModal(); if (initialFocus) dialog.querySelector<HTMLElement>(initialFocus)?.focus(); } });
    return () => {
      alive = false;
      dialog.close();
      void popBrowserPopoverOverlay();
      if (previous instanceof HTMLElement && previous.isConnected) previous.focus();
    };
  });
</script>

<dialog bind:this={dialog} aria-label={label} class:compact class:mobile
  onkeydown={(event) => event.stopPropagation()}
  oncancel={(event) => { event.preventDefault(); onClose(); }}
  onclick={(event) => {
    if (event.target !== dialog) return;
    const rect = dialog.getBoundingClientRect();
    if (event.clientX < rect.left || event.clientX > rect.right || event.clientY < rect.top || event.clientY > rect.bottom) onClose();
  }}>
  <div class="task-dialog-content">{@render children()}</div>
</dialog>

<style>
  dialog { padding:0; margin:auto; width:min(52rem,calc(100vw - 2rem)); height:min(44rem,calc(100dvh - 4rem)); max-width:none; max-height:calc(100dvh - 2rem); overflow:hidden; border:1px solid rgb(var(--color-surface-600) / .4); border-radius:16px; background:rgb(var(--color-surface-900)); color:rgb(var(--color-surface-100)); box-shadow:0 25px 50px -12px rgb(0 0 0 / .4); }
  dialog::backdrop { background:rgb(0 0 0 / .55); backdrop-filter:blur(2px); }
  .task-dialog-content { position:relative; display:flex; flex-direction:column; height:100%; min-height:0; overflow:auto; }
  dialog.compact { width:min(32rem,calc(100vw - 2rem)); height:fit-content; overflow:visible; }
  dialog.compact .task-dialog-content { height:auto; max-height:calc(100dvh - 2rem); }
  dialog.mobile:not(.compact) { inset:0; width:100%; height:100dvh; max-height:none; border:0; border-radius:0; padding-top:env(safe-area-inset-top,0px); padding-bottom:env(safe-area-inset-bottom,0px); }
</style>
