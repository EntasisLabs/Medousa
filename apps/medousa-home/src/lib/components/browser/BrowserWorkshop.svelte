<script lang="ts">
  import { onMount } from "svelte";
  import { ExternalLink, GripHorizontal, MessageCircle, Minus, X } from "@lucide/svelte";
  import ChatPanel from "$lib/components/chat/ChatPanel.svelte";
  import BrowserControlHandoff from "$lib/components/browser/BrowserControlHandoff.svelte";
  import { browser } from "$lib/stores/browser.svelte";
  import { browserWorkshop } from "$lib/stores/browserWorkshop.svelte";
  import { chat } from "$lib/stores/chat.svelte";
  import { humanBrowser } from "$lib/stores/humanBrowser.svelte";
  import { layout } from "$lib/stores/layout.svelte";

  interface Props {
    onOpenFullChat?: () => void;
  }

  let { onOpenFullChat }: Props = $props();

  let dragging = $state(false);
  let dragOffsetX = 0;
  let dragOffsetY = 0;

  $effect(() => {
    if (!browserWorkshop.open) return;
    if (layout.isMobile && layout.mobileTab !== "web") {
      browserWorkshop.close();
      return;
    }
    if (!layout.isMobile && layout.desktopSurface !== "web") {
      browserWorkshop.close();
    }
  });

  $effect(() => {
    if (!browserWorkshop.open) return;
    browserWorkshop.scopeLabel = humanBrowser.scopeLabel;
  });

  $effect(() => {
    if (browserWorkshop.open && !browserWorkshop.minimized) return;
    dragging = false;
    document.body.classList.remove("vault-workshop-dragging");
  });

  onMount(() => {
    void chat.ensureSessionHydrated();

    const onKeydown = (event: KeyboardEvent) => {
      if (event.key !== "Escape" || !browserWorkshop.open || browserWorkshop.minimized) return;
      const target = event.target as HTMLElement | null;
      const typing =
        target &&
        (target.tagName === "INPUT" ||
          target.tagName === "TEXTAREA" ||
          target.isContentEditable);
      if (typing) return;
      event.preventDefault();
      browserWorkshop.toggleMinimize();
    };
    window.addEventListener("keydown", onKeydown);
    return () => window.removeEventListener("keydown", onKeydown);
  });

  function handleClose() {
    browserWorkshop.close();
  }

  function handleOpenFullChat() {
    browserWorkshop.minimized = true;
    onOpenFullChat?.();
  }

  function handleDragStart(event: PointerEvent) {
    if ((event.target as HTMLElement).closest("button")) return;
    event.preventDefault();
    dragging = true;
    document.body.classList.add("vault-workshop-dragging");
    window.getSelection()?.removeAllRanges();
    dragOffsetX = event.clientX - browserWorkshop.x;
    dragOffsetY = event.clientY - browserWorkshop.y;
    (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
  }

  function handleDragMove(event: PointerEvent) {
    if (!dragging) return;
    browserWorkshop.setPosition(event.clientX - dragOffsetX, event.clientY - dragOffsetY);
  }

  function handleDragEnd(event: PointerEvent) {
    if (!dragging) return;
    dragging = false;
    document.body.classList.remove("vault-workshop-dragging");
    try {
      (event.currentTarget as HTMLElement).releasePointerCapture(event.pointerId);
    } catch {
      // ignore
    }
  }
</script>

{#if browserWorkshop.open}
  {#if browserWorkshop.minimized}
    <button
      type="button"
      class="vault-note-workshop-dock"
      aria-label="Restore browser workshop"
      onclick={() => (browserWorkshop.minimized = false)}
    >
      <MessageCircle size={16} strokeWidth={1.75} />
      <span class="truncate">{browserWorkshop.scopeLabel}</span>
    </button>
  {:else}
    <div
      class="vault-note-workshop"
      role="dialog"
      aria-modal="false"
      aria-label="Browser workshop"
      style:left="{browserWorkshop.x}px"
      style:top="{browserWorkshop.y}px"
    >
      <header
        class="vault-note-workshop-header"
        role="toolbar"
        tabindex="-1"
        aria-label="Browser workshop controls"
        onpointerdown={handleDragStart}
        onpointermove={handleDragMove}
        onpointerup={handleDragEnd}
        onpointercancel={handleDragEnd}
      >
        <GripHorizontal size={16} strokeWidth={1.75} class="shrink-0 text-surface-500" />
        <div class="min-w-0 flex-1">
          <p class="truncate text-sm font-medium text-surface-50">Browser workshop</p>
          <p class="truncate text-[11px] text-surface-400">{browserWorkshop.scopeLabel}</p>
        </div>
        <div class="hidden sm:block">
          <BrowserControlHandoff compact={true} />
        </div>
        <div class="flex shrink-0 items-center gap-1">
          {#if onOpenFullChat}
            <button
              type="button"
              class="vault-note-workshop-icon-btn"
              aria-label="Open full chat"
              title="Open full chat"
              onclick={handleOpenFullChat}
            >
              <ExternalLink size={15} strokeWidth={1.75} />
            </button>
          {/if}
          <button
            type="button"
            class="vault-note-workshop-icon-btn"
            aria-label="Minimize"
            title="Minimize"
            onclick={() => browserWorkshop.toggleMinimize()}
          >
            <Minus size={15} strokeWidth={1.75} />
          </button>
          <button
            type="button"
            class="vault-note-workshop-icon-btn"
            aria-label="Close browser workshop"
            title="Close"
            onclick={handleClose}
          >
            <X size={15} strokeWidth={2} />
          </button>
        </div>
      </header>

      <div class="vault-note-workshop-body">
        <ChatPanel
          visible={true}
          embedded={true}
          workshop={true}
          onOpenContext={handleOpenFullChat}
        />
      </div>
    </div>
  {/if}
{/if}
