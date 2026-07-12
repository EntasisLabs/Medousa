<script lang="ts">
  import { onMount } from "svelte";
  import {
    ExternalLink,
    GripHorizontal,
    History,
    MessageCircle,
    Minus,
    X,
  } from "@lucide/svelte";
  import ChatPanel from "$lib/components/chat/ChatPanel.svelte";
  import VaultNoteChatSessionMenu from "$lib/components/vault/VaultNoteChatSessionMenu.svelte";
  import { chat } from "$lib/stores/chat.svelte";
  import { layout } from "$lib/stores/layout.svelte";
  import { noteWorkshop } from "$lib/stores/noteWorkshop.svelte";
  import { vault } from "$lib/stores/vault.svelte";
  import {
    vaultContextHasSelection,
    vaultContextScopeHint,
  } from "$lib/utils/vaultNoteBridge";
  import { launchVaultNoteWorkshop } from "$lib/utils/vaultNoteWorkshop";

  interface Props {
    onOpenFullChat?: () => void;
    /** Sticky pop-out host — bottom sheet instead of floating IM card. */
    stickyHost?: boolean;
  }

  let { onOpenFullChat, stickyHost = false }: Props = $props();

  let panelEl = $state<HTMLDivElement | null>(null);
  let dragging = $state(false);
  let resizingSheet = $state(false);
  let sessionMenuOpen = $state(false);
  let dragOffsetX = 0;
  let dragOffsetY = 0;
  let sheetHeight = $state(loadStickySheetHeight());

  const STICKY_SHEET_MIN = 160;
  const STICKY_SHEET_KEY = "medousa-vault-sticky-sheet-height";

  function stickySheetMax(): number {
    if (typeof window === "undefined") return 420;
    return Math.max(STICKY_SHEET_MIN, Math.round(window.innerHeight * 0.92));
  }

  function loadStickySheetHeight(): number {
    if (typeof window === "undefined") return 260;
    try {
      const raw = localStorage.getItem(STICKY_SHEET_KEY);
      const parsed = raw ? Number(raw) : NaN;
      if (Number.isFinite(parsed)) {
        return Math.min(stickySheetMax(), Math.max(STICKY_SHEET_MIN, parsed));
      }
    } catch {
      /* ignore */
    }
    return Math.min(280, Math.round(window.innerHeight * 0.52));
  }

  function clampSheetHeight(next: number): number {
    return Math.min(stickySheetMax(), Math.max(STICKY_SHEET_MIN, Math.round(next)));
  }

  function saveStickySheetHeight(height: number) {
    if (typeof localStorage === "undefined") return;
    try {
      localStorage.setItem(STICKY_SHEET_KEY, String(height));
    } catch {
      /* ignore */
    }
  }

  function handleSheetResizeStart(event: PointerEvent) {
    event.preventDefault();
    resizingSheet = true;
    document.body.classList.add("vault-workshop-dragging");
    (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
  }

  function handleSheetResizeMove(event: PointerEvent) {
    if (!resizingSheet) return;
    const next = clampSheetHeight(window.innerHeight - event.clientY);
    sheetHeight = next;
  }

  function handleSheetResizeEnd(event: PointerEvent) {
    if (!resizingSheet) return;
    resizingSheet = false;
    document.body.classList.remove("vault-workshop-dragging");
    saveStickySheetHeight(sheetHeight);
    try {
      (event.currentTarget as HTMLElement).releasePointerCapture(event.pointerId);
    } catch {
      /* ignore */
    }
  }

  const scopeHint = $derived(
    chat.vaultNoteContext ? vaultContextScopeHint(chat.vaultNoteContext) : null,
  );
  const talkingAboutPassage = $derived(
    vaultContextHasSelection(chat.vaultNoteContext),
  );

  const title = $derived(
    chat.vaultNoteContext?.title ??
      vault.labelByPath().get(noteWorkshop.notePath ?? "") ??
      "Note workshop",
  );

  $effect(() => {
    if (!noteWorkshop.open || !noteWorkshop.notePath) return;
    if (vault.selectedPath !== noteWorkshop.notePath) {
      noteWorkshop.close();
      chat.clearVaultNoteContext();
    }
  });

  $effect(() => {
    if (!noteWorkshop.open || stickyHost) return;
    if (layout.desktopSurface !== "library") {
      noteWorkshop.close();
    }
  });

  $effect(() => {
    if (noteWorkshop.open && !noteWorkshop.minimized) return;
    dragging = false;
    resizingSheet = false;
    document.body.classList.remove("vault-workshop-dragging");
  });

  onMount(() => {
    void chat.ensureSessionHydrated();

    const onKeydown = (event: KeyboardEvent) => {
      if (event.key !== "Escape" || !noteWorkshop.open || noteWorkshop.minimized) return;
      const target = event.target as HTMLElement | null;
      const typing =
        target &&
        (target.tagName === "INPUT" ||
          target.tagName === "TEXTAREA" ||
          target.isContentEditable);
      if (typing) return;
      event.preventDefault();
      noteWorkshop.toggleMinimize();
    };
    window.addEventListener("keydown", onKeydown);
    return () => window.removeEventListener("keydown", onKeydown);
  });

  function handleClose() {
    noteWorkshop.close();
    chat.clearVaultNoteContext();
  }

  function handleOpenFullChat() {
    noteWorkshop.minimized = true;
    onOpenFullChat?.();
  }

  async function handleSessionSelect(session: "fresh" | string) {
    if (!noteWorkshop.notePath) return;
    sessionMenuOpen = false;
    await launchVaultNoteWorkshop({
      path: noteWorkshop.notePath,
      title: vault.title,
      content: vault.content,
      wikilinksOut: vault.wikilinksOut,
      backlinks: vault.backlinks,
      session,
      flushSave: vault.dirty ? async () => { await vault.flushSave(); } : undefined,
    });
  }

  function handleDragStart(event: PointerEvent) {
    if (stickyHost) return;
    if ((event.target as HTMLElement).closest("button")) return;
    event.preventDefault();
    dragging = true;
    document.body.classList.add("vault-workshop-dragging");
    window.getSelection()?.removeAllRanges();
    dragOffsetX = event.clientX - noteWorkshop.x;
    dragOffsetY = event.clientY - noteWorkshop.y;
    (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
  }

  function handleDragMove(event: PointerEvent) {
    if (!dragging || stickyHost) return;
    noteWorkshop.setPosition(event.clientX - dragOffsetX, event.clientY - dragOffsetY);
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

{#if noteWorkshop.open}
  {#if noteWorkshop.minimized}
    <button
      type="button"
      class="vault-note-workshop-dock {stickyHost ? 'vault-note-workshop-dock--sticky' : ''}"
      aria-label="Restore note workshop"
      onclick={() => (noteWorkshop.minimized = false)}
    >
      <MessageCircle size={16} strokeWidth={1.75} />
      <span class="truncate">{title}</span>
    </button>
  {:else if stickyHost}
    <div
      class="vault-note-workshop-sheet {resizingSheet ? 'vault-note-workshop-sheet--resizing' : ''}"
      role="dialog"
      aria-modal="false"
      aria-label="Note chat"
      tabindex="-1"
      style:height="{sheetHeight}px"
    >
      <div
        class="vault-note-workshop-sheet-resize"
        role="slider"
        aria-orientation="horizontal"
        aria-label="Resize chat height"
        aria-valuenow={sheetHeight}
        aria-valuemin={STICKY_SHEET_MIN}
        aria-valuemax={stickySheetMax()}
        tabindex="0"
        onpointerdown={handleSheetResizeStart}
        onpointermove={handleSheetResizeMove}
        onpointerup={handleSheetResizeEnd}
        onpointercancel={handleSheetResizeEnd}
        onkeydown={(event) => {
          if (event.key === "ArrowUp") {
            event.preventDefault();
            sheetHeight = clampSheetHeight(sheetHeight + 24);
            saveStickySheetHeight(sheetHeight);
          } else if (event.key === "ArrowDown") {
            event.preventDefault();
            sheetHeight = clampSheetHeight(sheetHeight - 24);
            saveStickySheetHeight(sheetHeight);
          }
        }}
      >
        <span class="vault-note-workshop-sheet-grip" aria-hidden="true"></span>
      </div>
      <header class="vault-note-workshop-sheet-header" aria-label="Note chat controls">
        <p class="min-w-0 flex-1 truncate text-[11px] text-surface-400">
          {talkingAboutPassage ? "Working on this passage" : "Talk about this note"}
        </p>
        <div class="flex shrink-0 items-center gap-0.5">
          <div class="relative">
            <button
              type="button"
              class="vault-note-workshop-icon-btn {sessionMenuOpen
                ? 'bg-surface-800 text-surface-100'
                : ''}"
              aria-label="Switch chat"
              title="Switch chat"
              aria-haspopup="menu"
              aria-expanded={sessionMenuOpen}
              onclick={() => (sessionMenuOpen = !sessionMenuOpen)}
            >
              <History size={14} strokeWidth={1.75} />
            </button>
            <VaultNoteChatSessionMenu
              open={sessionMenuOpen}
              onClose={() => (sessionMenuOpen = false)}
              onSelect={handleSessionSelect}
              class="vault-note-chat-session-menu-workshop"
            />
          </div>
          <button
            type="button"
            class="vault-note-workshop-icon-btn"
            aria-label="Minimize"
            title="Minimize"
            onclick={() => noteWorkshop.toggleMinimize()}
          >
            <Minus size={14} strokeWidth={1.75} />
          </button>
          <button
            type="button"
            class="vault-note-workshop-icon-btn"
            aria-label="Close note chat"
            title="Close"
            onclick={handleClose}
          >
            <X size={14} strokeWidth={2} />
          </button>
        </div>
      </header>
      <div class="vault-note-workshop-body">
        <ChatPanel
          visible={true}
          embedded={true}
          workshop={true}
          workshopSticky={true}
          showPopout={false}
        />
      </div>
    </div>
  {:else}
    <div
      bind:this={panelEl}
      class="vault-note-workshop"
      role="dialog"
      aria-modal="false"
      aria-label="Note workshop"
      style:left="{noteWorkshop.x}px"
      style:top="{noteWorkshop.y}px"
    >
      <header
        class="vault-note-workshop-header"
        role="toolbar"
        tabindex="-1"
        aria-label="Note workshop controls"
        onpointerdown={handleDragStart}
        onpointermove={handleDragMove}
        onpointerup={handleDragEnd}
        onpointercancel={handleDragEnd}
      >
        <GripHorizontal size={16} strokeWidth={1.75} class="shrink-0 text-surface-500" />
        <div class="min-w-0 flex-1">
          <p class="truncate text-sm font-medium text-surface-50">{title}</p>
          {#if scopeHint}
            <p class="truncate text-[11px] text-surface-400">{scopeHint}</p>
          {/if}
        </div>
        <div class="flex shrink-0 items-center gap-1">
          <div class="relative">
            <button
              type="button"
              class="vault-note-workshop-icon-btn {sessionMenuOpen
                ? 'bg-surface-800 text-surface-100'
                : ''}"
              aria-label="Switch chat"
              title="Switch chat"
              aria-haspopup="menu"
              aria-expanded={sessionMenuOpen}
              onclick={() => (sessionMenuOpen = !sessionMenuOpen)}
            >
              <History size={15} strokeWidth={1.75} />
            </button>
            <VaultNoteChatSessionMenu
              open={sessionMenuOpen}
              onClose={() => (sessionMenuOpen = false)}
              onSelect={handleSessionSelect}
              class="vault-note-chat-session-menu-workshop"
            />
          </div>
          {#if onOpenFullChat}
            <button
              type="button"
              class="vault-note-workshop-icon-btn"
              aria-label="Send to chat"
              title="Send to chat"
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
            onclick={() => noteWorkshop.toggleMinimize()}
          >
            <Minus size={15} strokeWidth={1.75} />
          </button>
          <button
            type="button"
            class="vault-note-workshop-icon-btn"
            aria-label="Close note workshop"
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
          showPopout={false}
          onOpenContext={handleOpenFullChat}
        />
      </div>
    </div>
  {/if}
{/if}
