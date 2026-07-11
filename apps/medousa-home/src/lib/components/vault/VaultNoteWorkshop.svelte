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
  import { vaultContextScopeHint } from "$lib/utils/vaultNoteBridge";
  import { launchVaultNoteWorkshop } from "$lib/utils/vaultNoteWorkshop";

  interface Props {
    onOpenFullChat?: () => void;
  }

  let { onOpenFullChat }: Props = $props();

  let panelEl = $state<HTMLDivElement | null>(null);
  let dragging = $state(false);
  let sessionMenuOpen = $state(false);
  let dragOffsetX = 0;
  let dragOffsetY = 0;

  const scopeHint = $derived(
    chat.vaultNoteContext ? vaultContextScopeHint(chat.vaultNoteContext) : null,
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
    if (!noteWorkshop.open) return;
    if (layout.desktopSurface !== "library") {
      noteWorkshop.close();
    }
  });

  $effect(() => {
    if (noteWorkshop.open && !noteWorkshop.minimized) return;
    dragging = false;
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
    if (!dragging) return;
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
      class="vault-note-workshop-dock"
      aria-label="Restore note workshop"
      onclick={() => (noteWorkshop.minimized = false)}
    >
      <MessageCircle size={16} strokeWidth={1.75} />
      <span class="truncate">{title}</span>
    </button>
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
