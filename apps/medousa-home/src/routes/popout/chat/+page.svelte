<script lang="ts">
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { chat } from "$lib/stores/chat.svelte";
  import ChatPanel from "$lib/components/chat/ChatPanel.svelte";
  import SessionSidebar from "$lib/components/chat/SessionSidebar.svelte";
  import { layout } from "$lib/runtime/layout.svelte";
  import { hideChatPopout, isTauri } from "$lib/window";
  import { workshops } from "$lib/stores/workshops.svelte";
  import { setActiveWorkshopIdPort, workshopScopedStorageKey } from "$lib/utils/workshopLocality";
  import { CHAT_POPOUT_KEY, readChatPopoutRequest, clearChatPopoutRequest } from "$lib/utils/chatPopout";
  import { activateWorkshopScope, connectWorkshop } from "$lib/workshopConnection";
  import { whenDocumentVisible } from "$lib/utils/whenDocumentVisible";

  setActiveWorkshopIdPort(() => workshops.activeWorkshopId);
  let ready = false;
  let opening = false;
  let openError = $state<string | null>(null);
  async function openRequestedConversation() {
    if (!ready || opening) return;
    const request = readChatPopoutRequest(workshops.activeWorkshopId);
    if (!request || request.scope !== chat.workshopScopeId) return;
    opening = true;
    openError = null;
    try {
      await chat.switchSession(request.sessionId);
      clearChatPopoutRequest(request);
    } catch (cause) { openError = cause instanceof Error ? cause.message : String(cause); }
    finally { opening = false; }
    const next = readChatPopoutRequest(workshops.activeWorkshopId);
    if (next && (next.requestedAt !== request.requestedAt || next.sessionId !== request.sessionId)) void openRequestedConversation();
  }

  onMount(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    let unlistenSession: (() => void) | undefined;
    if (isTauri()) {
      void listen("popout-close-requested", handleClose).then((stop) => {
        if (disposed) stop();
        else unlisten = stop;
      });
    }
    if (isTauri()) void listen("chat-popout-session", () => void openRequestedConversation()).then(stop => {
      if (disposed) stop(); else unlistenSession = stop;
    });
    const onStorage = (event: StorageEvent) => {
      if (event.key === workshopScopedStorageKey(CHAT_POPOUT_KEY, workshops.activeWorkshopId)) void openRequestedConversation();
    };
    window.addEventListener("storage", onStorage);
    const detach = whenDocumentVisible(() =>
      connectWorkshop({
        onHealthChange: health => {
          ready = Boolean(health?.ok);
          if (ready) { activateWorkshopScope(workshops.activeWorkshopId); void openRequestedConversation(); }
        },
        mode: "observer",
      }),
    );
    return () => {
      disposed = true;
      unlisten?.();
      unlistenSession?.();
      window.removeEventListener("storage", onStorage);
      detach();
    };
  });

  async function handleClose() {
    chat.flushDraftPersist();
    if (isTauri()) {
      await hideChatPopout();
    }
  }
</script>

<div class="flex h-screen w-screen flex-col bg-surface-950 text-surface-50">
  <header
    class="flex items-center justify-between border-b border-surface-500/20 px-4 py-2"
  >
    <div>
      <h1 class="text-sm font-semibold">Medousa Chat</h1>
      <p class="text-xs text-content-tertiary">Pop-out session</p>
    </div>
    {#if isTauri()}
      <button
        type="button"
        class="btn btn-sm variant-ghost-surface"
        onclick={handleClose}
      >
        Close
      </button>
    {/if}
  </header>

  {#if openError}<p role="alert" class="px-4 py-2 text-sm">{openError}</p>{/if}
  <div class="flex min-h-0 flex-1">
    <SessionSidebar
      open={layout.sessionDrawerOpen}
      onClose={() => layout.setSessionDrawerOpen(false)}
      variant="inline"
    />
    <ChatPanel visible={true} popout />
  </div>
</div>
