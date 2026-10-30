<script lang="ts">
  import { onMount, type Snippet } from 'svelte';
  import { ArrowLeft, ArrowLeftRight } from '@lucide/svelte';
  import { registerMobileBackHandler } from '$lib/mobileNavigation';

  let { title, participant = 'Agent', onClose, children, returnFocus, mobile = false }: { title: string; participant?: string; onClose: () => void; children: Snippet; returnFocus?: HTMLElement | null; mobile?: boolean } = $props();
  let root: HTMLDialogElement;
  function place() {
    const rect = root?.parentElement?.getBoundingClientRect();
    if (!rect) return;
    root.style.setProperty('--exchange-left', `${rect.left}px`);
    root.style.setProperty('--exchange-top', `${rect.top}px`);
    root.style.setProperty('--exchange-width', `${rect.width}px`);
    root.style.setProperty('--exchange-height', `${rect.height}px`);
  }
  onMount(() => {
    const origin = returnFocus ?? (document.activeElement instanceof HTMLElement ? document.activeElement : null);
    place(); root.showModal(); root.focus();
    const observer = typeof ResizeObserver !== 'undefined' ? new ResizeObserver(place) : null;
    if (root.parentElement) observer?.observe(root.parentElement);
    const back = registerMobileBackHandler(() => { onClose(); return true; });
    return () => { observer?.disconnect(); back(); if (origin?.isConnected && !origin.closest('[inert]')) origin.focus(); };
  });
  function keydown(event: KeyboardEvent) {
    if (event.key === 'Escape') { event.preventDefault(); event.stopPropagation(); onClose(); }
    if (event.key !== 'Tab') return;
    const focusable = [...root.querySelectorAll<HTMLElement>('button:not(:disabled), a[href], input:not(:disabled), textarea:not(:disabled), summary, [tabindex="0"]')].filter(el => el.getClientRects().length > 0);
    const first = focusable[0], last = focusable.at(-1);
    if (!first) { event.preventDefault(); return; }
    if (event.shiftKey && (document.activeElement === first || document.activeElement === root)) { event.preventDefault(); last?.focus(); }
    else if (!event.shiftKey && (document.activeElement === last || document.activeElement === root)) { event.preventDefault(); first.focus(); }
  }
</script>

<svelte:window onresize={place} />
<dialog bind:this={root} class="agent-exchange" class:mobile aria-modal="true" aria-label={title} tabindex="-1" onkeydown={keydown} onclose={onClose}>
  <header>
    <button type="button" class="back" onclick={onClose}><ArrowLeft size={16} /> <span>Our chat</span></button>
    <div class="relationship"><span>Medousa</span><ArrowLeftRight size={13} /><span>{participant}</span></div>
    <span class="reading">Viewing exchange</span>
  </header>
  <div class="exchange-scroll"><div class="exchange-body">{@render children()}</div></div>
  <footer><span>Reading view · Decisions require explicit action</span><button type="button" onclick={onClose}>Return to our chat</button></footer>
</dialog>

<style>
  .agent-exchange { position: fixed; inset: auto; left: var(--exchange-left, 0px); top: var(--exchange-top, 0px); width: var(--exchange-width, 100vw); height: var(--exchange-height, 100dvh); max-width: none; max-height: none; box-sizing: border-box; margin: 0; border: 0; padding: 0; z-index: 40; display: flex; flex-direction: column; min-width: 0; min-height: 0; background: rgb(var(--theme-card)); color: rgb(var(--theme-text-primary)); outline: none; }
  .agent-exchange::backdrop { background: transparent; }
  .agent-exchange.mobile { left: 0; top: 0; width: 100vw; height: 100dvh; }
  header { display: flex; align-items: center; justify-content: space-between; gap: 12px; min-height: 64px; padding: 12px 20px; border-bottom: 1px solid rgb(var(--theme-border) / .35); }
  button { display: inline-flex; align-items: center; justify-content: center; gap: 7px; border: 0; background: transparent; color: rgb(var(--theme-text-secondary)); padding: 8px 12px; border-radius: 20px; font-size: 12px; cursor: pointer; }
  button:hover { background: rgb(var(--theme-card-hover)); color: rgb(var(--theme-text-primary)); }
  button:focus-visible { outline: 2px solid rgb(var(--theme-link)); outline-offset: 2px; }
  .relationship { display: flex; align-items: center; gap: 10px; min-width: 0; padding: 8px 14px; border-radius: 24px; background: rgb(var(--theme-card)); font-size: 12px; }
  .relationship span:last-child { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .reading, footer span { color: rgb(var(--theme-text-tertiary)); font-size: 11px; }
  .exchange-scroll { flex: 1; min-height: 0; overflow-y: auto; overscroll-behavior: contain; }
  .exchange-body { box-sizing: border-box; width: min(100%, 760px); margin: 0 auto; padding: 32px 24px; }
  footer { display: flex; flex-direction: column; align-items: center; gap: 10px; padding: 12px 20px max(16px, env(safe-area-inset-bottom)); }
  footer button { background: rgb(var(--theme-card)); }
  @media (max-width: 600px) { header { padding: 10px; gap: 6px; } .reading { display: none; } .back { padding-inline: 6px; } .exchange-body { padding: 24px 16px; } }
  @media (pointer: coarse) { button { min-height: 44px; } }
</style>
