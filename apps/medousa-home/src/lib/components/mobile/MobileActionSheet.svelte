<script lang="ts">
  import type { Snippet } from "svelte";
  import { ChevronLeft, X } from "@lucide/svelte";
  import BodyPortal from "$lib/components/ui/BodyPortal.svelte";
  import { registerMobileBackHandler } from "$lib/mobileNavigation";
  import { attachMobileSheetGestures } from "$lib/utils/mobileSheetGestures";

  let { open = $bindable(false), title, children, footer, onback, onclose, full = false, busy = false }:
    { open?: boolean; title: string; children: Snippet; footer?: Snippet; onback?: () => void;
      onclose?: () => void; full?: boolean; busy?: boolean } = $props();
  let dialog = $state<HTMLDialogElement | null>(null);
  let header = $state<HTMLElement | null>(null);
  function close() { if (busy) return; open = false; onclose?.(); }
  function back() { if (busy) return; if (onback) onback(); else close(); }
  $effect(() => {
    if (!dialog) return;
    if (open && !dialog.open) dialog.showModal();
    if (!open && dialog.open) dialog.close();
  });
  $effect(() => {
    if (!open || !dialog) return;
    const detachBack = registerMobileBackHandler(() => { back(); return true; }, "modal");
    const detachGestures = attachMobileSheetGestures(dialog, header, {
      onDismiss: close,
      onSwipeBack: () => { back(); return true; },
    });
    return () => { detachBack(); detachGestures(); };
  });
</script>

<BodyPortal>
  <dialog bind:this={dialog} class="mobile-action-sheet" class:full aria-label={title}
    oncancel={(event) => { event.preventDefault(); back(); }}
    onclick={(event) => {
      event.stopPropagation();
      if (event.target !== dialog || !dialog) return;
      const rect = dialog.getBoundingClientRect();
      if (event.clientY < rect.top || event.clientY > rect.bottom || event.clientX < rect.left || event.clientX > rect.right) close();
    }}
    onkeydown={(event) => { if (event.key === "Escape") event.stopPropagation(); }}
  >
    <header bind:this={header}>
      <div class="grabber" aria-hidden="true"></div>
      <div class="heading">
        {#if onback}<button type="button" aria-label="Back" disabled={busy} onclick={back}><ChevronLeft size={21}/></button>{:else}<span></span>{/if}
        <h2>{title}</h2>
        <button type="button" aria-label="Close {title}" disabled={busy} onclick={close}><X size={20}/></button>
      </div>
    </header>
    <div class="sheet-body">{@render children()}</div>
    {#if footer}<footer>{@render footer()}</footer>{/if}
  </dialog>
</BodyPortal>

<style>
  .mobile-action-sheet {
    box-sizing: border-box; position: fixed; inset: auto 0 var(--mobile-keyboard-inset, 0px);
    margin: 0; width: 100%; max-width: none;
    max-height: calc(var(--mobile-layout-height, 100dvh) - var(--mobile-keyboard-inset, 0px) - max(12px, env(safe-area-inset-top)));
    padding: 0 0 env(safe-area-inset-bottom); border: 1px solid rgb(var(--theme-border) / .25);
    border-radius: 24px 24px 0 0; background: rgb(var(--theme-card)); color: rgb(var(--theme-text-primary));
    overflow: hidden; box-shadow: 0 -16px 60px #0005;
  }
  .mobile-action-sheet[open] { display: flex; flex-direction: column; }
  .full { height: calc(var(--mobile-layout-height, 100dvh) - var(--mobile-keyboard-inset, 0px) - max(12px, env(safe-area-inset-top))); }
  dialog::backdrop { background: #0008; }
  header, footer { flex-shrink: 0; }
  .grabber { width: 36px; height: 4px; border-radius: 4px; background: currentColor; opacity: .25; margin: 9px auto 0; }
  .heading { display: grid; grid-template-columns: 44px minmax(0,1fr) 44px; align-items: center; padding: 3px 8px 8px; }
  h2 { margin: 0; font-size: 17px; font-weight: 600; text-align: center; }
  .heading button { display: grid; place-items: center; min-height: 44px; }
  .sheet-body { min-height: 0; overflow-y: auto; overscroll-behavior: contain; padding: 8px 16px 20px; }
  .full .sheet-body { flex: 1; }
  footer { padding: 12px 16px; border-top: 1px solid rgb(var(--theme-border) / .25); }
  .sheet-body :global(.chat-runtime-option > span > span:first-child) { font-size: 15px; }
  .sheet-body :global(.composer-plus-menu-item), .sheet-body :global(.context-action), .sheet-body :global(.chat-runtime-option) { min-height: 48px; font-size: 15px; padding: 12px; }
</style>
