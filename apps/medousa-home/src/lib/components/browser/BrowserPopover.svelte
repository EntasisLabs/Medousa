<script lang="ts">
  import type { Snippet } from "svelte";
  import {
    popBrowserPopoverOverlay,
    popoverStyle,
    pushBrowserPopoverOverlay,
    type PopoverPlacement,
  } from "$lib/utils/browserPopoverOverlay";

  interface Props {
    open?: boolean;
    onClose?: () => void;
    anchorRect?: DOMRect | null;
    title?: string;
    /** above = Safari-style from bottom bar; panel = centered card; below = drop down */
    placement?: PopoverPlacement;
    /** Hide native webview while open (required on desktop Tauri). */
    hideNativeEmbed?: boolean;
    /** Subtle backdrop tap-to-dismiss */
    backdrop?: boolean;
    width?: number;
    maxHeight?: number;
    ariaLabel?: string;
    children: Snippet;
  }

  let {
    open = false,
    onClose,
    anchorRect = null,
    title = "",
    placement = "above",
    hideNativeEmbed = true,
    backdrop = true,
    width = 320,
    maxHeight = 360,
    ariaLabel,
    children,
  }: Props = $props();

  const style = $derived(
    popoverStyle(anchorRect, placement, { width, maxHeight }),
  );

  $effect(() => {
    if (!open || !hideNativeEmbed) return;
    void pushBrowserPopoverOverlay();
    return () => {
      void popBrowserPopoverOverlay();
    };
  });

  $effect(() => {
    if (!open) return;

    const onDocumentClick = (event: MouseEvent) => {
      const target = event.target;
      if (!(target instanceof Element)) return;
      if (target.closest("[data-browser-popover]") || target.closest("[data-browser-popover-trigger]")) {
        return;
      }
      onClose?.();
    };

    const onKeydown = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose?.();
    };

    const timer = window.setTimeout(() => {
      document.addEventListener("click", onDocumentClick, true);
      document.addEventListener("keydown", onKeydown);
    }, 0);

    return () => {
      window.clearTimeout(timer);
      document.removeEventListener("click", onDocumentClick, true);
      document.removeEventListener("keydown", onKeydown);
    };
  });
</script>

{#if open}
  {#if backdrop}
    <button
      type="button"
      class="browser-popover-backdrop fixed inset-0 z-[110] cursor-default border-0 p-0"
      aria-label="Dismiss"
      onclick={() => onClose?.()}
    ></button>
  {/if}

  <div
    data-browser-popover
    class="browser-popover fixed z-[120] flex flex-col overflow-hidden {placement === 'panel'
      ? 'browser-popover-panel'
      : 'browser-popover-anchor'}"
    style={style}
    role="dialog"
    aria-label={ariaLabel ?? title ?? "Menu"}
  >
    {#if title}
      <header class="browser-popover-header">
        <p class="truncate text-sm font-semibold text-surface-50">{title}</p>
        <button type="button" class="btn btn-icon btn-sm shrink-0" aria-label="Close" onclick={() => onClose?.()}>
          ✕
        </button>
      </header>
    {/if}
    <div class="browser-popover-body min-h-0 flex-1 overflow-y-auto">
      {@render children()}
    </div>
  </div>
{/if}
