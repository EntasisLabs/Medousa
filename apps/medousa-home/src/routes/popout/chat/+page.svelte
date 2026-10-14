<script lang="ts">
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { chat } from "$lib/stores/chat.svelte";
  import ChatPanel from "$lib/components/chat/ChatPanel.svelte";
  import SessionSidebar from "$lib/components/chat/SessionSidebar.svelte";
  import { layout } from "$lib/runtime/layout.svelte";
  import { hideChatPopout, isTauri } from "$lib/window";
  import { connectWorkshop } from "$lib/workshopConnection";
  import { whenDocumentVisible } from "$lib/utils/whenDocumentVisible";

  onMount(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    if (isTauri()) {
      void listen("popout-close-requested", handleClose).then((stop) => {
        if (disposed) stop();
        else unlisten = stop;
      });
    }
    const detach = whenDocumentVisible(() =>
      connectWorkshop({
        onHealthChange: () => {},
        mode: "observer",
      }),
    );
    return () => {
      disposed = true;
      unlisten?.();
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

  <div class="flex min-h-0 flex-1">
    <SessionSidebar
      open={layout.sessionDrawerOpen}
      onClose={() => layout.setSessionDrawerOpen(false)}
      variant="inline"
    />
    <ChatPanel visible={true} popout />
  </div>
</div>
